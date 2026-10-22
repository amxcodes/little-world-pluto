#Requires -Version 5.1
param(
    [string]$ExecutablePath = (Join-Path $PSScriptRoot '..\..\target\release\island-desktop.exe'),
    [int]$TransitionTimeoutMs = 4000
)

$ErrorActionPreference = 'Stop'
$ExecutablePath = [System.IO.Path]::GetFullPath($ExecutablePath)
if (-not (Test-Path -LiteralPath $ExecutablePath)) {
    throw "Build the product first. Executable not found: $ExecutablePath"
}
if (Get-Process -Name 'island-desktop' -ErrorAction SilentlyContinue) {
    throw 'An Island product process is already running; close it before this isolated hover check.'
}

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class IslandProductHoverProbe {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)]
    public struct POINT { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)]
    public struct MOUSEINPUT {
        public int dx, dy;
        public uint mouseData, dwFlags, time;
        public IntPtr dwExtraInfo;
    }
    [StructLayout(LayoutKind.Sequential)]
    public struct INPUT { public uint type; public MOUSEINPUT mi; }
    public delegate bool EnumWindowsCallback(IntPtr hwnd, IntPtr state);

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsCallback callback, IntPtr state);
    [DllImport("user32.dll")]
    public static extern bool SetProcessDpiAwarenessContext(IntPtr value);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)]
    public static extern int GetClassNameW(IntPtr hwnd, StringBuilder text, int maxCount);
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr hwnd, int command);
    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
    [DllImport("user32.dll")]
    public static extern IntPtr WindowFromPhysicalPoint(POINT point);
    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")]
    public static extern bool GetCursorPos(out POINT point);
    [DllImport("user32.dll")]
    public static extern int GetSystemMetrics(int index);
    [DllImport("user32.dll", SetLastError=true)]
    public static extern uint SendInput(uint count, INPUT[] inputs, int size);

    public static IntPtr FindWindowByClass(uint wantedProcessId, string exactClass) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((hwnd, state) => {
            uint processId;
            GetWindowThreadProcessId(hwnd, out processId);
            if (processId != wantedProcessId) return true;
            var className = new StringBuilder(256);
            GetClassNameW(hwnd, className, className.Capacity);
            if (className.ToString() == exactClass) {
                found = hwnd;
                return false;
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }

    public static uint MovePointerWithSendInput(int x, int y) {
        int left = GetSystemMetrics(76), top = GetSystemMetrics(77);
        int width = GetSystemMetrics(78), height = GetSystemMetrics(79);
        if (width < 2 || height < 2) return 0;
        INPUT input = new INPUT();
        input.type = 0;
        input.mi.dx = (int)Math.Round((x - left) * 65535.0 / (width - 1));
        input.mi.dy = (int)Math.Round((y - top) * 65535.0 / (height - 1));
        input.mi.dwFlags = 0x0001 | 0x8000 | 0x4000; // MOVE | ABSOLUTE | VIRTUALDESK
        return SendInput(1, new INPUT[] { input }, Marshal.SizeOf(typeof(INPUT)));
    }
}
'@

$process = $null
$probeProcess = [System.Diagnostics.Process]::GetCurrentProcess()
$originalPriority = $probeProcess.PriorityClass
[void][IslandProductHoverProbe]::SetProcessDpiAwarenessContext([IntPtr](-4))
$originalCursor = New-Object IslandProductHoverProbe+POINT
$haveOriginalCursor = [IslandProductHoverProbe]::GetCursorPos([ref]$originalCursor)
$originalLocalAppData = $env:LOCALAPPDATA
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$targetRoot = [System.IO.Path]::GetFullPath((Join-Path $repoRoot 'target'))
$targetPrefix = $targetRoot.TrimEnd('\') + '\'
$profileRoot = Join-Path $targetRoot ("phase1-hover-probe-" + [Guid]::NewGuid().ToString('N'))
$probeLocalAppData = Join-Path $profileRoot 'LocalAppData'
try {
    if (-not $haveOriginalCursor) { throw 'Could not save the current pointer position.' }
    # Keep the polling helper below the product's normal priority so measuring
    # HWND bounds does not steal time from the shell animation thread.
    $probeProcess.PriorityClass = [System.Diagnostics.ProcessPriorityClass]::BelowNormal
    [void][System.IO.Directory]::CreateDirectory($probeLocalAppData)
    $env:LOCALAPPDATA = $probeLocalAppData
    $process = Start-Process -FilePath $ExecutablePath -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    $shell = [IntPtr]::Zero
    $desk = [IntPtr]::Zero
    do {
        Start-Sleep -Milliseconds 100
        $shell = [IslandProductHoverProbe]::FindWindowByClass([uint32]$process.Id, 'IslandDesktopEdgeShell')
        $desk = [IslandProductHoverProbe]::FindWindowByClass([uint32]$process.Id, 'Window Class')
    } while (($shell -eq [IntPtr]::Zero -or $desk -eq [IntPtr]::Zero) -and [DateTime]::UtcNow -lt $deadline)
    if ($shell -eq [IntPtr]::Zero) { throw 'The product shell HWND did not appear.' }
    if ($desk -eq [IntPtr]::Zero) { throw 'The Agent Desk HWND did not appear.' }

    # The tray may be unavailable in an automation desktop. Keep the host
    # visible at startup, then hide it only for this controlled hover check.
    if ([IslandProductHoverProbe]::IsWindowVisible($desk)) {
        [void][IslandProductHoverProbe]::ShowWindow($desk, 0)
        Start-Sleep -Milliseconds 150
    }
    if ([IslandProductHoverProbe]::IsWindowVisible($desk)) {
        throw 'Could not hide the Agent Desk before testing shell hover.'
    }

    $folded = New-Object IslandProductHoverProbe+RECT
    if (-not [IslandProductHoverProbe]::GetWindowRect($shell, [ref]$folded)) {
        throw 'Could not read the folded shell bounds.'
    }
    $foldedWidth = $folded.Right - $folded.Left
    $foldedHeight = $folded.Bottom - $folded.Top
    # Notch bounds use 221.6 logical folded pixels and 680 expanded pixels.
    # Scale the acceptance threshold from the live folded HWND, so the probe
    # waits for the whole spring at 100%, 125%, and 150% display scale.
    $expandedTargetWidth = [int][Math]::Floor($foldedWidth * (680.0 / 221.6))
    $expandedTargetHeight = [int][Math]::Floor($foldedHeight * (339.6 / 33.6))
    $hoverPoint = New-Object IslandProductHoverProbe+POINT
    $hoverPoint.X = [int](($folded.Left + $folded.Right) / 2)
    $hoverPoint.Y = [int](($folded.Top + $folded.Bottom) / 2)
    if ([IslandProductHoverProbe]::WindowFromPhysicalPoint($hoverPoint) -ne $shell) {
        throw 'The selected hover point is not an opaque shell pixel.'
    }

    $foregroundBefore = [IslandProductHoverProbe]::GetForegroundWindow()
    $sent = [IslandProductHoverProbe]::MovePointerWithSendInput($hoverPoint.X, $hoverPoint.Y)
    if ($sent -ne 1) {
        "BLOCKED: Windows did not accept the SendInput pointer move to the shell; inserted $sent event(s). The app and original cursor position will be restored."
        return
    }
    $cursorAtHover = New-Object IslandProductHoverProbe+POINT
    [void][IslandProductHoverProbe]::GetCursorPos([ref]$cursorAtHover)
    if ([math]::Abs($cursorAtHover.X - $hoverPoint.X) -gt 1 -or
        [math]::Abs($cursorAtHover.Y - $hoverPoint.Y) -gt 1) {
        throw "The pointer did not reach the opaque shell point; actual=$($cursorAtHover.X),$($cursorAtHover.Y)."
    }

    $openWatch = [System.Diagnostics.Stopwatch]::StartNew()
    $openSamples = New-Object 'System.Collections.Generic.List[object]'
    $openStartedAt = $null
    $previousOpenBounds = ''
    $expanded = New-Object IslandProductHoverProbe+RECT
    do {
        [void][IslandProductHoverProbe]::GetWindowRect($shell, [ref]$expanded)
        $width = $expanded.Right - $expanded.Left
        if ($width -gt ($foldedWidth + 1) -and $null -eq $openStartedAt) {
            $openStartedAt = $openWatch.Elapsed.TotalMilliseconds
        }
        if ($null -ne $openStartedAt) {
            $bounds = '{0}x{1}' -f $width, ($expanded.Bottom - $expanded.Top)
            if ($bounds -ne $previousOpenBounds) {
                $openSamples.Add([pscustomobject]@{
                    ms = $openWatch.Elapsed.TotalMilliseconds - $openStartedAt
                    width = $width
                    bounds = $bounds
                })
                $previousOpenBounds = $bounds
            }
        }
        $height = $expanded.Bottom - $expanded.Top
        if ($width -ge $expandedTargetWidth -and $height -ge $expandedTargetHeight) { break }
        [void][System.Threading.Thread]::Yield()
    } while ($openWatch.ElapsedMilliseconds -lt $TransitionTimeoutMs)
    $openWatch.Stop()
    $expandedWidth = $expanded.Right - $expanded.Left
    $expandedHeight = $expanded.Bottom - $expanded.Top
    if ($expandedWidth -lt $expandedTargetWidth -or $expandedHeight -lt $expandedTargetHeight) {
        throw "Real pointer hover did not expand the shell within ${TransitionTimeoutMs}ms; final=${expandedWidth}x${expandedHeight}."
    }
    if ([IslandProductHoverProbe]::IsWindowVisible($desk)) {
        throw 'Hover unexpectedly opened the Agent Desk.'
    }
    $foregroundAfterHover = [IslandProductHoverProbe]::GetForegroundWindow()
    if ($foregroundAfterHover -ne $foregroundBefore) {
        throw 'Hover changed the foreground window; no-activate behavior regressed.'
    }

    $metrics = @()
    for ($index = 1; $index -lt $openSamples.Count; $index++) {
        $delta = [math]::Abs($openSamples[$index].width - $openSamples[$index - 1].width)
        if ($delta -ge 2) {
            $metrics += [pscustomobject]@{
                ms = [double]$openSamples[$index].ms - [double]$openSamples[$index - 1].ms
                delta = $delta
            }
        }
    }
    $stepMean = if ($metrics.Count) {
        [math]::Round(($metrics | Measure-Object -Property ms -Average).Average, 2)
    } else { 0 }
    $stepP95 = if ($metrics.Count) {
        $sortedMetrics = @($metrics | Sort-Object -Property ms)
        [math]::Round($sortedMetrics[[math]::Max(0, [math]::Ceiling($sortedMetrics.Count * 0.95) - 1)].ms, 2)
    } else { 0 }
    $stepMax = if ($metrics.Count) {
        [math]::Round(($metrics | Measure-Object -Property ms -Maximum).Maximum, 2)
    } else { 0 }
    $stepGaps = @($metrics | Where-Object { $_.ms -gt 33 }).Count

    $virtualLeft = [IslandProductHoverProbe]::GetSystemMetrics(76)
    $virtualTop = [IslandProductHoverProbe]::GetSystemMetrics(77)
    $virtualWidth = [IslandProductHoverProbe]::GetSystemMetrics(78)
    $virtualHeight = [IslandProductHoverProbe]::GetSystemMetrics(79)
    $outsideX = $virtualLeft + 20
    $outsideY = $virtualTop + $virtualHeight - 20
    if ($outsideX -ge $expanded.Left -and $outsideX -le $expanded.Right -and
        $outsideY -ge $expanded.Top -and $outsideY -le $expanded.Bottom) {
        $outsideX = $virtualLeft + $virtualWidth - 20
    }
    $sent = [IslandProductHoverProbe]::MovePointerWithSendInput($outsideX, $outsideY)
    if ($sent -ne 1) { throw "SendInput could not move the pointer away; inserted $sent event(s)." }
    $closeWatch = [System.Diagnostics.Stopwatch]::StartNew()
    $foldedAgain = New-Object IslandProductHoverProbe+RECT
    do {
        [void][IslandProductHoverProbe]::GetWindowRect($shell, [ref]$foldedAgain)
        $width = $foldedAgain.Right - $foldedAgain.Left
        if ($width -le ($foldedWidth + 2)) { break }
        [void][System.Threading.Thread]::Yield()
    } while ($closeWatch.ElapsedMilliseconds -lt $TransitionTimeoutMs)
    $closeWatch.Stop()
    $finalWidth = $foldedAgain.Right - $foldedAgain.Left
    if ($finalWidth -gt ($foldedWidth + 2)) {
        throw "The shell did not fold after pointer leave within ${TransitionTimeoutMs}ms; final width=$finalWidth."
    }
    if ([IslandProductHoverProbe]::GetForegroundWindow() -ne $foregroundBefore) {
        throw 'Pointer leave changed the foreground window.'
    }

    "PASS hover_point=$($hoverPoint.X),$($hoverPoint.Y) folded=${foldedWidth}x${foldedHeight} expanded=${expandedWidth}x${expandedHeight} open_steps_ge_2px=$($metrics.Count) open_step_mean_ms=$stepMean open_step_p95_ms=$stepP95 open_step_max_ms=$stepMax open_step_gaps_over_33ms=$stepGaps close_ms=$([math]::Round($closeWatch.Elapsed.TotalMilliseconds, 1)) noactivate=pass desk_hidden=pass foreground_stable=pass"
    'Evidence boundary: SendInput-generated Windows pointer events exercise the product HWND. This is not a human-driven visual review or cross-display/DPI acceptance.'
}
finally {
    $probeProcess.PriorityClass = $originalPriority
    $env:LOCALAPPDATA = $originalLocalAppData
    if ($haveOriginalCursor) {
        [void][IslandProductHoverProbe]::MovePointerWithSendInput($originalCursor.X, $originalCursor.Y)
    }
    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        [void]$process.WaitForExit(3000)
    }
    if (Test-Path -LiteralPath $profileRoot) {
        $resolvedProfileRoot = [System.IO.Path]::GetFullPath($profileRoot)
        if ($resolvedProfileRoot.StartsWith($targetPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
            Remove-Item -LiteralPath $resolvedProfileRoot -Recurse -Force
        }
    }
}
