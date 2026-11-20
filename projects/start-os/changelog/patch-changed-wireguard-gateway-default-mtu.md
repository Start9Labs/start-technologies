---
release-notes: false
---

- WireGuard gateways added or updated without an `MTU` setting now use 1280 instead of the automatic MTU, so pages and large TLS handshakes no longer stall when the gateway's path is narrower than usual, such as through another VPN.
