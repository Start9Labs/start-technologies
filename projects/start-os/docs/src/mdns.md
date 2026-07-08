# mDNS

[Multicast DNS (mDNS)](https://en.wikipedia.org/wiki/Multicast_DNS) gives your server a `<server-name>.local` address on your LAN. The address is derived from your [server name](server-name.md) by lowercasing it, removing non-alphanumeric characters, and replacing spaces with hyphens. For example, a server named "My Cool Server" gets the mDNS address `my-cool-server.local`.

## Watch The Video

<div class="yt-video" data-id="_Sutx4vhwr0" data-title="mDNS"></div>

## How It Works

mDNS resolves your server's `.local` address to its LAN IP address without relying on a DNS server. Any device on the same local network can reach your server using this address.

> [!TIP]
> The mDNS address is useful because your router may change your server's IP address on the LAN. If that happens, the mDNS address will continue to work — even if you move or get a new router.

## Limitations

mDNS only works on the local network. It does not work over [VPN](inbound-vpn.md) or the Internet. For remote access using a custom domain, see [Private Domains](private-domains.md).
