---
release-notes: false
---

- Encrypt normal browser and keyed CLI RPC, backend registry queries and WebSocket continuations after signed ephemeral-DH admission, without mode-based downgrade.
- Support encrypted setup, initialization, diagnostic RPC and continuations alongside legacy HTTP callers; invalidate old RPC sessions when the active startup context changes.
- Preserve HTTP resource/header, foreign-origin and legacy-client contracts, including consecutive browser RPC calls as prior connections finish closing.
- Reject unsupported parameter shapes without dropping an authenticated connection.
- Remember the browser's verified server identity and warn on changes without blocking the connection, with fingerprint details for verification.
- Bound replay-cache memory when clock synchronization is unavailable.
- Preserve request IDs on middleware errors.
- Prevent duplicate operations when a Unix RPC fails or loses its response.
- Keep encrypted browser RPC available over HTTP/2 connections.
