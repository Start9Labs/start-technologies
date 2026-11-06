---
release-notes: false
---

- Keep connected registries synchronized over one automatically signed WebSocket per registry for package listings, signing-key changes, and StartOS updates. Cached listings remain available when a registry is offline. Registries must support `db.watch`; deploy updated registries before this client.
- Retrieve private package and dependency icons using the server's registry identity, limited to saved registries' content-addressed icon endpoints.
