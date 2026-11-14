---
release-notes: false
---

- Initialization progress reports the actual initialization outcome. The initializing screen checks server state after success or failure and cancels observation when closed.
- Retire accepted initialization observations through terminal transport flush or cancellation before changing routers, bounded by the existing RPC write deadline; reject late bindings without delaying for unopened GUIDs.
