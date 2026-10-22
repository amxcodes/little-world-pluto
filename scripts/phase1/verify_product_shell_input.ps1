#Requires -Version 5.1
param(
    [string]$ExecutablePath = (Join-Path $PSScriptRoot '..\..\target\release\island-desktop.exe'),
    [string]$CapturePath = (Join-Path $PSScriptRoot '..\..\target\phase1-agent-desk-fallback.png')
)

$ErrorActionPreference = 'Stop'
$ExecutablePath = [System.IO.Path]::GetFullPath($ExecutablePath)
if (-not (Test-Path -LiteralPath $ExecutablePath)) {
    throw "Build the product first. Executable not found: $ExecutablePath"
}
if (Get-Process -Name 'island-desktop' -ErrorAction SilentlyContinue) {
    throw 'An Island product process is already running; close it before this isolated input check.'
}

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class IslandProductInputProbe {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }

    [StructLayout(LayoutKind.Sequential)]
    public struct POINT { public int X, Y; }

    public delegate bool EnumWindowsCallback(IntPtr hwnd, IntPtr state);

    [StructLayout(LayoutKind.Sequential)]
    public struct KEYBDINPUT { public ushort wVk, wScan; public uint dwFlags, time; public UIntPtr dwExtraInfo; }
    [StructLayout(LayoutKind.Explicit)]
    public struct INPUTUNION { [FieldOffset(0)] public KEYBDINPUT ki; }
    [StructLayout(LayoutKind.Sequential)]
    public struct INPUT { public uint type; public INPUTUNION U; }

    [DllImport("user32.dll")]
    public static extern bool SetProcessDpiAwarenessContext(IntPtr value);
    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsCallback callback, IntPtr state);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetClassNameW(IntPtr hwnd, StringBuilder text, int maxCount);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr FindWindowW(string className, string windowName);
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
    public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll", EntryPoint = "SendMessageW")]
    public static extern IntPtr SendMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll", SetLastError = true)]
    public static extern uint SendInput(uint count, INPUT[] inputs, int size);
    [DllImport("user32.dll", EntryPoint = "PostMessageW")]
    public static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);
    public static IntPtr FindWindowByClass(uint wantedProcessId, string exactClass, bool visibleOnly) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((hwnd, state) => {
            uint processId;
            GetWindowThreadProcessId(hwnd, out processId);
            if (processId != wantedProcessId || (visibleOnly && !IsWindowVisible(hwnd))) return true;
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
    public static uint SendKey(ushort virtualKey) {
        var inputs = new INPUT[2];
        inputs[0].type = 1;
        inputs[0].U.ki.wVk = virtualKey;
        inputs[1].type = 1;
        inputs[1].U.ki.wVk = virtualKey;
        inputs[1].U.ki.dwFlags = 2;
        return SendInput(2, inputs, Marshal.SizeOf(typeof(INPUT)));
    }
}
'@

[void][IslandProductInputProbe]::SetProcessDpiAwarenessContext([IntPtr](-4))
$process = $null
$originalLocalAppData = $env:LOCALAPPDATA
try {
    $testLocalAppData = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\target\phase1-probe-localappdata'))
    [void][System.IO.Directory]::CreateDirectory($testLocalAppData)
    $env:LOCALAPPDATA = $testLocalAppData
    $foregroundBefore = [IslandProductInputProbe]::GetForegroundWindow()
    $process = Start-Process -FilePath $ExecutablePath -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    $shell = [IntPtr]::Zero
    do {
        Start-Sleep -Milliseconds 100
        $shell = [IslandProductInputProbe]::FindWindowByClass([uint32]$process.Id, 'IslandDesktopEdgeShell', $true)
    } while ($shell -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if ($shell -eq [IntPtr]::Zero) { throw 'The product shell HWND did not appear.' }

    $tray = [IslandProductInputProbe]::FindWindowByClass([uint32]$process.Id, 'IslandDesktopTrayWindow', $false)
    $hiddenDesk = [IslandProductInputProbe]::FindWindowByClass([uint32]$process.Id, 'Window Class', $false)
    $deskInitiallyVisible = $hiddenDesk -ne [IntPtr]::Zero -and [IslandProductInputProbe]::IsWindowVisible($hiddenDesk)
    # The native shell HWND can appear before eframe applies its first queued
    # viewport command. Check the settled startup state, not that brief gap.
    if ($tray -ne [IntPtr]::Zero -and $deskInitiallyVisible) {
        $hiddenDeadline = [DateTime]::UtcNow.AddSeconds(3)
        while ($deskInitiallyVisible -and [DateTime]::UtcNow -lt $hiddenDeadline) {
            Start-Sleep -Milliseconds 25
            $deskInitiallyVisible = [IslandProductInputProbe]::IsWindowVisible($hiddenDesk)
        }
    }
    if ($tray -ne [IntPtr]::Zero -and $deskInitiallyVisible) {
        throw 'The Agent Desk should start hidden when tray initialization succeeds.'
    }
    if ($tray -eq [IntPtr]::Zero -and -not $deskInitiallyVisible) {
        throw 'The Agent Desk must start visible when tray initialization fails.'
    }
    if ($tray -eq [IntPtr]::Zero) {
        Add-Type -AssemblyName System.Drawing
        [void][IslandProductInputProbe]::SetForegroundWindow($hiddenDesk)
        Start-Sleep -Milliseconds 200
        $bounds = New-Object IslandProductInputProbe+RECT
        if ([IslandProductInputProbe]::GetWindowRect($hiddenDesk, [ref]$bounds)) {
            $captureWidth = $bounds.Right - $bounds.Left
            $captureHeight = $bounds.Bottom - $bounds.Top
            $capture = New-Object System.Drawing.Bitmap($captureWidth, $captureHeight)
            $graphics = [System.Drawing.Graphics]::FromImage($capture)
            $graphics.CopyFromScreen($bounds.Left, $bounds.Top, 0, 0, $capture.Size)
            $capture.Save([System.IO.Path]::GetFullPath($CapturePath), [System.Drawing.Imaging.ImageFormat]::Png)
            $graphics.Dispose()
            $capture.Dispose()
        }
    }
    if ($deskInitiallyVisible) {
        [void][IslandProductInputProbe]::ShowWindow($hiddenDesk, 0)
        Start-Sleep -Milliseconds 100
        if ([IslandProductInputProbe]::IsWindowVisible($hiddenDesk)) {
            throw 'The test could not hide the visible fallback desk before exercising pill click-to-open.'
        }
    }

    $activateResult = [IslandProductInputProbe]::SendMessage(
        $shell, 0x0021, $shell, [IntPtr]0x02010001
    ).ToInt64()
    if ($activateResult -ne 3) {
        throw "WM_MOUSEACTIVATE returned $activateResult; MA_NOACTIVATE is 3."
    }

    $folded = New-Object IslandProductInputProbe+RECT
    if (-not [IslandProductInputProbe]::GetWindowRect($shell, [ref]$folded)) {
        throw 'Could not read folded shell bounds.'
    }
    $foldedWidth = $folded.Right - $folded.Left
    $foldedHeight = $folded.Bottom - $folded.Top
    $foldedOpaque = New-Object IslandProductInputProbe+POINT
    $foldedOpaque.X = [int](($folded.Left + $folded.Right) / 2)
    $foldedOpaque.Y = [int](($folded.Top + $folded.Bottom) / 2)
    $foldedTransparent = New-Object IslandProductInputProbe+POINT
    $foldedTransparent.X = $folded.Left + 1
    $foldedTransparent.Y = $folded.Bottom - 1
    if ([IslandProductInputProbe]::WindowFromPhysicalPoint($foldedOpaque) -ne $shell) {
        throw 'A folded opaque point does not hit the product shell.'
    }
    if ([IslandProductInputProbe]::WindowFromPhysicalPoint($foldedTransparent) -eq $shell) {
        throw 'The folded transparent tail intercepts input.'
    }

    # Exercise the product click message path without injecting a physical click.
    $clickResult = [IslandProductInputProbe]::SendMessage($shell, 0x0202, [IntPtr]::Zero, [IntPtr]::Zero)
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    $desk = [IntPtr]::Zero
    do {
        Start-Sleep -Milliseconds 50
        $candidate = [IslandProductInputProbe]::FindWindowByClass([uint32]$process.Id, 'Window Class', $true)
        if ($candidate -ne [IntPtr]::Zero -and [IslandProductInputProbe]::IsWindowVisible($candidate)) {
            $desk = $candidate
        }
    } while ($desk -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if ($desk -eq [IntPtr]::Zero) {
        $afterClick = New-Object IslandProductInputProbe+RECT
        [void][IslandProductInputProbe]::GetWindowRect($shell, [ref]$afterClick)
        $clickWidth = $afterClick.Right - $afterClick.Left
        $clickHeight = $afterClick.Bottom - $afterClick.Top
        throw "The shell click message returned $clickResult without showing the Agent Desk; shell bounds after click=${clickWidth}x${clickHeight}."
    }

    $expanded = New-Object IslandProductInputProbe+RECT
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        Start-Sleep -Milliseconds 40
        [void][IslandProductInputProbe]::GetWindowRect($shell, [ref]$expanded)
    } while ((($expanded.Right - $expanded.Left) -lt ($foldedWidth * 2)) -and [DateTime]::UtcNow -lt $deadline)
    $expandedWidth = $expanded.Right - $expanded.Left
    $expandedHeight = $expanded.Bottom - $expanded.Top
    if ($expandedWidth -lt ($foldedWidth * 2) -or $expandedHeight -lt ($foldedHeight * 4)) {
        throw "The shell did not reach its expanded bounds: ${expandedWidth}x${expandedHeight}."
    }
    Start-Sleep -Milliseconds 350
    [void][IslandProductInputProbe]::GetWindowRect($shell, [ref]$expanded)
    $expandedWidth = $expanded.Right - $expanded.Left
    $expandedHeight = $expanded.Bottom - $expanded.Top

    $expandedOpaque = New-Object IslandProductInputProbe+POINT
    $expandedOpaque.X = [int](($expanded.Left + $expanded.Right) / 2)
    $expandedOpaque.Y = [int]($expanded.Top + $expandedHeight * 0.55)
    $expandedTransparent = New-Object IslandProductInputProbe+POINT
    $expandedTransparent.X = $expanded.Left + 1
    $expandedTransparent.Y = $expandedOpaque.Y
    if ([IslandProductInputProbe]::WindowFromPhysicalPoint($expandedOpaque) -ne $shell) {
        throw 'An expanded opaque point does not hit the product shell.'
    }
    if ([IslandProductInputProbe]::WindowFromPhysicalPoint($expandedTransparent) -eq $shell) {
        throw 'The expanded transparent side intercepts input.'
    }

    # Synthetic hotkey close validates the delayed best-effort focus restore path.
    [void][IslandProductInputProbe]::PostMessage($shell, 0x0312, [IntPtr]0x1D51, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 600
    $foregroundAfterClose = [IslandProductInputProbe]::GetForegroundWindow()
    if ($foregroundBefore -ne [IntPtr]::Zero -and $foregroundAfterClose -ne $foregroundBefore) {
        throw "Foreground was not restored: expected $foregroundBefore, got $foregroundAfterClose."
    }

    $trayLifecycle = 'not-run-no-tray'
    if ($tray -ne [IntPtr]::Zero) {
        # Deliver the same callback payload Explorer uses, then exercise the
        # real eframe close-to-tray and tray-reopen lifecycle without SendInput.
        [void][IslandProductInputProbe]::SendMessage($tray, 0x8011, [IntPtr]::Zero, [IntPtr]0x0202)
        $deadline = [DateTime]::UtcNow.AddSeconds(5)
        while (-not [IslandProductInputProbe]::IsWindowVisible($hiddenDesk) -and [DateTime]::UtcNow -lt $deadline) {
            Start-Sleep -Milliseconds 40
        }
        if (-not [IslandProductInputProbe]::IsWindowVisible($hiddenDesk)) {
            throw 'The tray open callback did not show the Agent Desk.'
        }

        [void][IslandProductInputProbe]::SendMessage($hiddenDesk, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
        $deadline = [DateTime]::UtcNow.AddSeconds(5)
        while ([IslandProductInputProbe]::IsWindowVisible($hiddenDesk) -and [DateTime]::UtcNow -lt $deadline) {
            Start-Sleep -Milliseconds 40
        }
        if ([IslandProductInputProbe]::IsWindowVisible($hiddenDesk) -or $process.HasExited) {
            throw 'Close-to-tray did not hide the Agent Desk while keeping the process alive.'
        }

        [void][IslandProductInputProbe]::SendMessage($tray, 0x8011, [IntPtr]::Zero, [IntPtr]0x0202)
        $deadline = [DateTime]::UtcNow.AddSeconds(5)
        while (-not [IslandProductInputProbe]::IsWindowVisible($hiddenDesk) -and [DateTime]::UtcNow -lt $deadline) {
            Start-Sleep -Milliseconds 40
        }
        if (-not [IslandProductInputProbe]::IsWindowVisible($hiddenDesk) -or $process.HasExited) {
            throw 'The tray callback did not reopen the Agent Desk after close-to-tray.'
        }

        if (-not [IslandProductInputProbe]::PostMessage($tray, 0x8011, [IntPtr]::Zero, [IntPtr]0x0205)) {
            throw 'Could not open the product tray menu for the Quit lifecycle check.'
        }
        $deadline = [DateTime]::UtcNow.AddSeconds(5)
        $menu = [IntPtr]::Zero
        do {
            Start-Sleep -Milliseconds 40
            $menu = [IslandProductInputProbe]::FindWindowW('#32768', $null)
        } while ($menu -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
        if ($menu -eq [IntPtr]::Zero) {
            throw 'The tray right-click callback did not open a native popup menu.'
        }
        [uint32]$menuProcessId = 0
        [void][IslandProductInputProbe]::GetWindowThreadProcessId($menu, [ref]$menuProcessId)
        if ($menuProcessId -ne [uint32]$process.Id) {
            throw 'The popup menu belongs to a different process.'
        }
        [void][IslandProductInputProbe]::SetForegroundWindow($menu)
        Start-Sleep -Milliseconds 100
        $keyOwner = [IslandProductInputProbe]::GetForegroundWindow()
        [uint32]$keyOwnerProcessId = 0
        [void][IslandProductInputProbe]::GetWindowThreadProcessId($keyOwner, [ref]$keyOwnerProcessId)
        $quitMenuResult = 'quit-menu=not-run-foreground-denied'
        if ($keyOwnerProcessId -eq [uint32]$process.Id) {
            if ([IslandProductInputProbe]::SendKey(0x23) -eq 2 -and [IslandProductInputProbe]::SendKey(0x0D) -eq 2) {
                $quitMenuResult = if ($process.WaitForExit(7000)) { 'quit-menu=pass process-exit=pass' } else { 'quit-menu=not-completed' }
            } else {
                $quitMenuResult = 'quit-menu=not-run-input-denied'
            }
        }
        $trayLifecycle = "close-to-tray=pass tray-reopen=pass $quitMenuResult"
    }

    "PASS pid=$($process.Id) shell=$shell folded=${foldedWidth}x${foldedHeight} expanded=${expandedWidth}x${expandedHeight} mouseactivate=$activateResult opaque-hit=pass transparent-hit=pass synthetic-click=pass foreground-restored=$($foregroundBefore -eq $foregroundAfterClose) tray-lifecycle=$trayLifecycle"
    "tray_hwnd=$tray startup_desk_visible=$deskInitiallyVisible capture=$CapturePath"
    'Limitation: the synthetic click message and WindowFromPhysicalPoint checks do not replace a physical mouse click or a SendInput click-through test.'
}
finally {
    $env:LOCALAPPDATA = $originalLocalAppData
    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        [void]$process.WaitForExit(3000)
    }
}
