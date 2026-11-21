---
release-notes: false
---

- Report the actual conflict or setup error when adding an SNI port forward instead of showing `SNI registration failed (code 192)` for every failure, and return PCP code 192 only for real hostname conflicts.
