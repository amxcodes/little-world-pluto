#Requires -Version 5.1
param(
    [Parameter(Mandatory = $true)][string]$FilePath
)
$ErrorActionPreference = 'Stop'
$file = [IO.Path]::GetFullPath($FilePath)
$sdk = $env:PLUTO_SIGNTOOL_PATH
$timestampTool = $env:PLUTO_OSSLSIGNCODE_PATH
$thumbprint = $env:PLUTO_SIGNING_CERT_THUMBPRINT
$timestampUrl = $env:PLUTO_TIMESTAMP_URL
$caFile = $env:PLUTO_HTTPS_CA_FILE
if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "File to sign does not exist: $file" }
if (-not (Test-Path -LiteralPath $sdk -PathType Leaf)) { throw 'Windows SDK SignTool was not found.' }
if (-not (Test-Path -LiteralPath $timestampTool -PathType Leaf)) { throw 'Pinned HTTPS timestamp tool was not found.' }
if (-not $caFile -or -not (Test-Path -LiteralPath $caFile -PathType Leaf)) { throw 'Trusted HTTPS CA bundle was not found.' }
if ($thumbprint -notmatch '^[0-9A-Fa-f]{40}$') { throw 'Invalid signing certificate thumbprint.' }
if ($timestampUrl -notmatch '^https://') { throw 'A validated HTTPS RFC 3161 timestamp URL is required.' }

& $sdk sign /fd SHA256 /s My /sha1 $thumbprint $file
if ($LASTEXITCODE -ne 0) { throw "Could not sign $file ($LASTEXITCODE)." }

$timestamped = Join-Path ([IO.Path]::GetDirectoryName($file)) ("pluto-timestamp-" + [Guid]::NewGuid().ToString('N') + '.exe')
$signerPem = Join-Path ([IO.Path]::GetDirectoryName($file)) ("pluto-signer-" + [Guid]::NewGuid().ToString('N') + '.pem')
try {
    & $timestampTool add -ts $timestampUrl -HTTPS-CAfile $caFile -in $file -out $timestamped
    if ($LASTEXITCODE -ne 0) { throw "Could not add HTTPS timestamp to $file ($LASTEXITCODE)." }
    $signer = Get-Item -LiteralPath "Cert:\CurrentUser\My\$thumbprint"
    $signerDer = $signer.Export([Security.Cryptography.X509Certificates.X509ContentType]::Cert)
    $signerBase64 = [Convert]::ToBase64String($signerDer, [Base64FormattingOptions]::InsertLineBreaks)
    [IO.File]::WriteAllLines($signerPem, [string[]]@('-----BEGIN CERTIFICATE-----', $signerBase64, '-----END CERTIFICATE-----'), [Text.UTF8Encoding]::new($false))
    & $timestampTool verify -CAfile $signerPem -TSA-CAfile $caFile -in $timestamped
    if ($LASTEXITCODE -ne 0) { throw "Signature or trusted timestamp verification failed for $file ($LASTEXITCODE)." }
    $signature = Get-AuthenticodeSignature -LiteralPath $timestamped
    if ($null -eq $signature.SignerCertificate -or
        $signature.SignerCertificate.Thumbprint -ine $thumbprint -or $null -eq $signature.TimeStamperCertificate) {
        throw "Signed and timestamped file did not verify: $file"
    }
    Copy-Item -LiteralPath $timestamped -Destination $file -Force
}
finally {
    if (Test-Path -LiteralPath $timestamped) { Remove-Item -LiteralPath $timestamped -Force }
    if (Test-Path -LiteralPath $signerPem) { Remove-Item -LiteralPath $signerPem -Force }
}
