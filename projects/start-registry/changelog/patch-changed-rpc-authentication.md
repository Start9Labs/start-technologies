---
release-notes: false
---

- Reject invalid native encrypted-admission CBOR representations before field interpretation.

- Authenticate client identities through encrypted WebSocket sessions while preserving legacy HTTP request-header authentication.
- Reject unsupported parameter shapes without dropping an authenticated connection.
- Bound replay-cache memory when clock synchronization is unavailable.
- Preserve request IDs on middleware errors.
- Keep encrypted browser RPC available over HTTP/2 connections.
