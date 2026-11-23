---
release-notes: true
---

- Sign in with a passkey instead of the master password: add one under System > Passkeys, then use Login with passkey. The sign-in key stays in memory and expires ten minutes after its last connection closes.

- Trusted keys accept P-256 passkey public keys, and StartOS verifies passkey (WebAuthn) signatures from them on packages, updates and signed requests.
