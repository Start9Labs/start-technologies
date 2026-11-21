---
release-notes: false
---

- StartOS no longer proxies registry RPC. The UI reads the synchronized registry caches, and installing a service requires a saved registry. `start-cli server update` uses the selected update registry unless given a registry URL, which it checks once. A failed synchronization reports the registry unreachable.
