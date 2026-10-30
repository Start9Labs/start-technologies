---
user-facing: true
---

- Manage named trusted keyrings in a responsive System table with row edit/delete menus and independent package/OS trust switches. New keyrings trust packages only by default. Import Ed25519 public keys in PEM or armored GPG format, honoring revocations present in GPG exports and retaining full certificates with names and email addresses. Built-in keyrings stay first, with read-only names and no Delete option; either trust switch can still be changed.
- Rename SSH Keys to SSH Access.
- Show trusted package signers, block untrusted Marketplace installs, and require explicit confirmation before sideloading untrusted software.
- Require trusted signers for CLI package installs and OS updates by default; `--trust` explicitly overrides signer trust while preserving signature and integrity checks.
