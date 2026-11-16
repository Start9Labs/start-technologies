---
release-notes: false
---

- Fixed shutdown and restart requests being lost during startup or blocked by shutdown-status persistence. Once shutdown or restart begins, further requests report the action already in progress.
- Stopped ongoing backup, restore, and cleanup work before unmounting storage during shutdown or restart.
