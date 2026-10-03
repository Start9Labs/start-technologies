# LAN Discovery Relay

Status: draft for review. Tracks #4056. Target: a StartOS minor release, not a
0.4.0.x patch.

## Background

Every service container gets one veth on `lxcbr0`
(`shared-libs/crates/start-core/src/lxc/config.template:27-29`), with an
address in `10.0.3.0/24` and a SLAAC ULA in `fd00:3::/64`. Nothing routes
multicast or broadcast between `lxcbr0` and the LAN gateways. avahi publishes
the server's own `.local` name and nothing else, and systemd-resolved runs with
`MulticastDNS no` (`projects/start-os/debian/postinst:185,193`).

The rest of the network model this spec has to preserve:

- The `forward` chain drops by default (`net/startos-base.nft`). Containers may
  open new outbound connections (`lxcbr0-egress` in `net/net_controller.rs`);
  inbound traffic reaches a container only through binding DNATs
  (`build/lib/scripts/forward-port`).
- `startos_egress_guard` drops forwarded UDP 5351 and 1900 so that only startd
  opens gateway ports, and drops DNS sent past the host's resolver.
- Outbound traffic follows the per-service and system-wide gateway selection
  described in `shared-libs/crates/start-core/policy-routing.md`.

A package whose job is talking to LAN devices cannot discover them. In the
case behind #4056, Home Assistant's Govee integration sends its scan to
`239.255.255.250:4001` and the packet never leaves the bridge; the lights'
status replies to UDP 4002 on the server's address arrive as new flows with no
DNAT and are dropped. TP-Link Tapo discovery fails the same way. Every
zeroconf, SSDP and broadcast-based discovery protocol fails for the same
reason.

## Goals

- A package the user grants it to can discover LAN devices and be discovered
  by them with no change to the upstream application or its package.
- The user grants it per service and per gateway, and can withdraw it at any
  time.
- Inbound traffic still reaches a package only through its bindings and
  replies to traffic it sent.
- Only startd opens router ports.
- A package cannot impersonate a LAN host or claim the server's names.
- No new LAN addresses, no router configuration, and it works when the server
  is on Wi-Fi.

## Non-goals

- Unicast and wide-area DNS-SD.
- SSDP advertisement by packages (packages acting as UPnP devices) in v1.
- Restricting which mDNS service types a package may advertise in v1.
- Layer-2 protocols: ARP scanning, EtherType-based protocols.
- Radios (Bluetooth, Zigbee, Thread); these are device grants.

## Alternatives considered

### mDNS reflector

avahi `enable-reflector` between the LAN gateways and `lxcbr0`.

- Covers mDNS only. SSDP, WS-Discovery and the vendor broadcast protocols
  (Govee, Tapo, Kasa, LIFX, WiZ, Yeelight) still fail.
- The reflector copies records as published, so a container's announcements
  reach the LAN carrying `10.0.3.x` addresses that no LAN host can reach.
- Reflects every container's mDNS to the LAN and the LAN's to every
  container. There is no per-package scope.

### Per-package multicast relay for declared groups

smcroute/igmpproxy-style forwarding of groups and ports the package declares.

- Each protocol is added one at a time, by each package, and a missing entry is
  a silent discovery failure.
- Kernel multicast routing never forwards `224.0.0.0/24`, `ff02::/16` or
  `255.255.255.255`, and decrements TTL. mDNS, LLMNR and most vendor
  discovery use those groups or TTL 1.
- Unicast replies to a multicast query match no conntrack entry, so the replies
  still need their own path in.

### Per-package macvlan

A second NIC with its own address on the LAN.

- Traffic on that NIC bypasses the host's `forward` chain: no binding model, no
  gateway policy, no outbound selection.
- The host cannot reach the container over macvlan without a host-side macvlan
  of its own.
- Does not work when the server's uplink is Wi-Fi: access points drop frames
  from unassociated MACs.
- The container emits arbitrary frames on the LAN, including router
  advertisements and DHCP offers.

### Host networking

