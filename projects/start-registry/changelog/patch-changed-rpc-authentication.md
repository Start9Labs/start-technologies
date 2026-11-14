---
release-notes: false
---

- Reject invalid native encrypted-admission CBOR representations before field interpretation.

- Authenticate client identities through encrypted WebSocket sessions while preserving legacy HTTP request-header authentication.
- Persist the server's handshake signing key in PatchDB.
- Reject unsupported parameter shapes without dropping an authenticated connection.
- Bound replay-cache memory when clock synchronization is unavailable.
- Preserve request IDs on middleware errors.
- Keep encrypted browser RPC available over HTTP/2 connections.
- Cancel timed-out or disposed WebSocket requests without interrupting sibling calls, and release pending handlers when the socket closes.
