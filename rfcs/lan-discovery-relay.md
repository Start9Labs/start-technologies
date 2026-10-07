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

- A package the user grants it to can send and receive LAN discovery traffic:
  its multicast and broadcast queries reach the LAN, the unicast replies they
  draw reach it, and LAN multicast and broadcast reach it.
- Discovery traffic crosses unchanged. StartOS carries it; the package
  configures its application to advertise the server's reachable address and
  the binding's external port.
- The user grants it per service and per gateway, and can withdraw it at any
  time.
- Inbound traffic still reaches a package only through its bindings and
  replies to traffic it sent.
- No new LAN addresses, no router configuration, and it works when the server
  is on Wi-Fi.

This spec promises a discovery transport, not compatibility with every LAN
device or application. An application that can be configured with its
advertised endpoint needs no upstream change; its package may need one.

## Non-goals

- Translating application protocols: rewriting mDNS records, SSDP headers or
  callback URLs (see Deferred translation).
- Unicast and wide-area DNS-SD.
- Layer-2 protocols: ARP scanning, EtherType-based protocols.
- Radios (Bluetooth, Zigbee, Thread); these are device grants.

## Alternatives considered

### mDNS reflector

avahi `enable-reflector` between the LAN gateways and `lxcbr0`.

- Covers mDNS only. SSDP, WS-Discovery and the vendor broadcast protocols
  (Govee, Tapo, Kasa, LIFX, WiZ, Yeelight) still fail.
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

The proxy-ARP variant needs nothing from the router, but the server must claim
and keep a LAN address it does not own: DHCP pool collisions, several leases on
one MAC, IP-to-MAC binding on some routers, and re-acquiring after a move or
renumber. That is inherent to claiming an address and breaks plugging the
server in anywhere. This spec keeps the server's one LAN identity and ordinary
NAT.

### Translating application protocols

A relay that parses mDNS and republishes a package's services through avahi
under the server's name and the binding's external port, strips link-local
AAAA records, and filters SSDP on content. It needs no package change, but it
puts a protocol translator in startd before any application has been shown to
fail with correct endpoint configuration. See Deferred translation.

## Design

### Overview

startd gains a `net/discovery` module next to `net/port_map`. It runs one relay
per granted LAN gateway, follows the gateway watcher for address and interface
changes, and is attached and detached per package the way outbound rules are
synced in `net_controller.rs`. It has three parts:

1. Outbound relay: a package's multicast and broadcast UDP is re-sent from the
   server's address on the LAN, payload unchanged.
2. Reply admission: on-link unicast replies to a relayed query reach the
   querying socket through a bounded, timed nft map.
3. Fan-in: LAN multicast for groups the package joined and LAN UDP broadcast
   reach that package, with destination group and device source address
   preserved.

The package's side is endpoint configuration: it reads its own bindings'
addresses and writes them into its application's configuration.

Unicast traffic between a package and a device it found is unchanged: it
leaves through NAT like any other outbound connection.

### Grant

The grant is user state, not a manifest field. Any package can be granted
discovery, and the package declares nothing.

**Storage.** `/public/packageData/<id>/lanDiscovery` in patch-db, a set of
`GatewayId` on `PackageDataEntry` next to `outboundGateway`. It defaults to
empty, so existing entries need no migration, and it is removed with the
package on uninstall. The service page edits it per gateway. An empty set is
revocation.

**Eligible gateways.** A gateway can be granted when it is an Ethernet,
wireless or bridge interface holding an address in RFC 1918 or `fc00::/7`
space. WireGuard, loopback, and a gateway with only public addresses cannot; a
server plugged straight into an ISP modem has nothing to grant.

**Revocation.** Removing a gateway from the grant, stopping or uninstalling the
package, or the gateway being replaced removes the access itself: the
package's flows close, their map elements are deleted, conntrack entries
created through them are flushed, and fan-in to the package stops on that
gateway. A reply admitted before revocation does not keep a path open after it.

### Outbound relay

**Capture.** An `AF_PACKET` socket on the host side of each granted package's
veth, with a BPF filter for UDP from that container's address to a multicast
address, `255.255.255.255`, the `lxcbr0` subnet's broadcast address, or the
broadcast address of a granted gateway's subnet. The host forwards none of
these (`bc_forwarding` stays 0), so the relay is their only way off the bridge.
Capture copies the packet and leaves the original alone. The relay captures
only from veths, so nothing it receives on a gateway is re-sent, and nothing
fanned in from one gateway is sent out another.

