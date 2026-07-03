# IPv6

StartTunnel can give every connected device a real, globally-routable IPv6
address drawn from a prefix your VPS delegates to the server. This is optional
and off by default — IPv4 port forwarding works without it.

## What your VPS provides

IPv6 addressing depends on the block your provider routes to your VPS. Most
budget providers give a single **/64** (Hetzner, Vultr, BuyVM); some give less
(DigitalOcean routes a /124 — 16 addresses); a few give a **/56** or larger on
request (Linode) or on dedicated servers. Check your provider's dashboard or
docs for the exact prefix.

## Requirements

Delegating an IPv6 prefix only works if the server can actually route it:

- **The server must have working IPv6 egress** — an IPv6 default route (`::/0`).
  A device given an IPv6 address routes *all* its IPv6 through the tunnel
  (`AllowedIPs = ::/0`); without upstream IPv6 on the server that traffic simply
  blackholes. `set-ipv6` **hard-errors** if the server has no IPv6 default route,
  leaving the configuration unchanged. Confirm with `ip -6 route show default`
  and configure IPv6 on the VPS before delegating a prefix.
- **The prefix must be delivered to the server** — either *on-link* on a WAN
  interface (the server holds a global address inside the covering /64, the usual
  single-/64 case) or *routed* to the server by your provider (a /56 or /64 the
  VPS statically routes to your host). If the prefix is neither on-link nor
  something this host can confirm, `set-ipv6` still succeeds but logs a warning:
  make sure your provider actually routes the block to this host, or connected
  devices will have no working IPv6.

## Configuring the prefix

Tell StartTunnel the routed prefix your provider assigned:

```bash
start-tunnel set-ipv6 --prefix 2001:db8:abcd::/64
```

To turn IPv6 back off, run `start-tunnel set-ipv6` with no `--prefix` argument
(or use the **Disable** button on the web UI's settings page).

Once set, StartTunnel re-renders every device's WireGuard config to include an
IPv6 address. Reconnect (or re-download the config) on each device to pick it up.

> [!NOTE]
> Devices can make **outbound** IPv6 connections and receive their replies
> today. Accepting **unsolicited inbound** connections to a device's IPv6
> address (hosting a service over IPv6) is not yet supported — that arrives in a
> later release, alongside the existing IPv4 port-forwarding.

## How addresses are assigned

- **A /64 (the common case).** Every device shares the one /64 and receives a
  single global address. The tunnel answers Neighbor Discovery for those
  addresses on your VPS's network, so traffic to a device's global address —
  including the replies to connections it opens — is delivered to it over the
  tunnel.
- **A prefix shorter than /64** (a /56, /48, …). Each device is *delegated its
  own /64*, routed to it over WireGuard. A StartOS server behind the tunnel can
  then hand global addresses to its own services and containers.
- **A prefix longer than /64** (e.g. a /124). Each device gets a single global
  address; the number of devices is limited by the block size.

The tunnel itself uses the first address of the prefix (`…::1`) as its own
address on the WireGuard interface and as the next hop for devices' IPv6 traffic.

## Routing

For devices with an IPv6 assignment, all IPv6 traffic is carried through the
tunnel (`AllowedIPs = ::/0`). This is required: replies sent from a device's
delegated global address have to return through the tunnel, since that address
belongs to your VPS, not the device's local network. IPv4 remains split-tunnel
(only the subnet is routed).

## DNS

Devices keep using the tunnel's IPv4 DNS resolver, which serves `AAAA` records
too. A device that is allowed to inject DNS records can publish an `AAAA` record
for its global address, so other devices on the tunnel can reach it by name. See
[DNS Records](dns-records.md).
