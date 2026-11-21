---
release-notes: false
---

- Kept runtime shutdown and restart RPCs unavailable until startup is ready, and prevented shutdown-status persistence from blocking accepted requests. Once shutdown or restart begins, further requests report the action already in progress.
- Stopped in-flight requests and ongoing package installs, updates, removals, rebuilds, shared-volume changes, disk formatting, backups, restores, and cleanup before unmounting storage during shutdown or restart, while keeping waiting CLI/API replies connected through service teardown. Package shutdown hooks retain access to ordinary effects.
- Preserved ownership and permission bits for copied backup files and directories.
