#Requires -Version 5.1
param(
    [string]$OutputPath = (Join-Path $PSScriptRoot '..\..\signing\Pluto-Labs-Cascade-Motion.pfx'),
    [switch]$GeneratePassword
)
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Run this on Windows.' }
$subject = 'CN=Pluto Labs Cascade Motion'
$existing = @(Get-ChildItem Cert:\CurrentUser\My | Where-Object {
    $_.Subject -ceq $subject -and $_.HasPrivateKey -and $_.NotAfter -gt [DateTime]::UtcNow
})
if ($existing.Count -gt 0) { throw 'A valid Pluto Labs Cascade Motion key already exists. Back it up or rotate it deliberately; do not create an accidental second release identity.' }
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$signing = [IO.Path]::GetFullPath((Join-Path $root 'signing')).TrimEnd('\') + '\'
$output = [IO.Path]::GetFullPath($OutputPath)
if (-not $output.StartsWith($signing, [StringComparison]::OrdinalIgnoreCase)) { throw 'The PFX must be stored inside the ignored signing directory.' }
if (Test-Path -LiteralPath $output) { throw "Refusing to overwrite existing PFX: $output" }
$passwordFile = Join-Path (Split-Path -Parent $output) 'password.dpapi'
if ($GeneratePassword -and (Test-Path -LiteralPath $passwordFile)) { throw "Refusing to overwrite existing password backup: $passwordFile" }
if ($GeneratePassword) {
    $bytes = New-Object byte[] 48
    $random = [Security.Cryptography.RandomNumberGenerator]::Create()
    try { $random.GetBytes($bytes) }
    finally { $random.Dispose() }
    $plainPassword = [Convert]::ToBase64String($bytes)
    [Array]::Clear($bytes, 0, $bytes.Length)
    $password = ConvertTo-SecureString $plainPassword -AsPlainText -Force
    $plainPassword = $null
}
else {
    $password = Read-Host 'Choose a strong PFX password; store it in a password manager' -AsSecureString
}
if ($password.Length -lt 16) { throw 'Use a PFX password of at least 16 characters.' }
$cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject $subject `
    -CertStoreLocation Cert:\CurrentUser\My -KeyAlgorithm RSA -KeyLength 3072 `
    -HashAlgorithm SHA256 -KeyExportPolicy Exportable -NotAfter ([DateTime]::UtcNow.AddYears(3))
try {
    New-Item -ItemType Directory -Path (Split-Path -Parent $output) -Force | Out-Null
    Export-PfxCertificate -Cert $cert -FilePath $output -Password $password | Out-Null
    if ($GeneratePassword) {
        $password | ConvertFrom-SecureString | Set-Content -LiteralPath $passwordFile -Encoding ascii
    }
    Write-Output "Certificate subject: $($cert.Subject)"
    Write-Output "Certificate thumbprint: $($cert.Thumbprint)"
    Write-Output "Encrypted private-key backup: $output"
    if ($GeneratePassword) { Write-Output "Current-user DPAPI password backup: $passwordFile" }
    Write-Output 'Keep this PFX and password private. Put the thumbprint in the release-signing environment variable.'
}
finally { $cert.Dispose(); $password.Dispose() }
