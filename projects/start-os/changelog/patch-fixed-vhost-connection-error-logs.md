---
release-notes: false
---

- Record virtual-host client/service connection failures at trace level instead of flooding normal logs. Use `start-cli server set-log-level trace` for temporary `start-core`/`start-os` trace output, or choose `error`, `warn`, `info`, or `debug`. The startup log filter is restored when the daemon restarts.
