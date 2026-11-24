---
release-notes: true
---

- Keeps the router reachable on overlapping profile and WAN /24s with the default routing settings, and warns about network overlaps. A narrower WAN network can still block some devices from the dashboard.
- Changing a security profile's subnet briefly reconnects its wired LAN ports to prompt address renewal. Devices behind a switch may still need reconnecting.
