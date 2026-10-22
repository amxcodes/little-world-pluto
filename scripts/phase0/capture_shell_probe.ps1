#Requires -Version 5.1
param(
    [switch]$Capsule,
    [switch]$Light,
    [ValidateRange(80, 200)][int]$Scale = 100,
    [ValidateRange(0, 100)][int]$Transparency = 0,
    [string]$OutputPath
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$probePath = Join-Path $repoRoot 'target\debug\examples\windows_shell_probe.exe'
$captureRoot = Join-Path $repoRoot 'target\phase0-shell-review'

if (-not (Test-Path -LiteralPath $probePath)) {
    throw "Build the shell probe first: cargo build --example windows_shell_probe --features windows-shell-probe --locked. Missing: $probePath"
}

$existing = Get-Process -Name 'windows_shell_probe' -ErrorAction SilentlyContinue |
    Where-Object { -not $_.HasExited }
if ($existing) {
    throw "A shell probe window is already open (PID $($existing.Id)); close it before this capture."
}

Add-Type -AssemblyName System.Drawing
if (-not ('IslandPhase0ShellCapture' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Text;

public static class IslandPhase0ShellCapture {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
    [StructLayout(LayoutKind.Sequential)]
    private struct POINT { public int X; public int Y; }
    public delegate bool EnumWindowsCallback(IntPtr hwnd, IntPtr state);

    [DllImport("user32.dll", ExactSpelling = true)]
    public static extern bool SetProcessDpiAwarenessContext(IntPtr value);
    [DllImport("user32.dll", ExactSpelling = true)]
    public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
    [DllImport("user32.dll", ExactSpelling = true)]
    public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll", ExactSpelling = true)]
    public static extern uint GetDpiForWindow(IntPtr hwnd);
    [DllImport("user32.dll", EntryPoint = "GetWindowTextW", CharSet = CharSet.Unicode, ExactSpelling = true)]
    private static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int maxCount);
    [DllImport("user32.dll", ExactSpelling = true)]
    private static extern bool EnumWindows(EnumWindowsCallback callback, IntPtr state);
    [DllImport("user32.dll", ExactSpelling = true)]
    private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
    [DllImport("user32.dll", ExactSpelling = true)]
    private static extern IntPtr WindowFromPoint(POINT point);
    [DllImport("user32.dll", EntryPoint = "PostMessageW", ExactSpelling = true)]
    private static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);

    public static IntPtr FindVisibleWindow(uint targetProcessId) {
        IntPtr match = IntPtr.Zero;
        EnumWindows((hwnd, state) => {
            uint ownerProcessId;
            GetWindowThreadProcessId(hwnd, out ownerProcessId);
            if (ownerProcessId != targetProcessId || !IsWindowVisible(hwnd)) return true;
            var title = new StringBuilder(256);
            GetWindowText(hwnd, title, title.Capacity);
            if (!title.ToString().StartsWith("Island Windows shell probe", StringComparison.Ordinal)) return true;
            RECT rect;
            if (!GetWindowRect(hwnd, out rect)) return true;
            if (rect.Right <= rect.Left || rect.Bottom <= rect.Top) return true;
            match = hwnd;
            return false;
        }, IntPtr.Zero);
        return match;
    }

    public static bool CenterPixelBelongsToWindow(IntPtr hwnd, RECT rect) {
        var point = new POINT { X = (rect.Left + rect.Right) / 2, Y = (rect.Top + rect.Bottom) / 2 };
        return WindowFromPoint(point) == hwnd;
    }

    public static bool CloseWindow(IntPtr hwnd) {
        return PostMessage(hwnd, 0x0010, IntPtr.Zero, IntPtr.Zero);
    }
}
'@ -ReferencedAssemblies @('System.Drawing.dll')
}

[void][IslandPhase0ShellCapture]::SetProcessDpiAwarenessContext([IntPtr](-4))
[void](New-Item -ItemType Directory -Path $captureRoot -Force)

if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    $shape = if ($Capsule) { 'capsule' } else { 'notch' }
    $theme = if ($Light) { 'light' } else { 'dark' }
    $OutputPath = Join-Path $captureRoot "windows-shell-$shape-$Scale-$theme.png"
}
elseif (-not [System.IO.Path]::IsPathRooted($OutputPath)) {
    $OutputPath = Join-Path $repoRoot $OutputPath
}
$OutputPath = [System.IO.Path]::GetFullPath($OutputPath)
if (Test-Path -LiteralPath $OutputPath) {
    throw "Refusing to overwrite existing evidence: $OutputPath"
}
[void](New-Item -ItemType Directory -Path (Split-Path -Parent $OutputPath) -Force)

$arguments = @('--scale', "$Scale", '--transparency', "$Transparency")
if ($Capsule) { $arguments += '--capsule' }
if ($Light) { $arguments += '--light' }
$stdoutPath = [System.IO.Path]::ChangeExtension($OutputPath, '.stdout.txt')
$stderrPath = Join-Path $captureRoot ([System.IO.Path]::GetFileNameWithoutExtension($OutputPath) + '.stderr.txt')
$process = $null
$windowHandle = [IntPtr]::Zero

try {
    # This probe must show its no-activate topmost HWND to capture actual desktop pixels.
    $process = Start-Process -FilePath $probePath -ArgumentList $arguments -WindowStyle Normal -PassThru `
        -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        $process.Refresh()
        if ($process.HasExited) { throw "The shell probe exited with code $($process.ExitCode)." }
        $windowHandle = [IslandPhase0ShellCapture]::FindVisibleWindow([uint32]$process.Id)
        if ($windowHandle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)

    if ($windowHandle -eq [IntPtr]::Zero) {
        throw 'The titled Windows shell probe window did not appear within 15 seconds.'
    }

    $rect = New-Object IslandPhase0ShellCapture+RECT
    if (-not [IslandPhase0ShellCapture]::GetWindowRect($windowHandle, [ref]$rect)) {
        throw 'Could not read the shell probe window bounds.'
    }
    if (-not [IslandPhase0ShellCapture]::CenterPixelBelongsToWindow($windowHandle, $rect)) {
        throw 'The center pixel does not resolve to the shell probe; no screenshot was saved.'
    }

    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    $bitmap = [System.Drawing.Bitmap]::new($width, $height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
        $bitmap.Save($OutputPath, [System.Drawing.Imaging.ImageFormat]::Png)
    }
    finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
    Write-Output "Captured HWND $windowHandle, DPI $([IslandPhase0ShellCapture]::GetDpiForWindow($windowHandle)), bounds ($($rect.Left),$($rect.Top))-($($rect.Right),$($rect.Bottom)), image ${width}x${height}: $OutputPath"
}
finally {
    if ($process -and -not $process.HasExited) {
        if ($windowHandle -ne [IntPtr]::Zero) {
            try { [void][IslandPhase0ShellCapture]::CloseWindow($windowHandle) } catch { }
        }
        try { [void]$process.WaitForExit(3000) } catch { }
        if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue }
    }
}
