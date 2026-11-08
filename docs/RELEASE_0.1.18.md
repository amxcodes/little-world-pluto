# Pluto v0.1.18

- Fixes in-app updates on Windows systems that do not trust Pluto's pinned,
  self-signed code-signing certificate as a root authority.
- Accepts that certificate only when the installer hash and pinned signer
  match, and Windows reports `UntrustedRoot` as the chain's sole issue.
- Keeps rejecting altered installers, mismatched signers, missing signatures,
  and all other certificate-chain errors.
