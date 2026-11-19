---
release-notes: false
---

- Backups no longer keep stale data: syncs with `delete` (the default) also remove paths matching `exclude` from the target, and a backup removes volumes no longer in the backup set from the target being updated.
