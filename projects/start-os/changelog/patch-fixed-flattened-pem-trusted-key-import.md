---
release-notes: false
---

- Accept PEM `PUBLIC KEY` blocks pasted on one line or with altered whitespace when importing trusted keys via `server trusted-keys add`, and return a clear error asking for original line breaks when an armored PGP public key is pasted on one line.
