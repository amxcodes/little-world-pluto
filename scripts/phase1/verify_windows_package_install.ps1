#Requires -Version 7.1
param(
    [string]$PackagePath = (Join-Path $PSScriptRoot '..\..\target\windows-package\Pluto-0.1.0.0.msix'),
    [string]$PublicCertificatePath = (Join-Path $PSScriptRoot '..\..\target\windows-package\Pluto-Developer.cer'),
    [switch]$CurrentUserTrust
)

$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Product MSIX install verification requires Windows.' }

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$targetRoot = [System.IO.Path]::GetFullPath((Join-Path $repoRoot 'target'))
$targetPrefix = $targetRoot.TrimEnd('\') + '\'
$resolvedPackage = [System.IO.Path]::GetFullPath($PackagePath)
$resolvedCertificate = [System.IO.Path]::GetFullPath($PublicCertificatePath)
foreach ($path in @($resolvedPackage, $resolvedCertificate)) {
    if (-not $path.StartsWith($targetPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Test inputs must be inside the repository target directory: $path"
    }
    if (-not (Test-Path -LiteralPath $path)) { throw "Required test input is missing: $path" }
}

$certificate = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new($resolvedCertificate)
$trustStore = if ($CurrentUserTrust) { 'Cert:\CurrentUser\TrustedPeople' } else { 'Cert:\LocalMachine\TrustedPeople' }
$trustPath = "$trustStore\$($certificate.Thumbprint)"
$packageName = 'Littleworld.Pluto.Desktop'
$package = $null
$process = $null
$trusted = $false
$installed = $false
$testRoot = Join-Path $targetRoot ("phase1-package-test-" + [Guid]::NewGuid().ToString('N'))
$isolatedLocalAppData = Join-Path $testRoot 'LocalAppData'
$markerDirectory = Join-Path $isolatedLocalAppData 'Pluto'
$markerPath = Join-Path $markerDirectory 'uninstall-preservation.marker'
$oldLocalAppData = $env:LOCALAPPDATA

if (Get-AppxPackage -Name $packageName -ErrorAction SilentlyContinue) {
    throw "A package named $packageName is already installed for this user; refusing to replace it."
}
if (Get-Process -Name 'Pluto' -ErrorAction SilentlyContinue) {
    throw 'Close Pluto before running the isolated package lifecycle test.'
}

try {
    Import-Certificate -FilePath $resolvedCertificate -CertStoreLocation $trustStore | Out-Null
    $trusted = $true
    Add-AppxPackage -Path $resolvedPackage
    $installed = $true
    $package = Get-AppxPackage -Name $packageName
    if (-not $package) { throw 'Package installation completed without a package inventory result.' }
    if ($package.Publisher -ne $certificate.Subject) {
        throw "Installed publisher mismatch: expected '$($certificate.Subject)', got '$($package.Publisher)'."
    }

    $manifest = Get-AppxPackageManifest -Package $package.PackageFullName
    $startupNode = $manifest.SelectSingleNode("//*[local-name()='StartupTask' and @TaskId='PlutoStartup']")
    if (-not $startupNode) { throw 'The installed package manifest does not expose the Pluto startup task.' }
    if ($startupNode.GetAttribute('Enabled') -ne 'false') {
        throw 'The package must leave startup disabled by default until the user opts in.'
    }

    New-Item -ItemType Directory -Path $markerDirectory -Force | Out-Null
    [System.IO.File]::WriteAllText($markerPath, 'preserve')
    $executable = Join-Path $package.InstallLocation 'Pluto.exe'
    if (-not (Test-Path -LiteralPath $executable)) { throw "Installed executable missing: $executable" }
    New-Item -ItemType Directory -Path $isolatedLocalAppData -Force | Out-Null
    $process = Start-Process -FilePath $executable -WorkingDirectory $package.InstallLocation -PassThru `
        -Environment @{ LOCALAPPDATA = $isolatedLocalAppData }
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        Start-Sleep -Milliseconds 100
        $process.Refresh()
    } while (-not $process.HasExited -and [DateTime]::UtcNow -lt $deadline)
    if ($process.HasExited) { throw "Packaged product exited during startup (exit code $($process.ExitCode))." }
    if (-not (Test-Path -LiteralPath $markerPath)) { throw 'The isolated user-data marker disappeared while the package was installed.' }
    Stop-Process -Id $process.Id -Force
    $process.WaitForExit()
    $process = $null

    Remove-AppxPackage -Package $package.PackageFullName
    $installed = $false
    if (Get-AppxPackage -Name $packageName -ErrorAction SilentlyContinue) {
        throw 'Package inventory still contains Pluto after uninstall.'
    }
    if ((Get-Content -LiteralPath $markerPath -Raw) -ne 'preserve') {
        throw 'User data marker changed during uninstall.'
    }
    Write-Output "PASS signed product MSIX installed: $($package.PackageFullName)"
    Write-Output 'PASS packaged executable started and remained live for the startup check.'
    Write-Output 'PASS startup task is registered and disabled by default.'
    Write-Output 'PASS uninstall removed package registration and preserved the isolated user-data marker.'
}
finally {
    $env:LOCALAPPDATA = $oldLocalAppData
    if ($process) {
        $process.Refresh()
        if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue }
    }
    if ($installed -and $package) {
        Remove-AppxPackage -Package $package.PackageFullName -ErrorAction SilentlyContinue
    }
    if ($trusted) {
        Remove-Item -LiteralPath $trustPath -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path -LiteralPath $testRoot) {
        $resolvedTestRoot = [System.IO.Path]::GetFullPath($testRoot)
        if ($resolvedTestRoot.StartsWith($targetPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
            Remove-Item -LiteralPath $resolvedTestRoot -Recurse -Force
        }
    }
}
