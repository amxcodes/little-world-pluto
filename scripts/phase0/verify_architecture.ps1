$ErrorActionPreference = 'Stop'

$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$sharedSource = Join-Path $repoRoot 'src'
if (-not (Test-Path -LiteralPath $sharedSource -PathType Container)) {
    throw "Shared Rust source directory is missing: $sharedSource"
}

$forbidden = '\b(?:windows|windows_sys|winapi|x11rb|wayland_client|zbus|reqwest|rusqlite|sqlx)::|\bstd::(?:net|fs)::|\bextern\s+"(?:system|C)"'
$violations = Get-ChildItem -LiteralPath $sharedSource -Filter '*.rs' -File -Recurse |
    Select-String -Pattern $forbidden

if ($violations) {
    $violations | ForEach-Object { "{0}:{1}: {2}" -f $_.Path, $_.LineNumber, $_.Line.Trim() }
    throw 'Shared core contains a platform, network, database, or FFI dependency; move it behind an adapter.'
}

$previousErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
try {
    $metadataOutput = & cargo metadata --manifest-path (Join-Path $repoRoot 'Cargo.toml') --no-deps --format-version 1 2>$null
    $metadataExitCode = $LASTEXITCODE
}
finally {
    $ErrorActionPreference = $previousErrorActionPreference
}
if ($metadataExitCode -ne 0) {
    throw 'cargo metadata failed; dependency target boundaries could not be verified.'
}
$metadata = ($metadataOutput -join "`n") | ConvertFrom-Json
$package = $metadata.packages | Where-Object { $_.name -eq 'island-desktop' } | Select-Object -First 1
if (-not $package) {
    throw 'cargo metadata did not return the island-desktop package.'
}

foreach ($name in @('windows', 'windows-sys')) {
    $dependency = $package.dependencies | Where-Object { $_.name -eq $name } | Select-Object -First 1
    if (-not $dependency -or $dependency.target -ne 'cfg(windows)') {
        throw "The $name API dependency must be gated to cfg(windows)."
    }
}

$x11 = $package.dependencies | Where-Object { $_.name -eq 'x11rb' } | Select-Object -First 1
if (-not $x11 -or $x11.target -ne 'cfg(target_os = "linux")' -or -not $x11.optional) {
    throw 'The archived X11 research dependency must remain optional and Linux-target-only.'
}

$sourceCount = @(Get-ChildItem -LiteralPath $sharedSource -Filter '*.rs' -File -Recurse).Count
Write-Output "Shared-core boundary check passed: $sourceCount Rust files; no OS, network, storage, database, or FFI imports. Windows API dependencies are Windows-only; X11 research is optional and Linux-only."
