---
release-notes: false
---

- Reject invalid encrypted-admission CBOR representations before native or browser field interpretation.

- Encrypt browser and CLI RPC, including anonymous setup and loopback local-token access, after signed ephemeral-DH admission, with same-peer, intended-GUID admission for secure-origin continuations, while preserving HTTP/header compatibility and reconnect behavior.
- Send first-time and flash passwords inside encrypted RPC without an extra password envelope; retain legacy HTTP/JWE and NDJSON setup compatibility.
- Report interrupted flash observations as errors rather than leaving the wizard stalled, without aborting accepted firmware writes.
- Establish CLI connections on the first call and reuse them for subsequent calls without silently repeating a possibly executed request.
- Detect firmware changes through authenticated reply metadata and verify the requested version before reporting recovery from an interrupted update.
- Remember the browser's verified server identity and warn on changes without blocking the connection, with fingerprint details for verification.
- Persist the server's handshake signing key in the authentication PatchDB retained during sysupgrade.
- Cancel pending device-key acquisition and enrollment across logout, replacement and rollback.
- Bound replay-cache memory when clock synchronization is unavailable.
- Preserve request IDs on middleware errors and consecutive browser RPC calls as prior connections finish closing.
- Reject unsupported parameter shapes without dropping an authenticated connection.
- Keep encrypted browser RPC available over HTTP/2 connections.
- Cancel timed-out or disposed WebSocket requests without interrupting sibling calls, and release pending handlers when the socket closes.
