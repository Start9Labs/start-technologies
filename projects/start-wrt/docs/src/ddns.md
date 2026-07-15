# Dynamic DNS

Dynamic DNS (DDNS) maps a stable domain name to your home IP address, even when your ISP changes it. This is essential for remote access features like [Inbound VPNs](inbound-vpn.md) and [Published Ports](published-ports.md), which require external devices to find your router on the Internet.

## Why You Need DDNS

Most home Internet connections have a dynamic IP address that can change without warning. When your IP changes, any remote VPN clients or port forwarding rules pointing to the old IP stop working. DDNS automatically updates a domain name to point to your current IP, so remote connections keep working.

## Start9 Dynamic DNS

StartWRT includes free integration with Start9's Dynamic DNS service. No account is required.

1. Navigate to `Internet > WAN Settings > Dynamic DNS`.

1. Toggle **Enable Dynamic DNS** on. **Start9** is already selected as the provider.

1. Click "Save". A unique domain name will be assigned to your router automatically.

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

1. Navigate to `Internet > WAN Settings > Dynamic DNS`.

1. Toggle **Enable Dynamic DNS** on and select your provider.

1. Fill in the fields your provider requires: DynDNS and No-IP need a username and password; DuckDNS and FreeDNS need an API token; Cloudflare needs an API token and the Zone ID of your domain. Every third-party provider also needs the hostname you have registered with it.

1. Click "Save".

## Checking Your DDNS Status

The Dynamic DNS tab on the WAN Settings page shows the current status of your dynamic DNS configuration: whether it is enabled, the provider, and the hostname.
