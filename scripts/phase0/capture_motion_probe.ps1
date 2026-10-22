#Requires -Version 5.1
param(
    [switch]$Glow,
    [switch]$Edge,
    [switch]$Light,
    [switch]$Capsule
)

$ErrorActionPreference = 'Stop'

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$probePath = Join-Path $repoRoot 'target\debug\examples\native_motion_probe.exe'
$renderer = if ($Glow) { 'glow' } else { 'wgpu' }
$mode = if ($Edge) { 'edge' } else { 'window' }
$theme = if ($Light) { 'light' } else { 'dark' }
$shape = if ($Capsule) { 'capsule' } else { 'notch' }
$outputRoot = Join-Path $repoRoot "target\phase0-motion-review-$renderer-$mode-$theme-$shape"
$stdoutPath = Join-Path $outputRoot 'probe.stdout.txt'
$stderrPath = Join-Path $outputRoot 'probe.stderr.txt'

if (-not (Test-Path -LiteralPath $probePath)) {
    throw "Build the Windows UI host first: cargo run --example native_motion_probe --features ui-host-probe --locked. Missing: $probePath"
}

$existing = Get-Process -Name 'native_motion_probe' -ErrorAction SilentlyContinue |
    Where-Object { -not $_.HasExited }
if ($existing) {
    throw "A motion probe window is already open (PID $($existing.Id)); close it before this capture."
}

Add-Type -AssemblyName System.Drawing
$drawingAssemblyPath = [System.Drawing.Graphics].Assembly.Location
$captureReferences = @($drawingAssemblyPath)
foreach ($assemblyName in @(
    'System.Drawing.Primitives.dll',
    'System.Private.Windows.GdiPlus.dll',
    'System.Private.Windows.Core.dll'
)) {
    $assemblyPath = Join-Path (Split-Path $drawingAssemblyPath) $assemblyName
    if (Test-Path -LiteralPath $assemblyPath) {
        $captureReferences += $assemblyPath
    }
}
Add-Type -TypeDefinition @'
using System;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Text;

public static class IslandPhase0WindowCapture {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
    [StructLayout(LayoutKind.Sequential)]
    public struct POINT { public int X; public int Y; }
    public delegate bool EnumWindowsCallback(IntPtr hwnd, IntPtr state);

