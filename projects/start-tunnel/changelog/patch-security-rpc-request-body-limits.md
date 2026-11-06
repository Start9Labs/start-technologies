---
release-notes: true
---

- Bound incoming JSON-RPC request bodies before authentication and decoding to prevent unbounded memory use. Oversized-request errors remain readable by browser clients.
