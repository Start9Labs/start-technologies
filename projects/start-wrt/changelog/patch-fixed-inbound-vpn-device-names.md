---
release-notes: false
---

- Inbound VPN clients can reach devices by bare name, such as `nas`, as devices on the router's own networks can: client configurations now set the `lan` search domain. For a client added earlier, end the `DNS` line of its configuration with `, lan`.