Removes the container's network namespace. It bypasses per-gateway binding,
port allocation, outbound selection and the egress guard entirely, and every
socket the package opens is a socket on the server.

### Routed LAN identity plus multicast mirror

Give each granted package an address on the LAN subnet, answer ARP/NDP for it
from the host (proxy ARP/NDP), route it to the container, and mirror multicast
and broadcast between the LAN and the container. A variant routes a separate
subnet to the server, which needs a static route on the router; most ISP
gateways and mesh systems have no static-route setting, and many routers NAT
only their own LAN subnet, so that variant is rejected outright.

The proxy-ARP variant needs nothing from the router and keeps traffic in the
host's `forward` chain. Its problems, and whether each can be fixed:

| Problem                                                                                                                                                               | Fixable                                                                                                                            |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| SSDP must be mirrored, which hands the package the router's IGD control URL; a port mapping requested from its own LAN address exposes it to the WAN outside StartOS. | Yes: drop forwarded traffic to the IGD control endpoint.                                                                           |
| Unicast replies to multicast queries are new conntrack flows, so inbound UDP has to be open.                                                                          | Mostly: nft sets with timeouts keyed on outbound query source ports, plus declared ports.                                          |
| The mirror carries router advertisements, DHCP offers, LLMNR/NBNS answers.                                                                                            | Yes: drop them in `netdev` rules and enforce the assigned source address.                                                          |
| The package can answer mDNS for any name, including the server's own `.local`.                                                                                        | Only with a userspace mirror that parses mDNS.                                                                                     |
| The server must claim and keep a LAN address it does not own.                                                                                                         | No. DHCP pool collisions, several leases on one MAC, IP-to-MAC binding on some routers, and re-acquiring after a move or renumber. |

The last row is inherent to claiming an address and breaks plugging the
server in anywhere. The fix for the fourth row is a userspace relay that parses
the traffic, and once that relay exists the separate address adds nothing. This
spec keeps the relay and drops the address.

## Design

### Overview

startd gains a `net/discovery` module next to `net/port_map`. It runs one relay
per granted LAN gateway, follows the gateway watcher for address and interface
changes, and is attached and detached per package the way outbound rules are
synced in `net_controller.rs`. It has four parts:

1. Outbound query relay: a package's multicast and broadcast UDP is re-sent
   from the server's address on the LAN, keeping its source port where it can.
   Unicast replies are DNATed to the querying socket by a timed nft map.
2. Fan-in: LAN multicast for groups the package joined, LAN UDP broadcast, and
   DHCP client broadcasts reach that package. Routable groups go through the
   kernel's multicast forwarding; link-local groups and broadcast, which the
   kernel will not forward, are written into the package's veth.
3. mDNS translator: a package's mDNS announcements are published by avahi on
   the server's name and the binding's external port.
4. Egress-guard additions that apply to every container.

startd is the control plane: it opens flows, programs nft maps and sets and the
multicast forwarding cache, and parses mDNS. A packet crosses userspace only
where the kernel has no way to forward it: outbound queries, which need a new
source address; fan-in of link-local groups and broadcast; SSDP in both
directions, which is filtered on content; and mDNS records, which are
rewritten.

Unicast traffic between a package and a device it found is unchanged: it
leaves through NAT like any other outbound connection.

### Grant

The grant is user state, not a manifest field. Any package can be granted
discovery, and the package declares nothing.

**Storage.** `/public/packageData/<id>/lanDiscovery` in patch-db, a set of
`GatewayId` on `PackageDataEntry` next to `outboundGateway`. It defaults to
empty, so existing entries need no migration, and it is removed with the
package on uninstall. The service page edits it per gateway. Removing a
gateway takes effect immediately: its relay flows close, fan-in for that
package stops on it, and the package's mDNS entries are withdrawn from it. An
empty set is revocation.

**Eligible gateways.** A gateway can be granted when it is an Ethernet,
wireless or bridge interface holding an address in RFC 1918 or `fc00::/7`
space. WireGuard, loopback, and a gateway with only public addresses cannot; a
server plugged straight into an ISP modem has nothing to grant.

