---
release-notes: false
---

- Record virtual-host client/service connection failures at trace level instead of flooding normal logs. Use `start-cli server set-trace true` for temporary daemon-wide trace output and `start-cli server set-trace false` to restore the startup filter.
