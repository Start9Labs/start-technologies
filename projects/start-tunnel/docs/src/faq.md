# FAQ

Answers to common questions about StartTunnel's security model, compatibility, VPS requirements, and what happens if Start9 goes away.

## Can anyone else see my traffic?

No. Published ports operate at Layer 3/4 (iptables DNAT), meaning the VPS rewrites IP headers and forwards packets without inspecting payloads. If your service uses HTTPS, TLS terminates at the service itself — the VPS never sees plaintext. For VPN traffic between devices, WireGuard provides end-to-end encryption. Since you own the VPS, there is no third party in the data path.

## Do I need a WireGuard client on my devices?

Only for private access — phones, laptops, and other devices connecting to your server over the VPN. For clearnet hosting (exposing ports to the public Internet), no WireGuard client is needed on the devices accessing your services.

## Can I run other services on the same VPS?

No. StartTunnel manages its own firewall rules and disables UFW. It is designed to be the sole application on the VPS.

## Does StartTunnel work behind CGNAT?

Yes. WireGuard clients initiate outbound UDP connections, so CGNAT is not a problem for connecting devices to the VPN. Publishing ports still works because public traffic arrives at the VPS's public IP.

## What if I forget my password?

SSH into your VPS and run:

```
start-tunnel auth reset-password
```

## What if Start9 goes away?

StartTunnel keeps working. It is fully self-hosted with no dependency on Start9 infrastructure. There is no coordination server, no telemetry, and no phone-home. The binary runs entirely on your VPS.

## How do I remove StartTunnel?

StartTunnel is designed to run on a dedicated VPS. To remove it, simply destroy the VPS through your hosting provider. All WireGuard keys and configuration are stored on the VPS and will be removed with it.

## What VPS providers work with StartTunnel?

Any provider that offers Debian 13 with root access and a **dedicated public IPv4 address**. Common choices include Hetzner, DigitalOcean, Linode, Vultr, and OVH. Budget VPS providers (~$5/mo) work fine — StartTunnel has minimal resource requirements.

> [!WARNING]
> StartTunnel's published ports (clearnet hosting) require a dedicated public IPv4 address. Shared IPv4 addresses (CGNAT, shared NAT, load-balanced IPs) will not work. Some budget providers and IPv6-only tiers do not include a dedicated IPv4 — confirm with your provider before purchasing.

Some providers (AWS, Google Cloud, Azure, Oracle Cloud, IONOS) have cloud-panel firewalls that block WireGuard (UDP 51820) by default. See [Installing — Cloud firewalls](installing.md#cloud-firewalls) for setup instructions.

## Does StartTunnel work on an IPv6-only VPS?

Partially. The WireGuard tunnel itself works over IPv6, so devices with IPv6 connectivity can join your private VPN and reach each other through the VPS. However, **published ports for clearnet hosting are IPv4-only** and cannot be used on an IPv6-only VPS. Additionally, any device joining the VPN must have IPv6 connectivity on its current network — most modern carriers and home ISPs are dual-stack, but some are still IPv4-only. For clearnet hosting, choose a VPS with a dedicated public IPv4 address.

## Does StartTunnel provide DDoS protection?

No. Your VPS IP is exposed on published ports. Use your VPS provider's built-in DDoS protection, or place a CDN in front if needed. See the [Architecture](./architecture.md) page for a full comparison of trade-offs.
