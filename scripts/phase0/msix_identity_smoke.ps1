#Requires -Version 5.1
param(
    [switch]$PackageOnly,
    [switch]$CurrentUserTrust
)

$ErrorActionPreference = 'Stop'

if (-not $IsWindows -and $env:OS -ne 'Windows_NT') {
    throw 'This package identity smoke test requires Windows.'
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$targetRoot = Join-Path $repoRoot 'target\phase0-msix-smoke'
$packageRoot = Join-Path $targetRoot 'package'
$runId = [guid]::NewGuid().ToString('N')
$packageName = "LwP0Smoke.$runId"
$publisher = 'CN=Littleworld Phase 0 MSIX Smoke'
$packageVersion = '0.0.1.0'
$identity = "${packageName}_${packageVersion}_x64"
$packagePath = Join-Path $targetRoot 'IslandPhase0PackageSmoke.msix'
$markerPath = $null
$failureMarkerPath = Join-Path $env:TEMP 'island-phase0-msix-failed.txt'
$certificate = $null
$trustStore = $null
$installed = $false

$sdkBin = 'C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64'
$makeAppx = Join-Path $sdkBin 'makeappx.exe'
$signTool = Join-Path $sdkBin 'signtool.exe'
foreach ($toolPath in @($makeAppx, $signTool)) {
    if (-not (Test-Path -LiteralPath $toolPath)) {
        throw "Required Windows SDK tool was not found: $toolPath"
    }
}

$rustc = Get-Command rustc -ErrorAction Stop
$cargo = Get-Command cargo -ErrorAction Stop
$rustHost = (& $rustc.Source -vV | Select-String '^host: ').ToString().Substring(6)
if ($rustHost -notmatch '^x86_64-pc-windows-') {
    throw "This smoke script builds the x64 package variant; rustc host is '$rustHost'."
}

New-Item -ItemType Directory -Path $targetRoot -Force | Out-Null
Get-ChildItem -LiteralPath $targetRoot -File -Filter 'LwP0Smoke.*.msix' -ErrorAction SilentlyContinue |
    Remove-Item -Force
Get-ChildItem -LiteralPath $targetRoot -File -Filter 'LwP0Smoke.*.cer' -ErrorAction SilentlyContinue |
    Remove-Item -Force
if (Test-Path -LiteralPath $packageRoot) {
    Remove-Item -LiteralPath $packageRoot -Recurse -Force
}
New-Item -ItemType Directory -Path (Join-Path $packageRoot 'Assets') -Force | Out-Null

$cargoTarget = Join-Path $targetRoot 'cargo-target'
& $cargo.Source build --locked --offline --target $rustHost --target-dir $cargoTarget `
    --example windows_media_probe --features windows-media-probe
if ($LASTEXITCODE -ne 0) {
    throw "Cargo failed with exit code $LASTEXITCODE."
}
$executablePath = Join-Path $cargoTarget "$rustHost\debug\examples\windows_media_probe.exe"
if (-not (Test-Path -LiteralPath $executablePath)) {
    throw "The packaged Windows OCR probe was not produced: $executablePath"
}
Copy-Item -LiteralPath $executablePath -Destination (Join-Path $packageRoot 'P0Smoke.exe') -Force

Add-Type -AssemblyName System.Drawing
foreach ($asset in @(
    @{ Name = 'StoreLogo.png'; Size = 50 },
    @{ Name = 'Square150x150Logo.png'; Size = 150 },
    @{ Name = 'Square44x44Logo.png'; Size = 44 }
)) {
    $bitmap = [System.Drawing.Bitmap]::new($asset.Size, $asset.Size)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.Clear([System.Drawing.Color]::FromArgb(32, 32, 32))
        $bitmap.Save((Join-Path $packageRoot "Assets\$($asset.Name)"), [System.Drawing.Imaging.ImageFormat]::Png)
    }
    finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

$manifest = @"
<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
         xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
         xmlns:desktop="http://schemas.microsoft.com/appx/manifest/desktop/windows10"
         xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
         IgnorableNamespaces="uap desktop rescap">
  <Identity Name="$packageName" Publisher="$publisher" Version="$packageVersion" ProcessorArchitecture="x64" />
  <Properties>
    <DisplayName>Island Phase 0 MSIX Smoke</DisplayName>
    <PublisherDisplayName>Local Phase 0 Test</PublisherDisplayName>
    <Logo>Assets\StoreLogo.png</Logo>
  </Properties>
  <Dependencies>
    <TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.17763.0" MaxVersionTested="10.0.26300.0" />
  </Dependencies>
  <Resources>
    <Resource Language="en-us" />
  </Resources>
  <Applications>
    <Application Id="P0Smoke" Executable="P0Smoke.exe" EntryPoint="Windows.FullTrustApplication">
      <uap:VisualElements DisplayName="Island Phase 0 MSIX Smoke" Description="Temporary package identity probe"
                          BackgroundColor="#202020" Square150x150Logo="Assets\Square150x150Logo.png"
                          Square44x44Logo="Assets\Square44x44Logo.png" />
      <Extensions>
        <desktop:Extension Category="windows.fullTrustProcess" Executable="P0Smoke.exe" />
      </Extensions>
    </Application>
  </Applications>
  <Capabilities>
    <rescap:Capability Name="runFullTrust" />
  </Capabilities>
</Package>
"@
Set-Content -LiteralPath (Join-Path $packageRoot 'AppxManifest.xml') -Value $manifest -Encoding UTF8

try {
    $certificate = New-SelfSignedCertificate `
        -Type Custom `
        -Subject $publisher `
        -KeyUsage DigitalSignature `
        -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3') `
        -CertStoreLocation 'Cert:\CurrentUser\My' `
        -KeyAlgorithm RSA `
        -KeyLength 2048 `
        -HashAlgorithm SHA256 `
        -NotAfter (Get-Date).AddDays(2)

    $certificatePath = Join-Path $targetRoot "$packageName.cer"
    [System.IO.File]::WriteAllBytes(
        $certificatePath,
        $certificate.Export([System.Security.Cryptography.X509Certificates.X509ContentType]::Cert)
    )

    & $makeAppx pack /d $packageRoot /p $packagePath /o
    if ($LASTEXITCODE -ne 0) {
        throw "MakeAppx failed with exit code $LASTEXITCODE."
    }

    & $signTool sign /fd SHA256 /s My /sha1 $certificate.Thumbprint /v $packagePath
    if ($LASTEXITCODE -ne 0) {
        throw "SignTool signing failed with exit code $LASTEXITCODE."
    }

    if ($PackageOnly) {
        Write-Output "PASS MakeAppx pack: $packagePath"
        Write-Output "PASS SignTool sign: publisher $publisher"
        Write-Output 'SKIP trust verification, install, and runtime identity: -PackageOnly was selected'
        return
    }

    if ($CurrentUserTrust) {
        $trustStore = 'Cert:\CurrentUser\TrustedPeople'
    }
    else {
        $principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
        if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
            throw 'Machine-wide MSIX trust requires an administrator session. Re-run elevated, or use -CurrentUserTrust for the per-user diagnostic path.'
        }
        $trustStore = 'Cert:\LocalMachine\TrustedPeople'
    }
    Import-Certificate -FilePath $certificatePath -CertStoreLocation $trustStore | Out-Null
    & $signTool verify /pa /v $packagePath
    if ($LASTEXITCODE -ne 0) {
        throw "SignTool verification failed with exit code $LASTEXITCODE."
    }
    Add-AppxPackage -Path $packagePath
    $installed = $true
    $package = Get-AppxPackage -Name $packageName
    if (-not $package) {
        throw 'The package was accepted but could not be read back from the current user package inventory.'
    }
    if ($package.Publisher -ne $publisher) {
        throw "Installed package publisher mismatch: '$($package.Publisher)'."
    }

    $markerPath = Join-Path $env:TEMP "island-phase0-msix-$($package.PackageFullName).txt"
    Remove-Item -LiteralPath $failureMarkerPath -Force -ErrorAction SilentlyContinue
    $appUserModelId = "$($package.PackageFamilyName)!P0Smoke"
    (New-Object -ComObject Shell.Application).ShellExecute("shell:AppsFolder\$appUserModelId")

    $deadline = [DateTime]::UtcNow.AddSeconds(45)
    while (-not (Test-Path -LiteralPath $markerPath) -and
        -not (Test-Path -LiteralPath $failureMarkerPath) -and
        [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 100
    }
    if (Test-Path -LiteralPath $failureMarkerPath) {
        throw "Packaged process failed to read identity: $(Get-Content -LiteralPath $failureMarkerPath -Raw)"
    }
    if (-not (Test-Path -LiteralPath $markerPath)) {
        throw "The registered application did not write its package identity/OCR result within 45 seconds. App ID: $appUserModelId"
    }
    $identityResult = Get-Content -LiteralPath $markerPath -Raw
    if ($identityResult -notmatch '^PASS ' -or $identityResult -notlike "*${identity}*") {
        throw "Package identity readback failed: $identityResult"
    }
    if ($identityResult -notmatch '(^|;\s*)OCR=TEST(;|$)') {
        throw "Packaged Windows OCR did not recognize the synthetic TEST fixture: $identityResult"
    }
    if ($identityResult -notmatch '(^|;\s*)FIXTURES=5/5(;|$)') {
        throw "Packaged Windows OCR did not pass the complete synthetic fixture corpus: $identityResult"
    }

    Write-Output "PASS MakeAppx pack: $packagePath"
    Write-Output "PASS SignTool sign and verify: publisher $publisher"
    Write-Output "PASS test certificate trust scope: $trustStore"
    Write-Output "PASS current-user install and package inventory: $($package.PackageFullName)"
    Write-Output "PASS packaged Rust process identity: $identityResult"
    Write-Output 'PASS packaged Windows OCR synthetic fixture corpus: 5/5'
}
finally {
    if ($installed) {
        Get-AppxPackage -Name $packageName -ErrorAction SilentlyContinue |
            Remove-AppxPackage -ErrorAction SilentlyContinue
    }
    if ($markerPath -and (Test-Path -LiteralPath $markerPath)) {
        Remove-Item -LiteralPath $markerPath -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path -LiteralPath $failureMarkerPath) {
        Remove-Item -LiteralPath $failureMarkerPath -Force -ErrorAction SilentlyContinue
    }
    if ($certificate) {
        if ($trustStore) {
            Remove-Item -LiteralPath "$trustStore\$($certificate.Thumbprint)" -Force -ErrorAction SilentlyContinue
        }
        Remove-Item -LiteralPath "Cert:\CurrentUser\My\$($certificate.Thumbprint)" -Force -ErrorAction SilentlyContinue
    }
    if ($certificatePath -and (Test-Path -LiteralPath $certificatePath)) {
        Remove-Item -LiteralPath $certificatePath -Force -ErrorAction SilentlyContinue
    }
}
