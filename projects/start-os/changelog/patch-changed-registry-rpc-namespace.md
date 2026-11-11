---
release-notes: false
---

- RPC and MCP clients manage connected registries under `server.registries`. `server.update-signers` is removed; use `server.trusted-keys`.
- The database keeps connected registries and the StartOS update registry at `serverInfo.registries` and `serverInfo.osRegistry`, no longer under `ui`.
