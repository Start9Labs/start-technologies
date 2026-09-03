# Published Ports

Published ports (port forwarding) allow devices on the Internet to reach specific devices on your LAN. Each rule maps a port on the router's public IP address to a port on a device behind the router.

## When to Use Port Forwarding

- **Self-hosting** — Expose a web server, email server, or other service running on a LAN device.
- **Remote access** — Allow external access to a specific device or application.
- **Gaming** — Open ports required by game servers or peer-to-peer connections.

> [!IMPORTANT]
> Port forwarding exposes devices directly to the Internet. Only forward ports for services you intend to be publicly accessible. For private remote access, use [Inbound VPNs](inbound-vpn.md) instead.

## Creating a Rule

1. Navigate to `Internet > Published Ports` and click "Add".

1. Configure the rule:
   - **Label** — A descriptive name (e.g. "Home Assistant", "Minecraft Server", "Bitcoin P2P").
   - **Device** — Select the target device from the list. The device is identified by name and IP address. If the device does not already have a reserved IPv4 address, one will be assigned automatically to ensure the rule always reaches the correct device.
   - **Port** — The port or port range on the device to expose. Enter a single port (e.g. `443`) or a range (e.g. `27015-27030`).
   - **Protocol** — TCP, UDP, or TCP + UDP.
   - **Source** — Who can connect. Select **Any** to allow connections from anywhere on the Internet, or **Custom** to restrict access to a specific IP address or CIDR range (e.g. `203.0.113.0/24`).
   - **IP Version** — IPv4, IPv6, or IPv4 + IPv6. If the selected device lacks an address for the chosen version, or WAN IPv6 is not configured, an error appears below the options and the rule cannot be saved until a compatible version is selected. Unlike IPv4, a device's IPv6 address cannot be reserved — each device chooses its own IPv6 address (via SLAAC) — so an IPv6 rule follows the device's current address automatically: the router watches the network for address changes and retargets the rule whenever the device picks a new address or your ISP rotates the delegated prefix. Where a device holds several IPv6 addresses at once, the rule targets its long-lived (stable) address rather than the short-lived privacy addresses that rotate daily.
   - **External Port** (IPv4 only) — **Same as device** keeps the external port identical to the internal port. Select **Other** to specify a different external port (e.g. forward WAN port `9090` to device port `8080`).

1. Click "Save".

> [!NOTE]
> If the device's [Security Profile](security-profiles.md) routes its traffic through an [Outbound VPN](outbound-vpn.md), creating or re-enabling a rule prompts for confirmation: published ports are reached over your public WAN address, not through the VPN, so the port is exposed on your real public IP.

