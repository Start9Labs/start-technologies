# start-tunnel CLI Reference

The `start-tunnel` CLI manages a StartTunnel server — a WireGuard-based gateway that provides clearnet access to devices behind NAT. Run it directly on the StartTunnel server, or use `start-cli tunnel` to manage it remotely from a StartOS server.

## Global Options

- `-c, --config <PATH>` — Configuration file path
- `-H, --host <URL>` — StartOS server URL
- `-r, --registry <URL>` — Registry URL
- `--registry-hostname <HOST>` — Registry server hostname
- `-t, --tunnel <URL>` — Tunnel server address
- `-p, --proxy <URL>` — HTTP/SOCKS proxy
- `--cookie-path <PATH>` — Cookie file path
- `--developer-key-path <PATH>` — Developer signing key path

## Authentication

Manage passwords, sessions, and authorized SSH keys for remote access.

### `start-tunnel auth login`

Log in and create an authenticated session.

### `start-tunnel auth logout <SESSION>`

End an authentication session.

### `start-tunnel auth set-password`

Set the web UI password.

### `start-tunnel auth reset-password`

Reset the web UI password.

### `start-tunnel auth get-pubkey`

Retrieve the server's public key.

### `start-tunnel auth session list`

List active sessions.

- `--format` — Output format

### `start-tunnel auth session kill [IDS...]`

Terminate sessions.

### `start-tunnel auth key add <NAME> <KEY>`

Add an authorized SSH key.

### `start-tunnel auth key list`

List authorized SSH keys.

- `--format` — Output format

### `start-tunnel auth key remove <KEY>`

Remove an authorized SSH key.

## Subnets

Create and remove WireGuard subnets. Each subnet is an isolated network. The `<SUBNET>` argument is passed to the parent `subnet` command.

### `start-tunnel subnet <SUBNET> add <NAME>`

Create a new subnet with the given name.

### `start-tunnel subnet <SUBNET> remove`

Remove a subnet and all its devices.

### `start-tunnel subnet <SUBNET> set-ipv6`

Set (or clear) the routed IPv6 prefix delegated to the subnet. Each host on the
subnet is assigned a globally-routable `/128` out of it. See [IPv6](ipv6.md).

- `--prefix <PREFIX>` — The routed prefix (e.g. `2001:db8:abcd::/64`). Omit to
  disable IPv6 on the subnet.

## Devices

Manage devices within a subnet. Each device gets a unique WireGuard configuration.

### `start-tunnel device add <SUBNET> <NAME> [IP]`

Add a device to a subnet. Optionally assign a specific IP address.

- `--kind <client|server>` — Device kind (default `client`). A `server` enables gateway autoconfiguration (DNS injection + auto-publish) by default.

### `start-tunnel device list <SUBNET>`

List all devices in a subnet.

- `--format` — Output format

### `start-tunnel device remove <SUBNET> <IP>`

Remove a device from a subnet.

### `start-tunnel device show-config <SUBNET> <IP> [WAN_ADDR]`

Display the WireGuard configuration file for a device. Optionally override the WAN address in the generated config.

## Port Forwarding

Expose a device's port on the server's public IP.

### `start-tunnel port-forward add <EXTERNAL_PORT> <TARGET>`

Add a port forwarding rule mapping a public external port to a private target. The external IP is fixed server-side to the target device's WAN.

- `--label <LABEL>` — Human-readable label
- `--sni <SNI>` — Hostname to SNI-demux on a shared external port (TLS services only); repeatable. Omit for a plain port forward.
- `--count <COUNT>` — Number of contiguous ports to forward as a range (a PCP PORT_SET range), counting up from both the external port and the target port. Defaults to 1. Not valid together with `--sni`.

### `start-tunnel port-forward remove <SOURCE>`

Remove a port forwarding rule.

### `start-tunnel port-forward set-enabled <SOURCE>`

Enable or disable a port forwarding rule.

