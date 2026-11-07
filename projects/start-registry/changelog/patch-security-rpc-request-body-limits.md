---
release-notes: true
---

- Bound incoming JSON-RPC request bodies to 1 MiB before authentication and decoding to prevent unbounded memory use. Inline registry icons must fit within this budget after encoding. Oversized-request errors remain readable by browser clients.
- Bound incoming legacy subscription frames and blocked writes while allowing large outgoing indexes.
