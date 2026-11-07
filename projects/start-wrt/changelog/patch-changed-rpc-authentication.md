---
release-notes: false
---

- Encrypt keyed browser RPC and continuations after signature-authenticated Noise admission while preserving anonymous setup, HTTP/header compatibility and reconnect behavior.
- Detect firmware changes through authenticated reply metadata and verify the requested version before reporting recovery from an interrupted update.
- Bound replay-cache memory when clock synchronization is unavailable.
- Preserve request IDs on middleware errors.
- Keep encrypted browser RPC available over HTTP/2 connections.
