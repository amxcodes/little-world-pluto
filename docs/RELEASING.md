# Windows releases

Pluto releases use a signed per-user EXE installer. The GitHub workflow builds from a version-matched `vX.Y.Z` tag, checks locked Rust and npm dependencies, verifies the committed UI bundle and Windows version resources, signs the standalone executable and installer, runs Microsoft Defender against the exact signed files, then publishes SHA-256 checksums, a build manifest, SBOMs, and GitHub artifact attestations. Publishing stops if any gate fails.

## One-time GitHub setup

1. Create a self-signed Code Signing certificate with the exact subject `CN=Pluto Labs Cascade Motion` using `scripts/phase1/create_release_certificate.ps1`. The encrypted PFX is kept in the ignored `signing/` directory, outside `target/`, so clean builds do not erase it. Back up the PFX and its password in a separate secure location. Keep the private key restricted to release maintainers. A generated `password.dpapi` file can only be decrypted by the same Windows user account; it is not a portable password backup.
2. Create a GitHub Actions environment named `release-signing`. Set environment variables `PLUTO_SIGNING_PUBLISHER=CN=Pluto Labs Cascade Motion`, `PLUTO_SIGNING_COMPANY_NAME=Pluto Labs Cascade Motion`, and `PLUTO_SIGNING_CERT_THUMBPRINT` to the certificate's 40-character thumbprint. Set environment secrets `PLUTO_SIGNING_PFX_BASE64` to the base64 PFX and `PLUTO_SIGNING_PFX_PASSWORD` to its password. The workflow imports the key only after building dependencies, deletes the temporary PFX, and removes the key from the runner after signing. Never commit the PFX or its base64 form.
3. Create a second environment named `release-publish`. Add required reviewers to both environments and restrict deployments to protected `v*` tags. Protect those tags with a repository ruleset that prevents unauthorized creation and force updates.
4. Enable immutable releases where available. Review Dependabot action updates before merging.

In repository **Settings**, enable Actions and allow GitHub Actions to create releases with the workflow's scoped `contents: write` permission. Under **Environments**, create `release-signing` and `release-publish`; restrict both to protected `v*` tags, and add a reviewer if another trusted maintainer is available. Under **Rules → Rulesets**, protect the main branch and `v*` tags from force updates and deletion, and require the Windows CI checks before merging. Under **Advanced Security**, enable secret scanning, push protection, and Dependabot alerts; under **Releases**, enable release immutability. These settings must be configured on GitHub; a workflow file cannot enable them for the repository.

Before the first tag, run the certificate creation script interactively on a trusted Windows PC. Put only its thumbprint and subject in environment variables; put its encrypted PFX as base64 and the PFX password in environment **secrets**. Do not upload the PFX as a release asset, repository file, issue attachment, or workflow artifact. Verify the two environment names and variable names against `.github/workflows/release-windows.yml` before pushing a tag.

If signing is not configured, the release intentionally fails. This self-signed certificate pins Pluto updates to one private key, but Windows does not automatically trust it. It does not establish a publicly verified publisher or remove Windows SmartScreen and Defender warnings. Do not ask users to disable protection or import the certificate into a trusted root store.

## Publishing

Update `Cargo.toml` to a three-part version, rebuild and commit `island-ui/dist/index.html`, and push the matching protected tag (for example `v0.1.2`). The workflow rejects a mismatched tag or source changes during build. After approval and verification, it publishes:

- `Pluto-Setup-vX.Y.Z-windows-x64.exe` — signed per-user installer and signed uninstaller.
- `Pluto-vX.Y.Z-windows-x64.exe` — signed standalone executable.
- `SHA256SUMS.txt`, `release-manifest.json`, and Rust/UI CycloneDX SBOMs.

The installer upgrades the per-user install under `%LOCALAPPDATA%\Programs\Pluto`. App data and saved meetings under `%LOCALAPPDATA%\Pluto` are not removed. Verify a downloaded file with `Get-FileHash -Algorithm SHA256`, Windows' signature details, and GitHub provenance:

An earlier MSIX installation is a separate Windows package identity. Remove that old package through Windows Installed apps after confirming its data is backed up; the EXE installer does not silently remove it or migrate data from a virtualized package folder.

In Settings → General → Updates, users can check manually and choose **Install**. Pluto downloads only the exact installer asset from the latest stable GitHub release, checks its published SHA-256 and Authenticode signature against the exact publisher subject and certificate thumbprint embedded in the installed build, then opens Windows Setup. An untrusted self-signed chain is accepted only when the signature and pinned thumbprint match. The app does not download or install a release without that click. Development builds without an embedded thumbprint cannot install updates through this control. Keep the same private key for subsequent releases: rotating it requires a transition release signed by the old key.

```powershell
gh attestation verify .\Pluto-Setup-vX.Y.Z-windows-x64.exe --repo amxcodes/little-world-pluto
```

## Defender detections

Signing establishes file integrity and a consistent release key; a self-signed certificate does not establish a publicly verified publisher or guarantee SmartScreen or Defender reputation for a new hash. If Defender flags a release, keep protection enabled. Record the exact file, SHA-256, detection name, intelligence version, tag, and source revision; submit that file through Microsoft's software-developer review route. Do not ask users to add exclusions.
