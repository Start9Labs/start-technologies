---
user-facing: true
---

- Fixed a daemon that keeps restarting, and repeated `exec` calls sharing one `abort` controller, logging `MaxListenersExceededWarning` and growing in memory. Rebuild packages with this SDK version to receive the fix.
