---
release-notes: false
---

- Every server setting the web interface manages can now be managed from `start-cli` as well:
  - backup targets: `backup target create`, `find`, `rename`, `update-cifs`, `remove`, `inspect` and `convert`
  - UPS monitoring: `server set-nut`, `clear-nut`, `nut-status`, `nut-scan` and `nut-list-ups`
  - `server registries refresh`
  - browser push subscriptions: `notification push list`, `unsubscribe` and `test`
  - `net dns status`, `net gateway edit` and `server trusted-keys remove-many`
- `package exec` runs a command in a service container and prints its output.
- `package force-stop` stops a stalled service using its current stop-episode deadline; it can cause data loss and does not prompt for confirmation.
