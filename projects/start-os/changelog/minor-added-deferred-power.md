---
user-facing: true
---

- **Restarting or shutting down during a backup offers to wait for it to finish.** The prompt stays open until you choose or dismiss it. A bar shows the pending action and lets you cancel it. Pressing the physical power button during a backup also waits when StartOS can intercept the key. Over the CLI, `start-cli server restart` and `server shutdown` wait by default; `--force` interrupts the backup. The command `start-cli server cancel-deferred-power` cancels the pending action. StartOS refuses new backups once a restart or shutdown is committed. See [Creating Backups](https://docs.start9.com/start-os/backup-create.html).
