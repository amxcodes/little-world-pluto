#Requires -Version 5.1
param(
    [string]$ExecutablePath = (Join-Path $PSScriptRoot '..\..\target\debug\island-desktop.exe'),
    [int]$DurationMs = 1200
)

$ErrorActionPreference = 'Stop'
$ExecutablePath = [System.IO.Path]::GetFullPath($ExecutablePath)
if (-not (Test-Path -LiteralPath $ExecutablePath)) {
    throw "Build the product first. Executable not found: $ExecutablePath"
}
if (Get-Process -Name 'island-desktop' -ErrorAction SilentlyContinue) {
    throw 'An Island product process is already running; close it before this isolated frame check.'
}

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
using System.Diagnostics;

public static class IslandShellFrameProbe {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    public delegate bool EnumWindowsCallback(IntPtr hwnd, IntPtr state);
    [DllImport("user32.dll")]
    public static extern bool SetProcessDpiAwarenessContext(IntPtr value);
    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsCallback callback, IntPtr state);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowTextW(IntPtr hwnd, StringBuilder text, int maxCount);
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
    [DllImport("user32.dll", EntryPoint = "PostMessageW")]
    public static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);

    public static IntPtr FindShell(uint wantedProcessId) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((hwnd, state) => {
            uint processId;
            GetWindowThreadProcessId(hwnd, out processId);
            if (processId != wantedProcessId) return true;
            var title = new StringBuilder(256);
            GetWindowTextW(hwnd, title, title.Capacity);
            if (title.ToString() == "Island edge shell") {
                found = hwnd;
                return false;
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }
}
'@

[void][IslandShellFrameProbe]::SetProcessDpiAwarenessContext([IntPtr](-4))
$process = $null
$originalLocalAppData = $env:LOCALAPPDATA
$originalCodexHome = $env:CODEX_HOME
try {
    $probeLocalAppData = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\target\phase1-frame-probe-localappdata'))
    $probeCodexHome = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\target\phase1-frame-isolated-codex'))
    [void][System.IO.Directory]::CreateDirectory($probeLocalAppData)
    [void][System.IO.Directory]::CreateDirectory($probeCodexHome)
    $env:LOCALAPPDATA = $probeLocalAppData
    $env:CODEX_HOME = $probeCodexHome
    $process = Start-Process -FilePath $ExecutablePath -WindowStyle Hidden -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    $shell = [IntPtr]::Zero
    do {
        Start-Sleep -Milliseconds 100
        $shell = [IslandShellFrameProbe]::FindShell([uint32]$process.Id)
    } while ($shell -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if ($shell -eq [IntPtr]::Zero) { throw 'The product shell HWND did not appear.' }

    foreach ($direction in @('open', 'close')) {
        # A synthetic hotkey gives a deterministic immediate retarget without
        # relying on this automation session's blocked physical cursor input.
        [void][IslandShellFrameProbe]::PostMessage($shell, 0x0312, [IntPtr]0x1D51, [IntPtr]::Zero)

        $samples = New-Object 'System.Collections.Generic.List[object]'
        $watch = [System.Diagnostics.Stopwatch]::StartNew()
        $previous = ''
        while ($watch.ElapsedMilliseconds -lt $DurationMs) {
            $rect = New-Object IslandShellFrameProbe+RECT
            if ([IslandShellFrameProbe]::GetWindowRect($shell, [ref]$rect)) {
                $bounds = '{0}x{1}@{2},{3}' -f ($rect.Right - $rect.Left), ($rect.Bottom - $rect.Top), $rect.Left, $rect.Top
                if ($bounds -ne $previous) {
                    $samples.Add([pscustomobject]@{
                        ms = $watch.Elapsed.TotalMilliseconds
                        bounds = $bounds
                        width = $rect.Right - $rect.Left
                        height = $rect.Bottom - $rect.Top
                    })
                    $previous = $bounds
                }
            }
            # Avoid letting the diagnostic's tight polling loop steal time from
            # the shell thread whose animation cadence it is measuring.
            [void][System.Threading.Thread]::Yield()
        }
        $watch.Stop()
        $intervals = @()
        for ($i = 1; $i -lt $samples.Count; $i++) {
            $intervals += [pscustomobject]@{
                ms = [double]$samples[$i].ms - [double]$samples[$i - 1].ms
                at_ms = [double]$samples[$i].ms
                from_bounds = $samples[$i - 1].bounds
                bounds = $samples[$i].bounds
                size_delta_px = [math]::Max(
                    [math]::Abs($samples[$i].width - $samples[$i - 1].width),
                    [math]::Abs($samples[$i].height - $samples[$i - 1].height)
                )
            }
        }
        if ($intervals.Count -gt 0) {
            $mean = ($intervals | Measure-Object -Property ms -Average).Average
            $max = ($intervals | Measure-Object -Property ms -Maximum).Maximum
            $sortedIntervals = @($intervals | Sort-Object -Property ms)
            $p95 = $sortedIntervals[[math]::Max(0, [math]::Ceiling($sortedIntervals.Count * 0.95) - 1)].ms
            $over33 = @($intervals | Where-Object { $_.ms -gt 33 }).Count
            $visibleSteps = @($intervals | Where-Object { $_.size_delta_px -ge 2 })
            if ($visibleSteps.Count -gt 0) {
                $visibleMean = ($visibleSteps | Measure-Object -Property ms -Average).Average
                $visibleMax = ($visibleSteps | Measure-Object -Property ms -Maximum).Maximum
                $sortedVisible = @($visibleSteps | Sort-Object -Property ms)
                $visibleP95 = $sortedVisible[[math]::Max(0, [math]::Ceiling($sortedVisible.Count * 0.95) - 1)].ms
                $visibleOver33 = @($visibleSteps | Where-Object { $_.ms -gt 33 }).Count
                "direction=$direction changed_bounds=$($samples.Count) raw_mean_ms=$([math]::Round($mean, 2)) raw_p95_ms=$([math]::Round($p95, 2)) raw_max_ms=$([math]::Round($max, 2)) gaps_over_33ms=$over33 size_steps_ge_2px=$($visibleSteps.Count) step_mean_ms=$([math]::Round($visibleMean, 2)) step_p95_ms=$([math]::Round($visibleP95, 2)) step_max_ms=$([math]::Round($visibleMax, 2)) step_gaps_over_33ms=$visibleOver33"
            } else {
                "direction=$direction changed_bounds=$($samples.Count) raw_mean_ms=$([math]::Round($mean, 2)) raw_p95_ms=$([math]::Round($p95, 2)) raw_max_ms=$([math]::Round($max, 2)) gaps_over_33ms=$over33 no_size_steps_ge_2px"
            }
            $samples | Select-Object -First 5 | ForEach-Object { "  t=$([math]::Round($_.ms, 1))ms $($_.bounds)" }
            $intervals | Where-Object { $_.ms -gt 33 } | ForEach-Object {
                "  slow_gap_ms=$([math]::Round($_.ms, 2)) at_t=$([math]::Round($_.at_ms, 1))ms from=$($_.from_bounds) to=$($_.bounds)"
            }
        } else {
            "direction=$direction changed_bounds=$($samples.Count) no_motion_observed"
        }
    }
}
finally {
    $env:LOCALAPPDATA = $originalLocalAppData
    $env:CODEX_HOME = $originalCodexHome
    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        [void]$process.WaitForExit(3000)
    }
}
