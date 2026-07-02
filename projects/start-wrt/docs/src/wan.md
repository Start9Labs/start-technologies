# WAN Settings

The WAN (Wide Area Network) page configures how the router connects to the Internet through your ISP. Most users will not need to change these settings — StartWRT auto-detects your Internet connection on first boot. Navigate to `Internet > WAN Settings`.

## IPv4

Configure the router's IPv4 Internet connection. The default is DHCP, which works for most ISPs.

- **DHCP** — The router obtains an IP address automatically from your ISP. This is the default and most common setting.

- **Static** — Manually configure a fixed IP address assigned by your ISP:
  - **WAN IP** — The static IPv4 address.
  - **Prefix Length** — The subnet prefix (e.g. `/24`). The equivalent subnet mask is displayed alongside (e.g. `255.255.255.0`).
  - **Gateway IP** — The default gateway provided by your ISP.

- **PPPoE** — Used by some DSL providers. Enter the credentials provided by your ISP:
  - **Username** — Your ISP account username.
  - **Password** — Your ISP account password.
  - **Device** — (Optional) Select a specific network interface for the PPPoE connection.

## IPv6

Configure IPv6 if your ISP supports it.

- **SLAAC** (default) — Automatic IPv6 configuration. The most common option if your ISP supports IPv6.
- **DHCPv6** — The ISP assigns an IPv6 address via DHCP. Use if SLAAC does not work with your ISP.
- **Static** — Manually configure a fixed IPv6 address, prefix length, and gateway. A **LAN Prefix** field sets the IPv6 prefix delegated to your LAN.
- **6RD** — Tunnels IPv6 over an IPv4 connection. Required by some ISPs that do not provide native IPv6. Configuration fields: IPv6 Prefix, IPv6 Prefix Length, IPv4 Prefix Length, and Border Relay IP (the IPv4 address of the ISP's relay server).
- **Disabled** — No IPv6 on the WAN interface.

For SLAAC and DHCPv6, an optional **IPv6 Prefix** field lets you request a specific prefix length from your ISP for prefix delegation (e.g. `/48`, `/56`, `/64`). Leave empty to let your ISP decide automatically.

The WAN summary shows an IPv6 status badge indicating whether IPv6 is Enabled or Disabled, along with its mode (SLAAC, DHCPv6, Static, or 6RD).

## DNS

Configure which DNS servers the router uses to resolve domain names.

- **Get from ISP** (default) — Use DNS servers provided automatically by your ISP via DHCP.
- **Custom** — Specify your own DNS servers. Up to three servers are supported (Primary required, Secondary and Tertiary optional). Each server has a **Secure (DoH)** toggle to enable DNS-over-HTTPS encryption for that server.

> [!NOTE]
> Not all DNS servers support DoH. Common servers that do include Cloudflare (`1.1.1.1`), Google (`8.8.8.8`), and Quad9 (`9.9.9.9`).

> [!TIP]
> Using privacy-focused DNS providers can improve both privacy and performance compared to your ISP's default DNS servers. Individual [Security Profiles](security-profiles.md) can override these DNS settings with their own Custom DNS configuration.

## MAC Address

Some ISPs bind your Internet connection to a specific MAC address. If you are replacing an existing router, you may need to clone the old router's MAC address.

- **Router** (default) — Use the router's built-in MAC address.
- **Custom** — Enter the MAC address of your previous router or modem.

> [!NOTE]
> If you do not know whether your ISP requires a specific MAC address, leave this setting at its default. You only need to change it if your Internet connection does not work after switching routers.

## Dynamic DNS

See [Dynamic DNS](ddns.md) for full details on configuring DDNS from this tab.
