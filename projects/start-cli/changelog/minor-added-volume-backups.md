---
user-facing: true
---

- Package services that opt into user-managed volumes.
- Select volumes independently of services for backup, and restore them as new local volumes without replacing existing data. Volume restore prompts for the backup password or reads `BACKUP_PASSWORD`, keeping it out of command arguments.
