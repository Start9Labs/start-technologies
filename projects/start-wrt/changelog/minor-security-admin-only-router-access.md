---
release-notes: true
---

- **Only the Admin profile can manage the router.** Devices on other Security Profiles can no longer open the router's web interface or connect to it over SSH. They still get addresses and DNS from the router, can use automatic port forwarding, and reach published ports, hostname routes and Inbound VPNs at the router's public address. To manage the router, join the Wi-Fi with an Admin password, plug into an Admin Ethernet port, or connect through an Inbound VPN on the Admin profile.
