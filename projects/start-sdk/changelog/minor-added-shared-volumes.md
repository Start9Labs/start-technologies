---
release-notes: true
---

- Declare support for shared volumes, read user-granted IDs, names and access modes reactively, and choose optional mount outcomes or essential workload mounts. The capability requires StartOS 0.4.2 and a volume-capable Start CLI 3.0.0 or later. Packaging rejects outputs that lose volume support.
- Dynamic daemon sets refresh shared-volume consumers and their dependents when sources change, including private connection changes that leave metadata unchanged. Unrelated containers remain running; nested reactive reads reconcile in place. Optional mounts added later do not restart for earlier source changes.
- Services opting in run backups in an isolated runtime without shared mounts or initialization hooks; read backup configuration from persistent state.
