---
release-notes: false
---

- Keep connected registries synchronized over one automatically authenticated, encrypted WebSocket per registry for package listings, signing-key changes, and StartOS updates. Poll registries without WebSocket RPC support every 15 minutes using their existing HTTP API. Cached listings remain available when a registry is offline.
- Show registry connection status beside the marketplace sort control and in registry management, with loading placeholders until a catalog is available, including failed attempts, and cached listings retained during reconnects. Select Error to read the synchronization failure details.
- Skip malformed saved registry URLs independently, preserving synchronization and listings from valid sources.
- Preserve deployment URL prefixes when diagnostic updates query a registry.
- Retrieve private package and dependency icons using the server's registry identity, limited to saved registries' content-addressed icon endpoints.
- Refresh a saved registry to discover later private-access grants without changing signing-key trust; retry failed icons when refreshed listings arrive. Unsaved registry links offer an explicit connection action rather than waiting for a catalog that is not being synchronized.
