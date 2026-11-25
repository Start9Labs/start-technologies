---
release-notes: true
---

- Declare support for shared volumes, read user-granted IDs, unique folder-safe names and access modes reactively, with renames refreshing reactive reads, and choose optional mount outcomes or essential workload mounts. The capability requires StartOS 0.4.3 and a volume-capable Start CLI 3.0.0 or later. Packaging rejects outputs that lose volume support.
- Dynamic daemon sets refresh shared-volume consumers and their dependents when sources change, including private connection changes that leave metadata unchanged. Unrelated containers remain running; nested reactive reads reconcile in place. Optional mounts added later do not restart for earlier source changes.
- Shared sources use host-owned staging and registered native data binds, with read-only empty placeholders beneath data mounts. Grant changes deny future unauthorized mounts and detach incompatible SDK-managed binds, including in manually managed subcontainers. Independently created aliases and open handles can retain access.
- Services opting in run backups in an isolated runtime without shared mounts or initialization hooks; read backup configuration from persistent state.
