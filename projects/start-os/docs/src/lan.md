# Local Access

Connect to your server over your local network using its [mDNS address](mdns.md), direct IP address, or [private domain](private-domains.md). This is the fastest connection method, as traffic stays entirely on your LAN and never reaches the Internet. You must be connected to the same Local Area Network (LAN) as your server, and you must [trust your Root CA](trust-ca.md). The StartOS dashboard is available at the base address (e.g. `my-cool-server.local`), while installed services are available on different ports of that same address.

## Watch the Video

<div class="yt-video" data-id="lAMI43MC7fQ" data-title="Connecting Locally"></div>

## mDNS, IP Address, and Private Domains

- **[mDNS](mdns.md)** — Your server's `.local` address (e.g. `my-cool-server.local`) is your [server name](server-name.md) with `.local` on the end, and resolves to your server's LAN IP address automatically.

- **IP address** — Connect using your server's LAN IP directly. The address can be found in your StartOS dashboard at `System -> StartOS UI`, in your router dashboard, or by pinging your server's mDNS address from a computer on the same network.

- **[Private Domains](private-domains.md)** — Assign custom domain names to service interfaces. Private domains work like the mDNS address but also work over [VPN](inbound-vpn.md) and can be anything you choose.

> [!IMPORTANT]
> We _highly_ recommend setting a static IP address for your server on the LAN. This becomes _necessary_ if you also intend to set up VPN or clearnet. All routers support this. Refer to AI or your router's user manual for detailed instructions.

## Tunnels Do Not Work Locally

[VPN](inbound-vpn.md) tunnels will not work when your device is on the same LAN as your server due to loopback. To test a tunnel-based connection, disconnect your device from WiFi and use mobile data instead.