### Outbound query relay

**Capture.** An `AF_PACKET` socket on the host side of each granted package's
veth, with a BPF filter for UDP from that container's address to a multicast
address, `255.255.255.255`, the `lxcbr0` subnet's broadcast address, or the
broadcast address of a granted gateway's subnet. The host forwards none of
these (`bc_forwarding` stays 0), so the relay is their only way off the bridge.
Capture copies the packet and leaves the original alone.

**Re-send.** Each `(container address, source port, destination, gateway)` is a
flow with its own UDP socket bound to the gateway's address. The socket takes
the package's source port when no host socket holds that port, and an
ephemeral port otherwise. It is bound without `SO_REUSEADDR`, so the bind fails
rather than sharing a port with a host service. Keeping the port matters where
the library binds a fixed port and the device answers to it: `govee-local-api`
binds 4002 and sends its scan from that socket, `pywizlight` binds 38899, and
`flux_led` binds 48899 because legacy Magic Home devices answer only queries
from that port. `python-kasa` (Kasa and Tapo), `aiolifx` and `yeelight` send
from an ephemeral port and take the reply wherever it lands. A query to the
`lxcbr0` broadcast address is sent to the gateway's subnet broadcast address. The relay
copies the packet's TTL or hop limit, sets the multicast interface to the
gateway, enables `SO_BROADCAST` for broadcast destinations, and sends the
payload. IPv6 flows to `ff02::/16` bind the gateway's link-local address. A
flow closes after 30 s without traffic in either direction.

**mDNS queries.** A query from port 5353 is re-sent from port 5353 with the QU
bit cleared, so responders answer by multicast and the answers reach the
package through fan-in. Re-sent from any other port it would be a legacy
unicast query (RFC 6762 §6.7), which avahi drops when it carries known answers
(`avahi-core/server.c`) and lwIP ignores when it has more than one question;
python-zeroconf sends both. The send uses a raw socket: avahi holds 5353 with
`SO_REUSEPORT`, and a UDP socket bound to the gateway's address on 5353 would
take the server's unicast mDNS from it. A query from another port is one-shot
and is relayed as an ordinary flow.

**Replies.** Opening a flow adds a timed element to an nft DNAT map keyed on
the gateway address and the flow's port, with the container's address and
source port as its value. A unicast UDP datagram from an on-link source to that
key is DNATed to the container before local delivery, so the flow socket only
holds the port and sends. The element carries the flow's 30 s timeout and is
re-added on each query. The DNAT rule marks the connection and the `forward`
chain accepts marked new flows; the container's answer returns through the
same conntrack entry, masqueraded to the gateway address. The container sees
the device's real address and answers it by ordinary unicast. `forward-port`
already drives nft this way for bindings; the map, the mark and the accept
were run in the namespace harness (see Validated).

SSDP flows, those whose query went to port 1900 on the SSDP group, are the
exception. Their replies are read from the flow socket and written to the
container by an `IP_TRANSPARENT` UDP socket (`net/transparent.rs` has the TCP
form, `transparent_connect`; this needs a UDP counterpart), after dropping any
reply whose `ST` or `USN` names `InternetGatewayDevice`, `WANIPConnection` or
`WANPPPConnection`. An M-SEARCH for `ssdp:all` draws a reply from the router
carrying its IGD description URL, and the package never sees it. The IGD
endpoint rule under Egress-guard additions is the second layer.

This covers protocols that send a query to a multicast or broadcast address and
expect unicast replies to the port the query came from: SSDP M-SEARCH,
WS-Discovery, Govee, Tapo, Kasa, LIFX, WiZ, Magic Home and Yeelight. Traffic a
device sends unprompted to a fixed port needs a binding (see Inbound to fixed
ports).

**Refused.** The relay drops, before re-sending:

