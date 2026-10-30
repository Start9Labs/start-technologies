---
user-facing: true
---

- Backup mount, backup information, package restore, and CIFS target add/update commands read passwords from a hidden prompt instead of positional arguments. Update automation to supply `BACKUP_PASSWORD` or `CIFS_PASSWORD`; `PASSWORD` remains the separate CLI authentication credential.
