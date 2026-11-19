---
release-notes: true
---

- A database restored by `withMysqlDump` or `withMariadbDump` accepts the service's connections again, so a restored service now starts instead of waiting on its database indefinitely.