| Traffic                                                             | Reason                                       |
| ------------------------------------------------------------------- | -------------------------------------------- |
| UDP 67, 68, 546, 547                                                | DHCP and DHCPv6 servers and relays           |
| UDP 5351                                                            | PCP and NAT-PMP                              |
| SSDP `NOTIFY`                                                       | packages do not advertise over SSDP in v1    |
| mDNS responses and probes (QR=1, or QR=0 with an authority section) | handed to the translator                     |
| UDP 5355, 137, 138                                                  | LLMNR and NetBIOS name and datagram services |

ICMP and ICMPv6 never reach the relay: it handles UDP only, so router
advertisements and redirects cannot be sent.

**Limits.** Per package: a cap on open flows and on packets per second, with
excess dropped and counted.

### Fan-in

**Joins.** The relay reads the IGMP and MLD reports each granted package's
kernel sends on its veth and joins those groups on each granted gateway,
ignoring `ff02::1` and solicited-node groups. A membership ends on a leave
report, when the package stops, or when the gateway's grant is removed. It
never expires on silence: Linux reports a join twice and afterwards only in
answer to a query (`net/ipv4/igmp.c`, `net/ipv6/mcast.c`). Joins use a socket
bound to no group port, so the relay never shares a port with a host service.

**Routable groups.** A group outside `224.0.0.0/24` and `ff02::/16` is
forwarded by the kernel. startd holds the multicast routing socket (`MRT_INIT`,
`MRT6_INIT`), registers each granted gateway and `lxcbr0` as interfaces, and
adds a forwarding-cache entry per joined group and gateway with `lxcbr0` as the
outgoing interface. The `forward` chain accepts UDP from a granted gateway to
`lxcbr0` for groups in an nft set the relay maintains, port 1900 excepted. A
prerouting rule raises the TTL or hop limit of multicast arriving on a granted
gateway to 4 when it is lower: forwarding decrements it and drops at zero, and
nothing listening on these groups checks it. `lxcbr0` snoops IGMP and MLD, so
the bridge delivers a group only to ports that reported a join, and a
bridge-family rule per non-granted veth drops forwarded multicast on that
port. A package without the grant receives nothing even when it has joined the
group.

**Userspace delivery.** The kernel forwards neither `224.0.0.0/24` nor
`ff02::/16` nor broadcast, so these are read with `AF_PACKET` on the gateway
and written into each granted package's veth as a frame with the group's or
broadcast MAC and the sender's address as its source. mDNS announcements
arrive this way. UDP broadcast received on a granted gateway from an on-link
source is delivered to every package granted on that gateway. SSDP, port 1900
on its group, takes this path too: it is the one fan-in filtered on content,
and the `forward` chain keeps it off the kernel path.

**DHCP.** DHCPDISCOVER and DHCPREQUEST broadcasts to UDP 67 are delivered like
any other broadcast. Home Assistant's `dhcp` integration uses them to find
devices that never announce themselves and to update the address of a device it
already manages after its lease changes. Outbound UDP 67 and 68 stay refused,
so a package can watch DHCP but never answer it. Unicast renewals are not seen.

**Excluded from fan-in.**

- mDNS queries. avahi answers them for the package (see the translator).
- LLMNR and NetBIOS queries, so a package never sees a name lookup it could
  try to answer.
- SSDP messages whose `ST`, `NT` or `USN` names `InternetGatewayDevice`,
  `WANIPConnection` or `WANPPPConnection`. The filter is on content, not on the
  router's address, so other UPnP devices the router hosts still get through.

### mDNS translator

Upstream applications publish their own services: Home Assistant's HomeKit
Bridge advertises `_hap._tcp` through python-zeroconf. Inside the container
those records carry `10.0.3.x`. The translator turns them into records avahi
publishes for the server.

**Input.** The mDNS responses the relay refused: messages from a granted
package to `224.0.0.251:5353` or `[ff02::fb]:5353` with QR=1. Probes are
ignored; avahi probes for what it publishes.

**Processing.**

1. Parse with hickory-proto, already in start-core's dependency tree through
   hickory-server, into service instances:
   PTR from service type to instance, SRV, TXT, and subtype PTRs. A and AAAA
   records are discarded.
