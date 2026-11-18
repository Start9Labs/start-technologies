---
release-notes: false
---

- Backup and restore syncs accept `deleteExcluded`, which removes paths matching `exclude` from the target, so adding an exclusion also clears those paths from existing backups.
