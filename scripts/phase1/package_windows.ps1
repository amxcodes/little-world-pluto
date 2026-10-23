#Requires -Version 5.1
param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '..\..\target\windows-package'),
    [string]$BinaryPath = (Join-Path $PSScriptRoot '..\..\target\release\pluto.exe'),
    [string]$SigningCertificateThumbprint = $env:PLUTO_SIGNING_CERT_THUMBPRINT,
    [string]$TimestampUrl = 'https://timestamp.digicert.com',
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'The product MSIX packager requires Windows.' }

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$outputRoot = [System.IO.Path]::GetFullPath($OutputDirectory)
$targetRoot = [System.IO.Path]::GetFullPath((Join-Path $repoRoot 'target'))
$targetPrefix = $targetRoot.TrimEnd('\') + '\'
if (-not $outputRoot.StartsWith($targetPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Output must remain under the repository target directory: $targetRoot"
}

$staging = Join-Path $outputRoot 'staging'
$cargoManifest = Join-Path $repoRoot 'Cargo.toml'
$cargoVersionLine = Select-String -LiteralPath $cargoManifest -Pattern '^version\s*=\s*"(\d+\.\d+\.\d+)"' |
    Select-Object -First 1
if (-not $cargoVersionLine) { throw 'A three-part numeric package version was not found in Cargo.toml.' }
$productVersion = $cargoVersionLine.Matches[0].Groups[1].Value
$packageVersion = "$productVersion.0"
$packagePath = Join-Path $outputRoot "Pluto-$packageVersion.msix"
$publicCertificatePath = Join-Path $outputRoot 'Pluto-Signing.cer'
$manifestTemplatePath = Join-Path $repoRoot 'packaging\windows\AppxManifest.xml'
$binaryPath = [System.IO.Path]::GetFullPath($BinaryPath)
$binaryPrefix = $targetRoot.TrimEnd('\') + '\'
if (-not $binaryPath.StartsWith($binaryPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Binary must remain under the repository target directory: $targetRoot"
}
$signingCertificate = $null

$rustc = Get-Command rustc -ErrorAction Stop
$rustHost = (& $rustc.Source -vV | Select-String '^host: ').ToString().Substring(6)
$architecture = switch -Regex ($rustHost) {
    '^x86_64-pc-windows-' { 'x64'; break }
    '^aarch64-pc-windows-' { 'arm64'; break }
    '^i686-pc-windows-' { 'x86'; break }
    default { throw "Unsupported Rust Windows host for MSIX: $rustHost" }
}

$windowsKitRoot = 'C:\Program Files (x86)\Windows Kits\10\bin'
$sdkTools = Get-ChildItem -LiteralPath $windowsKitRoot -Directory -ErrorAction Stop |
    Where-Object { $_.Name -match '^10\.\d+\.\d+\.\d+$' } |
    Sort-Object { [version]$_.Name } -Descending |
    ForEach-Object {
        $candidate = Join-Path $_.FullName $architecture
        if ((Test-Path (Join-Path $candidate 'makeappx.exe')) -and
            (Test-Path (Join-Path $candidate 'signtool.exe'))) { return $candidate }
    } | Select-Object -First 1
if (-not $sdkTools) { throw "MakeAppx and SignTool for $architecture were not found in $windowsKitRoot." }
$makeAppx = Join-Path $sdkTools 'makeappx.exe'
$signTool = Join-Path $sdkTools 'signtool.exe'

if ([string]::IsNullOrWhiteSpace($SigningCertificateThumbprint)) {
    throw @'
A trusted code-signing certificate is required for a release. Install it, with its private key, in Cert:\CurrentUser\My and pass -SigningCertificateThumbprint (or set PLUTO_SIGNING_CERT_THUMBPRINT). Self-signed certificates are for local testing and are not accepted here.
'@
}
$normalizedThumbprint = $SigningCertificateThumbprint -replace '\s', ''
$certificatePath = "Cert:\CurrentUser\My\$normalizedThumbprint"
if (-not (Test-Path -LiteralPath $certificatePath)) {
    throw "Signing certificate $normalizedThumbprint was not found in Cert:\CurrentUser\My."
}
$signingCertificate = Get-Item -LiteralPath $certificatePath
if (-not $signingCertificate.HasPrivateKey) {
    throw "Signing certificate $normalizedThumbprint has no accessible private key."
}
$now = [DateTime]::UtcNow
if ($now -lt $signingCertificate.NotBefore.ToUniversalTime() -or $now -ge $signingCertificate.NotAfter.ToUniversalTime()) {
    throw "Signing certificate is not currently valid (valid through $($signingCertificate.NotAfter.ToUniversalTime().ToString('u')))."
}
$codeSigningOid = '1.3.6.1.5.5.7.3.3'
$eku = $signingCertificate.Extensions |
    Where-Object { $_ -is [System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension] } |
    Select-Object -First 1
if (-not $eku -or -not ($eku.EnhancedKeyUsages | Where-Object { $_.Value -eq $codeSigningOid })) {
    throw "Signing certificate $normalizedThumbprint does not allow Code Signing."
}
$manifestTemplate = [System.IO.File]::ReadAllText($manifestTemplatePath)
$publisherMatch = [regex]::Match($manifestTemplate, 'Publisher="([^"]+)"')
if (-not $publisherMatch.Success -or $signingCertificate.Subject -cne $publisherMatch.Groups[1].Value) {
    $manifestPublisher = if ($publisherMatch.Success) { $publisherMatch.Groups[1].Value } else { '<missing>' }
    throw "Certificate subject '$($signingCertificate.Subject)' must exactly match the MSIX manifest publisher '$manifestPublisher'."
}
$chain = [System.Security.Cryptography.X509Certificates.X509Chain]::new()
try {
    [void]$chain.ChainPolicy.ApplicationPolicy.Add([System.Security.Cryptography.Oid]::new($codeSigningOid))
    if (-not $chain.Build($signingCertificate)) {
        $chainErrors = ($chain.ChainStatus | ForEach-Object { $_.StatusInformation.Trim() } | Where-Object { $_ }) -join '; '
        throw "Signing certificate does not chain to a trusted root for code signing: $chainErrors"
    }
}
finally {
    $chain.Dispose()
}

New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
if (Test-Path -LiteralPath $staging) {
    $resolvedStaging = [System.IO.Path]::GetFullPath($staging)
    if (-not $resolvedStaging.StartsWith($targetPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to clear staging outside target: $resolvedStaging"
    }
    Remove-Item -LiteralPath $resolvedStaging -Recurse -Force
}
New-Item -ItemType Directory -Path (Join-Path $staging 'Assets') -Force | Out-Null

try {
    if (-not $SkipBuild) {
        & cargo build --release --offline --locked --features product-app --bin pluto
        if ($LASTEXITCODE -ne 0) { throw "Cargo release build failed with exit code $LASTEXITCODE." }
    }
    if (-not (Test-Path -LiteralPath $binaryPath)) { throw "Product executable not found: $binaryPath" }

    $assetRoot = Join-Path $repoRoot 'packaging\windows\Assets'
    foreach ($assetName in @('StoreLogo.png', 'Square150x150Logo.png', 'Square44x44Logo.png')) {
        $assetPath = Join-Path $assetRoot $assetName
        if (-not (Test-Path -LiteralPath $assetPath -PathType Leaf)) {
            throw "Required Pluto package artwork is missing: $assetPath"
        }
        Copy-Item -LiteralPath $assetPath -Destination (Join-Path $staging "Assets\$assetName") -Force
    }

    $manifest = $manifestTemplate
    $manifest = $manifest.Replace('@@PACKAGE_VERSION@@', $packageVersion)
    $manifest = $manifest.Replace('@@PROCESSOR_ARCHITECTURE@@', $architecture)
    [System.IO.File]::WriteAllText(
        (Join-Path $staging 'AppxManifest.xml'),
        $manifest,
        [System.Text.UTF8Encoding]::new($false)
    )

    [System.IO.File]::WriteAllBytes(
        $publicCertificatePath,
        $signingCertificate.Export([System.Security.Cryptography.X509Certificates.X509ContentType]::Cert)
    )

    & $signTool sign /fd SHA256 /tr $TimestampUrl /td SHA256 /s My /sha1 $normalizedThumbprint /v $binaryPath
    if ($LASTEXITCODE -ne 0) { throw "SignTool could not sign the executable (exit code $LASTEXITCODE)." }
    & $signTool verify /pa /v $binaryPath
    if ($LASTEXITCODE -ne 0) { throw "SignTool could not verify the executable signature (exit code $LASTEXITCODE)." }
    Copy-Item -LiteralPath $binaryPath -Destination (Join-Path $staging 'Pluto.exe') -Force

    if (Test-Path -LiteralPath $packagePath) { Remove-Item -LiteralPath $packagePath -Force }
    & $makeAppx pack /d $staging /p $packagePath /o
    if ($LASTEXITCODE -ne 0) { throw "MakeAppx failed with exit code $LASTEXITCODE." }
    & $signTool sign /fd SHA256 /tr $TimestampUrl /td SHA256 /s My /sha1 $normalizedThumbprint /v $packagePath
    if ($LASTEXITCODE -ne 0) { throw "SignTool failed with exit code $LASTEXITCODE." }
    & $signTool verify /pa /v $packagePath
    if ($LASTEXITCODE -ne 0) { throw "SignTool could not verify the MSIX signature (exit code $LASTEXITCODE)." }

    $hash = (Get-FileHash -LiteralPath $packagePath -Algorithm SHA256).Hash
    Write-Output "PASS product release build: $binaryPath"
    Write-Output "PASS MakeAppx package: $packagePath"
    Write-Output "PASS trusted Authenticode signing: $($signingCertificate.Subject)"
    Write-Output "PASS SHA-256 RFC 3161 timestamp: $TimestampUrl"
    Write-Output "Public signing certificate: $publicCertificatePath"
    Write-Output "Architecture: $architecture; SHA-256: $hash"
    Write-Output 'Install/upgrade/uninstall is not performed by this script.'
}
finally {
    if ($signingCertificate) { $signingCertificate.Dispose() }
}
