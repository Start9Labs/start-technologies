---
release-notes: true
---

- Declare support for shared volumes, read user-granted IDs, names and access modes reactively, and choose optional mount outcomes or essential workload mounts. The capability requires StartOS 0.4.2 and a volume-capable Start CLI 3.0.0 or later. Packaging rejects outputs that lose volume support.
- Services opting in run backups in an isolated runtime without shared mounts or initialization hooks; read backup configuration from persistent state.