    [DllImport("user32.dll", ExactSpelling = true)]
    public static extern bool SetProcessDpiAwarenessContext(IntPtr value);
    [DllImport("user32.dll", ExactSpelling = true)]
    private static extern IntPtr GetDC(IntPtr hwnd);
    [DllImport("user32.dll", ExactSpelling = true)]
    private static extern int ReleaseDC(IntPtr hwnd, IntPtr hdc);
    [DllImport("gdi32.dll", ExactSpelling = true)]
    private static extern bool BitBlt(IntPtr destination, int x, int y, int width, int height, IntPtr source, int sourceX, int sourceY, uint operation);
    [DllImport("user32.dll", ExactSpelling = true)]
    private static extern bool PrintWindow(IntPtr hwnd, IntPtr destination, uint flags);
    [DllImport("user32.dll", ExactSpelling = true)]
    public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll", ExactSpelling = true)]
    public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll", ExactSpelling = true)]
    public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
    [DllImport("user32.dll", ExactSpelling = true)]
    public static extern IntPtr WindowFromPhysicalPoint(POINT point);
    [DllImport("user32.dll", ExactSpelling = true)]
    private static extern IntPtr GetAncestor(IntPtr hwnd, uint flags);
    [DllImport("user32.dll", ExactSpelling = true)]
    private static extern int GetWindowRgn(IntPtr hwnd, IntPtr region);
    [DllImport("gdi32.dll", ExactSpelling = true)]
    private static extern IntPtr CreateRectRgn(int left, int top, int right, int bottom);
    [DllImport("gdi32.dll", ExactSpelling = true)]
    private static extern bool DeleteObject(IntPtr handle);
    [DllImport("user32.dll", ExactSpelling = true)]
    public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW", ExactSpelling = true)]
    private static extern IntPtr GetWindowLongPtr(IntPtr hwnd, int index);
    [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW", ExactSpelling = true)]
    private static extern IntPtr SetWindowLongPtr(IntPtr hwnd, int index, IntPtr value);
    [DllImport("gdi32.dll", ExactSpelling = true)]
    private static extern bool EqualRgn(IntPtr first, IntPtr second);
    [DllImport("user32.dll", EntryPoint = "GetWindowTextW", CharSet = CharSet.Unicode, ExactSpelling = true)]
    private static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int maxCount);
    [DllImport("user32.dll", ExactSpelling = true)]
    public static extern int GetSystemMetrics(int index);
    [DllImport("user32.dll", ExactSpelling = true)]
    private static extern bool EnumWindows(EnumWindowsCallback callback, IntPtr state);
    [DllImport("user32.dll", ExactSpelling = true)]
    private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
    [DllImport("user32.dll", EntryPoint = "PostMessageW", ExactSpelling = true)]
    private static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);
    [DllImport("kernel32.dll", ExactSpelling = true)]
    private static extern void Sleep(uint milliseconds);
    [DllImport("user32.dll", ExactSpelling = true)]
    private static extern bool SetWindowPos(IntPtr hwnd, IntPtr insertAfter, int x, int y, int width, int height, uint flags);

    public static IntPtr FindVisibleWindow(uint targetProcessId) {
        IntPtr match = IntPtr.Zero;
        long largestArea = 0;
        EnumWindows((hwnd, state) => {
            uint ownerProcessId;
            GetWindowThreadProcessId(hwnd, out ownerProcessId);
            if (ownerProcessId != targetProcessId || !IsWindowVisible(hwnd)) return true;

            var title = new StringBuilder(256);
            GetWindowText(hwnd, title, title.Capacity);
            if (!title.ToString().StartsWith("Island Rust UI Host", StringComparison.OrdinalIgnoreCase)) return true;

            RECT rect;
            if (!GetWindowRect(hwnd, out rect)) return true;
            var width = rect.Right - rect.Left;
            var height = rect.Bottom - rect.Top;
            if (width < 500 || height < 400) return true;

            var area = (long)width * height;
            if (area > largestArea) {
                match = hwnd;
                largestArea = area;
            }
            return true;
        }, IntPtr.Zero);
        return match;
    }

    public static long GetExtendedStyle(IntPtr hwnd) {
        return GetWindowLongPtr(hwnd, -20).ToInt64();
    }

    public static bool EnsureEdgePassthroughStyles(IntPtr hwnd) {
        var current = GetWindowLongPtr(hwnd, -20).ToInt64();
        var requested = current | 0x08000000L | 0x00000020L;
        SetWindowLongPtr(hwnd, -20, new IntPtr(requested));
        return GetExtendedStyle(hwnd) == requested;
    }

    public static IntPtr TopLevelAt(POINT point) {
        var hit = WindowFromPhysicalPoint(point);
        return hit == IntPtr.Zero ? IntPtr.Zero : GetAncestor(hit, 2);
    }

    public static bool HasClippingNativeRegion(IntPtr hwnd) {
        var actual = CreateRectRgn(0, 0, 0, 0);
        if (actual == IntPtr.Zero) return true;
        IntPtr bounds = IntPtr.Zero;
        try {
            if (GetWindowRgn(hwnd, actual) == 0) return false;
            RECT rect;
            if (!GetWindowRect(hwnd, out rect)) return true;
            bounds = CreateRectRgn(0, 0, rect.Right - rect.Left, rect.Bottom - rect.Top);
            return bounds == IntPtr.Zero || !EqualRgn(actual, bounds);
        }
        finally {
            DeleteObject(actual);
            if (bounds != IntPtr.Zero) DeleteObject(bounds);
        }
    }

    public static bool RaiseWithoutActivation(IntPtr hwnd) {
        return SetWindowPos(hwnd, new IntPtr(-1), 0, 0, 0, 0, 0x0001 | 0x0002 | 0x0010 | 0x0040);
    }

    public static bool CaptureDesktopWithLayeredWindows(Graphics destination, int left, int top, int width, int height) {
        var source = GetDC(IntPtr.Zero);
        if (source == IntPtr.Zero) return false;
        var target = destination.GetHdc();
        try {
            return BitBlt(target, 0, 0, width, height, source, left, top, 0x00CC0020u | 0x40000000u);
        }
        finally {
            destination.ReleaseHdc(target);
            ReleaseDC(IntPtr.Zero, source);
        }
    }

    public static bool CaptureWindowSurface(Graphics destination, IntPtr hwnd) {
        var target = destination.GetHdc();
        try {
            return PrintWindow(hwnd, target, 0x00000002);
        }
        finally {
            destination.ReleaseHdc(target);
        }
    }

    public static bool CloseWindow(IntPtr hwnd) {
        return PostMessage(hwnd, 0x0010, IntPtr.Zero, IntPtr.Zero);
    }

    public static bool PressSpace(IntPtr hwnd) {
        var down = PostMessage(hwnd, 0x0100, new IntPtr(0x20), new IntPtr(1));
        Sleep(60);
        var up = PostMessage(hwnd, 0x0101, new IntPtr(0x20), new IntPtr(unchecked((long)0xC0000001)));
        return down && up;
    }

    public static int CountDifferentPixels(string firstPath, string secondPath) {
        using (var first = new Bitmap(firstPath))
        using (var second = new Bitmap(secondPath)) {
            if (first.Width != second.Width || first.Height != second.Height) return -1;
            var changed = 0;
            for (var y = 0; y < first.Height; y += 4) {
                for (var x = 0; x < first.Width; x += 4) {
                    var a = first.GetPixel(x, y);
                    var b = second.GetPixel(x, y);
                    var delta = Math.Abs(a.R - b.R) + Math.Abs(a.G - b.G) + Math.Abs(a.B - b.B);
                    if (delta > 60) changed++;
                }
            }
            return changed;
        }
    }

    public static int CountNonBlackPixels(string path) {
        using (var bitmap = new Bitmap(path)) {
            var count = 0;
            for (var y = 0; y < bitmap.Height; y += 2) {
                for (var x = 0; x < bitmap.Width; x += 2) {
                    var pixel = bitmap.GetPixel(x, y);
                    if (pixel.R > 8 || pixel.G > 8 || pixel.B > 8) count++;
                }
            }
            return count;
        }
    }

    public static int CountBrightPixels(string path) {
        using (var bitmap = new Bitmap(path)) {
            var count = 0;
            for (var y = 0; y < bitmap.Height; y += 2) {
                for (var x = 0; x < bitmap.Width; x += 2) {
                    var pixel = bitmap.GetPixel(x, y);
                    if (pixel.R > 220 && pixel.G > 220 && pixel.B > 220) count++;
                }
            }
            return count;
        }
    }
}
'@ -ReferencedAssemblies $captureReferences