**Re-send.** Each `(container address, source port, destination, gateway)` is a
flow with its own UDP socket bound to the gateway's address. The relay copies
the packet's TTL or hop limit, sets the multicast interface to the gateway,
enables `SO_BROADCAST` for broadcast destinations, and sends the payload
unchanged. A query to the `lxcbr0` broadcast address is sent to the gateway's
subnet broadcast address. IPv6 flows to `ff02::/16` bind the gateway's
link-local address. A flow closes after 30 s without traffic in either
direction.

**Source port.** The socket takes the package's source port when it is free,
and an ephemeral port otherwise. Keeping the port matters where the library
binds a fixed port and the device answers to it: `govee-local-api` binds 4002
and sends its scan from that socket, `pywizlight` binds 38899, and `flux_led`
binds 48899 because legacy Magic Home devices answer only queries from that
port. `python-kasa` (Kasa and Tapo), `aiolifx` and `yeelight` send from an
ephemeral port and take the reply wherever it lands.

A port is free when no host socket holds it and the binding allocator
(`AvailablePorts` in `net/forward.rs`) has not assigned it: a binding can DNAT
a UDP port with no host listener behind it. The socket is bound without
`SO_REUSEADDR`, so a host socket wins. Conflicts resolve this way:

| Conflict                                         | Outcome                                                             |
| ------------------------------------------------ | ------------------------------------------------------------------- |
| Port held by a host socket or binding            | Flow takes an ephemeral port; the remap is counted on the package.  |
| Binding allocated onto a port an open flow holds | The binding wins; the flow closes and reopens on an ephemeral port. |
| Two packages query from the same fixed port      | First flow keeps the port; the second takes an ephemeral port.      |

A remap is not assumed to work. A device that answers only to the query's
fixed source port does not answer a remapped flow, and that is a discovery
failure for that package, reported in its flow counters.

**mDNS queries.** A query from 5353 is re-sent from 5353 by a raw socket: avahi
holds 5353 with `SO_REUSEPORT`, and a UDP socket bound to the gateway's address
on 5353 would take the server's unicast mDNS from it. The payload is unchanged,
QU bit included. Answers sent by multicast reach the package through fan-in;
answers a QU query draws by unicast go to the server's 5353, where avahi holds
the port. How those answers reach the querying package, and that avahi keeps
receiving its own, is a validation case (see Testing). A query from another
port is a one-shot legacy query and is relayed as an ordinary flow.

**Refused.** The relay drops, before re-sending:

| Traffic              | Reason                                                |
| -------------------- | ----------------------------------------------------- |
| UDP 67, 68, 546, 547 | DHCP and DHCPv6 servers and relays                    |
| UDP 5351             | PCP and NAT-PMP; only startd requests router mappings |

The egress guard's 5351 drop is in the `forward` chain, which re-sent traffic
does not pass through, so the relay enforces it itself. ICMP and ICMPv6 never
reach the relay: it handles UDP only, so router advertisements and redirects
cannot be sent. Name-service answers and advertisements a package sends (mDNS
responses and probes, SSDP `NOTIFY`, LLMNR and NetBIOS answers) are relayed or
refused according to the first policy decision.

**Limits.** Per package: a cap on open flows and on packets per second, with
excess dropped and counted.

### Reply admission

Unicast replies to a multicast or broadcast query are not replies to the
query's conntrack tuple: they come from the device's address, not the group.
Opening a flow adds a timed element to an nft DNAT map keyed on the gateway
address and the flow's port, with the container's address and source port as
its value. A unicast UDP datagram from an on-link source to that key is DNATed
to the container before local delivery, so the flow socket only holds the port
and sends. The element carries the flow's 30 s timeout and is re-added on each
query. The DNAT rule marks the connection and the `forward` chain accepts
marked new flows; the container's answer returns through the same conntrack
entry, masqueraded to the gateway address. The container sees the device's
real address and answers it by ordinary unicast.

The map is bounded by the per-package flow cap. A datagram to a port with no
element, or from an off-link source, is not admitted; unsolicited traffic to a
fixed listener still needs a binding (see Inbound to fixed ports).

This covers protocols that send a query to a multicast or broadcast address and
expect unicast replies to the port the query came from: SSDP M-SEARCH,
WS-Discovery, Govee, Tapo, Kasa, LIFX, WiZ, Magic Home and Yeelight.

### Fan-in

**Contract.** For each granted gateway:

- LAN multicast for a group a granted package joined reaches that package with
  its destination group and the device's source address unchanged. Link-local
  groups (`224.0.0.0/24`, `ff02::/16`) and TTL-1 datagrams are included. A
  group is never turned into broadcast.
