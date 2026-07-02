# Architecture

StartWRT is a router operating system built on OpenWrt with a custom Rust backend and Angular web interface. It reimagines the router experience by abstracting raw networking primitives — VLANs, firewall zones, subnets, routing tables — behind the [Security Profile](security-profiles.md) model. The result is enterprise-grade network segmentation that anyone can configure in minutes.

## What StartWRT Adds to OpenWrt

OpenWrt is a powerful open-source router OS, but it exposes raw networking primitives through its LuCI interface. Configuring VLANs, firewall zones, and multi-password Wi-Fi requires understanding how these systems interact at a low level. StartWRT keeps OpenWrt's battle-tested networking stack and adds:

- **Security Profiles** — A single abstraction that replaces manual VLAN, firewall, subnet, and routing configuration. One click creates an isolated network segment with its own DHCP, DNS, firewall rules, and VPN routing.
- **Multi-password Wi-Fi** — One SSID with multiple passwords, each mapping to a different Security Profile. No separate SSIDs, no manual VLAN tagging.
- **VPN chaining** — Route traffic through multiple VPN providers in sequence for multi-jurisdictional privacy.
- **Modern web interface** — A purpose-built Angular UI that manages the full router configuration without requiring CLI knowledge. The underlying OpenWrt CLI and LuCI remain available for advanced users.
- **OTA updates** — Firmware updates delivered through the web interface.

## How It Works

StartWRT has three components:

- **OpenWrt** — The base operating system. Handles kernel-level networking, Wi-Fi drivers, and package management.
- **Rust backend** — A single binary (`startwrt`) that runs as the RPC server and CLI. It manages all configuration, service reloads, TLS certificates, authentication, and system operations.
- **Angular frontend** — A single-page application embedded in the backend binary. Communicates with the backend over JSON-RPC 2.0.

All persistent configuration lives in UCI files under `/etc/config/` — the same configuration system used by stock OpenWrt, with no separate configuration database. The backend reads and writes these files atomically, so the CLI, LuCI, and the StartWRT web interface all share a single source of truth. (The activity log and login sessions are stored separately under `/etc/startwrt/`, outside the UCI config system.)

## Security Profile Internals

When you create a [Security Profile](security-profiles.md), the backend orchestrates changes across multiple UCI config files:

| UCI Config | What Changes |
|------------|-------------|
| `network` | New bridge interface, VLAN, and subnet |
| `firewall` | New zone with inter-zone forwarding rules (fw4/nftables) |
| `dhcp` | New DHCP server for the profile's subnet |
| `wireless` | New PSK entry in `wpa_psk_file` (for Wi-Fi passwords) |

This is why the web interface never exposes raw VLANs or firewall rules — the profile abstraction handles all of it consistently. StartWRT's firewall is built on fw4/nftables, so any custom firewall rules you add must be written as nftables (fw4) rules.

## Network Isolation

Device isolation uses bridge VLAN filtering at Layer 2. Each Security Profile is assigned a unique VLAN ID. Traffic is tagged at the entry point (Ethernet port, Wi-Fi password, or VPN server) and can only reach destinations within the same VLAN unless the firewall explicitly allows inter-zone traffic.

## Multi-Password Wi-Fi

StartWRT's multi-password Wi-Fi uses WPA2's identity PSK feature with dynamic VLAN assignment. Each password in the PSK file is associated with a VLAN ID. When a device authenticates, the router matches the password, looks up the VLAN, and places the device on the correct network segment — all transparently.

## Security

- **Admin password** — Stored as a SHA-512 hash in `/etc/shadow`
- **Wi-Fi password** — Printed on the sticker and stored in the router's EEPROM
- **Sessions** — Random token with 1-day expiry; HTTP-only SameSite=Strict cookie
- **Rate limiting** — 3 login attempts per 20 seconds
- **SSH** — Public key authentication or password auth (Admin password)
- **TLS** — rustls with a Root CA certificate chain

## TLS and Certificates

Certificate generation is delegated to the StartOS SSL primitives. The trust chain uses a root CA with CN "StartWRT Local Root CA" and an intermediate CA with CN "StartWRT Local Intermediate CA" (both with Org/OU "StartWRT"), and the issued server certificate carries a default SAN of `router.lan`. The web server hot-reloads its TLS certificate when the LAN IP changes, so no restart is required. See [Trusting Your Root CA](trust-ca.md) for installation steps.

## IPv6

Each router generates a unique per-device ULA /48 prefix at first boot. This ensures that chained StartWRT routers never collide on the same ULA range.

## Source Code

The StartWRT source code lives in the [start-technologies monorepo](https://github.com/Start9Labs/start-technologies/tree/master/projects/start-wrt), alongside the other Start9 products.

To report bugs or request features, [open an issue](https://github.com/Start9Labs/start-technologies/issues).
