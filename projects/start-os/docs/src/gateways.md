# Gateways

A gateway is a network interface that connects your server to the Internet. Your router is the default gateway — it is always present. You can add additional gateways using WireGuard configuration files. All gateways are managed under `System > Gateways`.

## WATCH THE VIDEO

<div class="yt-video" data-id="ZCc8sZdalNE" data-title="Gateways"></div>

## Gateway Types

Every gateway routes outbound traffic from your server to the Internet. Some gateways also accept inbound connections. StartOS automatically detects the type:

- **Inbound/outbound** — routes outbound traffic _and_ accepts inbound connections. Your home router and [StartTunnel](/start-tunnel/) (a virtual private router running on a VPS) are inbound/outbound gateways. These are used for [inbound VPN](inbound-vpn.md) access and [clearnet](clearnet.md) hosting.

- **Outbound only** — routes outbound traffic but does not accept inbound connections. Commercial VPN providers (Mullvad, ProtonVPN, etc.) are outbound-only gateways. These are used as [outbound VPNs](outbound-vpn.md).

Service interface address tables list inbound/outbound gateways, where you can enable addresses for incoming connections. Every gateway, outbound-only included, can be selected for [outbound traffic](outbound-traffic.md).

> [!NOTE]
> A StartTunnel gateway can also carry IPv6. If the tunnel subnet your server belongs to has an [IPv6 prefix delegated](/start-tunnel/ipv6.html), your server receives its own global IPv6 address (GUA) through the gateway — usable for [DualStack public domains](clearnet.md) and controlled from each interface's address list (see [Interfaces](interfaces.md)).

> [!NOTE]
> If you are running StartOS on a VPS with a public IP address, there is no router gateway. Your server's network interface is directly exposed to the Internet.

> [!WARNING]
> If your ISP uses [CGNAT](cgnat.md), your router **cannot** accept inbound connections, even with port forwarding configured. This means your router gateway is effectively outbound-only: it cannot be used for [clearnet hosting](clearnet.md), [public IP access](public-ip.md), or [inbound VPN](inbound-vpn.md). Use a [StartTunnel](/start-tunnel/) gateway instead.

## Adding a Gateway

1. Navigate to `System > Gateways` and click "Add".

1. Upload or paste a WireGuard configuration file from your VPN provider or StartTunnel instance.

   StartOS will automatically detect the gateway type:
   - Config files containing `# inbound: yes` are marked as _inbound/outbound_ gateways. StartTunnel adds this marker to the configs it generates.
   - Older StartTunnel configs are also recognized by their StartTunnel header and marked as _inbound/outbound_ gateways.
   - WireGuard configs without either marker are marked as _outbound-only_ gateways.

## Updating a Gateway's Config

To re-import a gateway's WireGuard config — for example, a StartTunnel config re-issued with new settings — open the gateway's `⋮` menu, choose "Update config", and paste or upload the new file. The config is replaced **in place**: the gateway keeps its identity, so its port forwards and private/public domains are preserved. (Re-adding via "Add" would instead create a separate gateway.)

## DNS Settings

The **DNS** column shows each gateway's effective resolver addresses, marked **(Auto)** for gateway-provided DNS or **(Fallback)** for public fallback. Choose **Edit** from the gateway's `⋮` menu to change its name and [DNS settings](dns.md) together.

## Outbound Traffic

Which gateway carries the OS's and each service's outbound connections is set under [Outbound Traffic](outbound-traffic.md).

## Secure Gateways

For an interface whose SSL StartOS terminates, the gateway's secure setting decides whether its plain-HTTP private addresses are enabled by default. Loopback and the container bridge are secure, because they never leave your server. Other gateways — your router, WiFi, a WireGuard tunnel — are insecure by default.

On an insecure gateway, each plain-HTTP address requires an explicit [command-line opt-in](interfaces.md#plain-http-private-addresses). The UI hides those addresses until they are enabled and lets you disable them again. Marking a gateway secure instead enables the default plaintext addresses server-wide: your server's LAN IP addresses, its [`.local` name](mdns.md), and [private domains](private-domains.md) on that gateway.

This setting lives on the command line. [SSH](ssh.md) into your server, then:

```bash
start-cli net gateway set-secure <GATEWAY>
```

To hand the decision back to StartOS:

```bash
start-cli net gateway unset-secure <GATEWAY>
```

`start-cli net gateway list` shows each gateway's current setting, with `(auto)` marking one StartOS decided.

> [!WARNING]
> Marking a gateway secure takes effect across the whole server immediately. Plaintext addresses without a saved disable become enabled by default on that network. Anything on that network — or, over IPv4, on any private network routed to it — can read and alter traffic, including passwords. Prefer a per-address opt-in when a client cannot use HTTPS.
>
> Mark a gateway secure only when you control every device on the network it reaches and on every private network routed to it. Leave a guest network, an office LAN, a coffee-shop WiFi, or any network carrying devices you do not manage as it is.

Marking a gateway secure never exposes anything to the public internet. An address it unlocks is offered to devices on the same network as that gateway and, over IPv4, to devices on other private networks that your router forwards to it — a second VLAN, a separate WiFi network, a routed IoT network. StartOS never opens a port on your router for it and never carries it on a [public domain](clearnet.md). An IPv6 address it unlocks is offered only to the gateway's own network. On a server whose interface holds a public IP directly — a VPS, or a modem in bridge mode — that network is your provider's network, which contains machines you do not control.
