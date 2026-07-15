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
   - **IP Version** — IPv4, IPv6, or IPv4 + IPv6. If the selected device lacks an address for the chosen version, or WAN IPv6 is not configured, an error appears below the options and the rule cannot be saved until a compatible version is selected.
   - **External Port** (IPv4 only) — **Same as device** keeps the external port identical to the internal port. Select **Other** to specify a different external port (e.g. forward WAN port `9090` to device port `8080`).

1. Click "Save".

> [!NOTE]
> If the device's [Security Profile](security-profiles.md) routes its traffic through an [Outbound VPN](outbound-vpn.md), creating or re-enabling a rule prompts for confirmation: published ports are reached over your public WAN address, not through the VPN, so the port is exposed on your real public IP.

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

## Endpoints

The **Endpoints** column in the table shows the public addresses where each forwarded port can be reached. IPv4 endpoints display the router's public IP (or DDNS domain) with the external port. IPv6 endpoints display the device's IPv6 address with the port directly. These are useful for configuring external services or sharing access details.

> [!NOTE]
> Port forwarding requires a public IP address. If your ISP uses [CGNAT](cgnat.md), IPv4 forwarding will not work — and because the router has no way to detect CGNAT, the rule still shows an "Active" status even though inbound IPv4 traffic never arrives. IPv6 forwarding may still work, since many CGNAT ISPs provide globally routable IPv6.

> [!NOTE]
> IPv6 forwarding requires the target device to have a globally routable address. If the device only has a local-only ULA address (one that starts with `fc` or `fd`), an error explains that a global address — from your ISP's prefix delegation — is required, and the rule cannot be saved until its IP Version is set to IPv4.
