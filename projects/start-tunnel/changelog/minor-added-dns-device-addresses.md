---
release-notes: true
---

- A subnet whose DNS resolver is a device on the subnet now passes each lookup on from the device that asked, so resolvers such as Pi-hole and AdGuard Home can tell your devices apart instead of seeing every lookup come from StartTunnel.
