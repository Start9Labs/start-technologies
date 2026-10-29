---
user-facing: true
---

- **WireGuard configs can use a separate endpoint on NAT'd StartTunnel hosts.**
  `tunnel device show-config --endpoint` supplies an IPv4, IPv6, or DNS endpoint
  independently of the local WAN assignment. Requires StartTunnel 1.5.0 or later.
