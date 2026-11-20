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

1. Click "Add VPN". An "Add client device" dialog opens automatically so you can add your first client (see [Adding a Client](#adding-a-client)).

> [!NOTE]
> Each Security Profile can have at most one inbound VPN. Once every profile has one, the "Add" button is disabled.

> [!NOTE]
> The router keeps the VPN's port for itself: publishing it to a device in [Published Ports](published-ports.md) asks for confirmation first, and a device's [automatic port forwarding](published-ports.md#automatic-port-forwarding) request for it is refused.

## Managing a VPN Server

Each VPN server row has an actions menu:

- **Enable** / **Disable** — Toggle the server without deleting it. While disabled, clients cannot connect.
- **Manage clients** — Open the client management page (see below).
- **Edit** — Change the label, endpoint, or port, then click "Save VPN". A VPN server belongs to its Security Profile, so moving it to another profile means deleting it and adding a new one.
- **Delete** — Remove the server (see [Removing a VPN Server](#removing-a-vpn-server)).

## Managing Clients

Each VPN server has a client management page listing all peers. Navigate to a VPN server and click "Manage clients" from the actions menu to view the client list, which shows each peer's name, IP address, and routing mode.

### Adding a Client

1. Open the VPN server's client list ("Manage clients" in its actions menu) and click "Add".

1. Configure the client:
   - **Label** — A name for the client (e.g. "My iPhone", "Work Laptop").
   - **IP Address** — The client's address in the VPN's own subnet, a `/24` the router takes from the top of your [Network Block](lan.md) when you create the VPN. Profiles cannot use that subnet. The next available address is suggested.
   - **Public Key** — (Optional) Enter an existing WireGuard public key if the device already has a keypair configured. Leave empty to auto-generate a keypair.
   - **Route all traffic through tunnel** — When enabled, all of the client's Internet traffic routes through the VPN (full tunnel). When disabled (the default), only LAN traffic uses the tunnel and the client uses its own Internet connection for everything else (split tunnel).

   > [!TIP]
   > "Route all traffic through tunnel" is especially useful when the VPN server's Security Profile uses an [Outbound VPN](outbound-vpn.md). Most devices only support one active VPN at a time, so a phone, for example, could either use WireGuard to access your LAN or use Mullvad/Proton directly — but not both. With full tunnel routing, the device connects to your router via WireGuard and its Internet traffic is then routed through the Outbound VPN automatically, giving you both LAN access and VPN protection in a single connection. If that Outbound VPN is IPv6-capable, the client's IPv6 traffic is tunneled through it as well; with an IPv4-only Outbound VPN, only IPv4 is tunneled and IPv6 is blocked so it cannot leak. This adds some latency since traffic passes through two tunnels.

1. Click "Save". The client's WireGuard configuration is generated and displayed.

### Viewing Client Configuration

Right after a client is created, its full configuration is displayed in two formats. It opens blurred — click the eye icon to reveal it before copying, downloading, or scanning it.

- **File** — Displays the configuration as text. Use the copy button to copy to clipboard, or the download button to save as a `.conf` file that WireGuard apps can import.
- **QR** — Displays the configuration as a QR code. Scan with the WireGuard mobile app to configure the client without manual entry.

If you entered the device's own public key, the configuration is shown as text only, with a placeholder where the private key belongs. Replace the placeholder with the device's private key, or copy the other values, including the preshared key, into the device's existing WireGuard tunnel.

> [!IMPORTANT]
> This is the only time the full configuration is available. The auto-generated private key is never stored on the router, so download or copy the configuration (or scan the QR code) before closing the dialog. The "View Config" action on the client list later shows the configuration with placeholders for its private and preshared keys. If you lose the configuration, [reissue it](#reissuing-a-clients-configuration).

### Changing Client Routing

A client's routing is set when it is created. To change it, delete the client and add it again with "Route all traffic through tunnel" set the way you want, then import the new configuration on the device.

### Reissuing a Client's Configuration

Select "Reissue config" from the client's actions menu to give the client new keys and an address in the VPN's subnet. Its new configuration is displayed once, as when adding a client; its name, routing, and DNS record publishing permission are kept. The old configuration stops working, so import the new one on the device.

Clients added before each VPN had its own subnet are marked "Old address": their address is inside the Security Profile's own subnet. Such a client can lose its connection while it is also connected to your home network. Reissue its configuration to move it into the VPN's subnet.

### Renaming a Client

Select "Rename" from the client's actions menu to change its display name.

### Allowing DNS Record Publishing

Select "Allow DNS record publishing" from the client's actions menu to let it publish DNS names into the router, with the same rules as the [device permission](devices.md#device-detail-page). It is off by default, and enabling asks for confirmation; grant it only to a client you trust. "Revoke DNS record publishing" removes the client's published names immediately.

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

| VPN Server | Profile         | Endpoint    | Use Case                                       |
| ---------- | --------------- | ----------- | ---------------------------------------------- |
| Primary    | Admin           | DDNS domain | Your personal remote access to everything      |
| Family     | Shared Services | DDNS domain | Family members accessing the home server       |
| Friends    | Guest           | DDNS domain | Friends using your Internet connection via VPN |
