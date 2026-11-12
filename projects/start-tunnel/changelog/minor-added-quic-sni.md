---
release-notes: true
---

- Publish QUIC services by hostname over UDP. Choose TCP, UDP, or one TCP/UDP mapping with a shared destination, label, enabled state, owner, and automatic lease. Separate TCP-only and UDP-only mappings can use different devices for the same hostname and public port. Existing hostname mappings remain TCP; whole-forward removal without a protocol selector removes all selections.
- Preserve destination ownership when WAN addresses change, without splitting combined mappings or transferring automatic leases to manual entries.
- Refresh published-port hairpinning when the target subnet changes.
- Keep renewed automatic port mappings when renewal races their expiry cleanup.
