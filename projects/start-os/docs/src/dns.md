# DNS

StartOS resolves domain names through its own DNS server. The OS and services use their selected outbound gateways for DNS, with gateway-provided settings or their own static overrides.

## WATCH THE VIDEO

<div class="yt-video" data-id="_vnAqNTaBwM" data-title="DNS"></div>

## Gateway DNS

Each gateway has its own DNS settings on `System > Gateways`. The **DNS** column shows its effective server addresses, with **(Auto)** for gateway-provided DNS or **(Fallback)** for public fallback. Open the gateway's `⋮` menu and choose **Edit** to change its name and DNS settings.

In **Auto** mode, StartOS uses DNS servers supplied by that gateway: [DHCP](https://en.wikipedia.org/wiki/Dynamic_Host_Configuration_Protocol) for a physical connection, or the WireGuard configuration for a tunnel.

> [!WARNING]
> When a gateway supplies no DNS servers, StartOS uses **Cloudflare's public resolvers at `1.1.1.1:53` and `[2606:4700:4700::1111]:53`**. The DNS column marks these addresses **(Fallback)**, and the gateway's editor identifies Cloudflare. These queries travel through the selected gateway, including when that gateway is a VPN.

Which gateway a requester uses, and whether it uses that gateway's DNS or its own static servers, is set on its [Outbound Traffic](outbound-traffic.md) page. A disconnected selected gateway blocks its assigned DNS queries.

Host lookups of configured WireGuard endpoint hostnames use the automatic physical connection's DNS settings, allowing tunnels to reconnect. This exception applies to those exact endpoint names; service-originated lookups still follow the service's outbound gateway.

## Setup and Recovery DNS

During setup and early diagnostics, before stored network settings load, the same StartOS DNS server serves host lookups through the automatic physical gateway. It uses that connection's DNS servers, or the Cloudflare public fallback above. Service queries cannot use bootstrap DNS.

Once stored network settings load, DNS adopts the saved gateway choices and overrides. A later initialization failure does not restore the physical-gateway bootstrap policy: diagnostics keep using the loaded policy, including blocking queries assigned to an unavailable gateway. The host-only WireGuard endpoint exception above remains available.

## Static DNS Servers

Under `System > Gateways`, choose **Edit** from a gateway's `⋮` menu, set **DNS** to **Static**, and provide one to three DNS servers. Each address may include a port; the default is `53`. For IPv6 with a port, use brackets, for example `[2606:4700:4700::1111]:53`. Click "Save" to apply the override to that gateway. Choose **Auto** and save to restore gateway-provided DNS or the public fallback.

DNS overrides remain saved while a gateway is disconnected. When upgrading from system-wide static DNS settings, StartOS applies those servers to every known gateway that does not already have its own override. Newly added gateways start in Auto mode.

> [!NOTE]
> If you want every device on your network to use a specific DNS provider, configure it in your router. A gateway's settings in StartOS affect requests routed through that gateway by StartOS and its services.

## Private Domains

StartOS runs its own DNS server to resolve [private domains](private-domains.md) on your network. For setup details, see [DNS for Private Domains](private-domains.md#dns-for-private-domains).
