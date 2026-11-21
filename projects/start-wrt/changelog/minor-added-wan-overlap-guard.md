---
release-notes: true
---

- Detects security-profile subnets that overlap the WAN network, moves eligible profiles to free subnets, warns about unresolved overlaps, and rejects conflicting new subnets or static WAN addresses.
- Changing a security profile's subnet briefly reconnects its wired LAN ports to prompt address renewal. Devices behind a switch may still need reconnecting.
