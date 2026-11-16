---
release-notes: true
---

- Backup commands use registered target IDs instead of `disk-<logicalname>` paths. Add local-folder and drive-folder targets in System → Backups, then use the IDs from `start-cli backup target list`. Existing `cifs-N` registrations keep their IDs.
