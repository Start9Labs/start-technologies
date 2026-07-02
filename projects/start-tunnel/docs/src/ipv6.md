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

## Configuring the prefix

Tell StartTunnel the routed prefix your provider assigned:

```bash
start-tunnel set-ipv6 --prefix 2001:db8:abcd::/64
```

Pass `--prefix` with no value (or `null`) to turn IPv6 back off.

Once set, StartTunnel re-renders every device's WireGuard config to include an
IPv6 address. Reconnect (or re-download the config) on each device to pick it up.

## How addresses are assigned

- **A /64 (the common case).** Every device shares the one /64 and receives a
  single global address. The tunnel answers Neighbor Discovery for those
  addresses on your VPS's network so inbound traffic reaches the right device.
- **A prefix shorter than /64** (a /56, /48, …). Each device is *delegated its
  own /64*, routed to it over WireGuard. A StartOS server behind the tunnel can
  then hand global addresses to its own services and containers.
- **A prefix longer than /64** (e.g. a /124). Each device gets a single global
  address; the number of devices is limited by the block size.

The tunnel itself uses the first address of the prefix (`…::1`) and advertises
it to devices as their IPv6 gateway and DNS server.

## Routing

For devices with an IPv6 assignment, all IPv6 traffic is carried through the
tunnel (`AllowedIPs = ::/0`). This is required: replies sent from a device's
delegated global address have to return through the tunnel, since that address
belongs to your VPS, not the device's local network. IPv4 remains split-tunnel
(only the subnet is routed).

## DNS

A device that is allowed to inject DNS records can publish an `AAAA` record for
its global address, so other devices on the tunnel can reach it by name. See
[DNS Records](dns-records.md).
