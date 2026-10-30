---
user-facing: true
---

- Added opt-in background conversion of existing backups to the authenticated filesystem format, with weighted overall progress, all phases listed upfront, explicit resumption after interruption, and success or failure notifications naming the target. Creating a backup offers to update this server’s older format or skip conversion; restoring marks each old-format server backup with an explanatory warning tag. Unconverted backups remain usable for backups and restores.