2. For each instance, find a binding of the package whose internal port
   matches the SRV port. Where the package binds that port for both TCP and
   UDP, take the one matching the service type's `_tcp` or `_udp` label;
   Matter's `_matter._tcp` is served over UDP. Publish
   the instance on each granted gateway where that binding is enabled, with the
   binding's external port on that gateway. An instance with no matching
   binding is dropped; nothing could reach it. Advertising follows the same
   per-gateway policy as reaching the service.
3. The SRV target is the server's own name. In avahi's `AddService` that is the
   default host. No package publishes an A or AAAA record, so no package can
   claim a hostname.
4. Instance names pass through. avahi probes them and renames on a conflict
   (`Home Assistant Bridge #2`). The container is not told.
5. TXT records pass through unchanged, within caps on record count and size.

**Publishing.** One avahi entry group per package, within the existing
`entries-per-entry-group-max 128`. avahi answers LAN queries and handles
re-announcement.

**Lifecycle.**

- A goodbye (TTL 0) withdraws the instance.
- A TXT change for a published instance becomes `UpdateServiceTxt`. HomeKit
  changes `c#` and `s#` this way.
- A binding reallocation or a gateway enabled or disabled on a binding
  republishes or withdraws the instance on that gateway.
- Stopping or uninstalling the package, or removing its last granted gateway,
  frees its entry group.
- Repeated announcements of an unchanged instance are ignored.

### Egress-guard additions

These apply to every container, granted or not:

- Drop forwarded traffic from `lxcbr0` to each gateway's IGD control endpoint.
  startd already discovers the address and control URL
  (`net/port_map/upnp.rs`); the rule follows it. Today a container that
  guesses the control URL can reach it over TCP and request mappings to the
  server's address. The relay hands a granted package every unicast reply to
  its queries; the SSDP filter strips the router's own, and this rule is what
  holds if the filter misses.
- Drop forwarded UDP from `lxcbr0` with source or destination port 5353, 5355
  or 137. A package that saw a LAN query could otherwise answer it by unicast
  through NAT, and masquerade usually keeps source port 5353, which receivers
  accept. With these rules a package's mDNS goes through the translator only.

A package that sends unicast mDNS, LLMNR or NetBIOS to LAN hosts today would
lose that traffic. None is known; check the published packages before merging.

### Inbound to fixed ports

Traffic a device sends unprompted to a port on the server uses a binding as
today: HomeKit Bridge's TCP 21063, a Matter controller's UDP port. The port has
to be the one the device sends to, so a conflict on that external port breaks
the protocol rather than moving it. Replies to a port the package queried from
need no binding; the flow socket receives them.

### Matter

Matter controllers document that bridge networking does not work
(matterjs-server `docs/os_requirements.md`). For granted packages:

- **Link-local addresses.** connectedhomeip (`IPAddressSorter.h`) and matter.js
  (`ServerAddress.ts`) try a device's link-local address first, and a container
  cannot reach it. The relay removes `fe80::/10` AAAA records from mDNS
  messages it delivers into a granted container, so the controller uses the
  device's ULA or global address. Containers reach those through IPv6 NAT
  (`LXC_IPV6_NAT` in `debian/postinst`) whenever the LAN has an IPv6 router or
  a Thread border router; OpenThread border routers advertise a ULA on-link
  prefix when nothing else does.
- **UDP idle timeout.** Battery-powered devices report less often than
  conntrack's 120 s UDP stream timeout. UDP from a granted package's addresses
  gets an nft `ct timeout` policy with a 3600 s replied timeout, the value
  matterjs-server recommends.
- **Device-initiated sessions.** Devices open sessions to the controller for
  subscription resumption, ICD check-in and OTA queries. The package binds the
  controller's UDP port, the translator publishes its `_matter._tcp` record, and
  the binding accepts IPv6 on the gateway's ULA addresses as well as global
  ones.

### Interaction with existing rules

