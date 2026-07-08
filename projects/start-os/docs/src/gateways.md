# Gateways

A gateway is a network interface that connects your server to the Internet. Your router is the default gateway — it is always present. You can add additional gateways using WireGuard configuration files. All gateways are managed under `System > Gateways`.

## WATCH THE VIDEO

<div class="yt-video" data-id="ZCc8sZdalNE" data-title="Gateways"></div>

## Gateway Types

Every gateway routes outbound traffic from your server to the Internet. Some gateways also accept inbound connections. StartOS automatically detects the type:

- **Inbound/outbound** — routes outbound traffic _and_ accepts inbound connections. Your home router and [StartTunnel](/start-tunnel/) (a virtual private router running on a VPS) are inbound/outbound gateways. These are used for [inbound VPN](inbound-vpn.md) access and [clearnet](clearnet.md) hosting.

- **Outbound only** — routes outbound traffic but does not accept inbound connections. Commercial VPN providers (Mullvad, ProtonVPN, etc.) are outbound-only gateways. These are used as [outbound VPNs](outbound-vpn.md).

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