[void][IslandPhase0WindowCapture]::SetProcessDpiAwarenessContext([IntPtr](-4))
[void](New-Item -ItemType Directory -Path $outputRoot -Force)
foreach ($name in @('folded.png', 'open-80ms.png', 'open-160ms.png', 'open-320ms.png', 'open-600ms.png', 'close-80ms.png', 'close-160ms.png', 'close-320ms.png', 'close-600ms.png')) {
    Remove-Item -LiteralPath (Join-Path $outputRoot $name) -Force -ErrorAction SilentlyContinue
}
$process = $null
$windowHandle = [IntPtr]::Zero

function Save-ProbeWindow([string]$name, [IntPtr]$handle) {
    $process.Refresh()
    $rect = New-Object IslandPhase0WindowCapture+RECT
    if ($handle -eq [IntPtr]::Zero -or -not [IslandPhase0WindowCapture]::GetWindowRect($handle, [ref]$rect)) {
        throw 'Could not read the motion probe window bounds.'
    }

    $screenWidth = [IslandPhase0WindowCapture]::GetSystemMetrics(0)
    $screenHeight = [IslandPhase0WindowCapture]::GetSystemMetrics(1)
    $left = [Math]::Max(0, $rect.Left)
    $top = [Math]::Max(0, $rect.Top)
    $right = [Math]::Min($screenWidth, $rect.Right)
    $bottom = [Math]::Min($screenHeight, $rect.Bottom)
    if ($right -le $left -or $bottom -le $top) {
        throw "The probe window is outside the primary display: $($rect.Left),$($rect.Top)-$($rect.Right),$($rect.Bottom)."
    }

    $bitmap = [System.Drawing.Bitmap]::new($right - $left, $bottom - $top)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        # Include layered HWNDs using SRCCOPY | CAPTUREBLT; Graphics.CopyFromScreen
        # rejects this combined raster operation because CopyPixelOperation is an enum.
        if (-not [IslandPhase0WindowCapture]::CaptureDesktopWithLayeredWindows($graphics, $left, $top, $bitmap.Width, $bitmap.Height)) {
            throw 'BitBlt failed to capture the desktop including layered windows.'
        }
        $path = Join-Path $outputRoot $name
        $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
        Write-Output "$path ($($bitmap.Width)x$($bitmap.Height), bounds=$($rect.Left),$($rect.Top)-$($rect.Right),$($rect.Bottom))"
        $windowBitmap = [System.Drawing.Bitmap]::new($rect.Right - $rect.Left, $rect.Bottom - $rect.Top)
            $windowGraphics = [System.Drawing.Graphics]::FromImage($windowBitmap)
            try {
                if ([IslandPhase0WindowCapture]::CaptureWindowSurface($windowGraphics, $handle)) {
                    $windowPath = Join-Path $outputRoot "hwnd-$name"
                    $windowBitmap.Save($windowPath, [System.Drawing.Imaging.ImageFormat]::Png)
                    Write-Output "$windowPath (PrintWindow HWND client capture)"
                } else {
                    Write-Output "PrintWindow returned false for $name."
                }
            }
            finally {
                $windowGraphics.Dispose()
                $windowBitmap.Dispose()
            }
    }
    finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

