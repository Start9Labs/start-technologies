---
release-notes: false
---

- StartOS no longer proxies registry RPC. The UI reads the synchronized registry caches, and installing a service requires a saved registry and updating StartOS uses the selected update registry; either reports the registry unreachable when its synchronization fails.
