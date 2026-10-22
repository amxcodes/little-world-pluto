#Requires -Version 5.1
param(
    [ValidateRange(5, 120)][int]$WarmupSeconds = 15,
    [ValidateRange(5, 120)][int]$SampleSeconds = 20,
    [switch]$Release,
    [switch]$IsolatedBackends,
    [switch]$SmallShell
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$configuration = if ($Release) { 'release' } else { 'debug' }
$probePath = Join-Path $repoRoot "target\$configuration\examples\native_motion_probe.exe"
if (-not $IsolatedBackends -and -not (Test-Path -LiteralPath $probePath)) {
    $buildMode = if ($Release) { '--release ' } else { '' }
    throw "Build the UI host first: cargo build ${buildMode}--example native_motion_probe --features ui-host-probe --locked."
}
if (Get-Process -Name 'native_motion_probe' -ErrorAction SilentlyContinue) {
    throw 'Close existing native_motion_probe processes before this idle sample.'
}

$processorCount = [Environment]::ProcessorCount
$profileSurface = if ($SmallShell) { 'shell-230x30-logical' } else { 'window-1060x720-logical' }
foreach ($renderer in @('wgpu', 'glow')) {
    if ($IsolatedBackends) {
        $feature = "ui-host-profile-$renderer"
        $buildArgs = @('build', '--locked', '--offline', '--no-default-features', '--example', 'native_motion_probe', '--features', $feature)
        if ($Release) { $buildArgs += '--release' }
        & cargo @buildArgs
        if ($LASTEXITCODE -ne 0) {
            throw "Building the isolated $renderer renderer returned exit code $LASTEXITCODE."
        }
        if (-not (Test-Path -LiteralPath $probePath)) {
            throw "The isolated $renderer renderer build did not produce $probePath."
        }
        $runArgs = @()
    }
    else {
        $runArgs = @()
        if ($renderer -eq 'glow') { $runArgs += '--glow' }
    }
    if ($SmallShell) { $runArgs += '--small-shell' }
    if ($runArgs.Count -gt 0) {
        $process = Start-Process -FilePath $probePath -ArgumentList $runArgs -PassThru
    }
    else {
        $process = Start-Process -FilePath $probePath -PassThru
    }
    try {
        $deadline = [DateTime]::UtcNow.AddSeconds(20)
        do {
            Start-Sleep -Milliseconds 100
            $process.Refresh()
            if ($process.HasExited) {
                throw "$renderer motion probe exited during startup with code $($process.ExitCode)."
            }
        } while ($process.MainWindowHandle -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)

        if ($process.MainWindowHandle -eq [IntPtr]::Zero) {
            throw "$renderer motion probe did not expose a main window within 20 seconds."
        }

        Start-Sleep -Seconds $WarmupSeconds
        $process.Refresh()
        $cpuBefore = $process.CPU
        $privateBefore = $process.PrivateMemorySize64
        $workingBefore = $process.WorkingSet64
        $clock = [Diagnostics.Stopwatch]::StartNew()
        Start-Sleep -Seconds $SampleSeconds
        $clock.Stop()
        $process.Refresh()

        $cpuSeconds = $process.CPU - $cpuBefore
        $privateAfter = $process.PrivateMemorySize64
        $workingAfter = $process.WorkingSet64
        $oneCorePercent = 100.0 * $cpuSeconds / $clock.Elapsed.TotalSeconds
        $machinePercent = $oneCorePercent / $processorCount
        Write-Output (
            'profile={0} renderer={1} pid={2} warmup_s={3} sample_s={4:F3} cpu_s={5:F4} one_core_pct={6:F2} machine_pct={7:F3} private_mib={8:F1}->{9:F1} working_set_mib={10:F1}->{11:F1} surface={12}' -f `
                $configuration, $renderer, $process.Id, $WarmupSeconds, $clock.Elapsed.TotalSeconds, $cpuSeconds,
                $oneCorePercent, $machinePercent, ($privateBefore / 1MB), ($privateAfter / 1MB),
                ($workingBefore / 1MB), ($workingAfter / 1MB), $profileSurface
        )
    }
    finally {
        if ($process -and -not $process.HasExited) {
            [void]$process.CloseMainWindow()
            if (-not $process.WaitForExit(3000)) {
                Stop-Process -Id $process.Id -Force
            }
        }
    }
}
