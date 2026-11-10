---
release-notes: false
---

- Encrypt normal browser and keyed CLI RPC, backend registry queries and WebSocket continuations after signed ephemeral-DH admission, keeping setup, initialization, diagnostics and state probing on explicit legacy transports without mode-based downgrade.
- Preserve HTTP resource/header, foreign-origin and legacy-client contracts, including consecutive browser RPC calls as prior connections finish closing.
- Reject unsupported parameter shapes without dropping an authenticated connection.
- Remember the browser's verified server identity and warn on changes without blocking the connection, with fingerprint details for verification.
- Bound replay-cache memory when clock synchronization is unavailable.
- Preserve request IDs on middleware errors.
- Keep encrypted browser RPC available over HTTP/2 connections.
