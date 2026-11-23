---
release-notes: true
---

- A security profile whose subnet overlaps the network the WAN is on no longer cuts its devices off from the router. The router loses that part of the upstream network instead, and Security Profiles warns about the overlap.
- Changing a security profile's subnet briefly reconnects its wired LAN ports to prompt address renewal. Devices behind a switch may still need reconnecting.
