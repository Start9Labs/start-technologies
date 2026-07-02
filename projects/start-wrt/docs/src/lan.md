# LAN Settings

The LAN (Local Area Network) page configures the router's internal network addressing. Most users will not need to change these settings — the defaults work for typical home networks. Navigate to `Network > LAN Settings`.

## IPv4

Configure the router's LAN IPv4 addressing.

- **Network Block** — Select the private IP block for your network. The first octet determines the RFC 1918 block: `192.168.x.x`, `172.16.x.x`, or `10.0.x.x`. The second octet selects which `/16` within that block, and the editable range depends on the block:
  - `192.168.0.0/16` — the second octet is locked to `168`.
  - `172.16.0.0/12` — the second octet is editable, `16`–`31`.
  - `10.0.0.0/8` — the second octet is editable, `0`–`255`.

  The default is `192.168`, so existing networks are unaffected. Only RFC 1918 private ranges are accepted; out-of-range values are flagged inline (e.g. "Second octet must be 16–31") and block saving rather than being auto-corrected. Each [Security Profile](security-profiles.md) receives its own `/24` subnet within this block, allowing up to 256 separate subnets with 254 devices each.

- **Router address** — The router's LAN IP is the gateway (`.1`) of the primary **Admin** [Security Profile](security-profiles.md)'s subnet. The first two octets come from the Network Block above; the third octet is set by the Admin profile's **Subnet** field, not on this page. For example, with the `192.168.x.x` block and an Admin subnet of `1`, the router is reachable at `192.168.1.1`. This is the address you use to access the web interface (or simply `router.lan`).

> [!NOTE]
> DHCP is managed automatically for each Security Profile. You do not need to configure DHCP ranges or lease times.

> [!WARNING]
> Changing the Network Block changes the router's LAN address. If you are connected to the gateway IP address (opposed to `router.lan`), you will need to navigate to the new address to access the web interface. If any inbound VPN servers exist, they will be deleted because their client configurations become invalid with the new addressing.

> [!NOTE]
> Changing the second octet counts as a subnet change. Like other subnet changes, it is blocked while any device has a static IP reservation. Remove those reservations first.

## IPv6

Configure IPv6 addressing on the LAN.

- **SLAAC** — Toggle to enable or disable IPv6 on the LAN via Stateless Address Autoconfiguration. When enabled, devices generate their own IPv6 addresses from the router's advertised prefix.

- **Prefix Length** — Shown when SLAAC is enabled. The LAN IPv6 prefix length must be larger (a higher number) than your WAN prefix to create a valid subnet. For example, if your ISP assigns you a `/48` prefix, you can use `/56`, `/60`, or `/64` for the LAN. A `/64` is recommended for most home networks.

> [!NOTE]
> If any device has a static IPv6 reservation, SLAAC cannot be disabled until those reservations are removed.
