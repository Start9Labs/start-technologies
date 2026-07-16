# Devices

The Devices page shows all devices that have connected to your router, organized into Online (currently connected) and Offline (previously seen) groups. Each device is associated with a [Security Profile](security-profiles.md) based on its [point of entry](points-of-entry.md).

Devices appear here even without an active DHCP lease: devices with static IPs, IPv6-only devices, and devices connected through an external switch (learned from the bridge forwarding table) are all listed. Names are resolved on the router from the device's hostname, with mDNS/Bonjour used as a fallback to recover a friendly name.

## Viewing Devices

Navigate to `Network > Devices` to see the device list. A search box above the list filters it as you type. Each entry shows:

- **Name** — The device's hostname or a custom name you have assigned. Click to open the device detail page.
- **Connection** — How the device connects: Ethernet, Wi-Fi 2.4GHz, Wi-Fi 5GHz, or VPN.
- **Security Profile** — The [Security Profile](security-profiles.md) the device is assigned to.
- **MAC address** — The device's unique hardware identifier.
- **IP address** — The device's IPv4 and IPv6 addresses. A lock icon indicates a reserved (static) IPv4 address.
- **Data and Speed** — Cumulative data usage and real-time upload/download speed for online devices.

## Device Detail Page

Click a device name to open its detail page:

- **Summary** — Displays the device's current status (online/offline), connection type, [Security Profile](security-profiles.md), IPv4 and IPv6 addresses, and real-time upload/download speed.

- **Data Usage** — A chart showing historical upload and download over time. Use the dropdown to select a time period: Last Week (7 days), Last 30 Days, or Last 3 Months (90 days). Usage history survives firmware updates.

- **Name** — Edit the custom display name for this device. If left empty, the device's hostname is used. Saving shows a brief spinner and a confirmation, then refreshes from the router so the displayed name always matches the saved state.

- **Reserve** — Toggle on to assign a fixed IPv4 address that persists across reboots. Enter the desired address within the device's profile subnet. Useful for servers, printers, NAS devices, or any device that needs a consistent address. If you change the reserved address, the device picks up the new one the next time it requests an address from the router — reconnecting or rebooting the device usually applies it right away; otherwise it can take up to 12 hours, and the interface reminds you of this when you save. While an enabled [Published Ports](published-ports.md) rule uses the device's IPv4 address, the Reserve toggle is locked. IPv6 addresses cannot be reserved: each device chooses its own IPv6 address (via SLAAC), so the router has no say in it — the IPv6 field is shown for reference only, and IPv6 published-port rules follow the device's current address instead.

- **Forget** — Remove an offline device from the list. Custom name, reserved IP settings, and the device's cached name are lost. If the device reconnects, it will appear as a new entry.

> [!TIP]
> Reserve an IPv4 address for any device you plan to use with [Published Ports](published-ports.md). IPv4 port forwarding rules require a stable address to ensure traffic always reaches the correct device (publishing a port reserves one automatically).

> [!NOTE]
> Forgetting a device only removes it from the list. If the device reconnects, it will reappear. To prevent a device from accessing the network, delete the [Wi-Fi password](wifi.md) or [Inbound VPN client](inbound-vpn.md) it uses to connect.
