#Requires -Version 5.1
param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '..\..\target\windows-package'),
    [string]$BinaryPath = (Join-Path $PSScriptRoot '..\..\target\release\island-desktop.exe'),
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
$packagePath = Join-Path $outputRoot "Island-$packageVersion.msix"
$publicCertificatePath = Join-Path $outputRoot 'Island-Developer.cer'
$temporarySigningCertificatePath = Join-Path $outputRoot '.Island-Developer-temp.pfx'
$manifestTemplatePath = Join-Path $repoRoot 'packaging\windows\AppxManifest.xml'
$binaryPath = [System.IO.Path]::GetFullPath($BinaryPath)
$binaryPrefix = $targetRoot.TrimEnd('\') + '\'
if (-not $binaryPath.StartsWith($binaryPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Binary must remain under the repository target directory: $targetRoot"
}
$certificate = $null
$signingKey = $null

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
        & cargo build --release --offline --locked --features product-app --bin island-desktop
        if ($LASTEXITCODE -ne 0) { throw "Cargo release build failed with exit code $LASTEXITCODE." }
    }
    if (-not (Test-Path -LiteralPath $binaryPath)) { throw "Product executable not found: $binaryPath" }
    Copy-Item -LiteralPath $binaryPath -Destination (Join-Path $staging 'IslandDesktop.exe') -Force

    Add-Type -AssemblyName System.Drawing
    foreach ($asset in @(
        @{ Name = 'StoreLogo.png'; Size = 50 },
        @{ Name = 'Square150x150Logo.png'; Size = 150 },
        @{ Name = 'Square44x44Logo.png'; Size = 44 }
    )) {
        $bitmap = [System.Drawing.Bitmap]::new($asset.Size, $asset.Size)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
            $graphics.Clear([System.Drawing.Color]::FromArgb(21, 23, 29))
            $pillWidth = [int]($asset.Size * 0.68)
            $pillHeight = [int]($asset.Size * 0.30)
            $left = [int](($asset.Size - $pillWidth) / 2)
            $top = [int]($asset.Size * 0.30)
            $radius = [int]($pillHeight / 2)
            $path = [System.Drawing.Drawing2D.GraphicsPath]::new()
            try {
                $path.AddArc($left, $top, 2 * $radius, 2 * $radius, 90, 180)
                $path.AddArc($left + $pillWidth - 2 * $radius, $top, 2 * $radius, 2 * $radius, 270, 180)
                $path.CloseFigure()
                $graphics.FillPath([System.Drawing.Brushes]::Black, $path)
            }
            finally { $path.Dispose() }
            $dotSize = [Math]::Max(2, [int]($asset.Size * 0.075))
            $dotBrush = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(145, 162, 255))
            try { $graphics.FillEllipse($dotBrush, [int]($asset.Size * 0.63), [int]($asset.Size * 0.395), $dotSize, $dotSize) }
            finally { $dotBrush.Dispose() }
            $bitmap.Save((Join-Path $staging "Assets\$($asset.Name)"), [System.Drawing.Imaging.ImageFormat]::Png)
        }
        finally {
            $graphics.Dispose()
            $bitmap.Dispose()
        }
    }

    $manifest = [System.IO.File]::ReadAllText($manifestTemplatePath)
    $manifest = $manifest.Replace('@@PACKAGE_VERSION@@', $packageVersion)
    $manifest = $manifest.Replace('@@PROCESSOR_ARCHITECTURE@@', $architecture)
    [System.IO.File]::WriteAllText(
        (Join-Path $staging 'AppxManifest.xml'),
        $manifest,
        [System.Text.UTF8Encoding]::new($false)
    )

    $publisher = 'CN=Littleworld Island Desktop'
    # Use an exportable ephemeral key. Creating a certificate in the Windows
    # personal store can fail in managed build environments without enrollment
    # rights, and the private key must never enter the MSIX staging directory.
    $signingKey = [System.Security.Cryptography.RSA]::Create(2048)
    $request = [System.Security.Cryptography.X509Certificates.CertificateRequest]::new(
        $publisher,
        $signingKey,
        [System.Security.Cryptography.HashAlgorithmName]::SHA256,
        [System.Security.Cryptography.RSASignaturePadding]::Pkcs1
    )
    $request.CertificateExtensions.Add([System.Security.Cryptography.X509Certificates.X509BasicConstraintsExtension]::new($false, $false, 0, $true))
    $request.CertificateExtensions.Add([System.Security.Cryptography.X509Certificates.X509KeyUsageExtension]::new(
        [System.Security.Cryptography.X509Certificates.X509KeyUsageFlags]::DigitalSignature, $true
    ))
    $extendedUsage = [System.Security.Cryptography.OidCollection]::new()
    $extendedUsage.Add([System.Security.Cryptography.Oid]::new('1.3.6.1.5.5.7.3.3')) | Out-Null
    $request.CertificateExtensions.Add([System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension]::new($extendedUsage, $true))
    $certificate = $request.CreateSelfSigned([DateTimeOffset]::Now.AddMinutes(-5), [DateTimeOffset]::Now.AddDays(30))
    [System.IO.File]::WriteAllBytes(
        $publicCertificatePath,
        $certificate.Export([System.Security.Cryptography.X509Certificates.X509ContentType]::Cert)
    )
    $random = [System.Security.Cryptography.RandomNumberGenerator]::Create()
    $randomBytes = New-Object byte[] 24
    $random.GetBytes($randomBytes)
    $random.Dispose()
    $signingPassword = [Convert]::ToBase64String($randomBytes)
    [System.IO.File]::WriteAllBytes(
        $temporarySigningCertificatePath,
        $certificate.Export([System.Security.Cryptography.X509Certificates.X509ContentType]::Pfx, $signingPassword)
    )

    if (Test-Path -LiteralPath $packagePath) { Remove-Item -LiteralPath $packagePath -Force }
    & $makeAppx pack /d $staging /p $packagePath /o
    if ($LASTEXITCODE -ne 0) { throw "MakeAppx failed with exit code $LASTEXITCODE." }
    & $signTool sign /fd SHA256 /f $temporarySigningCertificatePath /p $signingPassword /v $packagePath
    if ($LASTEXITCODE -ne 0) { throw "SignTool failed with exit code $LASTEXITCODE." }

    $hash = (Get-FileHash -LiteralPath $packagePath -Algorithm SHA256).Hash
    Write-Output "PASS product release build: $binaryPath"
    Write-Output "PASS MakeAppx package: $packagePath"
    Write-Output "PASS SignTool developer signing: $publisher"
    Write-Output "Public certificate for local trust workflows: $publicCertificatePath"
    Write-Output "Architecture: $architecture; SHA-256: $hash"
    Write-Output 'Install/upgrade/uninstall is not performed by this script.'
}
finally {
    if (Test-Path -LiteralPath $temporarySigningCertificatePath) { Remove-Item -LiteralPath $temporarySigningCertificatePath -Force }
    if ($certificate) { $certificate.Dispose() }
    if ($signingKey) { $signingKey.Dispose() }
}
