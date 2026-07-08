# CGNAT (Carrier-Grade NAT)

CGNAT is a networking technique where your ISP places your home network behind an additional layer of NAT that you do not control. This page explains what CGNAT is, how to tell if you're behind it, and what it means for self-hosting on StartOS.

## Watch The Video

<div class="yt-video" data-id="nMGNKv8Mor0" data-title="CGNAT"></div>

## What Is CGNAT?

Normally, your router is assigned a public IP address by your ISP. This allows devices on the Internet to initiate connections to your router, which can then forward them to your server via port forwarding.

With CGNAT, your ISP does _not_ give your router a public IP. Instead, many customers share a single public IP managed by the ISP's equipment. Your router's "WAN IP" is actually a private address on the ISP's internal network. Because you don't control the ISP's NAT, you cannot forward ports, and no one on the Internet can initiate a connection to your router.

## Who Is Affected?

CGNAT is common with:

- **Satellite Internet** — Starlink, HughesNet, Viasat
- **Cellular/fixed wireless** — T-Mobile Home Internet, Verizon Home Internet, and similar 4G/5G home broadband services
- **Some fiber and cable ISPs** — particularly in regions with IPv4 address shortages

If you're unsure whether your ISP uses CGNAT, see [How to Check](#how-to-check) below.

## Impact on StartOS

CGNAT blocks all inbound connections to your router. This means your router gateway **cannot** be used for:

- **[Clearnet hosting](clearnet.md)** — Public domains require port forwarding, which CGNAT prevents.
- **[Public IP access](public-ip.md)** — Exposing a service by IP address and port requires inbound connections.
- **[Inbound VPN](inbound-vpn.md)** — A router-based VPN server needs to accept connections from the Internet.

CGNAT does **not** affect:

- **[LAN access](lan.md)** — Devices on your local network connect directly, bypassing the ISP entirely.
- **[Tor](tor.md)** — Tor routes through its overlay network using only outbound connections.
- **[Outbound VPN](outbound-vpn.md)** — Outbound connections are not blocked by CGNAT.

## The Solution: StartTunnel

[StartTunnel](/start-tunnel/) is a virtual private router (VPR) — a minimal, self-hosted router that runs on a VPS with a public IP address. Your server connects _outbound_ to the VPS, and the VPS accepts inbound connections on your behalf — just like a home router, but in the cloud. Because the VPS has a real public IP, CGNAT is completely bypassed.

With a StartTunnel gateway, you get full [clearnet hosting](clearnet.md), [public IP access](public-ip.md), and [inbound VPN](inbound-vpn.md) — regardless of your ISP's network configuration.

## How to Check

Compare your router's WAN IP with your actual public IP:

1. Log into your router's admin panel and find its WAN IP address (sometimes called "Internet IP" or "External IP").

1. Visit a site like [whatismyip.com](https://whatismyip.com) from a device on the same network.

1. If the two addresses **match**, you are _not_ behind CGNAT. If they **differ**, you are likely behind CGNAT.

> [!TIP]
> Another indicator: if your router's WAN IP is in the `100.64.0.0/10` range (100.64.x.x through 100.127.x.x), that is the CGNAT address block defined by RFC 6598 and confirms you are behind CGNAT.

> [!NOTE]
> Some ISPs offer a way to opt out of CGNAT, either through a support request or by purchasing a static IP add-on. Check with your ISP before assuming CGNAT is permanent.