- The `forward` chain gains two accepts, both scoped to what the relay
  programmed: new UDP flows the reply map marked, and UDP from a granted
  gateway to `lxcbr0` for groups in the joined set, port 1900 excepted. Every
  other container-bound packet is still dropped by default. The userspace
  paths add nothing to the chain: re-sent queries and injected frames leave
  from host sockets.
- Flow sockets are bound to a gateway address and send to on-link or
  link-scoped destinations, which the `specific` invariant in
  `policy-routing.md` routes through `main` under any outbound selection. No
  change to the rule ladder.
- A package's unicast traffic to devices keeps following its outbound
  selection; LAN destinations are reachable under it by the same invariant.

## Limitations and tradeoffs

**What the grant exposes.** A granted package learns the device inventory of
each granted LAN, including the MAC address, hostname and vendor class of every
device that broadcasts a DHCP request. It can send any multicast or broadcast
UDP payload outside the refused list, which includes payloads aimed at buggy
device parsers, and its traffic is attributed to the server on the LAN. It
cannot impersonate a LAN host, send router advertisements or DHCP, open router
ports, publish hostnames, or receive inbound connections outside its bindings.

**Addresses inside payloads.** The relay rewrites no payload except to remove
link-local AAAA records from mDNS. Protocols that carry the controller's address
in-band still carry `10.0.3.x`, and a binding does not help:

- WiZ push updates (`phoneIp`). Home Assistant falls back to polling every 15 s.
- Yeelight music mode.
- Tuya protocol 3.5 discovery.
- Shelly Gen1's CoIoT peer.
- Media URLs handed to speakers and cast targets, and TXT records such as
  `base_url` in `_home-assistant._tcp`. Home Assistant's internal URL setting
  covers its own case.

**Device callbacks.** UPnP GENA subscriptions name the controller's address and
listener port in the `CALLBACK` header.

- Sonos detects the missing events and polls every 10 s. Events work with a
  binding whose external port equals its internal port and `advertise_addr`
  set to the server's address.
- DLNA renderers accept the subscription, so Home Assistant stops polling them
  and their state goes stale, unless the integration's callback URL override
  points at a binding.
- Samsung TVs lose volume and mute events; there is no override.