function Assert-EdgePassThrough([string]$state) {
    $rect = New-Object IslandPhase0WindowCapture+RECT
    if (-not [IslandPhase0WindowCapture]::GetWindowRect($windowHandle, [ref]$rect)) {
        throw "Could not read the edge HWND bounds during $state hit-map verification."
    }
    $scale = ($rect.Right - $rect.Left) / 760.0
    $centerX = [int][Math]::Round(($rect.Left + $rect.Right) / 2.0)
    $bodyTop = if ($Capsule) { 8.0 } else { 0.0 }
    $visiblePoint = New-Object IslandPhase0WindowCapture+POINT
    $visiblePoint.X = $centerX
    $visiblePoint.Y = $rect.Top + [int][Math]::Round(($bodyTop + 3.0) * $scale)
    $bodyHit = [IslandPhase0WindowCapture]::TopLevelAt($visiblePoint)
    if ($bodyHit -eq $windowHandle) { throw "Visible $shape body intercepted input during $state." }

    $blankPoint = New-Object IslandPhase0WindowCapture+POINT
    $blankPoint.X = $rect.Left + [int][Math]::Round(20.0 * $scale)
    $blankPoint.Y = $rect.Top + [int][Math]::Round(200.0 * $scale)
    $blankHit = [IslandPhase0WindowCapture]::TopLevelAt($blankPoint)
    if ($blankHit -eq $windowHandle) { throw "Blank host point intercepted input during $state." }

    $cutoutPoint = New-Object IslandPhase0WindowCapture+POINT
    if ($Capsule) {
        $cutoutPoint.X = $centerX
        $cutoutPoint.Y = $rect.Top + [int][Math]::Round(4.0 * $scale)
    } else {
        $cutoutPoint.X = $rect.Left + [int][Math]::Round(1.0 * $scale)
        $cutoutPoint.Y = $rect.Top + [int][Math]::Round(1.0 * $scale)
    }
    $cutoutHit = [IslandPhase0WindowCapture]::TopLevelAt($cutoutPoint)
    if ($cutoutHit -eq $windowHandle) { throw "$shape cutout intercepted input during $state." }
    Write-Output ("{0} pass-through: body -> desktop 0x{1:X}; blank host -> desktop 0x{2:X}; cutout -> desktop 0x{3:X}" -f $state, $bodyHit.ToInt64(), $blankHit.ToInt64(), $cutoutHit.ToInt64())
}

