---
release-notes: false
---

- Reject invalid native encrypted-admission CBOR representations before field interpretation.

- Encrypt normal browser and keyed CLI RPC, backend registry queries and WebSocket continuations after signed ephemeral-DH admission, without mode-based downgrade.
- Encrypt browser setup, initialization, diagnostic RPC and continuations without enrolling or persisting a missing device key; preserve legacy HTTP callers and invalidate old RPC sessions when the active startup context changes.
- Retain encrypted startup on ordinary LAN HTTP without a browser secure context.
- Send setup and recovery secrets inside encrypted RPC without duplicate password encryption; retain legacy encrypted-password compatibility.
- Preserve HTTP resource/header, foreign-origin and legacy-client contracts, including consecutive browser RPC calls as prior connections finish closing.
- Reject unsupported parameter shapes without dropping an authenticated connection.
- Remember the browser's verified normal-server identity and warn on changes without blocking the connection, with fingerprint details for verification; startup peers do not enter that history.
- Cancel pending device-key acquisition and enrollment across logout, replacement and rollback.
- Bound replay-cache memory when clock synchronization is unavailable.
- Preserve request IDs on middleware errors.
- Prevent duplicate operations when a Unix RPC fails or loses its response.
- Keep encrypted browser RPC available over HTTP/2 connections.
