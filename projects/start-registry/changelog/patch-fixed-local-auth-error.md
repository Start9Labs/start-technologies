---
release-notes: false
---

- When no registry hostname is configured for the CLI, report a clear permission error when running `start-registry` locally without root access, and report when `start-registryd` is not running instead of asking for `--registry`.