> [!WARNING]
> Some ports the router answers on itself. If [Remote Access](settings.md#remote-access) is on — including the default "When behind NAT" mode while the router sits behind another router — the router serves its own web interface, and optionally SSH, on WAN ports 80, 443, and 22; an [Inbound VPN](inbound-vpn.md) listens on its configured port. Publishing one of these ports sends that traffic to your device instead, cutting the router's own service off from outside your network (access from your LAN is unaffected). Saving such a rule therefore prompts for confirmation first — you can override it deliberately, e.g. to run your own web server on 443 when you don't use remote access to the router. You are asked once per rule; editing the rule asks again. When the port's actual holder is a device's [hostname routes](#hostname-routes-shared-ports) rather than a router service, the prompt says so instead, naming the routed hostnames and the device they belong to.

## Editing a Rule

1. Navigate to `Internet > Published Ports` and select "Edit" from the rule's actions menu.

1. Modify any settings and click "Save".

## Enabling and Disabling Rules

Each rule can be toggled on and off without deleting it. Use the "Enable" or "Disable" option in the rule's actions menu.

## Deleting a Rule

1. Navigate to `Internet > Published Ports` and select "Delete" from the rule's actions menu.

## Status Indicators

Each published port rule shows a status indicator in the table:

- **Active** (green) — The rule is enabled and the target device is online with the addresses the rule needs.
- **Partial** (yellow) — Only one of the rule's IP versions is currently usable. For example, the device is missing its IPv4 or IPv6 address, or its IPv6 address is out of date after your ISP rotated the delegated prefix.
- **Paused** (orange) — The target device is offline or not found.
- **Error** (red) — No usable address is available for the rule — the device lacks an address for the selected IP version(s), or its IPv6 address is out of date.
- **Disabled** (grey) — The rule has been toggled off.

The status reflects the rule and the device's addresses on your LAN — it does not test whether traffic actually arrives from the Internet.

## Reaching a Published Port from Your Own Network

A device on your own network can reach a published port at the router's public address — or at a domain name that points there — instead of at the target device's LAN address. The router recognizes those connections and sends them on to the published device, so one address works from both sides of your Internet connection. This matters for anything configured with a single address, such as a phone app or a bookmarked domain name. (This is commonly called NAT loopback, or hairpinning.) It applies to addresses actually on the router's WAN interface — behind [CGNAT](cgnat.md) or another upstream router, your public IP is not one of them, and connections to it from inside your network will not loop back.

A published port is a public resource, so this works from every [Security Profile](security-profiles.md) that could reach it from the Internet, and from every profile whose **Access** setting already includes the target device's profile. A profile could reach it from the Internet when its **WAN Access** is **All**, or **Blacklist** without the router's public address among the blocked entries, or **Whitelist** with the router's public address (or a range holding it) among the allowed entries — in each case outside any [blackout window](security-profiles.md#wan-blackout). A profile with neither ground — for instance **WAN Access** set to **None**, and no Access to the device — is not given this route: for its connections to the router's public address, the router answers instead of the device. The device's global IPv6 address is its public address, and is reachable on the published port from the same profiles; for it, a Whitelist or Blacklist entry counts when it holds the device's address rather than the router's. Ports opened through [automatic port forwarding](#automatic-port-forwarding) are served the same way.

Only the published port is opened this way. A profile without Access to the device still cannot reach it at its LAN IPv4 address, or on any other port.

> [!NOTE]
> A rule whose **Source** is restricted to specific addresses is never served this way. That route cannot distinguish one local device from another, so serving the rule over it would let any device on your network past the restriction. Reach a restricted rule from inside your network at the device's own LAN address instead.

## Automatic Port Forwarding

Some devices can configure port forwarding for themselves using the standard UPnP and PCP protocols instead of you creating rules by hand — StartOS servers do this automatically, and game consoles and torrent clients commonly support it too.

This is **off by default** for every device. To allow it, open the device's [detail page](devices.md#device-detail-page) and turn on **Allow automatic port forwarding**. From then on, that device — and only that device — can ask the router to forward ports, and only to itself: a device can never open a port that routes traffic to another device.

Port uses created this way appear in the **Automatic** section of the Published Ports page, showing which device opened them, their kind (PCP, UPnP, or SNI), and when they expire. They are read-only:

- The device itself creates, renews, and removes its forwards.
- A forward the device stops renewing expires and is removed automatically once the lifetime the device asked for runs out — about an hour for typical clients, and never longer than a week even for a device that asks to keep the port indefinitely.
- A forward is also removed once the device no longer holds the address it points at — if the device leaves the network long enough for its DHCP lease to lapse, or comes back on a different address. This keeps a forward from quietly delivering Internet traffic to whichever device is given that address next. Devices with a reserved address are unaffected.
- To stop a device from creating forwards, turn its toggle back off on the device page — or forget the device entirely. Either way its existing forwards and hostname routes are closed immediately, and it can no longer open new ones.

Automatic forwards survive router reboots, so a self-configured device stays reachable while the router restarts. They can never take over a port that one of your manual rules already uses — the device's request is refused instead. The reverse also holds: if you publish a port manually that an automatic forward is currently using, your manual rule wins and the automatic forward is removed.

Ports the router answers on itself are protected the same way. If you have [Remote Access](settings.md#remote-access) turned on, or an [inbound VPN](inbound-vpn.md) reachable from the Internet, a device cannot take those ports over — requests for them are refused, so automatic forwarding can never cost you access to your own router. (Publishing such a port manually asks you to confirm instead — a device can't be asked, but you can.)

### Hostname routes (shared ports)

A device can also ask for a **hostname route** instead of a whole port: the router inspects each incoming TLS connection's requested hostname (SNI) and delivers it to whichever device registered that hostname, so several devices — or several services on one StartOS server — can share a single external port such as 443. StartOS servers use this automatically when you give services on a shared port their own domains.

Hostname routes appear in the same **Automatic** section with `SNI` in the Kind column and the hostname shown alongside. They follow the same rules as other automatic forwards — per-device permission, the device renews them, they expire on their own — with two differences: a shared port is claimed whole (an ordinary forward on that port is refused while hostname routes hold it, and publishing it manually asks you to confirm), and hostname routes do not survive a router restart — the device simply re-registers them within a few minutes, so no action is needed.

A routed hostname also works from inside your own network, not only from the Internet: a laptop on your LAN can open the same public address and reach the device, with the router turning the connection around. The device sees those connections as coming from the router itself rather than from the laptop, so its own access logs and any per-client rules it applies won't tell one local device from another. Connections from a different [Security Profile](security-profiles.md), and from the Internet, still arrive with the original address intact.

Hostname routes and [Remote Access](settings.md#remote-access) can share port 443. While routes hold the port, a connection naming a routed hostname reaches its device, and everything else — including browsing the router by its IP address — still reaches the router's own interface, accepted from exactly the sources your Remote Access setting allows. Enabling one feature never disables the other. (Router SSH and an inbound VPN's port still refuse hostname routes outright — those protocols can't share a port this way.)

> **A note on trust.** The PCP protocol runs over plain UDP, which carries no proof of who sent a request. The router verifies that each request actually arrives from the network the requesting device is on, so a device on one network can never open forwards on behalf of a device on another. Within a single network, though, automatic forwarding trusts the devices sharing it — exactly as UPnP and PCP do on every router, which is why it is off by default. If you run devices you don't fully trust, keep them on their own [Security Profile](security-profiles.md) so they cannot act for the devices you do. A profile governs which devices can reach one another directly; a hostname route is served by the router itself, so it stays reachable from every profile — the same exposure the service already has on the Internet.

## Endpoints

The **Endpoints** column in the table shows the public addresses where each forwarded port can be reached. IPv4 endpoints display the router's public IP (or DDNS domain) with the external port. IPv6 endpoints display the device's IPv6 address with the port directly. These are useful for configuring external services or sharing access details.

> [!NOTE]
> Port forwarding requires a public IP address. If your ISP uses [CGNAT](cgnat.md), IPv4 forwarding will not work — and because the router has no way to detect CGNAT, the rule still shows an "Active" status even though inbound IPv4 traffic never arrives. IPv6 forwarding may still work, since many CGNAT ISPs provide globally routable IPv6.

> [!NOTE]
> IPv6 forwarding requires the target device to have a globally routable address. If the device only has a local-only ULA address (one that starts with `fc` or `fd`), an error explains that a global address — from your ISP's prefix delegation — is required, and the rule cannot be saved until its IP Version is set to IPv4.
