---
release-notes: true
---

- Publish QUIC services by hostname over UDP, independently of TLS-over-TCP mappings on the same public address and port. Manage each transport's destination, label, enabled state, and automatic lease separately; existing hostname mappings remain TCP.
- Keep renewed automatic port mappings when renewal races their expiry cleanup.
