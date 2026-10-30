# FAQ

Answers to common questions about StartWRT's features, security model, and compatibility.

## What is StartWRT?

StartWRT is a router operating system built on OpenWrt, designed specifically for home-based self-hosting. It replaces traditional networking concepts (VLANs, firewall rules, routing tables) with [Security Profiles](security-profiles.md) — a simple model where how a device connects determines what it can access.

## How is StartWRT different from stock OpenWrt?

Stock OpenWrt exposes raw networking primitives through the LuCI interface, requiring users to understand VLANs, firewall zones, and routing tables. StartWRT abstracts all of this behind Security Profiles and provides a modern web interface that makes advanced features accessible without CLI expertise. Under the hood, StartWRT still uses OpenWrt's networking stack — the difference is entirely in the management layer.

## How does multi-password Wi-Fi work?

StartWRT uses WPA2's identity PSK feature. A single SSID (`StartWRT`) accepts multiple passwords, each mapped to a different Security Profile. When a device connects, the router identifies which password was used and places the device on the corresponding VLAN and subnet automatically. See [Wi-Fi](wifi.md) for details.

## Is VPN chaining really more private?

Yes, with caveats. VPN chaining routes traffic through multiple providers so that no single provider sees both your identity (home IP) and your destination. However, if the providers collaborate or are compelled by law enforcement across jurisdictions, correlation is still theoretically possible. For most users, the practical benefit is significant — especially when chaining providers in different legal jurisdictions. See [Outbound VPNs](outbound-vpn.md) for setup instructions.

## Does StartWRT work with my ISP?

StartWRT supports DHCP, static IP, and PPPoE WAN connections, which covers the vast majority of ISPs. If your ISP uses CGNAT, you can still use all local features, but inbound connections (VPN servers, port forwarding) will not work. See [CGNAT](cgnat.md) to learn more and check if you are affected.

## Can I still use the OpenWrt CLI?

Yes. StartWRT is built on OpenWrt, and the full CLI is accessible over [SSH](ssh.md). You can use `apk` to install packages, edit UCI files directly, and run standard Linux networking tools. Changes made via the CLI are respected by the web interface.

## What happens if I forget my admin password?

You have two options:

1. **Factory reset** — Perform a [factory reset](factory-reset.md) from the web interface (if you are still logged in). This wipes all settings; Wi-Fi comes back up with the sticker password.
2. **Reflash** — Boot from a microSD card and choose "Keep settings" to reinstall the firmware while preserving settings. You will be prompted to create a new admin password. See [Installing StartWRT](installing.md).

## What if I lose my Wi-Fi sticker password?

