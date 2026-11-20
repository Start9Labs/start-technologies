---
release-notes: false
---

- WireGuard gateways without an `MTU` setting now use 1280 instead of the automatic MTU, so pages and large TLS handshakes no longer stall when the gateway's path is narrower than usual, such as through another VPN. Existing gateways switch to 1280 on upgrade and briefly reconnect.
