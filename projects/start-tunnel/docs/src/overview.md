# Overview

StartTunnel is a virtual private router: a minimal router that runs on a Debian VPS you rent, instead of in your home, and you administer it from your browser. Like a home router, it creates private networks, gives devices addresses and forwards ports from the internet. Because it runs in a data center, it works however limited your home connection is, including behind carrier-grade NAT (CGNAT).

- [Subnets](subnets.md) are isolated private networks. Devices on one subnet can all reach each other. StartTunnel comes with one subnet, and you can add more to keep groups of devices apart.
- [Devices](devices.md): every device on a subnet gets its own WireGuard configuration, either as a server, such as a StartOS server, or as a client, such as a phone or a laptop. A StartOS server uses its StartTunnel configuration as a gateway.
- [Published ports](published-ports.md) expose a device's port to the internet, over IPv4 at your VPS's public address, which needs a VPS with a dedicated public IPv4 address, or over IPv6 at the device's own address. Visitors see only the VPS, never your home, and HTTPS ends at your own service.
- [DNS records](dns-records.md): StartTunnel answers DNS for the devices on your tunnel, so they reach your services by private names that are never published to the internet.
- An [HTTP-to-HTTPS redirect](http-redirects.md) answers on port 80 of every public IPv4 address your VPS holds. It is on by default.
- StartTunnel manages your VPS's firewall itself, and disables UFW, so it expects to be the only application on its VPS. See [Installing](installing.md).
