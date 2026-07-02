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

1. **Factory reset** — Perform a [factory reset](factory-reset.md) from the web interface (if you are still logged in). This wipes all settings but preserves the Wi-Fi password.
2. **Reflash** — Boot from a microSD card and choose "Keep settings" to reinstall the firmware while preserving settings. You will be prompted to create a new admin password. See [Installing StartWRT](installing.md).

## What if I lose my Wi-Fi sticker password?

The Wi-Fi password is printed on the sticker on the bottom of the router and stored in the router's EEPROM — it can also be displayed in the StartWRT GUI on the WiFi tab as the Admin Profile 'Default' label. The EEPROM value is only re-read on a [factory reset](factory-reset.md); if you have replaced the **Default** password with your own, that new password is what's in effect. On a DIY or unprogrammed board with no EEPROM Wi-Fi password, set one via the GUI (if connected via ethernet) or with `startwrt-cli set-wifi-password`. See [Installing StartWRT](installing.md#diy-and-unprogrammed-boards) for the full procedure.

## Why doesn't one of my profiles have IPv6 Internet access?

If your ISP delegates only a single IPv6 prefix (for example, a `/64`), that prefix is assigned to your primary LAN. Non-admin [Security Profiles](security-profiles.md) routed **Direct** to the Internet then receive only a local-only ULA address and have no global IPv6 connectivity — IPv4 still works normally. To give such a profile global IPv6, route it through an IPv6-capable [Outbound VPN](outbound-vpn.md), or ask your ISP for a larger prefix delegation (such as a `/56` or `/48`).

## Does StartWRT phone home or collect telemetry?

No. StartWRT has no telemetry, no analytics, and no phone-home behavior. The only outbound connection the router initiates on your behalf is to check for firmware updates and to register with a Dynamic DNS provider (if configured). Both are optional and user-initiated — StartWRT never updates automatically, and you can also update entirely offline by reflashing from a microSD card. See [Updating](updating.md).

## Can I use StartWRT with StartOS?

Absolutely. StartWRT and [StartOS](/start-os/) are complementary products. StartOS runs your self-hosted services; StartWRT handles the networking. Together, they provide a complete self-hosting stack with proper network isolation, VPN access, and port forwarding — all without touching the command line.

## Where can I report bugs or request features?

Open an issue on the [start-technologies GitHub repository](https://github.com/Start9Labs/start-technologies/issues) — StartWRT lives in the monorepo alongside the other Start9 products.
