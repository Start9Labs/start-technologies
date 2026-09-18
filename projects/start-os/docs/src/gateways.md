# Gateways

A gateway is a network interface that connects your server to the Internet. Your router is the default gateway — it is always present. You can add additional gateways using WireGuard configuration files. All gateways are managed under `System > Gateways`.

## WATCH THE VIDEO

<div class="yt-video" data-id="ZCc8sZdalNE" data-title="Gateways"></div>

## Gateway Types

Every gateway routes outbound traffic from your server to the Internet. Some gateways also accept inbound connections. StartOS automatically detects the type:

- **Inbound/outbound** — routes outbound traffic _and_ accepts inbound connections. Your home router and [StartTunnel](/start-tunnel/) (a virtual private router running on a VPS) are inbound/outbound gateways. These are used for [inbound VPN](inbound-vpn.md) access and [clearnet](clearnet.md) hosting.

- **Outbound only** — routes outbound traffic but does not accept inbound connections. Commercial VPN providers (Mullvad, ProtonVPN, etc.) are outbound-only gateways. These are used as [outbound VPNs](outbound-vpn.md).

Service interface address tables list inbound/outbound gateways, where you can enable addresses for incoming connections. Outbound-only gateways remain available in the system-wide and per-service outbound gateway selectors.

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

## WAN IP

A gateway's WAN IP is the public address the Internet reaches your server at through it. StartOS discovers it by asking your router over UPnP, and otherwise by making an outbound request to an echo service and reading back the address it appears to come from. That address is what [public IP access](public-ip.md), [clearnet](clearnet.md) domains, and the port-forwarding rules StartOS shows you are all built on.

Discovery answers "where does my outbound traffic come from", which is the wrong question whenever outbound and inbound traffic take different paths. The usual case is a router that sends all outbound traffic through a commercial VPN while inbound connections still arrive on your real WAN address through port forwards. StartOS then reports the VPN exit address, and every address derived from it points somewhere your server is not.

To correct it, open the gateway's `⋮` menu under `System > Gateways` and choose "Edit WAN IP". The dialog shows the address StartOS detected and takes the one you enter instead; "Reset to detected" clears it again. The address must be a public IPv4 — a private, [CGNAT](cgnat.md), or otherwise unroutable address is rejected, because inbound connections from the Internet cannot arrive on one. An outbound-only gateway has no WAN IP to pin, since nothing arrives through it.

Setting it changes every address derived from the gateway at once: the public addresses offered for each service interface, and the port-forwarding rules StartOS tells you to add. Discovery keeps running while the override is set, so the detected address stays visible in the dialog and clearing the override restores it.

The same setting is on the command line:

```bash
start-cli net gateway set-wan-ip <GATEWAY> <IP>
start-cli net gateway unset-wan-ip <GATEWAY>
```

`start-cli net gateway list` marks an address you set as `(manual)`.

## Secure Gateways

Some service interfaces are served without SSL — plain HTTP, or another protocol carrying no encryption of its own. StartOS offers those addresses only on a network it treats as secure. Loopback and the container bridge are secure, because they never leave your server. Every other gateway — your router, WiFi, a WireGuard tunnel — is not, so a service interface bound without SSL is neither listed nor reachable through it.

Marking a gateway secure tells StartOS that you trust the network on the other side of it. Those addresses are then offered there: your server's LAN IP addresses, its [`.local` name](mdns.md), and any [private domains](private-domains.md) you have added on that gateway.

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
> This is one switch for the whole server, and it takes effect immediately. Every service you have installed that has a non-SSL address gains it on that network at once, and those addresses are enabled as soon as they are offered — there is no per-service confirmation. A service you normally reach over HTTPS may still have a plaintext leg, and anything on that network — or, over IPv4, on any private network routed to it — can read and alter traffic to it, including the passwords you type into it.
>
> Mark a gateway secure only when you control every device on the network it reaches and on every private network routed to it. Leave a guest network, an office LAN, a coffee-shop WiFi, or any network carrying devices you do not manage as it is.

Marking a gateway secure never exposes anything to the public internet. An address it unlocks is offered to devices on the same network as that gateway and, over IPv4, to devices on other private networks that your router forwards to it — a second VLAN, a separate WiFi network, a routed IoT network. StartOS never opens a port on your router for it and never carries it on a [public domain](clearnet.md). An IPv6 address it unlocks is offered only to the gateway's own network. On a server whose interface holds a public IP directly — a VPS, or a modem in bridge mode — that network is your provider's network, which contains machines you do not control.