AirPlay audio streaming (pyatv's RAOP control and timing sockets) and ESPHome
voice audio over legacy UDP also receive on dynamic ports, and fail.
Battery-powered Shelly Gen2 devices connect to Home Assistant's internal URL,
which has to name the server's LAN address and a bound port.

**No SSDP advertisement.** A package acting as a UPnP device can be reached by
unicast but is not announced. Supporting it needs an SSDP responder on the host
that rewrites `LOCATION`.

**Hostnames are not published.** `homeassistant.local` does not resolve. The
package is reached at its StartOS interface addresses.

**Rename drift.** After a conflict rename, the application's view of its
instance name and the published name differ.

**Link-local-only devices.** Matter over Wi-Fi on a LAN with no IPv6 router
and no Thread border router gives devices only `fe80::` addresses. Link-local
destinations cannot be forwarded or NATed, so a package cannot reach them.
Options are a userspace UDP proxy for link-local destinations or a ULA prefix
on the LAN, which StartOS should not advertise itself.

**Cost.** Outbound queries, SSDP, link-local multicast and broadcast cross
userspace per packet; replies and routable-group multicast stay in the kernel.
Discovery traffic is low-rate; per-package limits bound the rest.

**Untrusted input in startd.** The relay parses SSDP headers and the translator
parses DNS messages from packages. Both use memory-safe parsers with size caps
and pass only validated strings to avahi over D-Bus.

## Validated

Checked against library source and in a network-namespace harness on Debian 13
with kernel 6.12 and nftables 1.1:

- `govee-local-api` binds 4002 and sends its scan from that socket
  (`controller.py`), `pywizlight` binds 38899 (`discovery.py`), and `flux_led`
  binds 48899, falling back to an ephemeral port when it is taken
  (`scanner.py`). `python-kasa`, `aiolifx` and `yeelight` send from an
  ephemeral port. Source-port preservation covers the first three; the rest
  take the reply on whatever port the relay sent from.
- A concatenated timed DNAT map admits a reply from an on-link source to the
  container with the device's address as its source, the container's answer
  returns masqueraded, a connection mark set by the DNAT rule carries it
  through a default-drop `forward` chain, and a reply from a new source after
  the element expires is dropped.
- A forwarding-cache entry carries a routable-group datagram from the LAN
  interface onto a bridge through a default-drop `forward` chain with a
  set-keyed accept; the TTL rewrite lets a TTL 1 datagram through; the bridge
  learns both member ports from their IGMP reports without a querier; and a
  bridge-family drop on one port keeps the datagram off it while the other
  port receives it.

## Assumptions to validate

These depend on device behaviour, not on the protocols, and gate merging the
implementation, not this spec:

- Vendor devices reply to the address and port the query came from. Govee's
  fixed 4002 is the library's source port, so it is the same case.
- A `(*,G)` forwarding-cache entry accepts any source; the harness used
  per-source entries.
- Embedded mDNS responders answer one-shot queries from a port other than 5353
  by unicast. A responder that answers by multicast instead is still reached
  through fan-in.
- Matter devices reach the controller on the server's ULA or global address,
  not its link-local one.
- Devices accept queries re-sent with the server's address as source and the
  original TTL.
- Home Assistant's `dhcp` integration works from broadcasts delivered into its
  veth.

## Testing

- Network-namespace harness under `unshare -rn`, following
  `net/policy_routing_model.sh`: a LAN namespace with responder processes for
  each protocol class (multicast query with source-port replies, broadcast
  query, fixed-port reply, mDNS announcement), the host with `lxcbr0`, and two
  container namespaces, one granted and one not. Assert:
  - queries leave with the gateway's address and the original TTL, and replies
    reach the querying socket with the device's source address through the
    DNAT map, with only SSDP replies crossing the relay;
  - a query keeps its source port when no host socket holds it, and takes an
    ephemeral port when one does;
  - a query from 5353 leaves from 5353 with QU cleared, avahi keeps receiving
    the server's unicast mDNS, and multicast answers reach the package;
  - queries to the `lxcbr0` and gateway subnet broadcast addresses are relayed;
  - a routable group reaches the granted container through the kernel path and
    is dropped on the other container's bridge port; a link-local group and a
    broadcast reach it through the relay; both stay joined with no further
    reports until a leave;
  - an SSDP `NOTIFY` naming the IGD is dropped and any other `NOTIFY` is
    delivered;
  - link-local AAAA records are removed from mDNS delivered to the granted
    container;
  - UDP flows from the granted container carry the 3600 s conntrack timeout;
  - every refused class is dropped, in both directions;
  - DHCP broadcasts are delivered and outbound 67/68 is dropped;
  - the IGD endpoint and 5353/5355/137 forward drops hold for both containers;
  - revoking a gateway closes flows and withdraws entries.
- Translator unit tests: binding matching per gateway, SRV target rewrite, A
  and AAAA removal, goodbye and TXT update, entry caps, malformed input.
- Device validation on a bench: Govee, Tapo, Kasa, WiZ, Magic Home, Tuya,
  Shelly, Hue (SSDP and mDNS), Chromecast, ESPHome, a HomeKit Bridge paired from
  an iPhone, Matter over Wi-Fi and over Thread commissioned from the Home
  Assistant app, and DHCP-based discovery in Home Assistant.

## Rollout

1. Egress-guard additions. Independent of the rest; the IGD rule closes an
   existing gap.
2. Grant (the `PackageDataEntry` field, TS bindings and the service page),
   outbound relay and fan-in.
3. mDNS translator and the Matter additions.

No package needs a new release or SDK bump to be granted discovery.

## Open questions

1. Whether packages need a way to request the grant at runtime, so the user
   learns a package wants it without reading its instructions.
2. Whether a restored backup carries the grant or resets it to empty.
3. Eligibility for a LAN that has only global IPv6 addresses.
4. An SSDP responder for package advertisements.
5. Link-local-only Matter devices: userspace proxy or out of scope.
