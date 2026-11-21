---
release-notes: false
---

- Dependent daemons keep running through up to two consecutive failed ready-check polls of a daemon they require, and still pause at once when it crashes or exits.
- Ready checks with `display: null` log their failures.
- A ready check that throws synchronously, or throws a non-`Error` value, reports a failure instead of stopping health checks.
