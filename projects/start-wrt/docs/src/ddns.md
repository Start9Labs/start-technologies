# Dynamic DNS

Dynamic DNS (DDNS) maps a stable domain name to your home IP address, even when your ISP changes it. This is essential for remote access features like [Inbound VPNs](inbound-vpn.md) and [Published Ports](published-ports.md), which require external devices to find your router on the Internet.

## Why You Need DDNS

Most home Internet connections have a dynamic IP address that can change without warning. When your IP changes, any remote VPN clients or port forwarding rules pointing to the old IP stop working. DDNS automatically updates a domain name to point to your current IP, so remote connections keep working.

## Setting Up DDNS

StartWRT supports the following DDNS providers:

- Cloudflare
- DuckDNS
- DynDNS
- FreeDNS
- No-IP

To set up DDNS:

1. Navigate to `Internet > WAN Settings > Dynamic DNS`.

1. Toggle **Enable Dynamic DNS** on and select your provider.

1. Fill in the fields your provider requires: DynDNS and No-IP need a username and password; DuckDNS and FreeDNS need an API token; Cloudflare needs an API token and the zone — the domain registered with Cloudflare, e.g. `example.com`. Every provider also needs the hostname you have registered with it; for Cloudflare, the hostname must be the zone itself or a name under it, such as `router.example.com`.

1. Click "Save".

> [!NOTE]
> For Cloudflare, the DNS record must already exist in your zone — StartWRT updates it but does not create it, so add the A record in the Cloudflare dashboard first. The API token needs **Zone → Read** and **DNS → Edit** permissions for the zone.

## Checking Your DDNS Status

The Dynamic DNS tab on the WAN Settings page shows the current status of your dynamic DNS configuration: whether it is enabled, the provider, and the hostname.
