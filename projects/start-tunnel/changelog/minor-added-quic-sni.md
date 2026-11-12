---
release-notes: true
---

- Publish QUIC services by hostname over UDP. Choose TCP, UDP, or one TCP/UDP mapping with a shared destination, label, enabled state, owner, and automatic lease. Separate TCP-only and UDP-only mappings can use different devices for the same hostname and public port. Existing hostname mappings and catch-alls remain TCP, including catch-all renewals and same-device port changes. Management without a protocol selector finds the mapping that owns TCP; bare whole-forward removal removes all selections.
- Preserve destination ownership when WAN addresses change, without splitting combined mappings or transferring automatic leases to manual entries.
- Show TCP/UDP published-port reservations in HTTP redirect settings; UDP-only mappings remain independent.
- Refresh published-port hairpinning when the target subnet changes, retaining installed scope while the subnet is temporarily unobserved.
- Keep renewed automatic port mappings when renewal races their expiry cleanup.
