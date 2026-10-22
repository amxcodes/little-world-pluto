#Requires -Version 5.1
param(
    [string]$ExecutablePath = (Join-Path $PSScriptRoot '..\..\target\release\island-desktop.exe'),
    [int]$Seconds = 60
)

$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class IslandIdleProbe {
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
    public delegate bool EnumWindowsCallback(IntPtr window, IntPtr state);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsCallback callback, IntPtr state);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassNameW(IntPtr window, StringBuilder text, int size);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT point);
    [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT point);
    public static IntPtr FindShell(uint wantedProcessId) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((window, state) => {
            uint processId;
            GetWindowThreadProcessId(window, out processId);
            if (processId != wantedProcessId) return true;
            var className = new StringBuilder(256);
            GetClassNameW(window, className, className.Capacity);
            if (className.ToString() == "IslandDesktopEdgeShell") { found = window; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
}
'@
$ExecutablePath = [IO.Path]::GetFullPath($ExecutablePath)
if (-not (Test-Path -LiteralPath $ExecutablePath -PathType Leaf)) { throw 'Release executable is missing.' }
if ($Seconds -lt 10 -or $Seconds -gt 120) { throw 'Seconds must be between 10 and 120.' }
if (Get-Process -Name 'island-desktop' -ErrorAction SilentlyContinue) { throw 'Close the existing Island app before measuring.' }

$oldLocalAppData = $env:LOCALAPPDATA
$oldCodexHome = $env:CODEX_HOME
$process = $null
try {
    $root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\target\phase1-idle-profile'))
    $env:LOCALAPPDATA = Join-Path $root 'localappdata'
    $env:CODEX_HOME = Join-Path $root 'codex'
    [void][IO.Directory]::CreateDirectory($env:LOCALAPPDATA)
    [void][IO.Directory]::CreateDirectory($env:CODEX_HOME)
    $process = Start-Process -FilePath $ExecutablePath -PassThru
    Start-Sleep -Seconds 5
    if ($process.HasExited) { throw "Island exited during startup: $($process.ExitCode)" }
    $shell = [IntPtr]::Zero
    $shellDeadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        $shell = [IslandIdleProbe]::FindShell([uint32]$process.Id)
        if ($shell -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 100 }
    } while ($shell -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $shellDeadline)
    if ($shell -eq [IntPtr]::Zero) { throw 'Product shell HWND was not found; idle sample would be invalid.' }
    [uint32]$shellProcessId = 0
    $shellThreadId = [IslandIdleProbe]::GetWindowThreadProcessId($shell, [ref]$shellProcessId)
    if ($shellProcessId -ne [uint32]$process.Id) { throw 'Shell HWND belongs to a different process.' }
    $point = New-Object IslandIdleProbe+POINT
    $cursorWindow = [IntPtr]::Zero
    if ([IslandIdleProbe]::GetCursorPos([ref]$point)) { $cursorWindow = [IslandIdleProbe]::WindowFromPoint($point) }
    'shell_thread_id={0} shell_process_id={1} cursor=({2},{3}) cursor_on_shell={4}' -f $shellThreadId, $shellProcessId, $point.X, $point.Y, ($cursorWindow -eq $shell)
    $sample = Get-Process -Id $process.Id
    $startCpu = $sample.TotalProcessorTime.TotalSeconds
    $threadStarts = @{}
    foreach ($thread in $sample.Threads) {
        try { $threadStarts[$thread.Id] = $thread.TotalProcessorTime.TotalSeconds } catch { }
    }
    $watch = [Diagnostics.Stopwatch]::StartNew()
    Start-Sleep -Seconds $Seconds
    $watch.Stop()
    $sample = Get-Process -Id $process.Id
    $cpuSeconds = $sample.TotalProcessorTime.TotalSeconds - $startCpu
    $cpuOneCorePercent = 100 * $cpuSeconds / $watch.Elapsed.TotalSeconds
    $cpuMachinePercent = $cpuOneCorePercent / [Environment]::ProcessorCount
    $privateMiB = $sample.PrivateMemorySize64 / 1MB
    'pid={0} interval_s={1:N2} cpu_s={2:N3} cpu_one_core_pct={3:N2} cpu_machine_pct={4:N2} private_mib={5:N2} processors={6}' -f $process.Id, $watch.Elapsed.TotalSeconds, $cpuSeconds, $cpuOneCorePercent, $cpuMachinePercent, $privateMiB, [Environment]::ProcessorCount
    $topThreads = @($sample.Threads | ForEach-Object {
        try {
            if ($threadStarts.ContainsKey($_.Id)) {
                [pscustomobject]@{ id = $_.Id; cpu_s = $_.TotalProcessorTime.TotalSeconds - $threadStarts[$_.Id]; started = $_.StartTime }
            }
        } catch { }
    } | Sort-Object cpu_s -Descending | Select-Object -First 4)
    foreach ($thread in $topThreads) { 'thread_id={0} cpu_s={1:N3} started={2:o}' -f $thread.id, $thread.cpu_s, $thread.started }
}
finally {
    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        $process.WaitForExit(5000) | Out-Null
    }
    $env:LOCALAPPDATA = $oldLocalAppData
    $env:CODEX_HOME = $oldCodexHome
}
