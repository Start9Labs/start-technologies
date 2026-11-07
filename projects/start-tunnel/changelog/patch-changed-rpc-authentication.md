---
release-notes: false
---

- Encrypt browser RPC and PatchDB WebSocket traffic after signature-authenticated Noise admission, preserving public HTTP bootstrap, resource headers and legacy clients.
- Bound replay-cache memory when clock synchronization is unavailable.
- Preserve request IDs on middleware errors.
- Keep encrypted browser RPC available over HTTP/2 connections.
