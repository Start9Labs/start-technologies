# Dynamic DNS

Dynamic DNS (DDNS) maps a stable domain name to your home IP address, even when your ISP changes it. This is essential for remote access features like [Inbound VPNs](inbound-vpn.md) and [Published Ports](published-ports.md), which require external devices to find your router on the Internet.

## Why You Need DDNS

Most home Internet connections have a dynamic IP address that can change without warning. When your IP changes, any remote VPN clients or port forwarding rules pointing to the old IP stop working. DDNS automatically updates a domain name to point to your current IP, so remote connections keep working.

## Start9 Dynamic DNS

StartWRT includes free integration with Start9's Dynamic DNS service. No account is required.

1. Navigate to `Internet > WAN Settings > DDNS`.

1. Select **Start9** as the provider.

1. Toggle **Enable Dynamic DNS** on and click "Save". A unique domain name will be assigned to your router automatically.

> [!TIP]
> The Start9 DDNS domain is all you need for VPN access. You do not need to own a custom domain name.

## Other Providers

StartWRT also supports third-party DDNS providers:

- Cloudflare
- DuckDNS
- DynDNS
- FreeDNS
- No-IP

To use a third-party provider:

1. Navigate to `Internet > WAN Settings > DDNS`.

1. Select the provider and enter your credentials or API token.

1. Enter the domain name or hostname you have registered with the provider.

1. Click "Save".

## Checking Your DDNS Status

The DDNS section on the WAN Settings page shows the current status of your dynamic DNS configuration: whether it is enabled, the provider, and the hostname.
