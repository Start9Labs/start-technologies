---
user-facing: true
---

- Add `--trust` to package installation and OS-update commands to explicitly override signer trust. On StartOS 0.4.1 and later, these operations require an enabled trusted signer by default; signature and integrity verification remain mandatory with the override.
- Diagnostic updates no longer require a registry argument that had no effect.
