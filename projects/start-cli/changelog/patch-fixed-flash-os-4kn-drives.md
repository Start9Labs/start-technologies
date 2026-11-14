---
release-notes: false
---

- `start-cli flash-os` no longer fails on a drive with 4096-byte sectors. A drive with a logical sector size other than 512 or 4096 bytes is refused before anything is written to it.
