---
release-notes: false
---

- A `withPgDump`, `withMysqlDump`, or `withMariadbDump` backup that fails while copying its database dump no longer destroys the previous backup's dump.
