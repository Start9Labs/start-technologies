# IPv6

StartTunnel can give the devices on a subnet a real, globally-routable IPv6
address drawn from a prefix your VPS delegates. IPv6 is configured **per subnet**
— each subnet can point at its own prefix (or none). This is optional and off by
default; IPv4 published ports work without it.

## What your VPS provides

IPv6 addressing depends on the block your provider routes to your VPS. Most
budget providers give a single **/64** (Hetzner, Vultr, BuyVM); some give less
(DigitalOcean routes a /124 — 16 addresses); a few give a **/56** or larger on
request (Linode) or on dedicated servers. Check your provider's dashboard or
docs for the exact prefix. A **/64 per subnet** is the natural fit.

## Requirements

Delegating a prefix only works if the server can actually route it:

- **The server must have working IPv6 egress** — an IPv6 default route (`::/0`).
  A device given an IPv6 address routes _all_ its IPv6 through the tunnel
  (`AllowedIPs = ::/0`); without upstream IPv6 on the server that traffic simply
  blackholes. `subnet … set-ipv6` **hard-errors** if the server has no IPv6
  default route, leaving the configuration unchanged. Confirm with
  `ip -6 route show default` and configure IPv6 on the VPS first. StartTunnel
  does not configure the server's own WAN IPv6 — that's the host/provider's job
  (RA, `netplan`, or `cloud-init`).
- **The prefix must be delivered to the server** — either _on-link_ on a WAN
  interface (the server holds a global address inside the covering /64, the usual
  single-/64 case) or _routed_ to the server by your provider. If the prefix is
  neither on-link nor something this host can confirm, the command still succeeds
  but logs a warning: make sure your provider actually routes the block to this
  host, or the subnet's devices will have no working IPv6.

## Configuring a subnet's prefix

Assign the routed prefix your provider gave you to a subnet:

```bash
start-tunnel subnet 10.59.0.0/24 set-ipv6 --prefix 2001:db8:abcd::/64
```

Or set the **IPv6 Prefix** field in the subnet's Add/Edit dialog in the web UI.
To turn IPv6 back off for a subnet, run the command with no `--prefix` argument
(or clear the field in the UI).

Once set, StartTunnel re-renders the WireGuard configs of that subnet's devices
to include an IPv6 address. Reconnect (or re-download the config) on each device
to pick it up.

> [!NOTE]
> Devices can make **outbound** IPv6 connections and receive their replies. To
> accept **unsolicited inbound** connections to a device's IPv6 address (hosting
> a service over IPv6), publish a port for it — see
> [Published Ports](./published-ports.md). Over IPv6 a published port is a firewall
> _pinhole_ on the device's global address (no NAT); a connected StartOS server
> also opens these automatically via PCP.

## How addresses are assigned

Every host on a subnet — the tunnel itself and each device — gets **one `/128`**
out of the subnet's prefix, with its tunnel IPv4 embedded in the low bits
(`prefix-network | tunnel-IPv4`). So a device's IPv6 is stable and predictable:
the same address every time, derivable from its tunnel IPv4 alone. The tunnel
uses the subnet's `.1` host as its own address on the WireGuard interface and as
the next hop for the subnet's IPv6 traffic.

When the prefix is delivered **on-link** (the common single-/64 case), the
tunnel answers Neighbor Discovery for each device's address on your VPS's
network, so traffic to it — including the replies to connections it opens — is
delivered over the tunnel. A **routed** prefix reaches the host without that
step. A `/64` is the natural size (its 64 host bits hold the whole tunnel IPv4);
a smaller block works too but keeps only its low host bits of the IPv4. Every
host must get a distinct address, so if a block is too small — or two devices'
low IP bits would collide — StartTunnel rejects adding the device or setting the
prefix rather than hand out a duplicate. Keep the number of devices (and their
low IP bits) within what the block can hold.

## Routing

For devices with an IPv6 assignment, all IPv6 traffic is carried through the
tunnel (`AllowedIPs = ::/0`). This is required: replies sent from a device's
global address have to return through the tunnel, since that address belongs to
your VPS, not the device's local network. IPv4 remains split-tunnel (only the
subnet is routed).

## DNS

Devices keep using the tunnel's IPv4 DNS resolver, which serves `AAAA` records
too. A device that is allowed to inject DNS records can publish an `AAAA` record
for its global address, so other devices on the tunnel can reach it by name. See
[DNS Records](dns-records.md).