- LAN UDP broadcast from an on-link source reaches every package granted on
  that gateway. A datagram to the gateway's subnet broadcast address is
  delivered to the container's subnet broadcast address, since the container
  has no interface on the gateway's subnet; `255.255.255.255` is delivered as
  is.
- DHCPDISCOVER and DHCPREQUEST broadcasts to UDP 67 are delivered like any
  other broadcast. Home Assistant's `dhcp` integration uses them to find
  devices that never announce themselves. Outbound 67 and 68 stay refused, so a
  package can watch DHCP but never answer it.
- A package without the grant on that gateway receives none of it, even when it
  has joined the group.

**Joins.** The relay reads the IGMP and MLD reports each granted package's
kernel sends on its veth and joins those groups on each granted gateway,
ignoring `ff02::1` and solicited-node groups. A membership ends on a leave
report, when the package stops, or when the gateway's grant is removed. It
never expires on silence: Linux reports a join twice and afterwards only in
answer to a query (`net/ipv4/igmp.c`, `net/ipv6/mcast.c`).

**Mechanism.** Two candidates are compared in the harness; the one that meets
the contract with fewer lifecycles is chosen:

- **Kernel copy.** A netdev-family `dup` or tc `mirred` on each granted
  gateway's ingress, matching the joined-group and broadcast sets, copying to
  each granted package's veth. A copy is not routed, so TTL and link-local
  scope survive. The subnet-broadcast case needs its destination address
  rewritten on the copy.
- **Routing plus injection.** Routable groups through the kernel's multicast
  forwarding cache (startd holds `MRT_INIT`/`MRT6_INIT`, one entry per joined
  group and gateway, a prerouting TTL raise, `lxcbr0` IGMP/MLD snooping, and a
  bridge-family drop per non-granted veth). Link-local groups and broadcast,
  which the kernel does not forward, are read with `AF_PACKET` on the gateway
  and written into each granted veth.

### Endpoint configuration

An application that advertises itself or hands devices a callback carries its
own address in the payload: inside the container that is `10.0.3.x` and the
binding's internal port, neither of which a LAN device can reach. The package
configures the application to advertise the endpoint the LAN reaches instead.

`sdk.host.getOwn(effects, hostId)` returns the package's host, whose bindings
carry address records per gateway (`update_addresses` in `net/host/mod.rs`):
numeric IPv4 and IPv6 addresses with the gateway's ID and scope, the binding's
assigned external port, and the TLS flag. The package selects the record for
the granted gateway and the application protocol, and writes it into the
application's configuration. Reads are reactive: with `const()`, init re-runs
when an endpoint changes, and the package regenerates the configuration and
reloads or restarts the application. `getOsIp()` returns the bridge gateway and
is not this endpoint.

Examples of the configuration this uses:

- Home Assistant HomeKit Bridge: `advertise_ip` and `port`.
- Home Assistant's internal URL, which Shelly Gen2 devices and media URLs use.
- Sonos `advertise_addr`, with a binding whose external port equals its
  internal port.

