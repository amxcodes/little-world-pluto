#Requires -Version 5.1
param(
    [string]$ExecutablePath = (Join-Path $PSScriptRoot '..\..\target\release\island-desktop.exe')
)

$ErrorActionPreference = 'Stop'
$ExecutablePath = [System.IO.Path]::GetFullPath($ExecutablePath)
if (-not (Test-Path -LiteralPath $ExecutablePath)) {
    throw "Build the product first. Executable not found: $ExecutablePath"
}
if (Get-Process -Name 'island-desktop' -ErrorAction SilentlyContinue) {
    throw 'Close the existing Island process before running the isolated hotkey registration check.'
}

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class IslandHotkeyProbe {
    public delegate bool EnumWindowsCallback(IntPtr hwnd, IntPtr state);
    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool RegisterHotKey(IntPtr hwnd, int id, uint modifiers, uint virtualKey);
    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool UnregisterHotKey(IntPtr hwnd, int id);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetClassNameW(IntPtr hwnd, StringBuilder name, int maxCount);
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsCallback callback, IntPtr state);
    public static IntPtr FindShell(uint wantedProcessId) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((hwnd, state) => {
            uint processId;
            GetWindowThreadProcessId(hwnd, out processId);
            if (processId != wantedProcessId) return true;
            var name = new StringBuilder(256);
            GetClassNameW(hwnd, name, name.Capacity);
            if (name.ToString() == "IslandDesktopEdgeShell") {
                result = hwnd;
                return false;
            }
            return true;
        }, IntPtr.Zero);
        return result;
    }
}
'@

$modAlt = [uint32]0x0001
$modControl = [uint32]0x0002
$modShift = [uint32]0x0004
$modNoRepeat = [uint32]0x4000
$bindings = @(
    @{ Name = 'Toggle island'; Mods = $modControl -bor $modAlt; Key = 0x49 },
    @{ Name = 'Command bar'; Mods = $modControl -bor $modAlt; Key = 0x20 },
    @{ Name = 'AI chat'; Mods = $modControl -bor $modAlt; Key = 0x41 },
    @{ Name = 'Voice to text'; Mods = $modControl -bor $modAlt; Key = 0x44 },
    @{ Name = 'Screen recording'; Mods = $modControl -bor $modAlt; Key = 0x52 },
    @{ Name = 'Area screenshot'; Mods = $modControl -bor $modAlt; Key = 0x53 },
    @{ Name = 'Full screenshot'; Mods = $modControl -bor $modAlt -bor $modShift; Key = 0x53 },
    @{ Name = 'Screen OCR'; Mods = $modControl -bor $modAlt; Key = 0x4F },
    @{ Name = 'Clipboard history'; Mods = $modControl -bor $modAlt; Key = 0x56 },
    @{ Name = 'Paste plain text'; Mods = $modControl -bor $modAlt -bor $modShift; Key = 0x56 },
    @{ Name = 'Microphone mute'; Mods = $modControl -bor $modAlt; Key = 0x4D },
    @{ Name = 'Color picker'; Mods = $modControl -bor $modAlt; Key = 0x43 },
    @{ Name = 'Screen QR'; Mods = $modControl -bor $modAlt; Key = 0x51 }
)

$baseline = @{}
for ($i = 0; $i -lt $bindings.Count; $i++) {
    $id = 0x6700 + $i
    $binding = $bindings[$i]
    $available = [IslandHotkeyProbe]::RegisterHotKey([IntPtr]::Zero, $id, ($binding.Mods -bor $modNoRepeat), [uint32]$binding.Key)
    if ($available) {
        [void][IslandHotkeyProbe]::UnregisterHotKey([IntPtr]::Zero, $id)
        $baseline[$binding.Name] = 'Free'
    } else {
        $baselineError = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
        $baseline[$binding.Name] = if ($baselineError -eq 1409) { 'Conflict' } else { "Blocked:$baselineError" }
    }
}
if (@($baseline.Values | Where-Object { $_ -eq 'Free' }).Count -eq 0) {
    foreach ($entry in $baseline.GetEnumerator()) {
        Write-Output "BASELINE-$($entry.Value) $($entry.Key)"
    }
    $blockedCount = @($baseline.Values | Where-Object { $_ -like 'Blocked:*' }).Count
    $conflictCount = @($baseline.Values | Where-Object { $_ -eq 'Conflict' }).Count
    if ($blockedCount -gt 0) {
        throw "Not run: RegisterHotKey baseline was blocked for $blockedCount chord(s); Win32 error 5 means Access denied. This does not prove an existing chord conflict."
    }
    throw "Not run: RegisterHotKey baseline found no free chords ($conflictCount pre-existing conflict(s))."
}

$originalLocalAppData = $env:LOCALAPPDATA
$profile = Join-Path $PSScriptRoot ("..\..\target\hotkey-probe-" + [Guid]::NewGuid().ToString('N'))
$process = $null
try {
    [void][System.IO.Directory]::CreateDirectory($profile)
    $env:LOCALAPPDATA = [System.IO.Path]::GetFullPath($profile)
    $process = Start-Process -FilePath $ExecutablePath -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    $shell = [IntPtr]::Zero
    do {
        Start-Sleep -Milliseconds 100
        $shell = [IslandHotkeyProbe]::FindShell([uint32]$process.Id)
    } while ($shell -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if ($shell -eq [IntPtr]::Zero) { throw 'The product shell HWND did not appear.' }
    Start-Sleep -Milliseconds 400

    $checked = 0
    $failures = [System.Collections.Generic.List[string]]::new()
    $unverified = [System.Collections.Generic.List[string]]::new()
    foreach ($binding in $bindings) {
        if ($baseline[$binding.Name] -eq 'Conflict') {
            Write-Output "BASELINE-CONFLICT $($binding.Name)"
            continue
        }
        if ($baseline[$binding.Name] -ne 'Free') {
            Write-Output "BASELINE-BLOCKED $($binding.Name) $($baseline[$binding.Name])"
            continue
        }
        $checked++
        $id = 0x6800 + $checked
        $canRegister = [IslandHotkeyProbe]::RegisterHotKey([IntPtr]::Zero, $id, ($binding.Mods -bor $modNoRepeat), [uint32]$binding.Key)
        if ($canRegister) {
            [void][IslandHotkeyProbe]::UnregisterHotKey([IntPtr]::Zero, $id)
            $failures.Add("$($binding.Name): still available after product startup")
        } else {
            $registrationError = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
            if ($registrationError -eq 1409) {
                Write-Output "CLAIMED-AFTER-START $($binding.Name)"
            } else {
                Write-Output "POST-START-BLOCKED $($binding.Name) lastError=$registrationError"
                $unverified.Add("$($binding.Name): native registration check returned $registrationError")
            }
        }
    }
    if ($checked -eq 0) {
        throw 'Not run: no default shortcut could be baseline-tested. Access denied or pre-existing registrations prevent native verification.'
    }
    if ($failures.Count -gt 0) {
        throw ($failures -join '; ')
    }
    if ($unverified.Count -gt 0) {
        throw ('Native hotkey verification incomplete: ' + ($unverified -join '; '))
    }
    Write-Output "PASS product shell started; $checked baseline-free default shortcuts became unavailable to a second registration attempt."
    Write-Output 'Limitation: RegisterHotKey has no owner-query API; an unrelated app claiming a chord between baseline and post-start probes can produce a false attribution.'
} finally {
    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force
        $process.WaitForExit()
    }
    $env:LOCALAPPDATA = $originalLocalAppData
}
