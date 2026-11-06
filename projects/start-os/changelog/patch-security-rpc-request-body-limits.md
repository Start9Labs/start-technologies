---
release-notes: true
---

- Bound incoming JSON-RPC request bodies before authentication and decoding to prevent unbounded memory use.
- Bound anonymously available WebSocket continuation input and blocked writes while preserving bulk authenticated container and terminal traffic.