Whether an application accepts its advertised endpoint is a property of that
application and is checked per package. Payloads that carry the controller's
address with no override (WiZ `phoneIp`, Yeelight music mode, Tuya 3.5
discovery, Shelly Gen1's CoIoT peer, Samsung TV GENA callbacks) still carry
`10.0.3.x`; Home Assistant falls back to polling for some of them.

### Inbound to fixed ports

Traffic a device sends unprompted to a port on the server uses a binding as
today: HomeKit Bridge's TCP port, a Matter controller's UDP port. The port has
to be the one the device sends to, so a conflict on that external port breaks
the protocol rather than moving it. Replies to a port the package queried from
need no binding; reply admission delivers them.

### Matter

Matter controllers document that bridge networking does not work
(matterjs-server `docs/os_requirements.md`). For granted packages:

- **UDP idle timeout.** Battery-powered devices report less often than
  conntrack's 120 s UDP stream timeout. UDP from a granted package's addresses
  gets an nft `ct timeout` policy with a 3600 s replied timeout, the value
  matterjs-server recommends.
- **Device-initiated sessions.** Devices open sessions to the controller for
  subscription resumption, ICD check-in and OTA queries. The package binds the
  controller's UDP port and advertises it, and the binding accepts IPv6 on the
  gateway's ULA addresses as well as global ones.
- **Link-local addresses.** connectedhomeip (`IPAddressSorter.h`) and matter.js
  (`ServerAddress.ts`) try a device's link-local address first, and a container
  cannot reach it. Whether controllers fall back to the device's ULA or global
  address is a validation case.

### Egress guard

Existing rules are kept. One rule is added for every container, granted or not:
drop forwarded traffic from `lxcbr0` to each gateway's IGD control endpoint.
startd already discovers the address and control URL (`net/port_map/upnp.rs`);
the rule follows it. Today a container that guesses the control URL can reach
it over TCP and request mappings to the server's address.

These rules cover traffic the container sends through the `forward` chain. The
relay's re-sent queries leave from host sockets and do not pass through it, so
the existing UDP 1900 drop does not stop a granted package's M-SEARCH, and the
router's IGD description reaches the package. Learning the control URL does not
create a mapping: the request that would is unicast TCP from the container,
which passes through `forward` and meets the endpoint drop. The relay refuses
UDP 5351 itself (see Refused).

### Interaction with existing rules

- The `forward` chain gains accepts scoped to what the relay programmed: new
  UDP flows the reply map marked, and whatever fan-in mechanism is chosen.
  Every other container-bound packet is still dropped by default.
- Flow sockets are bound to a gateway address and send to on-link or
  link-scoped destinations, which the `specific` invariant in
  `policy-routing.md` routes through `main` under any outbound selection. No
  change to the rule ladder.
- A package's unicast traffic to devices keeps following its outbound
  selection; LAN destinations are reachable under it by the same invariant.

## Policy decisions

Opaque forwarding does not carry the guarantees a translator would. These are
decided for the grant before the traffic classes they govern are relayed.

1. **Names and advertisements.** A relayed mDNS response can assert any name,
   including the server's own `.local`, and a package's SSDP `NOTIFY`, LLMNR or
   NetBIOS answer reaches the LAN as sent. NAT changes the source address, not
   the names in the payload. Which of these a granted package may send, and
   how that is enforced, is undecided. Relaying them is what lets an
   application publish its own service records (`_hap._tcp`,
   `_home-assistant._tcp`) and the hostnames they target, with its own probing
   and conflict handling.
2. **Router mappings on the relayed path.** A granted package can learn the
   router's IGD control URL through a relayed M-SEARCH. The mapping requests
   that follow meet the forward-chain IGD endpoint drop, and the relay refuses
   PCP and NAT-PMP. Whether those two rules suffice to keep router mappings
   host-owned is undecided.

## Deferred translation

Each of these is added only when a supported application fails with correct
endpoint configuration and transport, with the application and version, the
observed failure and the configuration options exhausted brought to review
first. Failure alone does not make translation the remedy.

| Translation                                               | Evidence that would raise it                                                    |
| --------------------------------------------------------- | ------------------------------------------------------------------------------- |
| Republishing a package's mDNS services through avahi      | An application that cannot be configured to advertise the server's endpoint.    |
| Clearing the QU bit on relayed mDNS queries               | QU-query unicast answers that cannot reach the package with the payload intact. |
| Removing link-local AAAA records from mDNS into a package | A Matter controller that does not fall back from an unreachable `fe80::`.       |
| SSDP content filtering                                    | Policy decision 2 requiring it.                                                 |
| SSDP `LOCATION` or GENA `CALLBACK` rewriting              | An application with no callback override.                                       |

## Limitations and tradeoffs

**What the grant exposes.** A granted package learns the device inventory of
each granted LAN, including the MAC address, hostname and vendor class of every
device that broadcasts a DHCP request. It can send any multicast or broadcast
UDP payload outside the refused list, which includes payloads aimed at buggy
device parsers, and its traffic is attributed to the server on the LAN. It
cannot send router advertisements or DHCP, or receive inbound connections
outside its bindings and reply admission.

**Package work.** An application that advertises itself or registers callbacks
works only once its package configures the advertised endpoint. Discovery
alone needs no package change.

**Addresses inside payloads.** Payloads with no override carry `10.0.3.x` (see
Endpoint configuration). AirPlay audio streaming (pyatv's RAOP control and
timing sockets) and ESPHome voice audio over legacy UDP receive on dynamic
ports, and fail.

**Link-local-only devices.** Matter over Wi-Fi on a LAN with no IPv6 router
and no Thread border router gives devices only `fe80::` addresses. Link-local
destinations cannot be forwarded or NATed, so a package cannot reach them.

**Cost.** Outbound queries cross userspace per packet; replies stay in the
kernel, and fan-in stays there to the extent the chosen mechanism allows.
Discovery traffic is low-rate; per-package limits bound the rest.

## Validated

Checked against library source and in a network-namespace harness on Debian 13
with kernel 6.12 and nftables 1.1:

- `govee-local-api` binds 4002 and sends its scan from that socket
  (`controller.py`), `pywizlight` binds 38899 (`discovery.py`), and `flux_led`
  binds 48899, falling back to an ephemeral port when it is taken
  (`scanner.py`). `python-kasa`, `aiolifx` and `yeelight` send from an
  ephemeral port.
- A concatenated timed DNAT map admits a reply from an on-link source to the
  container with the device's address as its source, the container's answer
  returns masqueraded, a connection mark set by the DNAT rule carries it
  through a default-drop `forward` chain, and a reply from a new source after
  the element expires is dropped.
- For the routing-plus-injection candidate: a forwarding-cache entry carries a
  routable-group datagram from the LAN interface onto a bridge through a
  default-drop `forward` chain with a set-keyed accept; the TTL rewrite lets a
  TTL 1 datagram through; the bridge learns both member ports from their IGMP
  reports without a querier; and a bridge-family drop on one port keeps the
  datagram off it while the other port receives it.

No part of the outbound relay, the kernel-copy candidate or endpoint
configuration has been run.

## Assumptions to validate

These depend on device and application behaviour, and gate merging the
implementation, not this spec:

- Vendor devices reply to the address and port the query came from.
- Devices accept queries re-sent with the server's address as source and the
  original TTL.
- Embedded mDNS responders answer one-shot queries from a port other than 5353
  by unicast.
- Home Assistant's HomeKit Bridge pairs from an iPhone when configured with the
  server's address and the binding's external port.
- Matter controllers fall back from a device's link-local address.
- Home Assistant's `dhcp` integration works from broadcasts delivered into its
  veth.

## Testing

- Network-namespace harness under `unshare -rn`, following
  `net/policy_routing_model.sh`: a LAN namespace with responder processes for
  each protocol class (multicast query with source-port replies, broadcast
  query, fixed-port reply, group-bound listener, mDNS announcement), the host
  with `lxcbr0` and avahi, and three container namespaces, two granted and one
  not. Assert:
  - queries leave with the gateway's address, the original TTL and an
    unchanged payload, and replies reach the querying socket with the device's
    source address;
  - a query keeps its source port when it is free, and takes an ephemeral port
    when a host socket or a listener-less binding holds it; a binding allocated
    onto a flow's port closes the flow;
  - two granted containers querying the same group, from the same fixed port
    and from ephemeral ports, each receive their replies;
  - a query from 5353 leaves from 5353, avahi keeps receiving the server's
    unicast mDNS, and both multicast and QU-drawn answers reach the package;
  - queries to the `lxcbr0` and gateway subnet broadcast addresses are relayed;
  - both fan-in candidates against the contract: routable and link-local
    groups, TTL 1, limited and subnet broadcast, delivery to group-bound
    listeners, nothing to the ungranted container, and no group delivered as
    broadcast;
  - nothing received on a gateway is re-sent to it or to another gateway;
  - the refused classes are dropped, DHCP broadcasts are delivered and
    outbound 67/68 is dropped;
  - a granted container's multicast or broadcast to UDP 5351 is not re-sent;
  - the IGD endpoint drop holds for every container;
  - UDP flows from a granted container carry the 3600 s conntrack timeout;
  - a gateway address change moves the relay to the new address;
  - revoking a gateway, stopping the package and replacing the gateway each
    close flows, empty the map and leave no conntrack entry that admits a
    reply.
- Application exercises over real bindings, with each package's endpoint
  configuration: a HomeKit Bridge paired from an iPhone, Matter over Wi-Fi and
  over Thread commissioned from the Home Assistant app, Sonos events, and
  Home Assistant's internal URL from a Shelly Gen2 device.
- Device validation on a bench: Govee, Tapo, Kasa, WiZ, Magic Home, Tuya,
  Shelly, Hue (SSDP and mDNS), Chromecast, ESPHome, and DHCP-based discovery in
  Home Assistant.

## Rollout

1. The IGD endpoint drop. Independent of the rest; it closes an existing gap.
2. Grant (the `PackageDataEntry` field, TS bindings and the service page),
   outbound relay and reply admission.
3. Fan-in, once the mechanism is chosen.
4. Advertisement traffic, once policy decision 1 is made, and endpoint
   configuration in the Home Assistant package.

## Open questions

1. Whether packages need a way to request the grant at runtime, so the user
   learns a package wants it without reading its instructions.
2. Whether a restored backup carries the grant or resets it to empty.
3. Eligibility for a LAN that has only global IPv6 addresses.
