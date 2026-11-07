---
release-notes: false
---

- Encrypt normal keyed StartOS RPC and WebSocket continuations, along with registry and Tunnel calls, while retaining GPG-agent identities, proxy/TLS settings and local-token HTTP access.
- Keep state probing, setup, initialization and diagnostics on explicit HTTP/legacy transports independently of stored keys.
- Preserve deployment URL prefixes ending in a slash for RPC and continuation calls.
