# Public IP

Access a service interface directly using a gateway's public IP address and port number, without a domain name. Some protocols — such as Bitcoin P2P, Lightning, and other peer-to-peer services — communicate using raw IP addresses and ports rather than domain names. For these services, a public IP address may be all that's needed.

For hosting websites or APIs that people access in a browser, use a [public domain](clearnet.md) instead. Public IPs accessed in a browser will display certificate warnings because Let's Encrypt does not sign certificates for IP addresses. Visitors would need to [trust your Root CA](trust-ca.md), which is not reasonable for public access.

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

The selected port must be forwarded in the corresponding gateway. StartOS tests port forwarding automatically when you add or enable a public IP address, and will guide you through the setup if the test fails.

- **Routers**: Refer to your router's manual for instructions on port forwarding.

- **StartTunnel**: Refer to the [StartTunnel Port Forwarding guide](/start-tunnel/port-forwarding.html).
