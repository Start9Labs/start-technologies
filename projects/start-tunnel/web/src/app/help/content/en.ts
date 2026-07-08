// prettier-ignore
/**
 * English help content (source of truth, eager). Key -> markdown.
 *
 * Keys are either a top-level route (`/subnets`, `/devices`, …) resolved by the
 * URL-keyed sidebar (`aside.ts`), or a synthetic dialog key (`/subnets/add`, …)
 * provided by a dialog via `provideHelp()` + `ModalHelp`. Each entry opens with
 * a `## Title` (hidden by `.g-help h2`, used as its search/label) and closes with
 * a "Learn more" link into https://start9.com/start-tunnel/.
 *
 * Rendered through marked -> DOMPurify, so inline HTML is allowed — use
 * `<code>` rather than markdown backticks (backticks would close this template
 * literal).
 */
export const HELP_EN: Record<string, string> = {
  '/subnets': `## Subnets

A subnet is an isolated private network (a VLAN). A <code>/24</code> (254 devices) by default, but the range is configurable — you can make it larger. Every device you add belongs to one subnet, and devices on the same subnet can reach each other. StartTunnel ships with one default subnet, which is all most setups need.

### Name

A friendly label so you can tell your networks apart.

### IPv4 Range

The block of private addresses devices on this subnet draw from — a <code>/24</code> by default, or larger (e.g. <code>10.59.7.0/24</code>). Fixed once the subnet is created.

### DNS

How the subnet resolves domain names — the VPS provider's resolvers, a device on the subnet, or your own custom servers.

### WAN IPv4 &amp; IPv6 Prefix

Which public address the subnet's traffic exits from, and its optional routed IPv6 block.

<a href="https://start9.com/start-tunnel/subnets.html" target="_blank" rel="noreferrer">Learn more →</a>`,

  '/subnets/add': `## Add / Edit Subnet

Create a private network or change an existing one. The IP range can only be set when creating — everything else can be edited later.

### Name

A friendly label for the subnet (required).

### IP Range

The private IP block for this subnet, in CIDR form — a <code>/24</code> by default (254 devices), or any larger block (e.g. <code>/16</code>) for more; a range smaller than <code>/24</code> isn't allowed. Pre-filled with a free suggestion. Only shown when creating — the range can't change afterward.

### DNS

- **Default (VPS provider)** — use the resolvers your VPS provides (simplest).
- **Device** — point the subnet at a device on it that runs its own resolver.
- **Custom** — enter up to three DNS server addresses.

This is the subnet's upstream resolver, separate from the DNS records StartTunnel serves for your own hostnames.

### WAN IP

Which of the VPS's public IPv4 addresses this subnet's outbound traffic leaves from. **System default** lets StartTunnel choose (the address is shown in parentheses). Only matters if your VPS has more than one public IP.

### IPv6 Prefix

An optional routed IPv6 block (e.g. <code>2001:db8:abcd::/64</code>) so every device gets a stable global address. Leave blank for none.

<a href="https://start9.com/start-tunnel/subnets.html#creating-a-subnet" target="_blank" rel="noreferrer">Learn more →</a>`,

  '/devices': `## Devices

Every device on your tunnel, split into two tables.

### Servers

Devices that host services others reach (typically a StartOS server). When you allow it, a server can manage the tunnel's DNS records and publish its own ports automatically — the two switches on each row.

### Clients

Phones, laptops, and other peers that only connect out; they have no gateway-configuration abilities.

### Columns

Name, subnet, LAN IPv4, the public address traffic exits from (WAN IPv4), and each device's IPv6. Use a row's menu to edit it, view its WireGuard config, switch its role, or remove it.

<a href="https://start9.com/start-tunnel/devices.html" target="_blank" rel="noreferrer">Learn more →</a>`,

  '/devices/add': `## Add / Edit Device

Add a device to a subnet, or rename and re-home an existing one. Whether it's a **Server** or **Client** is set by which "Add" button you used. After you save a new device, its WireGuard config opens automatically.

### Name

A friendly name for the device (required).

### Subnet

The private network to place the device in. Auto-selected when only one subnet exists. Only shown when adding.

### LAN IP

The device's address within the subnet. Pre-filled with the next free address; appears once a subnet is chosen (adding only).

### WAN IP

Which public address this device's outbound traffic leaves from. **Subnet default** inherits the subnet's setting (shown in parentheses); or pin a specific address.

### Allow DNS Injection / Allow auto-publish

Servers only. Let this server manage the tunnel's DNS records, and publish its own ports via PCP/UPnP. On by default — enable only for devices you trust.

<a href="https://start9.com/start-tunnel/devices.html#adding-a-device" target="_blank" rel="noreferrer">Learn more →</a>`,

  '/devices/config': `## Device Configuration

The WireGuard configuration for this device — use it to connect the device to the tunnel. Opens automatically after adding a device, and any time via **View Config**.

### File

The configuration as text. **Copy** it to the clipboard, or **Download** it as a <code>start-tunnel.conf</code> file to import into a WireGuard client.

### QR

A QR code of the same configuration. Scan it with the WireGuard mobile app to set up a phone or tablet without typing anything.

For a StartOS server, add it under <code>System › Gateways</code> instead of a WireGuard app.

<a href="https://start9.com/start-tunnel/devices.html#adding-a-device" target="_blank" rel="noreferrer">Learn more →</a>`,

  '/published-ports': `## Published Ports

Route incoming traffic from a public address and port to a port on one of your servers. Two tables:

### Manual

Ports you publish here — toggle each on or off, rename, or delete it.

### Automatic

Ports a server publishes for itself automatically (PCP/UPnP). Read-only here; manage them where auto-publish is configured on the device.

### Columns

Label, the target **Server**, an optional TLS **Hostname** (SNI), the External and Internal ports, the protocol (always TCP/UDP), and the public **IP** it's reachable on.

<a href="https://start9.com/start-tunnel/published-ports.html" target="_blank" rel="noreferrer">Learn more →</a>`,

  '/published-ports/add': `## Add published port

Send traffic from a public port to a chosen server and internal port.

### Label

A name to identify this published port (required).

### External Port

The public port people connect to. With a range, this is where it starts.

### Server

Which server receives the traffic (clients can't). Its public address becomes the published port's public IP automatically.

### Internal Port

The port on the server that receives the traffic.

### Number of Ports

Publish several consecutive ports at once, counting up from the external and internal ports. Leave at 1 for a single port. A range can't use an SNI hostname.

### IP Version

Reach the service over IPv4, IPv6, or both. IPv6 requires the server's subnet to have an IPv6 prefix.

### Hostname (optional)

A TLS/SSL domain (SNI) so several hostnames can share one external port. IPv4 only, and not available for ranges.

<a href="https://start9.com/start-tunnel/published-ports.html#add-a-port-manually" target="_blank" rel="noreferrer">Learn more →</a>`,

  '/published-ports/edit-label': `## Edit Label

Rename this published port, or add a label if it doesn't have one yet. The label is only for your reference in the list — it doesn't affect routing.

<a href="https://start9.com/start-tunnel/published-ports.html#manual-and-automatic-ports" target="_blank" rel="noreferrer">Learn more →</a>`,

  '/dns': `## DNS Records

Private DNS for your tunnel. These records let devices on the tunnel reach the services you host by a memorable hostname (e.g. <code>home.example.com</code>) instead of a raw tunnel IP — and they resolve only inside your tunnel, never on the public internet.

### Manual

Records you add by hand: point a hostname at one of your servers (A/AAAA), alias it (CNAME), or store text (TXT).

### Automatic

A trusted server with **DNS injection** enabled registers its own service domains here automatically, so you don't maintain them by hand. Read-only here — the devices manage them.

Each row shows the record's Hostname, Type, target Server (or value), and TTL.

<a href="https://start9.com/start-tunnel/dns-records.html" target="_blank" rel="noreferrer">Learn more →</a>`,

  '/dns/add': `## Add DNS Record

Map a hostname to a device, or to a custom value.

### Hostname

The hostname this record answers for, e.g. <code>home.example.com</code> (required).

### Type

- **A / AAAA** — point a name at an IPv4 / IPv6 address.
- **CNAME** — alias to another name.
- **TXT** — arbitrary text.

### Server / Value

For A and AAAA, pick one of your server devices — or **Other (custom)** to type an address by hand. For CNAME and TXT, enter the target name or text in **Value**.

### TTL (seconds)

How long other systems may cache this record before re-checking. Defaults to 300.

<a href="https://start9.com/start-tunnel/dns-records.html#viewing-and-managing-records" target="_blank" rel="noreferrer">Learn more →</a>`,

  '/settings': `## Settings

### Version

The installed StartTunnel version. **Check for updates** looks for a newer release; when one is available, **Update to …** downloads and installs it.

### HTTP Redirect (80 → 443)

For each public IPv4, whether plain <code>http://</code> visitors are redirected to secure <code>https://</code>. On by default. An address can't have both a redirect and a published port on port 80 — the toggle is disabled while port 80 is published. See <a href="https://start9.com/start-tunnel/http-redirects.html" target="_blank" rel="noreferrer">HTTP Redirects</a>.

### Language

The language the interface is shown in.

### Account

Change your login password, reboot the VPS, or log out.

<a href="https://start9.com/start-tunnel/updating.html" target="_blank" rel="noreferrer">Learn more →</a>`,

  '/settings/change-password': `## Change Password

Set a new password for logging in to this StartTunnel VPS.

### New password

Your new password — must be 8–64 characters.

### Confirm new password

Re-type it exactly; you'll see an error if the two don't match.

If you ever forget it, reset it from the VPS with <code>start-tunnel auth reset-password</code>.

<a href="https://start9.com/start-tunnel/faq.html#what-if-i-forget-my-password" target="_blank" rel="noreferrer">Learn more →</a>`,
}

export default HELP_EN
