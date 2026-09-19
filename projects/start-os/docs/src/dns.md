# DNS

This page covers how StartOS resolves domain names and when you might need to change the defaults.

## WATCH THE VIDEO

<div class="yt-video" data-id="_vnAqNTaBwM" data-title="DNS"></div>

## DHCP

By default, StartOS obtains its DNS servers from your router via [DHCP](https://en.wikipedia.org/wiki/Dynamic_Host_Configuration_Protocol). For most users, the default settings require no changes. If the network provides no usable DNS server, StartOS keeps service name resolution available through its built-in Cloudflare fallback at `1.1.1.1`.

## Static DNS Servers

To view or change the DNS servers StartOS uses, navigate to `System > DNS`. To override the defaults, select "Static" and provide up to three DNS servers in order of preference.

> [!NOTE]
> If you want to use a specific DNS provider (such as Cloudflare or Quad9), it is generally better to configure it in your router so that all devices on your network benefit, not just your server.

## Private Domains

StartOS runs its own DNS server to resolve [private domains](private-domains.md) on your network. For setup details, see [DNS for Private Domains](private-domains.md#dns-for-private-domains).
