---
release-notes: false
---

- Encrypt browser RPC, PatchDB WebSocket traffic and keyed remote CLI administration after signature-authenticated Noise admission, preserving public HTTP bootstrap, resource headers and local-token access.
- Remember the browser's verified server identity and warn on changes without blocking the connection, with fingerprint details for verification.
- Bound replay-cache memory when clock synchronization is unavailable.
- Preserve request IDs on middleware errors.
- Keep encrypted browser RPC available over HTTP/2 connections.
