---
release-notes: false
---

- Reject invalid native encrypted-admission CBOR representations before field interpretation.

- Encrypt normal keyed StartOS RPC and WebSocket continuations, along with registry and Tunnel calls, while retaining GPG-agent identities, proxy/TLS settings and local-token HTTP access.
- Establish encrypted connections on the first call and reuse them for subsequent calls without silently repeating a possibly executed request.
- Cancel abandoned keyed RPC requests without interrupting other calls on the same connection.
- Encrypt state probing, setup, initialization and diagnostic calls and continuations independently of stored keys, preserving diagnostic local-token authorization.
- Send setup secrets over the encrypted session without an extra public-key exchange.
- Preserve deployment URL prefixes ending in a slash for RPC and continuation calls.
- Preserve logical HTTP hostnames and ports when `.local` resolution pins an encrypted RPC or continuation's dial address.
- Warn on registry CLI server-identity changes without interrupting commands; remember the new verified key while preserving JSON output and exit status.
