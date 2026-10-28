#Requires -Version 5.1
param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '..\..\target\windows-package'),
    [string]$BinaryPath = (Join-Path $PSScriptRoot '..\..\target\release\pluto.exe'),
    [string]$SigningCertificateThumbprint = $env:PLUTO_SIGNING_CERT_THUMBPRINT,
    [string]$TimestampUrl = 'https://timestamp.digicert.com',
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Pluto installer packaging requires Windows.' }
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$target = [IO.Path]::GetFullPath((Join-Path $root 'target')).TrimEnd('\') + '\'
$out = [IO.Path]::GetFullPath($OutputDirectory)
$binary = [IO.Path]::GetFullPath($BinaryPath)
foreach ($path in @($out, $binary)) {
    if (-not $path.StartsWith($target, [StringComparison]::OrdinalIgnoreCase)) { throw "Release path escaped target: $path" }
}
$versionLine = Select-String -LiteralPath (Join-Path $root 'Cargo.toml') -Pattern '^version\s*=\s*"(\d+\.\d+\.\d+)"' | Select-Object -First 1
if (-not $versionLine) { throw 'Cargo.toml needs a three-part numeric version.' }
$version = $versionLine.Matches[0].Groups[1].Value
$setup = Join-Path $out "Pluto-Setup-v$version-windows-x64.exe"
$thumbprint = $SigningCertificateThumbprint -replace '\s', ''
if (-not $thumbprint) { throw 'A code-signing certificate thumbprint is required.' }
$certPath = "Cert:\CurrentUser\My\$thumbprint"
if (-not (Test-Path -LiteralPath $certPath)) { throw "Signing certificate $thumbprint not found." }
$cert = Get-Item -LiteralPath $certPath
$previousCompany = $env:PLUTO_SIGNING_COMPANY_NAME
$previousPublisher = $env:PLUTO_SIGNING_PUBLISHER
$previousThumbprint = $env:PLUTO_SIGNING_CERT_THUMBPRINT
$previousRequiredResource = $env:PLUTO_REQUIRE_WINDOWS_VERSION_RESOURCE
try {
    if (-not $cert.HasPrivateKey) { throw 'Signing certificate has no private key.' }
    $now = [DateTime]::UtcNow
    if ($now -lt $cert.NotBefore.ToUniversalTime() -or $now -ge $cert.NotAfter.ToUniversalTime()) { throw 'Signing certificate is not valid today.' }
    $oid = '1.3.6.1.5.5.7.3.3'
    $eku = $cert.Extensions | Where-Object { $_ -is [Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension] } | Select-Object -First 1
    if (-not $eku -or -not ($eku.EnhancedKeyUsages | Where-Object Value -eq $oid)) { throw 'Certificate lacks Code Signing EKU.' }
    $chain = [Security.Cryptography.X509Certificates.X509Chain]::new()
    try {
        [void]$chain.ChainPolicy.ApplicationPolicy.Add([Security.Cryptography.Oid]::new($oid))
        $chainTrusted = $chain.Build($cert)
        if (-not $chainTrusted -and ($cert.Subject -cne 'CN=Pluto Labs Cascade Motion' -or $cert.Issuer -cne $cert.Subject)) {
            throw 'Certificate is neither publicly trusted nor the pinned Pluto Labs Cascade Motion self-signed certificate.'
        }
    }
    finally { $chain.Dispose() }
    $company = $cert.GetNameInfo([Security.Cryptography.X509Certificates.X509NameType]::SimpleName, $false)
    if ($env:PLUTO_SIGNING_PUBLISHER -and $env:PLUTO_SIGNING_PUBLISHER -cne $cert.Subject) { throw 'Certificate publisher mismatch.' }
    if ($env:PLUTO_SIGNING_COMPANY_NAME -and $env:PLUTO_SIGNING_COMPANY_NAME -cne $company) { throw 'Certificate company mismatch.' }
    if ($env:PLUTO_SIGNING_CERT_THUMBPRINT -and $env:PLUTO_SIGNING_CERT_THUMBPRINT -ine $thumbprint) { throw 'Certificate thumbprint mismatch.' }
    $env:PLUTO_SIGNING_COMPANY_NAME = $company
    $env:PLUTO_SIGNING_PUBLISHER = $cert.Subject
    $env:PLUTO_SIGNING_CERT_THUMBPRINT = $thumbprint
    $env:PLUTO_REQUIRE_WINDOWS_VERSION_RESOURCE = '1'
    $sdk = Get-ChildItem -LiteralPath 'C:\Program Files (x86)\Windows Kits\10\bin' -Directory |
        Where-Object Name -Match '^10\.\d+\.\d+\.\d+$' | Sort-Object { [version]$_.Name } -Descending |
        ForEach-Object { Join-Path $_.FullName 'x64\signtool.exe' } |
        Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
    if (-not $sdk) { throw 'Windows SDK x64 SignTool was not found.' }
    $iscc = (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source
    if (-not $iscc) {
        $iscc = @('C:\Program Files (x86)\Inno Setup 6\ISCC.exe', 'C:\Program Files\Inno Setup 6\ISCC.exe') |
            Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
    }
    if (-not $iscc) { throw 'Inno Setup 6 compiler ISCC.exe was not found.' }
    if (-not $SkipBuild) {
        & cargo build --release --offline --locked --features product-app --bin pluto
        if ($LASTEXITCODE -ne 0) { throw "Cargo release build failed ($LASTEXITCODE)." }
    }
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw "Missing Pluto executable: $binary" }
    $info = (Get-Item -LiteralPath $binary).VersionInfo
    if ($info.ProductName -ne 'Littleworld Pluto' -or $info.CompanyName -cne $company -or
        $info.OriginalFilename -ne 'pluto.exe' -or $info.FileVersion -ne "$version.0" -or
        $info.ProductVersion -ne "$version.0") { throw "Pluto executable version resource does not match $version.0." }
    New-Item -ItemType Directory -Path $out -Force | Out-Null
    & $sdk sign /fd SHA256 /tr $TimestampUrl /td SHA256 /s My /sha1 $thumbprint $binary
    if ($LASTEXITCODE -ne 0) { throw "Could not sign Pluto.exe ($LASTEXITCODE)." }
    $binarySignature = Get-AuthenticodeSignature -LiteralPath $binary
    if ($binarySignature.Status -notin @('Valid', 'NotTrusted') -or $null -eq $binarySignature.SignerCertificate -or
        $binarySignature.SignerCertificate.Thumbprint -ine $thumbprint) { throw 'Pluto.exe signature verification failed.' }
    $env:PLUTO_VERSION = $version
    $env:PLUTO_BINARY = $binary
    try {
        $sign = '-spluto=$q{0}$q sign /fd SHA256 /tr {1} /td SHA256 /s My /sha1 {2} $f' -f $sdk, $TimestampUrl, $thumbprint
        & $iscc $sign "-o$out" (Join-Path $root 'packaging\windows\Pluto.iss')
        if ($LASTEXITCODE -ne 0) { throw "Inno Setup build failed ($LASTEXITCODE)." }
    }
    finally {
        Remove-Item Env:PLUTO_VERSION -ErrorAction SilentlyContinue
        Remove-Item Env:PLUTO_BINARY -ErrorAction SilentlyContinue
    }
    if (-not (Test-Path -LiteralPath $setup -PathType Leaf)) { throw "Installer not produced: $setup" }
    $setupSignature = Get-AuthenticodeSignature -LiteralPath $setup
    if ($setupSignature.Status -notin @('Valid', 'NotTrusted') -or $null -eq $setupSignature.SignerCertificate -or
        $setupSignature.SignerCertificate.Thumbprint -ine $thumbprint) { throw 'Installer signature verification failed.' }
    Write-Output "PASS signed Pluto.exe and installer: $setup"
    Write-Output "SHA-256: $((Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash)"
}
finally {
    $env:PLUTO_SIGNING_COMPANY_NAME = $previousCompany
    $env:PLUTO_SIGNING_PUBLISHER = $previousPublisher
    $env:PLUTO_SIGNING_CERT_THUMBPRINT = $previousThumbprint
    $env:PLUTO_REQUIRE_WINDOWS_VERSION_RESOURCE = $previousRequiredResource
    $cert.Dispose()
}
