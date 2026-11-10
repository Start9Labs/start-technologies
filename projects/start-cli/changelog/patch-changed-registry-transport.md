---
release-notes: false
---

- Encrypt normal keyed StartOS RPC and WebSocket continuations, along with registry and Tunnel calls, while retaining GPG-agent identities, proxy/TLS settings and local-token HTTP access.
- Establish encrypted connections on the first call and reuse them for subsequent calls without silently repeating a possibly executed request.
- Keep state probing, setup, initialization and diagnostics on explicit HTTP/legacy transports independently of stored keys.
- Preserve deployment URL prefixes ending in a slash for RPC and continuation calls.
- Warn on registry CLI server-identity changes without interrupting commands; remember the new verified key while preserving JSON output and exit status.
