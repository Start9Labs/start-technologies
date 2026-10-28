---
user-facing: true
---

- **A gateway's WAN IP can be set from the command line.** Where inbound and
  outbound traffic take different paths, StartOS could detect an address the
  server is not reachable at, and every public address and port-forwarding rule
  it showed pointed there. `start-cli net gateway set-wan-ip` sets it and
  `unset-wan-ip` returns to the detected one. See
  [Gateways](https://docs.start9.com/start-os/gateways.html).
