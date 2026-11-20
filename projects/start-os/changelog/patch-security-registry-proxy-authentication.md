---
release-notes: true
---

- Require authentication for registry requests relayed through the server, preventing anyone who can reach its interface from directing it to connect to hosts on its own network.
- Bound registry error replies to prevent unbounded memory use.
