---
release-notes: false
---

- Encrypt normal browser and keyed CLI RPC, backend registry queries and WebSocket continuations after signature-authenticated Noise admission, keeping setup, initialization, diagnostics and state probing on explicit legacy transports without mode-based downgrade.
- Preserve HTTP resource/header, foreign-origin and legacy-client contracts.
- Bound replay-cache memory when clock synchronization is unavailable.
- Preserve request IDs on middleware errors.
- Keep encrypted browser RPC available over HTTP/2 connections.
