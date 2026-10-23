# Windows package signing

`scripts/phase1/package_windows.ps1` builds Pluto, signs `Pluto.exe` and the MSIX, and applies an RFC 3161 SHA-256 timestamp. Release signing requires a publicly trusted code-signing certificate with its private key installed in `Cert:\CurrentUser\My`. The certificate must be valid, include the Code Signing EKU, chain to a trusted root, and have a subject that exactly matches the MSIX manifest publisher (`CN=Littleworld Pluto`). The script refuses to create or use a self-signed release certificate.

List eligible certificates without exposing private keys:

```powershell
Get-ChildItem Cert:\CurrentUser\My |
  Where-Object { $_.HasPrivateKey } |
  Select-Object Subject, Thumbprint, NotAfter
```

Then build using the certificate thumbprint:

```powershell
./scripts/phase1/package_windows.ps1 -SigningCertificateThumbprint "THUMBPRINT"
```

The thumbprint can also be set in `PLUTO_SIGNING_CERT_THUMBPRINT`. The private key stays in the Windows certificate store; only the public certificate is exported to `target/windows-package/Pluto-Signing.cer`. The executable is written to `target/release/pluto.exe`; the signed package is written to `target/windows-package/Pluto-<version>.msix`.

Obtain the signing identity from a trusted code-signing provider. This script expects the resulting certificate and private key in the Windows certificate store; managed signing services such as Azure Artifact Signing need a separate signing integration. A public signature verifies publisher identity and file integrity; it does not guarantee that a new download has SmartScreen reputation immediately. The Windows package install lifecycle checker is `scripts/phase1/verify_windows_package_install.ps1` and must be run separately on a machine that trusts the signing chain.

The manifest declares a disabled-by-default Windows startup task (`PlutoStartup`). In the packaged app, Settings reads the effective Windows state and can request enable/disable. The UI preserves Windows' `DisabledByUser` and policy states, and the API is unavailable in an unpackaged run. Use Windows Startup settings/Task Manager when Windows has disabled the task for the user or policy.
