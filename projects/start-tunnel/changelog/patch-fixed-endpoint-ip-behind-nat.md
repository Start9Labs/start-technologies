- **WireGuard configs can use a separate endpoint when StartTunnel is behind NAT.**
  CLI `device show-config --endpoint-ip` accepts IPv4 or IPv6 independently of
  the WAN address used for SNAT and port forwarding. Non-public WAN overrides
  require an explicit endpoint instead of silently generating a private one.
