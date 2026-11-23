---
release-notes: false
---

- `start-container` inside a service container now has a command for every service effect except the callback and file-transfer plumbing, and every command has a description. Effects with structured parameters, such as `bind`, `set-health` and `export-service-interface`, read them as JSON on stdin. `get-status`, `get-dependencies` and `check-dependencies` now print their results.
