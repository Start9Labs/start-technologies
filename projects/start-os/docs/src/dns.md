# DNS

This page covers how StartOS resolves domain names and when you might need to change the defaults.

## WATCH THE VIDEO

<div class="yt-video" data-id="_vnAqNTaBwM" data-title="DNS"></div>

## DHCP

By default, StartOS obtains its DNS servers from your router via [DHCP](https://en.wikipedia.org/wiki/Dynamic_Host_Configuration_Protocol). For most users, the default settings require no changes.

## Static DNS Servers

To view or change the DNS servers StartOS uses, navigate to `System > DNS`. To override the defaults, select "Static" and provide up to three DNS servers in order of preference.

> [!NOTE]
> If you want to use a specific DNS provider (such as Cloudflare or Quad9), it is generally better to configure it in your router so that all devices on your network benefit, not just your server.

## When DNS is not working

If your DNS servers do not answer, StartOS still reaches anything addressed by IP — so
the server looks healthy — while every service that connects to the internet by name
fails. Symptoms are unhelpfully varied: a service that cannot reach an update server, a
Bitcoin node whose I2P router never finds peers, a download that never starts.

The DNS page checks this for you and shows a warning when StartOS cannot resolve a name.

This is most common on DHCP, where the servers come from your router. A router may filter
DNS, hand out a server it does not actually run, or serve DNS only to certain clients. If
you see the warning, select **Static** and enter a resolver you trust — your router's own
IP if it does run one, or a public resolver such as `1.1.1.1` or `9.9.9.9`.

## Private Domains

StartOS runs its own DNS server to resolve [private domains](private-domains.md) on your network. For setup details, see [DNS for Private Domains](private-domains.md#dns-for-private-domains).
