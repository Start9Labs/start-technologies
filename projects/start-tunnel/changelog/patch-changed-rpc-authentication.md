---
release-notes: false
---

- Reject invalid native encrypted-admission CBOR representations before field interpretation.

- Encrypt browser RPC, PatchDB WebSocket traffic and keyed remote CLI administration after signed ephemeral-DH admission, preserving public HTTP bootstrap, resource headers and local-token access.
- Remember the browser's verified server identity and warn on changes without blocking the connection, with fingerprint details for verification.
- Cancel pending device-key acquisition and enrollment across logout, replacement and rollback.
- Bound replay-cache memory when clock synchronization is unavailable.
- Preserve request IDs on middleware errors and consecutive browser RPC calls as prior connections finish closing.
- Reject unsupported parameter shapes without dropping an authenticated connection.
- Keep encrypted browser RPC available over HTTP/2 connections.
