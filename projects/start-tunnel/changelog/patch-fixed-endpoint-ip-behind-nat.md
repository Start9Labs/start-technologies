---
user-facing: true
---

- **WireGuard configs can use a separate endpoint when StartTunnel is behind NAT.**
  The UI config dialog defaults to the browser URL's host and accepts an edited IP
  address or DNS name. CLI `device show-config --endpoint` supplies the same
  per-config override independently of the WAN address used for SNAT and forwarding.
