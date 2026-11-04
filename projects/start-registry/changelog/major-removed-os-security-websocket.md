---
release-notes: false
---

- Registry synchronization uses public `db.subscribe` RPC continuations for package, signer, and OS metadata. The `/os/security` WebSocket and `--security-notifications` option are removed; older StartOS clients relying on that endpoint must upgrade to receive background security-update notifications.
- Unauthenticated RPC WebSocket continuations use bounded buffers, incoming messages, and sends; slow registry subscribers disconnect and resynchronize.
