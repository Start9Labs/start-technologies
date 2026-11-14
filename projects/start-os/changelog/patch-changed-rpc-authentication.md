---
release-notes: false
---

- Reject invalid encrypted-admission CBOR representations before native or browser field interpretation.

- Encrypt normal browser and keyed CLI RPC, backend registry queries and WebSocket continuations after signed ephemeral-DH admission, without mode-based downgrade.
- Name the server-owned signing key `server_identity_key`, preserving its value and existing database and backup formats.
- Preserve explicit serialized field and variant names in PatchDB accessors, including combined serde attributes.
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
- Cancel timed-out or disposed WebSocket requests without interrupting sibling calls, and release pending handlers when the socket closes.
