---
release-notes: false
---

- Keep connected registries synchronized over one automatically authenticated, encrypted WebSocket per registry for package listings, signing-key changes, and StartOS updates. Poll registries without WebSocket RPC support every 15 minutes using their existing HTTP API. Cached listings remain available when a registry is offline.
- Show newly published StartOS builds with the installed version number without a manual update check.
- Registry management gives every registry's connection status a dedicated column, with a retry for an Unreachable registry. Select Unreachable to read synchronization failure details. An Unreachable registry raises a warning on System and its Registries page.
- Show green update counts. When the registry selected in the Marketplace or Updates is unreachable, a notice says its listings may be out of date, with Retry and Manage Registries. Cached listings stay available, and an unavailable empty catalog keeps loading placeholders instead of claiming every service is up to date. In the Marketplace header, Start9's registries show only their beta and alpha testing warnings; a custom registry shows its description alongside the custom-registry warning.
- Skip malformed saved registry URLs independently, preserving synchronization and listings from valid sources.
- Preserve deployment URL prefixes when diagnostic updates query a registry.
- Retrieve private package and dependency icons using the server's registry identity, limited to saved registries' content-addressed icon endpoints.
- Refresh any saved registry from its registry-management More menu, regardless of connection status, to discover later private-access grants or poll legacy hosts immediately without changing signing-key trust; retry failed icons when refreshed listings arrive. Unsaved registry links offer an explicit connection action with the registry selector still available, and a remembered registry that is no longer saved gives way to the default.
