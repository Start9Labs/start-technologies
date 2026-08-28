# Public IP

Access a service interface directly using a gateway's public IP address and port number, without a domain name. Some protocols — such as Bitcoin P2P, Lightning, and other peer-to-peer services — communicate using raw IP addresses and ports rather than domain names. For these services, a public IP address may be all that's needed.

For hosting websites or APIs that people access in a browser, use a [public domain](clearnet.md) instead. Public IPs accessed in a browser will display certificate warnings because Let's Encrypt does not sign certificates for IP addresses. Visitors would need to [trust your Root CA](trust-ca.md), which is not reasonable for public access.

> [!NOTE]
> A public IP address reaches your server from the Internet. If your server is behind a NAT router, typing that address from a device on the same network as the server produces a certificate warning: the router rewrites the connection to your server's LAN address, so your server sees a local connection and presents the certificate for that address instead. Use its [`.local` address, a private domain, or its LAN IP](lan.md) from inside your own network. A server that holds its public address directly, or that you reach over [StartTunnel](/start-tunnel/), is unaffected.

## Watch The Video

<div class="yt-video" data-id="xKYhCMNN3gw" data-title="Public IP"></div>

## Enable Public IP Access

1. In the **Interfaces** tab, expand the service interface and locate the gateway you want to use.

1. Find its public IPv4 address and toggle it on.

> [!WARNING]
> If your ISP uses [CGNAT](cgnat.md), you cannot use your router gateway for public IP access because port forwarding will not work. Use a [StartTunnel](/start-tunnel/) gateway instead.

## Home IP Address Stability

If your gateway is your home router, be aware that your ISP can change your home IP address at any time. When this happens, any peers or services configured to reach you at the old IP will lose connectivity. Unlike [clearnet domains](clearnet.md), there is no dynamic DNS equivalent for raw IP addresses.

If you need a stable public IP, use a [StartTunnel](/start-tunnel/) gateway. VPS providers assign static IPs that don't change.

## Configure Port Forwarding

The selected port must be forwarded in the corresponding gateway. StartOS tests port forwarding automatically when you add or enable a public IP address, and will guide you through the setup if the test doesn't pass. When the port is served directly by the service (the usual case for a raw IP), this test needs the service to be **running** — it is disabled while the service is stopped, and if the service restarts as you enable the address StartOS shows it as untested rather than failed until it is back up (see [Interfaces](interfaces.md)).

- **Routers**: Refer to your router's manual for instructions on port forwarding.

- **StartTunnel**: Refer to the [StartTunnel Port Forwarding guide](/start-tunnel/port-forwarding.html).