- `--enabled` — Enable the rule

### `start-tunnel port-forward update-label <SOURCE> [LABEL]`

Change or clear the label on a port forwarding rule.

## HTTP Redirects

StartTunnel runs an HTTP→HTTPS redirect on port 80 of every public IPv4 it holds, so a plain `http://` request to an exposed service bounces to `https://`. These are **on by default**; each address can be turned off individually. A redirect and a port-80 forward are mutually exclusive and never both enabled — forwarding port 80 is rejected while the redirect is on, and enabling the redirect is rejected while port 80 is forwarded.

### `start-tunnel http-redirect list`

Show the port-80 redirect status of every public IPv4: whether it is enabled, and whether a port forward already occupies port 80 (which blocks enabling the redirect).

- `--format` — Output format

### `start-tunnel http-redirect set-enabled <IP>`

Turn the port-80 HTTP→HTTPS redirect on or off for a public IPv4. Enabling is rejected if a port-80 forward exists on that IP — delete the forward first.

- `--enabled` — Enable the redirect; omit the flag to turn it off

## IPv6 Pinholes

Expose a device's port over IPv6 by opening a firewall pinhole on the device's own global address (GUA — see [IPv6](./ipv6.md)). Unlike an IPv4 forward there is no NAT; a differing internal port turns it into a port-only translation on the same GUA (e.g. an `80 → 443` redirect). The GUA must be an address the tunnel delegates to a client (its subnet needs an IPv6 prefix).

### `start-tunnel pinhole add <GUA> <EXTERNAL_PORT>`

Open a pinhole for `[GUA]:EXTERNAL_PORT`.

- `--internal-port <PORT>` — Destination port on the GUA. Omit for a pure pinhole (internal == external); set a different value for a port remap (e.g. `80 → 443`).
- `--label <LABEL>` — Human-readable label
- `--count <COUNT>` — Number of contiguous ports to open as a range, counting up from both the external and internal ports. Defaults to 1.

### `start-tunnel pinhole remove <GUA> <EXTERNAL_PORT>`

Remove a pinhole.

### `start-tunnel pinhole set-enabled <GUA> <EXTERNAL_PORT>`

Enable or disable a pinhole.

- `--enabled` — Enable the pinhole

### `start-tunnel pinhole update-label <GUA> <EXTERNAL_PORT> [LABEL]`

Change or clear the label on a pinhole.

## Updates

### `start-tunnel update check`

Check the registry for available updates.

- `--format` — Output format

### `start-tunnel update apply`

Apply an available update.

- `--format` — Output format

## Server Management

### `start-tunnel restart`

Reboot the StartTunnel server.

## Web Interface

Manage the admin web UI.

### `start-tunnel web init`

Initialize the web UI (interactive setup).

### `start-tunnel web uninit`

Remove web UI configuration.

### `start-tunnel web enable`

Enable the web UI.

### `start-tunnel web disable`

Disable the web UI.

### `start-tunnel web set-listen <LISTEN>`

Set the IP and port the web UI listens on.

### `start-tunnel web get-listen`

Display the current listen address.

- `--format` — Output format

### `start-tunnel web generate-certificate [SUBJECT...]`

Generate a self-signed TLS certificate for the web UI. Pass Subject Alternative Names for the certificate.

### `start-tunnel web import-certificate`

Import a TLS certificate from stdin.

### `start-tunnel web get-certificate`

Display the current TLS certificate.

- `--format` — Output format

### `start-tunnel web get-available-ips`

List available IP addresses for binding.

- `--format` — Output format

## Database

Low-level access to the StartTunnel database.

### `start-tunnel db dump [-p <POINTER>] [PATH]`

Dump database contents, optionally filtered by JSON pointer.

- `-p, --pointer <PTR>` — JSON pointer to specific value
- `--format` — Output format

### `start-tunnel db apply <EXPR> [PATH]`

Apply a patch expression to the database.
