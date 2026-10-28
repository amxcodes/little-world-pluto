# Pluto Windows installer

`Pluto.iss` builds a per-user EXE installer. Setup displays only the existing transparent `Assets/Square150x150Logo.png`. There is no welcome artwork, background image, or generated illustration. The logo sits on the installer's solid light or dark surface; `Assets/Pluto.ico` is its Windows icon.

The installer places `Pluto.exe` in `%LOCALAPPDATA%\Programs\Pluto`, creates a Start menu entry, and upgrades through a stable Inno Setup `AppId`. User settings and meetings under `%LOCALAPPDATA%\Pluto` are outside the install directory and survive upgrades and uninstall. Installation needs no administrator rights.

If a prior portable Pluto copy already has Start with Windows enabled, Setup points that existing startup entry at the new installed executable. A fresh install does not turn startup on by itself; uninstall removes the installer-owned entry.

`scripts/phase1/package_windows_installer.ps1` builds Pluto and signs the executable, installer, and uninstaller with the Pluto Labs Cascade Motion self-signed Code Signing certificate. It checks the private key, validity dates, Code Signing EKU, subject, and pinned thumbprint. The script refuses unsigned release output. Windows will show an unverified publisher warning on other computers because this certificate has no public trust chain.

On Windows with the Windows SDK and Inno Setup 6.7.1:

```powershell
Get-ChildItem Cert:\CurrentUser\My |
  Where-Object { $_.HasPrivateKey } |
  Select-Object Subject, Thumbprint, NotAfter

./scripts/phase1/create_release_certificate.ps1
./scripts/phase1/package_windows_installer.ps1 -SigningCertificateThumbprint 'THUMBPRINT'
```

The signed output is `target/windows-package/Pluto-Setup-vX.Y.Z-windows-x64.exe`. See [release instructions](../../docs/RELEASING.md) for GitHub setup, tag requirements, and verification.
