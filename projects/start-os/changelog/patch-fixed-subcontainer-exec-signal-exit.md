---
user-facing: false
---

- `start-container subcontainer exec` reports a command killed by a signal with the shell exit code (128 plus the signal number) instead of exiting 0 for SIGTERM or 1 for other signals.
