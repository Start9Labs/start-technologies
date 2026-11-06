---
release-notes: true
---

- Bound incoming JSON-RPC request bodies before authentication and decoding to prevent unbounded memory use.
- Bound incoming legacy subscription frames and blocked writes while allowing large outgoing indexes.
