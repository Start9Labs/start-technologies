---
release-notes: false
---

- `make install HOST=<profile-or-URL>` selects the device for architecture detection and installation. Local development installs use `--trust` to override signer trust while retaining signature and integrity checks.
- `make sign` builds the configured targets and endorses all local `.s9pk` files with the configured identity key.
- `make universal` and `make publish` run even when a file has the target's name.
