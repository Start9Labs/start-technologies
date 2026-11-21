---
release-notes: false
---

- Each gateway's public IP address is now checked against a public-IP lookup service at its own random times, with the lookup services tried in a random order, so the lookup service can no longer link your home IP address to your VPN's address by seeing both at once.
