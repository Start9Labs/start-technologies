# Private Domains

A private domain works like your server's [mDNS address](mdns.md), except it also works over [VPN](inbound-vpn.md) and it can be _anything_. It can be a real domain you control, a made up domain, or even a domain controlled by someone else.

Private domains can be added to any gateway: wired (Ethernet), wireless (WiFi), or WireGuard ([StartTunnel](/start-tunnel/)). They can only be accessed when connected to the same network as your server — your LAN, or a VPN / tunnel — and they require trusting your server's Root CA.

## Adding a Private Domain

> [!NOTE]
> How a private domain is made to resolve depends on the gateway type. On an **Ethernet** or **WiFi** gateway, you point your router's DNS at this server — or, if the router is a [StartWRT](/start-wrt/) router, simply enable **Allow DNS record publishing** for this server. On a **WireGuard ([StartTunnel](/start-tunnel/))** gateway, you either enable **DNS Injection** for this server in StartTunnel or point the tunnel subnet's DNS at this server. See [DNS for Private Domains](#dns-for-private-domains) below.

1. For an Ethernet or WiFi gateway, assign a static IP address to your server on the LAN if you haven't already (refer to your router's user manual). A WireGuard gateway already uses a fixed tunnel address, so this step does not apply.

1. On the service interface page, click "Add Domain" on the desired gateway table and select "Private Domain".

1. Enter a fully qualified domain name. It can be _anything_. For example: `domain.com`, `private.domain.internal`, `nextcloud.private`, `nextcloud.fake-tld`, or `facebook.com`.

   > [!TIP]
   > Any name works, but the choice has consequences. A subdomain of a domain you own (`nextcloud.example.com`) is the most forgiving: a lookup that escapes to public DNS before your setup is finished is forgotten within minutes, and the name can never collide with a real website. If you would rather not own a domain, prefer an ending under `.internal` (reserved worldwide for exactly this) or `home.arpa` over an invented ending like `.private` — an invented ending can become a real TLD later (`.dev` did, breaking private setups everywhere), and a public lookup of a nonexistent ending is cached as "no such domain" for up to a day on some devices (see [If a device still can't resolve the domain](#if-a-device-still-cant-resolve-the-domain)). A domain someone else controls resolves privately only on devices that use this network's DNS — anything else, such as a browser with DNS-over-HTTPS enabled or a device that leaves the network, reaches the real site instead. If the router is a StartWRT router, names ending in `.lan` cannot be used — StartWRT reserves that zone for device hostnames — and neither can a domain that already resolves on the Internet to anywhere but that router.

1. Click "Save".

1. StartOS will automatically test your DNS configuration. If the test passes, the domain is ready to use. If it fails, a setup modal will appear with instructions to configure your DNS server and the ability to re-test.

## DNS for Private Domains

A private domain resolves only when the gateway's DNS serves its record. StartOS tests this automatically when you add a private domain and guides you through the setup if needed. How you configure it depends on the gateway type.

### Ethernet & WiFi gateways

On a [StartWRT](/start-wrt/) router, enable **Allow DNS record publishing** for this server on its device page; StartOS then publishes the domain's record to the router automatically. Enable the toggle **before** adding the domain. StartWRT refuses names ending in `.lan`, and a domain that already resolves on the Internet to anywhere but the router.

On any other router, set StartOS as the router's primary DNS server. All routers support this feature. Refer to your router's user manual for detailed instructions.

> [!WARNING]
> It is possible that StartOS is already using your router for DNS. In this case, you cannot instruct your router to use StartOS for DNS, as this would be circular. If StartOS detects a potential circular DNS situation, it will warn you. To resolve this, switch to [static DNS servers](dns.md#static-dns-servers) so StartOS no longer relies on your router.

### WireGuard (StartTunnel) gateways

StartTunnel runs the DNS resolver for the tunnel, so you have two options:

- **DNS Injection** (simplest): in StartTunnel, enable the **DNS Injection** toggle for this server. StartOS then publishes the domain's record directly to StartTunnel's resolver — there is nothing else to configure.
- **Point the subnet at this server**: in StartTunnel, set the DNS for this server's subnet to this server's tunnel address. StartTunnel then forwards DNS for the subnet to StartOS.

> [!TIP]
> If your private domain is a real domain that you control, you can alternatively configure its DNS record at your registrar to resolve to your server's address on that network — its _LAN IP address_ for an Ethernet or WiFi gateway, or its _tunnel address_ for a WireGuard gateway. In this case, the StartOS DNS server is not needed.

### If a device still can't resolve the domain

A device that looked the domain up _before_ DNS was configured may have cached the "no such domain" answer it received from public DNS. The cache expires on its own — within minutes for a subdomain of a real domain, but up to **24 hours** for an invented ending, since that answer comes from the DNS root zone. To clear it immediately:

- **macOS**: `sudo dscacheutil -flushcache; sudo killall -HUP mDNSResponder`
- **Windows**: `ipconfig /flushdns`
- **Linux** (systemd-resolved): `resolvectl flush-caches`

Then restart the browser — browsers keep a DNS cache of their own.

If the command line resolves the domain but a browser still can't, the browser is using **DNS-over-HTTPS** (sometimes labeled "Secure DNS"), which asks a public resolver directly and bypasses your network's DNS entirely. Turn it off in the browser's privacy settings, or add an exception for your domain. Firefox's _default-enabled_ DNS-over-HTTPS turns itself off automatically on networks behind a StartWRT router; a browser where you enabled it explicitly must be configured by hand.

## Split DNS: the Same Domain, Public and Private

You can add the _same_ domain as both a [clearnet](clearnet.md) (public) domain on a StartTunnel gateway and a private domain on an Ethernet or WiFi gateway. StartOS serves it as split DNS:

- When you are on your LAN or connected over [VPN](inbound-vpn.md), StartOS resolves the domain to your server's local IP address, so traffic stays on your network at full LAN speed.
- When you are away, the same domain resolves through public DNS to your StartTunnel gateway, so the service is reachable over the internet.

A [StartWRT](/start-wrt/) router refuses to publish the private side, since public DNS already resolves the domain to the StartTunnel gateway; set StartOS as the router's DNS server instead.

It's the same domain and the same TLS certificate either way, with no [hairpin routing](https://en.wikipedia.org/wiki/Hairpinning) (LAN traffic looping out to the gateway and back). The two sides are served separately, so the local side keeps working on your server's Root CA certificate while a public certificate for the same domain has yet to be issued.

> [!TIP]
> This is especially useful for services that embed their access URL in generated links, such as Nextcloud or Immich share links. Configure the service with the public domain so the links work for external recipients, and you'll still get direct LAN-speed access when you're home.