The Wi-Fi password is printed on the sticker on the bottom of the router and stored in the router's EEPROM — it can also be displayed in the StartWRT GUI on the WiFi tab as the Admin Profile 'Default' label. The EEPROM value is only re-read on a [factory reset](factory-reset.md); if you have replaced the **Default** password with your own, that new password is what's in effect. On a DIY or unprogrammed board with no EEPROM Wi-Fi password, connect via Ethernet and set one in the web interface. See [Installing StartWRT](installing.md#diy-and-unprogrammed-boards) for the full procedure.

## Why doesn't one of my profiles have IPv6 Internet access?

If your ISP delegates only a single IPv6 prefix (for example, a `/64`), that prefix is assigned to your primary LAN. Non-admin [Security Profiles](security-profiles.md) routed **Direct** to the Internet then receive only a local-only ULA address and have no global IPv6 connectivity — IPv4 still works normally. To give such a profile global IPv6, route it through an IPv6-capable [Outbound VPN](outbound-vpn.md), or ask your ISP for a larger prefix delegation (such as a `/56` or `/48`).

## Does StartWRT phone home or collect telemetry?

No. StartWRT has no telemetry, no analytics, and no phone-home behavior. For full transparency, these are the outbound connections the router itself initiates:

- **Time synchronization (NTP)** — Enabled by default. The router keeps its clock accurate by querying the `openwrt.pool.ntp.org` servers.
- **Update checks** — Whenever you load the web interface while logged in, it checks for newer firmware releases so it can notify you. _Installing_ an update always requires your explicit action — StartWRT never updates automatically, and you can also update entirely offline by reflashing from a microSD card. See [Updating](updating.md).
- **Dynamic DNS** — Only if you have configured a [DDNS](ddns.md) provider, the router periodically registers its WAN address with that provider.

## Can I use StartWRT with StartOS?

Absolutely. StartWRT and [StartOS](/start-os/) are complementary products. StartOS runs your self-hosted services; StartWRT handles the networking. Together, they provide a complete self-hosting stack with proper network isolation, VPN access, and port forwarding — all without touching the command line. Turn on **Allow automatic port forwarding** for the server on its [device page](devices.md#device-detail-page) and StartOS opens and renews the ports its services need by itself, including [hostname routes](published-ports.md#hostname-routes-shared-ports) that let several services with their own domains share port 443.

## Why can't my StartOS server or game console open ports on its own?

Automatic port forwarding (UPnP/PCP) is off for every device until you allow it. Open the device's [detail page](devices.md#device-detail-page) and turn on **Allow automatic port forwarding**. A device can only ever forward ports to itself, and the router refuses requests for ports a manual [Published Ports](published-ports.md) rule or the router itself uses — remote access to its web interface, SSH, or an inbound VPN. See [Automatic port forwarding](published-ports.md#automatic-port-forwarding).

## Why can't my phone resolve `.local` names over the VPN?

`.local` names are resolved by mDNS, which does not cross a VPN. Over the [Inbound VPN](inbound-vpn.md), use a name the router serves, such as a [published DNS record](devices.md#device-detail-page) or the device's `.lan` hostname, or connect the device to the network directly.

## Where can I report bugs or request features?

Open an issue on the [start-technologies GitHub repository](https://github.com/Start9Labs/start-technologies/issues) — StartWRT lives in the monorepo alongside the other Start9 products.

## Troubleshooting

Most problems reaching the router or your own devices come from a setting on the device itself, often one turned on for privacy, that sends its traffic or DNS lookups somewhere other than the router.

### Why does typing `router.lan` show search results?

Browsers don't recognize `.lan` as a web address, so a bare `router.lan` typed into the address bar is treated as a search. Type `http://router.lan/` instead (or `https://router.lan/` once you have [trusted your Root CA](trust-ca.md)). After the first visit, the browser's history suggests the full address.

### Why can't my device find `router.lan` when the router's IP address works?

Names ending in `.lan` — `router.lan` and your devices' names — are answered only by the router. A device that sends its DNS lookups anywhere else gets "server not found" (`DNS_PROBE_FINISHED_NXDOMAIN` in Chrome, "Hmm. We're having trouble finding that site." in Firefox) for these names while their IP addresses keep working.

On a computer, run `nslookup router.lan` to test the system's DNS, then compare it with a query sent directly to the router: `nslookup router.lan <router-IP>` (for example, `nslookup router.lan 192.168.0.1`). If only the direct query succeeds, check the system's DNS settings below. The `Server:` line can show a normal local resolver such as `127.0.0.53`; on Linux with systemd-resolved, [resolvectl status](https://www.freedesktop.org/software/systemd/man/latest/resolvectl.html) shows its upstream servers. If both queries succeed but the browser still fails, check the browser's secure DNS setting below — `nslookup` does not test that DNS path.

Check the device for the following. Menu locations vary between versions.

- **A VPN app** (Mullvad, Proton VPN, Tailscale, NordVPN, etc.) — most send all DNS lookups to the VPN provider while connected. Disconnect it to confirm, then see [the next question](#why-cant-i-reach-the-router-or-devices-on-other-profiles-while-a-vpn-app-is-on).
- **A WireGuard tunnel left on at home**, such as a StartTunnel configuration with a `DNS =` line. On an iPhone or Mac, edit the tunnel in the WireGuard app, enable **On-Demand**, and exclude your home Wi-Fi network, so the tunnel turns off when you are home. On other devices, turn the tunnel off while at home.
- **Android or GrapheneOS Private DNS** set to a provider hostname. Set `Settings > Network & internet > Private DNS` to **Automatic** — it then uses encrypted DNS only when the network supports it. On GrapheneOS, Private DNS also takes precedence over a VPN's DNS servers.
- **Chrome secure DNS set to a specific provider.** In `Settings > Privacy and security > Security > Use secure DNS`, select **With your current service provider**, or turn the setting off. Edge, Brave, and Vanadium (GrapheneOS) have the same setting in their privacy or security settings.
- **Firefox Max Protection.** In `Settings > Privacy & Security > DNS over HTTPS`, **Max Protection** never falls back to the router. Choose **Default Protection** or **Increased Protection**, or click **Manage Exceptions** and add `lan`. Firefox's default setting falls back to the router for `.lan` names and needs no change.
- **An encrypted DNS profile** on an iPhone, iPad, or Mac (NextDNS, AdGuard, Cloudflare 1.1.1.1). Remove it under `Settings > General > VPN & Device Management` (Mac: `System Settings > General > Device Management`), or turn it off in the provider's app.
- **DNS servers entered manually** in the device's network settings. Remove them so the device uses the DNS servers the router provides:
  - Windows: `Settings > Network & internet > Wi-Fi` (or `Ethernet`) `> <your network> > DNS server assignment > Edit`, then choose **Automatic (DHCP)**.
  - Mac: `System Settings > Network > <your connection> > Details > DNS`, then remove the entries under **DNS Servers**.
  - iPhone and iPad: `Settings > Wi-Fi > ⓘ > Configure DNS`, then choose **Automatic**.
  - Android: Android uses the router's DNS unless Private DNS (above) or an app overrides it.

iCloud Private Relay and **Limit IP Address Tracking** do not affect local names and can stay on.

To keep encrypted or filtered DNS for every device, set it up on the router instead: choose **Custom** DNS with **Secure (DoH)** in [WAN Settings](wan.md) or on a [Security Profile](security-profiles.md#creating-a-profile), and the router still answers `.lan` names itself.

### Why can't I reach the router or devices on other profiles while a VPN app is on?

A VPN app that sends all traffic through its tunnel also sends traffic for your local network there, where it never arrives. The router's address and your other devices time out (`ERR_CONNECTION_TIMED_OUT` or `ERR_ADDRESS_UNREACHABLE` in Chrome) until you disconnect the VPN.

Most apps have a setting that exempts the local network. Settings and defaults differ by app and platform:

- **Mullvad** — `Settings > VPN settings > Local network sharing`. Off by default. On iOS, local network connections are allowed by default with **Force all apps** disabled; with it enabled, turn on **Local network sharing** (see [Mullvad's iOS guide](https://mullvad.net/en/help/using-mullvad-app-on-ios)).
- **Proton VPN** — `Settings > Advanced > Allow LAN connections`. On Windows, `.lan` names also need **Access devices by name**.
- **Tailscale** (when using an exit node) — **Allow Local Network Access** in the exit node menu. Off by default.
- **Other apps** — look for a setting named "LAN access", "local network", or "split tunneling", often next to the app's kill switch.

On Android and GrapheneOS, an always-on VPN with **Block connections without VPN** turned on (`Settings > Network & internet > VPN`, then the gear next to the VPN) blocks the local network even when the app allows it. Turn that setting off to reach your local devices.

Allowing the local network lets the device reach its own network, but most VPN apps still send DNS to the VPN provider, so `.lan` names can keep failing; use IP addresses or check the app's DNS settings.

Some apps treat only the subnet the device is on as local. If the router's address works but devices on your other [Security Profiles](security-profiles.md) still time out, the app is sending those profiles' traffic into the tunnel, and the only fixes are to disconnect it or use an app that lets you add your network range (such as `192.168.0.0/16`) to its local or excluded routes.

To keep a VPN for a device's Internet traffic without these problems, run the VPN on the router as an [Outbound VPN](outbound-vpn.md) and route the device's profile through it. The device needs no VPN app, and local names and other profiles keep working.

### Why can't my Mac's browser reach the router when Safari can?

macOS asks for permission before an app can connect to devices on the local network. If you declined that prompt for Chrome, Firefox, Brave, or Edge, the browser shows errors such as `ERR_ADDRESS_UNREACHABLE` for the router and your local devices. Allow the browser in `System Settings > Privacy & Security > Local Network`, then restart it.

### Why can't a device on one profile reach a device on another?

Check these in order:

1. **LAN Access** — The profile that _starts_ the connection needs access to the other profile, set under **Access** on the profile's **LAN** tab (see [Creating a Profile](security-profiles.md#creating-a-profile)). Replies are allowed automatically, so the other profile needs no access back.
2. **The device's own firewall** — Many devices accept connections only from their own subnet, so a connection from another profile's subnet is refused even when the router allows it. On Windows, for example, file and printer sharing, remote desktop, and ping rules are often limited to the local subnet, and a network marked **Public** blocks more than one marked **Private**. To allow the other profile, open **Windows Defender Firewall with Advanced Security**, select **Inbound Rules**, open the rule for the service, and under `Scope > Remote IP address` add the other profile's subnet (for example, `192.168.1.0/24`). Other operating systems and devices have their own firewall settings for this.
3. **A VPN app** on either device — see [above](#why-cant-i-reach-the-router-or-devices-on-other-profiles-while-a-vpn-app-is-on).

### Why don't AirPlay, Chromecast, or printers show up from another profile?

Automatic discovery — AirPlay, Chromecast, network printers, and `.local` names — only works between devices on the same Security Profile. Put devices that need to discover each other on the same profile, or connect to the other device by its IP address, or by name once you have given it a custom name and reserved its address on its [device page](devices.md#device-detail-page).

### Why does my phone keep showing up as a new device?

Phones, tablets, and laptops can hide their hardware (MAC) address behind a private, randomly generated one. StartWRT recognizes a device by that address, so when it changes, the device appears on the [Devices](devices.md) page as a new entry and loses its custom name, reserved address, and permissions such as **Allow automatic port forwarding**. A [Published Ports](published-ports.md) rule pointing at its old address no longer reaches it.

The address changes when **Rotating** is selected on an iPhone or iPad, and after resetting a device's network settings. GrapheneOS picks a new address every time it connects, by default. For devices you want the router to remember, keep one address for your network. Menu locations vary between versions and phone makers.

- **iPhone and iPad** — `Settings > Wi-Fi > ⓘ` next to your network `> Private Wi-Fi Address`, then choose **Fixed** or **Off**.
- **Mac** — `System Settings > Wi-Fi > Details` next to your network `> Private Wi-Fi address`, then choose **Fixed** or **Off**.
- **Android** — `Settings > Network & internet > Internet`, tap the gear next to your network, then `Privacy > Use device MAC`.
- **GrapheneOS** — the same menu, then choose **Use per-network randomized MAC**. This keeps one address on your network and a different one on every other network. **Use device MAC** also works.
- **Windows** — `Settings > Network & internet > Wi-Fi > <your network> properties > Random hardware addresses`, then turn it **Off**.

Afterward, [Forget](devices.md#device-detail-page) the stale offline entry and set the name, reservation, and permissions again on the device's current entry.

### Why does the router's HTTP page not load in Firefox?

With **HTTPS-Only Mode** on, Firefox tries `https://router.lan` first and shows "Secure Site Not Available" until you [trust your Root CA](trust-ca.md). Click **Continue to HTTP Site** to download the certificate.

### Why can't I reach my home network over the Inbound VPN from some places?

If the network you are visiting uses the same addresses as your home network, your device sends traffic for those addresses to the network it is on rather than through the tunnel. The WireGuard app shows the tunnel connected with a recent handshake, but your home addresses time out or open the visited network's own router page.

StartWRT's default addresses (`192.168.0.x` for the Admin profile, then `192.168.1.x` for the next profile) are among the most common on home, hotel, and office networks. To avoid the overlap, choose a less common **Network Block** in [LAN Settings](lan.md#ipv4), such as `10.47`. Changing the Network Block removes your existing [Inbound VPN](inbound-vpn.md) servers, so do it before setting up remote access, or recreate the servers and re-import their client configurations afterward.

### Why doesn't the router get an Internet address after I replaced my old router?

Some ISPs only give an address to the device they already know, which they identify by its MAC address. In [WAN Settings](wan.md#mac-address), set **MAC Address** to **Custom** and enter the MAC address of your old router's Internet port. Take it from the old router's admin page, not its label, which usually shows a different port's address. Then disconnect the old router.
