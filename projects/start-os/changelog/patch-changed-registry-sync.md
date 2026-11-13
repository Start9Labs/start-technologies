---
release-notes: false
---

- Keep connected registries synchronized over one automatically authenticated, encrypted WebSocket per registry for package listings, signing-key changes, and StartOS updates. Poll registries without WebSocket RPC support every 15 minutes using their existing HTTP API. Cached listings remain available when a registry is offline.
- Show an Unreachable registry between the marketplace registry selector and search, followed by an icon-only retry, as a status dot when collapsed. Registry management gives every registry's connection status a dedicated column, with an Unreachable-only retry. Select Unreachable to read synchronization failure details; retain cached listings or loading placeholders while unavailable.
- Show green update counts, replacing the Updates navigation count with a warning while any registry errors. Registry sidebar rows retain their counts alongside warnings; an Unreachable selected registry shows its status and a retry without resetting the selection. An unavailable empty catalog keeps loading placeholders instead of claiming every service is up to date.
- Skip malformed saved registry URLs independently, preserving synchronization and listings from valid sources.
- Preserve deployment URL prefixes when diagnostic updates query a registry.
- Retrieve private package and dependency icons using the server's registry identity, limited to saved registries' content-addressed icon endpoints.
- Refresh any saved registry from its registry-management More menu, regardless of connection status, to discover later private-access grants or poll legacy hosts immediately without changing signing-key trust; retry failed icons when refreshed listings arrive. Unsaved registry links offer an explicit connection action rather than waiting for a catalog that is not being synchronized.
