---
user-facing: true
---

- Added opt-in background conversion of existing backups to the authenticated filesystem format, with phase progress and explicit resumption after interruption. Unconverted backups remain usable for backups and restores.
- After a daemon restart, running conversion attempts are marked as interrupted failures and can be explicitly resumed, preserving their progress and finished-attempt history.
