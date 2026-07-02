# StartWRT

The only router OS designed specifically to accommodate the complex needs of home-based self-hosting.

StartWRT is [Start9's](https://start9.com) fork of [OpenWrt](https://openwrt.org), reimagining the router experience from first principles. It pairs a hardened OpenWrt backend with a modern web interface that makes advanced networking capabilities — VPN chaining, per-device security profiles, WiFi schedules, dynamic DNS — accessible to everyone.

## Features

### Security Profiles

The core concept in StartWRT. Every device on the network receives a **Security Profile**, which governs what it can access — LAN devices, Internet, DNS servers, VPN tunnels, and time-of-day restrictions. A device's Security Profile is determined by *how* it gained access to the network: which Ethernet port it plugged into, which WiFi password it used, or which VPN server it connected to.

### Ethernet

Each Ethernet port maps to a different Security Profile. The port a device plugs into determines its Security Profile.

> A device plugs into Ethernet port 1 and receives the "Admin" profile. Another device plugs into Ethernet port 2 and receives the "Guest" profile.

### WiFi

Instead of creating multiple WiFi networks, StartWRT uses **one network with multiple passwords**. Each password maps to a different Security Profile. The password a device uses determines its Security Profile.

> Paul connects to WiFi using the "Admin" password, granting him full access to the LAN and Internet through Mullvad VPN. His four children use the "Child" password, granting them Internet access during the day through a custom WireGuard VPN with DNS filtering — and no Internet access at night. Friends visiting for dinner get the "Guest" password, granting full Internet access through Proton VPN and access to only certain LAN devices. Paul's Roku and Nest thermostat use the "Smart Device" password, granting limited Internet access with no LAN access at all.

### Inbound VPNs

Create unlimited inbound WireGuard VPN servers for remote access to the LAN. Each VPN server maps to a different Security Profile.

> Julie uses the "Primary" VPN server with the "Admin" profile, granting full access to the Internet and LAN. She gives friends the "Friends" VPN server with the "Shared Services" profile, granting access to a family server for photos, passwords, and Bitcoin transactions — but nothing else on her network, and no Internet access through her connection.

### Outbound VPNs and VPN Chaining

Connect unlimited network-wide outbound WireGuard VPN clients for Internet privacy. Optionally chain VPN clients together to avoid consolidating activity with a single provider, achieving multi-jurisdictional resilience.

> Mark has accounts with Mullvad VPN and Proton VPN. His Internet requests go through Mullvad, then through Proton, then to the final destination. Neither provider knows his full Internet activity unless they collaborate with each other.

### WiFi Schedules

Optionally disable WiFi on a schedule — for example, 10pm to 7am — to prevent late-night usage or limit radiofrequency EMF exposure.

### Dynamic DNS

Use Start9 Dynamic DNS for free with a single click. No account necessary. Optionally use another dynamic DNS provider.

### Published Ports

Forward ports from the Internet to specific devices on the LAN, with per-rule controls for protocol, IP version, source filtering, and external port mapping.

### Help Mode

Toggle Help Mode to get a detailed explanation of everything in the current view, including links to external resources. Toggle again to dismiss.

## Web Interface

The StartWRT web interface is a single-page application that communicates with the OpenWrt backend over JSON-RPC 2.0. It manages the router's UCI configuration — network interfaces, firewall rules, DHCP, WireGuard tunnels, and more — through a clean, form-driven UI built with Angular 22 and [Taiga UI v5](https://taiga-ui.dev/next).

**Accessible complexity.** StartWRT exposes powerful networking features that traditionally require CLI expertise. The web interface makes them approachable through progressive disclosure — simple defaults up front, advanced options available when needed.

**One password, one profile.** Rather than presenting raw firewall rules, VLAN tags, and routing tables, every access control decision is expressed through profiles. Users assign profiles via intuitive entry points: WiFi passwords, Ethernet ports, or VPN servers.

**Help where you are.** Every page has a collapsible aside panel with contextual help — plain-language explanations of what each setting does, why you'd change it, and links to external resources.

The interface is organized around the router's feature areas:

| Section | Routes | Description |
|---------|--------|-------------|
| **Internet** | WAN Settings, Published Ports, Outbound VPNs | External connectivity, port forwarding, VPN clients |
| **Network** | LAN Settings, Devices | Local network configuration and device management |
| **Security Profiles** | Profiles | Create and manage access control profiles |
| **Points of Entry** | Ethernet, Wi-Fi, Inbound VPNs | How devices join the network and get profiled |
| **System** | Settings | General, advanced, password, SSH keys, logs, activity |

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) — System architecture, data flow, build pipeline
- [CONTRIBUTING.md](CONTRIBUTING.md) — Development setup and build commands
- [API_CONTRACT.md](API_CONTRACT.md) — Complete RPC endpoint contract with Rust types
- [backend/](backend/) — Rust backend documentation
- [web/](web/) — Angular frontend documentation

## License

Copyright Start9 Labs, Inc.
