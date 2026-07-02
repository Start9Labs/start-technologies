# CGNAT (Carrier-Grade NAT)

CGNAT is a networking technique where your ISP places your home network behind an additional layer of NAT that you do not control. If your ISP uses CGNAT, several StartWRT features will not work.

## What Is CGNAT?

Normally, your router is assigned a public IP address by your ISP. This allows devices on the Internet to initiate connections to your router, which can then forward them to devices on your LAN.

With CGNAT, your ISP does _not_ give your router a public IP. Instead, many customers share a single public IP managed by the ISP's equipment. Your router's "WAN IP" is actually a private address on the ISP's internal network. Because you don't control the ISP's NAT, no one on the Internet can initiate a connection to your router.

## Who Is Affected?

CGNAT is common with:

- **Satellite Internet** — Starlink, HughesNet, Viasat
- **Cellular/fixed wireless** — T-Mobile Home Internet, Verizon Home Internet, and similar 4G/5G home broadband services
- **Some fiber and cable ISPs** — particularly in regions with IPv4 address shortages

## Impact on StartWRT

CGNAT blocks all inbound connections to your router. This significantly limits StartWRT's feature set:

- **[Inbound VPNs](inbound-vpn.md)** — VPN servers need to accept connections from the Internet. Behind CGNAT, remote devices cannot reach your router.
- **[Published Ports](published-ports.md)** — Port forwarding requires a public IP. Rules will show "Error" or "Partial" status behind CGNAT.
- **[Dynamic DNS](ddns.md)** — DDNS maps a domain to your IP, but if that IP is behind CGNAT, the domain still cannot receive inbound connections.

CGNAT does **not** affect:

- **Local network access** — Devices on your LAN connect directly, bypassing the ISP entirely.
- **[Outbound VPNs](outbound-vpn.md)** — Outbound connections are not blocked by CGNAT.
- **All other StartWRT features** — Security Profiles, Wi-Fi management, Ethernet configuration, WAN Blackout, and backups work normally.

## The Solution: StartTunnel

[StartTunnel](/start-tunnel/) is a virtual private router (VPR) — a minimal, self-hosted router that runs on a VPS with a public IP address. Your devices connect _outbound_ to the VPS, and the VPS accepts inbound connections on their behalf. Because the VPS has a real public IP, CGNAT is completely bypassed.

## How to Check

Compare your router's WAN IP with your actual public IP:

1. In StartWRT, navigate to `Internet > WAN Settings` and note your WAN IP address.

1. Visit a site like [whatismyip.com](https://whatismyip.com) from a device on the same network.

1. If the two addresses **match**, you are _not_ behind CGNAT. If they **differ**, you are likely behind CGNAT.

> [!TIP]
> Another indicator: if your router's WAN IP is in the `100.64.0.0/10` range (100.64.x.x through 100.127.x.x), that is the CGNAT address block defined by RFC 6598 and confirms you are behind CGNAT.

> [!NOTE]
> Some ISPs offer a way to opt out of CGNAT, either through a support request or by purchasing a static IP add-on. Check with your ISP before assuming CGNAT is permanent.
