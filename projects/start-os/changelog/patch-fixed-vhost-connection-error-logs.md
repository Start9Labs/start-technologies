---
release-notes: false
---

- Silence connection errors from unavailable or misbehaving services and clients in the virtual-host proxy.
- Reject source-preserving proxy connections when the client's address cannot be preserved, instead of substituting the host's address.
