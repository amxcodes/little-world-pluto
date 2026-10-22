#Requires -Version 5.1
$ErrorActionPreference = 'Stop'

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$targetRoot = [System.IO.Path]::GetFullPath((Join-Path $repoRoot 'target'))
$outputRoot = [System.IO.Path]::GetFullPath((Join-Path $targetRoot 'phase1-codex-schema'))
$targetPrefix = $targetRoot.TrimEnd('\') + '\'
if (-not $outputRoot.StartsWith($targetPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Schema output must stay under the repository target directory: $targetRoot"
}

$baselinePath = Join-Path $repoRoot 'schemas\codex-schema-baseline.json'
$baseline = [System.IO.File]::ReadAllText($baselinePath) | ConvertFrom-Json
$codex = Get-Command codex -CommandType Application -ErrorAction Stop
$codexPath = [System.IO.Path]::GetFullPath($codex.Source)
$versionOutput = @(& $codexPath --version 2>&1)
if ($LASTEXITCODE -ne 0) { throw 'Could not read the installed Codex CLI version.' }
$version = ($versionOutput | Where-Object { $_ -match '^codex-cli\s+' } | Select-Object -First 1).Trim()
if (-not $version) { throw 'The installed executable did not report a codex-cli version.' }
$cliHash = (Get-FileHash -LiteralPath $codexPath -Algorithm SHA256).Hash

if (Test-Path -LiteralPath $outputRoot) {
    Remove-Item -LiteralPath $outputRoot -Recurse -Force
}
New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
$generationOutput = @(& $codexPath app-server generate-json-schema --out $outputRoot 2>&1)
if ($LASTEXITCODE -ne 0) {
    throw "Codex schema generation failed: $($generationOutput -join [Environment]::NewLine)"
}

$schemaPath = Join-Path $outputRoot 'codex_app_server_protocol.v2.schemas.json'
$requestPath = Join-Path $outputRoot 'ClientRequest.json'
if (-not (Test-Path -LiteralPath $schemaPath) -or -not (Test-Path -LiteralPath $requestPath)) {
    throw 'Schema generation completed without the expected v2 schema bundle and ClientRequest.json.'
}
$schemaHash = (Get-FileHash -LiteralPath $schemaPath -Algorithm SHA256).Hash
$requestHash = (Get-FileHash -LiteralPath $requestPath -Algorithm SHA256).Hash

$differences = [System.Collections.Generic.List[string]]::new()
if ($version -cne $baseline.cli_version) { $differences.Add("CLI version: expected $($baseline.cli_version), found $version") }
if ($cliHash -cne $baseline.cli_sha256) { $differences.Add("CLI SHA-256: expected $($baseline.cli_sha256), found $cliHash") }
if ($schemaHash -cne $baseline.protocol_v2_sha256) { $differences.Add("v2 schema SHA-256: expected $($baseline.protocol_v2_sha256), found $schemaHash") }
if ($requestHash -cne $baseline.client_request_sha256) { $differences.Add("ClientRequest SHA-256: expected $($baseline.client_request_sha256), found $requestHash") }
if ($differences.Count -gt 0) {
    throw "Codex protocol baseline changed. Review the installed schema before updating schemas/codex-schema-baseline.json:`n - $($differences -join "`n - ")"
}

Write-Output "PASS Codex CLI: $version"
Write-Output "PASS CLI SHA-256: $cliHash"
Write-Output "PASS v2 schema SHA-256: $schemaHash"
Write-Output "PASS ClientRequest SHA-256: $requestHash"
Write-Output "Schemas: $outputRoot"