try {
    $arguments = @()
    if ($Glow) { $arguments += '--glow' }
    if ($Edge) { $arguments += '--edge' }
    if ($Light) { $arguments += '--light' }
    if ($Capsule) { $arguments += '--capsule' }
    if ($arguments.Count -gt 0) {
        $windowStyle = if ($Edge) { 'Hidden' } else { 'Normal' }
        if ($Edge) {
            Remove-Item -LiteralPath $stdoutPath,$stderrPath -Force -ErrorAction SilentlyContinue
            $process = Start-Process -FilePath $probePath -ArgumentList $arguments -WindowStyle $windowStyle -PassThru -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
        } else {
            $process = Start-Process -FilePath $probePath -ArgumentList $arguments -WindowStyle $windowStyle -PassThru
        }
    } else {
        $process = Start-Process -FilePath $probePath -PassThru
    }
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        $process.Refresh()
        if ($process.HasExited) { throw "The motion probe exited with code $($process.ExitCode)." }
        $windowHandle = [IslandPhase0WindowCapture]::FindVisibleWindow([uint32]$process.Id)
        if ($windowHandle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)

    if ($windowHandle -eq [IntPtr]::Zero) {
        throw 'The titled eframe motion probe window did not appear within 20 seconds.'
    }
    if ($Edge) {
        $passthroughReady = $false
        $passthroughDeadline = [DateTime]::UtcNow.AddSeconds(30)
        do {
            $process.Refresh()
            if ($process.HasExited) { throw "The motion probe exited before native input setup (code $($process.ExitCode))." }
            if (Test-Path -LiteralPath $stdoutPath) {
                $passthroughReady = (Get-Content -LiteralPath $stdoutPath -Raw) -match 'Edge renderer remains fully click-through'
            }
            if (-not $passthroughReady) { Start-Sleep -Milliseconds 100 }
        } while (-not $passthroughReady -and [DateTime]::UtcNow -lt $passthroughDeadline)
        if (-not $passthroughReady) {
            throw 'The motion probe did not confirm transparent whole-window input pass-through within 30 seconds.'
        }
        Write-Output ("Edge HWND=0x{0:X}" -f $windowHandle.ToInt64())
        if (-not [IslandPhase0WindowCapture]::RaiseWithoutActivation($windowHandle)) {
            throw 'Could not place the edge diagnostic above the desktop without activation.'
        }
        if (-not [IslandPhase0WindowCapture]::EnsureEdgePassthroughStyles($windowHandle)) {
            throw 'Could not retain no-activate and click-through styles after showing the renderer.'
        }
        $extendedStyle = [IslandPhase0WindowCapture]::GetExtendedStyle($windowHandle)
        $hasClippingRegion = [IslandPhase0WindowCapture]::HasClippingNativeRegion($windowHandle)
        Write-Output ("Edge HWND exstyle=0x{0:X}; layered={1}; no-activate={2}; whole-window-passthrough={3}; topmost={4}; shape-clipping-region={5}" -f $extendedStyle, (($extendedStyle -band 0x00080000) -ne 0), (($extendedStyle -band 0x08000000) -ne 0), (($extendedStyle -band 0x00000020) -ne 0), (($extendedStyle -band 0x00000008) -ne 0), $hasClippingRegion)
        if (($extendedStyle -band 0x00080000) -eq 0 -or ($extendedStyle -band 0x08000000) -eq 0 -or ($extendedStyle -band 0x00000020) -eq 0 -or $hasClippingRegion) {
            Save-ProbeWindow 'clipping-debug.png' $windowHandle
            throw 'The visual renderer must remain layered, no-activate, whole-window click-through, and free of a clipping native region.'
        }
    }
    Start-Sleep -Milliseconds 1000

    Save-ProbeWindow 'folded.png' $windowHandle
    if ($Edge) {
        $foldedHwndPath = Join-Path $outputRoot 'hwnd-folded.png'
        $visibleSamples = if ($Light) {
            [IslandPhase0WindowCapture]::CountBrightPixels($foldedHwndPath)
        } else {
            [IslandPhase0WindowCapture]::CountNonBlackPixels($foldedHwndPath)
        }
        $minimumVisibleSamples = if ($Light) { 1000 } else { 10 }
        for ($attempt = 0; $visibleSamples -lt $minimumVisibleSamples -and $attempt -lt 15; $attempt++) {
            Start-Sleep -Milliseconds 75
            Save-ProbeWindow 'folded.png' $windowHandle
            $visibleSamples = if ($Light) {
                [IslandPhase0WindowCapture]::CountBrightPixels($foldedHwndPath)
            } else {
                [IslandPhase0WindowCapture]::CountNonBlackPixels($foldedHwndPath)
            }
        }
        if ($visibleSamples -lt $minimumVisibleSamples) {
            throw "PrintWindow did not capture a rendered folded status surface after retrying ($theme, samples=$visibleSamples)."
        }

        Assert-EdgePassThrough 'folded'
    }
    if (-not [IslandPhase0WindowCapture]::PressSpace($windowHandle)) {
        throw 'Could not send the synthetic open command to the motion probe.'
    }
    foreach ($delay in @(80, 160, 320, 600)) {
        Start-Sleep -Milliseconds $delay
        Save-ProbeWindow "open-$($delay)ms.png" $windowHandle
        if ($Edge -and $delay -in @(160, 600)) { Assert-EdgePassThrough "open-$($delay)ms" }
    }
    if (-not [IslandPhase0WindowCapture]::PressSpace($windowHandle)) {
        throw 'Could not send the synthetic close command to the motion probe.'
    }
    foreach ($delay in @(80, 160, 320, 600)) {
        Start-Sleep -Milliseconds $delay
        Save-ProbeWindow "close-$($delay)ms.png" $windowHandle
        if ($Edge -and $delay -in @(160, 600)) { Assert-EdgePassThrough "close-$($delay)ms" }
    }
    $capturePrefix = 'hwnd-'
    $foldedPath = Join-Path $outputRoot "${capturePrefix}folded.png"
    $expandedPath = Join-Path $outputRoot "${capturePrefix}open-600ms.png"
    $restoredPath = Join-Path $outputRoot "${capturePrefix}close-600ms.png"
    $openDiff = [IslandPhase0WindowCapture]::CountDifferentPixels($foldedPath, $expandedPath)
    $restoreDiff = [IslandPhase0WindowCapture]::CountDifferentPixels($foldedPath, $restoredPath)
    if ($openDiff -lt 500) {
        throw "The expanded frame did not differ enough from folded state ($openDiff changed samples)."
    }
    if ($restoreDiff -lt 0 -or $restoreDiff -gt 3000) {
        throw "The reverse transition did not return near the folded state ($restoreDiff changed samples)."
    }
    if ($Edge -and (Test-Path -LiteralPath $stdoutPath)) {
        Get-Content -LiteralPath $stdoutPath | ForEach-Object { Write-Output $_ }
    }
    Write-Output "PASS captured $mode motion; folded-to-expanded changed $openDiff sampled pixels and restored-to-folded changed $restoreDiff. Screenshots remain diagnostic, not product visual acceptance."
}
finally {
    if ($process -and -not $process.HasExited) {
        if ($windowHandle -ne [IntPtr]::Zero) {
            try { [void][IslandPhase0WindowCapture]::CloseWindow($windowHandle) } catch { }
        }
        try { [void]$process.CloseMainWindow() } catch { }
        try { [void]$process.WaitForExit(3000) } catch { }
        if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue }
    }
}
