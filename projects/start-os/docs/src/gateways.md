# Gateways

A gateway is a network interface that connects your server to the Internet. Your router is the default gateway — it is always present. You can add additional gateways using WireGuard configuration files. All gateways are managed under `System > Gateways`.

## WATCH THE VIDEO

<div class="yt-video" data-id="ZCc8sZdalNE" data-title="Gateways"></div>

## Gateway Types

Every gateway routes outbound traffic from your server to the Internet. Some gateways also accept inbound connections. StartOS automatically detects the type:

- **Inbound/outbound** — routes outbound traffic _and_ accepts inbound connections. Your home router and [StartTunnel](/start-tunnel/) (a virtual private router running on a VPS) are inbound/outbound gateways. These are used for [inbound VPN](inbound-vpn.md) access and [clearnet](clearnet.md) hosting.

- **Outbound only** — routes outbound traffic but does not accept inbound connections. Commercial VPN providers (Mullvad, ProtonVPN, etc.) are outbound-only gateways. These are used as [outbound VPNs](outbound-vpn.md).

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
   - **StartTunnel** config files are recognized and marked as _inbound/outbound_ gateways.
   - **All other** WireGuard configs are marked as _outbound-only_ gateways.

## Updating a Gateway's Config

To re-import a gateway's WireGuard config — for example, a StartTunnel config re-issued with new settings — open the gateway's `⋮` menu, choose "Update config", and paste or upload the new file. The config is replaced **in place**: the gateway keeps its identity, so its port forwards and private/public domains are preserved. (Re-adding via "Add" would instead create a separate gateway.)

## Secure Gateways

Some service interfaces are served without SSL — plain HTTP, or another protocol carrying no encryption of its own. StartOS offers those addresses only on a network it treats as secure. Loopback and the container bridge are secure, because they never leave your server. Every other gateway — your router, WiFi, a WireGuard tunnel — is not, so a service's non-SSL addresses are neither listed nor reachable through it.

Marking a gateway secure tells StartOS that you trust the network on the other side of it. A service's non-SSL addresses are then offered there: your server's LAN IP addresses, its [`.local` name](mdns.md), and any [private domains](private-domains.md) you have added on that gateway.

This setting lives on the command line. [SSH](ssh.md) into your server, then:

```bash
start-cli net gateway set-secure <GATEWAY>
```

To mark a network as never secure, pass `false`; to hand the decision back to StartOS, unset it:

```bash
start-cli net gateway set-secure <GATEWAY> false
start-cli net gateway unset-secure <GATEWAY>
```

`start-cli net gateway list` shows each gateway's current setting, with `(auto)` marking one StartOS decided.

> [!WARNING]
> Any device on a network you mark secure can read and alter traffic to a non-SSL address on it, including passwords typed into a service's web interface. Mark a gateway secure only when you control every device on that network. Leave a guest network, an office LAN, a coffee-shop WiFi, or any network carrying devices you do not manage as it is.

The public internet is never secure, whatever a gateway is set to: a non-SSL address is never forwarded to the WAN, and never carried by a [public domain](clearnet.md).
