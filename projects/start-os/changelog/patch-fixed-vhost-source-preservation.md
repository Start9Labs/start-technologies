---
release-notes: true
---

- Reject source-preserving proxy connections when the client's address cannot be preserved, instead of substituting the host's address. Services on unproxied TLS bindings need an IPv6 listener to accept IPv6 clients; TLS rejection does not trigger automatic IPv4 fallback.
