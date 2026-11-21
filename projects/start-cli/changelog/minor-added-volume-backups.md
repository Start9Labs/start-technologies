---
release-notes: true
---

- Package services that opt into user-managed volumes.
- Select volumes independently of services for backup, and restore them as new local volumes without replacing existing data. Volume restore prompts for the backup password or reads `BACKUP_PASSWORD`, keeping it out of command arguments.
- Manage shared volumes with `start-cli volumes`: list, create on the data drive, an external partition or a CIFS share, rename, update CIFS connection details, grant or revoke service access, inspect availability and remove. CIFS passwords are prompted for or read from `CIFS_PASSWORD`.
