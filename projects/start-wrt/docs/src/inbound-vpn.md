# Inbound VPNs

Create WireGuard VPN servers on your router for secure remote access to your home network. Each VPN server maps to a [Security Profile](security-profiles.md), so remote devices receive the same access controls as if they were connected locally.

## How It Works

An inbound VPN server listens for WireGuard connections from the Internet. When a remote device connects, it is assigned the VPN server's Security Profile — gaining access to the LAN, Internet, or both, according to that profile's rules. This is like giving someone a key to a specific door in your house rather than handing them the master key.

> [!IMPORTANT]
> Inbound VPN requires either a public IP address or [Dynamic DNS](ddns.md) so remote devices can reach your router. If your ISP uses CGNAT, inbound connections cannot reach your router directly. Consider using [StartTunnel](/start-tunnel/) as a gateway instead.

## Creating a VPN Server

1. Navigate to `Points of Entry > Inbound VPNs` and click "Add".

1. Configure the server:

   - **Label** — A descriptive name (e.g. "Home VPN", "Friends", "Work").
   - **Endpoint** — The address where remote clients will connect. Select from available options: WAN IPv4 address, WAN IPv6 address, or a DDNS domain (if [Dynamic DNS](ddns.md) is configured). If you have a dynamic IP, use a DDNS domain so clients do not need to update their configuration when your IP changes.
   - **Security Profile** — The [Security Profile](security-profiles.md) to assign to connecting clients.
   - **Port** — The WireGuard listen port (default: `51820`). Must be unique across all VPN servers. If the default is already in use, the next available port is suggested.

1. Click "Save".

## Managing Clients

Each VPN server has a client management page listing all peers. Navigate to a VPN server and click "Manage clients" from the actions menu to view the client list, which shows each peer's name, LAN IP address, and routing mode.

### Adding a Client

1. Select the VPN server and click "Add".

1. Configure the client:

   - **Label** — A name for the client (e.g. "My iPhone", "Work Laptop").
   - **LAN IP Address** — The IP address assigned to this client on the VPN subnet.
   - **Public Key** — (Optional) Enter an existing WireGuard public key if the device already has a keypair configured. Leave empty to auto-generate a keypair.
   - **Route all traffic through tunnel** — When enabled, all of the client's Internet traffic routes through the VPN (full tunnel). When disabled (the default), only LAN traffic uses the tunnel and the client uses its own Internet connection for everything else (split tunnel).

   > [!TIP]
   > "Route all traffic through tunnel" is especially useful when the VPN server's Security Profile uses an [Outbound VPN](outbound-vpn.md). Most devices only support one active VPN at a time, so a phone, for example, could either use WireGuard to access your LAN or use Mullvad/Proton directly — but not both. With full tunnel routing, the device connects to your router via WireGuard and its Internet traffic is then routed through the Outbound VPN automatically, giving you both LAN access and VPN protection in a single connection. If that Outbound VPN is IPv6-capable, the client's IPv6 traffic is tunneled through it as well; with an IPv4-only Outbound VPN, only IPv4 is tunneled and IPv6 is blocked so it cannot leak. This adds some latency since traffic passes through two tunnels.

1. A WireGuard configuration is generated.

### Viewing Client Configuration

After creating a client, the configuration can be viewed in two formats:

- **File** — Displays the configuration as text. Use the copy button to copy to clipboard, or the download button to save as a `.conf` file that WireGuard apps can import.
- **QR** — Displays the configuration as a QR code. Scan with the WireGuard mobile app to configure the client without manual entry.

### Changing Client Routing

You can switch between routing modes from the actions menu on the client list:

- **Switch to All traffic** — Full tunnel. All Internet traffic routes through the VPN.
- **Switch to LAN only** — Split tunnel. Only local network traffic uses the tunnel.

> [!WARNING]
> Changing the routing mode deletes the existing peer and creates a new one. You will need to reconfigure the device with the new configuration.

### Renaming a Client

Select "Rename" from the client's actions menu to change its display name.

### Removing a Client

Select "Delete" from the client's actions menu. The client's WireGuard configuration is immediately invalidated.

## Connecting Remote Devices

Install the [WireGuard app](https://www.wireguard.com/install/) on the remote device and import the configuration:

- **Phone or tablet**: Scan the QR code from the client configuration page using the WireGuard app.
- **Laptop or desktop**: Download the `.conf` file and import it into the WireGuard app.

## Removing a VPN Server

1. Navigate to `Points of Entry > Inbound VPNs` and select "Delete" from the server's actions menu.

> [!WARNING]
> Deleting a VPN server immediately disconnects all clients and invalidates their configuration files. Clients will need new config files if a new server is created.

## Example

| VPN Server | Profile | Endpoint | Use Case |
|------------|---------|----------|----------|
| Primary | Admin | DDNS domain | Your personal remote access to everything |
| Family | Shared Services | DDNS domain | Family members accessing the home server |
| Friends | Guest | DDNS domain | Friends using your Internet connection via VPN |
