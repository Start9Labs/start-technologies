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
> For Cloudflare, the DNS record must already exist in your zone — StartWRT updates it but does not create it, so add the A record in the Cloudflare dashboard first. The API token needs **Zone → Read** and **DNS → Edit** permissions for the zone. Proxied (orange-cloud) records work: the router reads the registered address through the Cloudflare API rather than through DNS, which would only ever show the proxy's address.

## Checking Your DDNS Status

The Dynamic DNS tab on the WAN Settings page shows the current status of your dynamic DNS configuration: whether it is enabled, the provider, and the hostname. The router updates the record whenever the WAN connection comes up — after a modem reboot or a PPPoE reconnect — and on a periodic check in between, so a changed address is registered within minutes.

## Using a Trusted Certificate

Browsers trust the router's own certificate only on devices that have [trusted your Root CA](trust-ca.md). To open the web interface at your DDNS hostname from any browser without a warning, the router can get a certificate for the hostname from Let's Encrypt:

1. Set [Remote Access](settings.md#remote-access) to "Always". Let's Encrypt confirms you control the hostname by connecting to your router on port 443 from the Internet. If your router sits behind another router, forward port 443 on that router to it; behind [CGNAT](cgnat.md), Let's Encrypt cannot reach it.

1. On the Dynamic DNS tab, toggle **Use a Let's Encrypt certificate** on and click "Save".

The Dynamic DNS summary shows the certificate's expiry date once it is issued, or why the last attempt failed; failures also appear in the [Activity](settings.md#activity) log. The router retries a failed attempt and renews the certificate before it expires. Until a certificate is issued, the hostname is served with one from your Root CA.

> [!NOTE]
> A proxied (orange-cloud) Cloudflare record cannot get a Let's Encrypt certificate this way: Cloudflare answers on port 443 in place of your router.

> [!NOTE]
> Let's Encrypt publishes every certificate it issues in public Certificate Transparency logs, so your hostname is listed there.
