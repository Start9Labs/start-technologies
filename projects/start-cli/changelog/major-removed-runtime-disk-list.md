---
release-notes: true
---

- Remove `start-cli disk list` and the runtime `disk.list` RPC. Automation must read attached drives from the public PatchDB `disks` state instead. Pre-setup `setup disk list` remains available.
