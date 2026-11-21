---
release-notes: false
---

- Removing a daemon or health check in `Daemons.dynamic`, or hiding its health display, now clears its health entry via the new `clearHealth` effect instead of leaving a stale result until service restart.
