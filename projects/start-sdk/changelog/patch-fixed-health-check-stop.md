---
release-notes: false
---

- Stopping a service no longer waits for an in-flight health check, whose result is discarded. `ready.fn` receives an `AbortSignal` that aborts when the check stops, and `exec`'s `abort` option accepts it.
