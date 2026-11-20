---
v: 3
title: AGCP Tunnel Encapsulation Extension
abbrev: AGCP Tunnel Encapsulation
docname: draft-start9-agcp-tunnel-encap-00
category: info
submissiontype: independent
ipr: trust200902
area: Internet
workgroup: Independent Submission
keyword:
  - gateway
  - tunnel
  - MTU
  - fragmentation
  - GRE
  - WireGuard
date: 2026-11-20
author:
  - ins: A. McClelland
    name: Aiden McClelland
    org: Start9
    email: me@drbonez.dev
normative:
  RFC791:
  RFC792:
  RFC2784:
  RFC4443:
  RFC6040:
  RFC7676:
  RFC8200:
  AGCP:
    title: Authenticated Gateway Control Protocol (AGCP)
    seriesinfo:
      Internet-Draft: draft-start9-agcp-00
    date: 2026-11-13
    author:
      - ins: A. McClelland
        name: Aiden McClelland
        org: Start9
    target: https://github.com/Start9Labs/start-technologies/blob/master/rfcs/draft-start9-agcp.md
informative:
  RFC1191:
  RFC4459:
  RFC4821:
  RFC4963:
  RFC8086:
  RFC8201:
  RFC8899:
  RFC8900:
  WIREGUARD:
    title: 'WireGuard: Next Generation Kernel Network Tunnel'
    date: 2017
    author:
      - ins: J. A. Donenfeld
        name: Jason A. Donenfeld
    target: https://www.wireguard.com/papers/wireguard.pdf
---

--- abstract

This document extends the Authenticated Gateway Control Protocol (AGCP)
for hosts that reach their gateway through an authenticated tunnel, such
as WireGuard. The two endpoints of the tunnel measure the largest packet
the tunnel's path delivers, and may agree to carry the host's traffic in a
GRE encapsulation that they fragment and reassemble inside the tunnel.
Traffic through the tunnel then sees a fixed MTU while delivery packet
fragments are sized to the tunnel's measured delivery capacity. The
underlying tunnel transport retains its own fragmentation behavior.

--- middle

# Introduction {#intro}

## Motivation {#motivation}

A gateway such as a VPS that publishes a host's services to the Internet
often reaches that host through an authenticated tunnel. Each tunnel
encapsulation consumes part of the path's MTU: WireGuard {{WIREGUARD}}
adds 60 bytes over IPv4 and 80 over IPv6. Its customary interface MTU of
1420 assumes the underlying path carries 1500-byte packets. Paths that
cross PPPoE, cellular networks, cloud overlays, or another VPN carry less,
and a tunnel nested in two other WireGuard tunnels over IPv6 can carry at
most 1260 bytes.

When a tunnel's interface MTU exceeds what its path delivers, packets near
full size are lost while small ones arrive. Connections establish, small
requests succeed, and transfers that need a full-size packet stall: a TLS
certificate flight, a large ClientHello, or an HTTP response body.
Standard path MTU discovery {{RFC1191}} {{RFC8201}} does not recover:

- An ICMP Packet Too Big message for an oversized outer tunnel packet
  reaches the tunnel endpoint that sent it, not the host that sent the
  inner packet. WireGuard does not relay it inward.
- Such messages are unauthenticated, and networks often filter them.
  A tunnel endpoint that lowered its MTU on receipt of one would let any
  off-path attacker degrade the tunnel.
- The hosts on either side of a gateway that forwards public traffic into
  the tunnel are not tunnel endpoints. They negotiate TCP segment sizes
  for an ordinary path. A gateway can clamp forwarded TCP's MSS to its
  tunnel MTU, but only an MTU it knows, and clamping does not cover UDP.

Fragmentation in transit does not recover reliably either. Routers never
fragment forwarded IPv6, and IPv4 fragments are dropped by many
firewalls, NATs, and cloud networks {{RFC8900}}. The narrowest link of a
nested tunnel is commonly a router of another operator in the middle of
the path, where fragmentation is least dependable.

IPv6 compounds the failure. A link carrying IPv6 must have an MTU of at
least 1280 {{RFC8200}}. A tunnel path below that cannot carry IPv6 at all
unless a layer beneath IPv6 fragments and reassembles, and WireGuard has
no fragmentation of its own.

## Approach {#approach}

This extension applies the pattern {{RFC4459}} describes for tunnels
that cannot rely on the path MTU: fragment at the tunnel ingress and
reassemble at the egress. The two endpoints of an AGCP tunnel binding:

1. measure the tunnel's path MTU with probes through the tunnel itself,
   relying on no unauthenticated signal ({{measurement}});
2. size their tunnel to the measurement, clamping forwarded TCP to it
   ({{sizing}}); and
3. optionally agree, through an AGCP grant, to carry the binding's
   traffic in GRE {{RFC2784}} with a fixed MTU, fragmenting the GRE
   delivery packets before the tunnel encrypts them
   ({{encapsulation}}).

Delivery packet fragments travel only inside the authenticated tunnel.
The hosts and services beyond the tunnel see a fixed MTU, IPv6 included,
provided the tunnel carries those delivery fragments. This does not
exclude fragmentation of the tunnel transport's outer packets
({{probes}}); those fragments remain subject to the hazards of
{{RFC8900}}.

## Scope {#scope}

This extension applies only to tunnel bindings ({{AGCP}}, Section
"Bindings"): the AGCP host is the peer at the far end of the tunnel that
carried its connection. That peer can be a server that uses the gateway
directly, or a router whose upstream interface is the tunnel and which
forwards the decapsulated traffic to hosts on its own networks. In the
second case the router is the AGCP host for this extension, and the hosts
behind it need not implement anything.

Encapsulation between a gateway and a host that sits behind a third
party's router, where the tunnel ends at that router, is out of scope.
It would require an encapsulation that traverses NAT, such as GRE-in-UDP
{{RFC8086}}, and is left to a future extension.

## Requirements Language {#conventions}

{::boilerplate bcp14-tagged}

## Terminology {#terminology}

This document uses the terms of {{AGCP}}, and the following:

Tunnel:
: the authenticated tunnel that carries a tunnel binding, for example a
WireGuard peer.

Tunnel endpoint:
: the gateway or the host, each at one end of the tunnel.

Tunnel address:
: an address an endpoint holds on its tunnel interface.

Tunnel path MTU:
: the size of the largest IP packet an endpoint can send into the tunnel
that its peer receives, as measured in {{measurement}}.

Shim:
: the GRE encapsulation of {{encapsulation}}, between the two tunnel
endpoints, inside the tunnel.

Shim MTU:
: the MTU of the shim interface: the largest packet the shim accepts
from the traffic it carries.

Delivery packet:
: an IP packet carrying a GRE header and one encapsulated packet, before
fragmentation or after reassembly.

# Overview {#overview}

This section is informative.

1. The host and the gateway each measure the tunnel path MTU by sending
   probes to the other's tunnel address ({{measurement}}).
2. Each sizes its sending path into the tunnel to the measurement, and
   the gateway clamps forwarded TCP to it ({{sizing}}).
3. The host reads the gateway's capabilities with `gateway.info`. When
   the gateway offers `tunnel-encap`, the host MAY call
   `tunnel-encap.set`, reporting its measurement and requesting a shim
   MTU ({{m-encap-set}}).
4. Both endpoints create the shim, route the binding's traffic through
   it, and fragment delivery packets larger than the tunnel path MTU
   before the tunnel encrypts them ({{encapsulation}}).
5. The host renews the grant before its lease expires, and re-measures
   when the path may have changed.

Steps 1 and 2 need no AGCP exchange and improve the tunnel on their own.

# Tunnel Path MTU Measurement {#measurement}

## Probes {#probes}

An endpoint measures the tunnel path MTU with ICMP Echo Requests
{{RFC792}} when both endpoints have IPv4 tunnel addresses, or ICMPv6 Echo
Requests {{RFC4443}} otherwise, sent through the tunnel to the peer's
tunnel address of the common family. A probe of size N is an Echo Request
whose IP packet is N bytes long. A probe succeeds when the matching Echo
Reply arrives.

Requests and replies MUST enter the tunnel as single, unfragmented IP
packets. Both IPv4 messages MUST set the Don't Fragment flag; neither
IPv6 message may contain a Fragment header. Probes and their replies
travel through the tunnel, never through the shim.

Endpoints MUST provide a probe path that can send and receive packets up
to the local maximum of {{search}}, independently of the ordinary-traffic
limit of {{sizing}} and cached PMTU limits. A local send rejection MUST
NOT count as a failed path probe. Tunnel endpoints implementing this
extension MUST reply to Echo Requests from their peer up to that local
maximum, even when their ordinary-traffic limit is smaller. They MUST
NOT rate-limit those replies below one per probe interval that
{{search}} permits.

Because the reply is the same size as the request, a successful probe
shows that packets of N bytes cross the tunnel in both directions
without fragmentation of those inner IP packets. Their fragmentation
restrictions do not apply to the tunnel's outer packets or those of a
nested tunnel. A successful probe therefore measures delivery capacity,
not the absence of outer IP fragmentation.

## Search {#search}

The endpoint's local maximum is the MTU of the interface that carries the
tunnel's outer packets minus the tunnel's per-packet overhead for that
interface's address family. The initial probe size is the lower of 1280
and that local maximum. The endpoint searches toward the local maximum
when the initial size succeeds, for example by bisection.

A size fails only after three consecutive probes of that size go
unanswered, each sent at least one second after the previous one. A
single lost probe is not evidence of an MTU limit. The result is the
largest size that succeeded.

When the initial size fails, the endpoint searches below it, down to 576
for either probe address family. When the local maximum is below 576 or
no probe of any size succeeds, the result is unknown: the endpoint keeps
its previous tunnel sizing and MUST NOT treat the failure as a
measurement. Some tunnel peers, such as a phone's WireGuard client,
answer no probes.

An endpoint MUST NOT lower its tunnel path MTU on receipt of an ICMP
Destination Unreachable or Packet Too Big message that did not arrive
through the tunnel.

## Re-Measurement {#remeasure}

An endpoint MUST measure again:

- when the tunnel completes a handshake after a period without one, or
  the peer's outer endpoint address changes;
- after it observes loss of packets at the tunnel path MTU while smaller
  packets are delivered, the black-hole condition of {{RFC8899}}; and
- at least every 600 seconds, starting from the current result, to
  detect a path MTU increase.

Re-measurement after black-hole detection starts with the initial probe
size of {{search}}.

Probing a size above the current result does not change the sizing in
use until the probe succeeds.

# Tunnel Sizing {#sizing}

An endpoint limits ordinary traffic sent to a peer through the tunnel to
its current tunnel path MTU. The limit MUST cover every destination
routed through that peer, including networks behind a router acting as
the host, not just the peer's own tunnel addresses.

The tunnel interface MUST remain large enough for the probes and replies
of {{probes}}. Endpoints can apply the ordinary-traffic limit with
per-peer routes or equivalent forwarding controls, while the probe path
bypasses that limit. Raising the probe ceiling MUST NOT raise the limit
on ordinary traffic before a larger probe succeeds.

An endpoint with IPv6 tunnel addresses MUST NOT size an interface or
route below 1280. When its tunnel path MTU is below 1280, it keeps 1280
for IPv6 and uses the shim ({{encapsulation}}) or reports that IPv6
through the tunnel is unusable.

An endpoint that forwards TCP over the binding MUST clamp the MSS in
both directions, including SYN-ACKs, to fit the tunnel's ordinary-traffic
limit. When the shim is in use, the endpoint SHOULD instead clamp to the
lower of the shim MTU and the delivery-packet limit of {{m-encap-set}}
minus the delivery IP and GRE headers, to avoid delivery fragmentation.
The MSS also excludes the inner IP and TCP headers. Other path
constraints can require a smaller MSS.

Endpoints SHOULD enable packetization-layer path MTU discovery
{{RFC4821}} for TCP connections they terminate themselves, including AGCP
connections, so that those connections recover from an MTU that is
temporarily too large.

# Shim Encapsulation {#encapsulation}

## Delivery Packets {#delivery}

The shim is GRE {{RFC2784}} with no checksum, key, or sequence number.
The delivery packet's protocol type is 0x0800 for an encapsulated IPv4
packet and 0x86DD for IPv6.

The delivery packet's source and destination are the endpoints' shim
addresses, returned by `tunnel-encap.set` ({{m-encap-set}}): one IPv4
tunnel address of each endpoint, or, when either endpoint has none, one
IPv6 tunnel address of each, with GRE in IPv6 as {{RFC7676}} describes.

The encapsulating endpoint copies the inner packet's ECN field to the
delivery header, and the decapsulating endpoint combines them, as
{{RFC6040}} specifies for the normal mode.

## Fragmentation and Reassembly {#fragmentation}

The encapsulating endpoint MUST fragment a delivery packet larger than
its tunnel path MTU before the tunnel encrypts it, regardless of any
fragmentation restriction on the inner packet:

- An IPv4 delivery packet MUST have Don't Fragment clear, and is
  fragmented as {{RFC791}} specifies.
- An IPv6 delivery packet is fragmented by its source with a Fragment
  header, as {{RFC8200}} specifies.

The decapsulating endpoint reassembles delivery packets before it
decapsulates them, and MUST support reassembly of delivery packets up to
the shim MTU plus the delivery headers. The inner packet is then delivered
or forwarded unchanged.

An endpoint MUST NOT send a delivery packet, or a fragment of one, through
the shim itself, and MUST NOT send a delivery packet to the peer except
through the tunnel.

## Shim MTU {#shim-mtu}

The shim MTU is fixed for the life of a grant, independent of the tunnel
path MTU. Hosts SHOULD request 1376: a delivery packet carrying a
1376-byte packet is 1400 bytes over IPv4 and 1420 over IPv6, which a
tunnel with WireGuard's customary 1420-byte MTU carries without
fragmentation.

A shim MTU at or above 1280 carries IPv6 when the tunnel can deliver the
resulting fragments.

## Routing {#routing}

While a grant is active:

- Each endpoint MUST accept, from its peer through the tunnel, both
  delivery packets and packets without the shim.
- The host routes the binding's traffic into the shim once
  `tunnel-encap.set` succeeds and its receive-side shim is ready.
- The gateway routes the binding's traffic into the shim once it has
  received a valid delivery packet from the host under the grant, and
  until the grant ends.

The binding's traffic is everything the endpoint would otherwise route to
the peer through the tunnel, except AGCP connections, path-MTU probes and
their replies, and delivery packets. Both directions of every AGCP
connection MUST bypass the shim, whether or not either endpoint retains
a grant. At a router acting as the host, the binding's traffic includes
traffic it forwards for the hosts on its networks.

After each successful `tunnel-encap.set`, the host MUST send an ICMP Echo
Request inside the shim once its receive-side shim is ready, without
waiting for ordinary traffic. This activation request is distinct from a
path-MTU probe. The gateway MUST reply inside the shim; the host retries
an unanswered activation request at intervals of at least one second
until it receives the matching reply or the grant ends. An activation
exchange MUST obey the shim MTU and the delivery fragmentation rules of
{{fragmentation}}. The gateway's first valid delivery packet from the
host can be an activation request.

When a grant ends, each endpoint stops sending delivery packets to its
peer. The host removes the grant, with `tunnel-encap.remove`, only after
it has stopped routing traffic into the shim.

A decapsulated packet is subject to every check the endpoint applies to a
packet arriving through the tunnel from that peer, including the tunnel's
filtering of source addresses, such as WireGuard's allowed IPs. An
endpoint MUST discard a decapsulated packet whose source address the
tunnel would not accept from that peer.

An endpoint MUST discard delivery packets from a peer that holds no
active grant.

# AGCP Extension {#agcp}

## Capability {#capability}

This extension adds the capability `"tunnel-encap"`. A gateway MUST NOT
grant it to an enrollment whose binding is not a tunnel binding. A host
SHOULD request a shim only when its tunnel path MTU is below the shim
MTU plus the delivery headers, or below 1280 when it carries IPv6.

The `gateway.info` result's `limits` object adds `maxShimMtu`, the
largest shim MTU the gateway grants, for an enrollment with the
capability.

## Grant {#grant}

This extension adds the grant kind `"tunnel-encap"`, which follows the
identifier, replacement, and lease rules of {{AGCP}}, Section "Grants".
An enrollment holds at most one `tunnel-encap` grant; a set with a new
identifier while one is held fails with QUOTA_EXCEEDED. `grant.list`
accepts `"tunnel-encap"` as its `kind`, and `event.wait` reports its
removal with a `"grant-removed"` event.

## tunnel-encap.set {#m-encap-set}

Requires the `tunnel-encap` capability.

Parameters:

`id`:
: the grant identifier.

`encapsulation`:
: `"gre"`.

`shimMtu`:
: the requested shim MTU, from 1280 to 65000.

`pathMtu` (optional):
: the host's current tunnel path MTU, or omitted when unknown.

`lifetime`:
: the requested lease in seconds.

Result:

`id`, `encapsulation`, `lifetime`, and `epoch`:
: as for other grants.

`shimMtu`:
: the granted shim MTU, no greater than requested or than `maxShimMtu`.

`pathMtu`:
: the gateway's current tunnel path MTU toward the host, or null when
unknown.

`hostShimAddress` and `gatewayShimAddress`:
: the delivery packet addresses of the host and of the gateway, both
of the same family, chosen from the endpoints' tunnel addresses.

Each endpoint fragments at its own measurement, or its previous tunnel
sizing when that measurement is unknown, following {{search}}. It SHOULD
use the lower of that limit and the peer's measurement when the latter
is known.

A set that repeats the parameters of the held grant renews it; `pathMtu`
is not part of that comparison.

## tunnel-encap.remove {#m-encap-remove}

Parameters: `id`. Result: an empty object.

## Epoch Changes {#epoch}

A gateway that may have lost the grant changes its epoch, per {{AGCP}}.
Until the host sets the grant again, the gateway discards the host's
delivery packets. Direct AGCP connections remain available under
{{routing}}. A host that observes an epoch change MUST stop routing
traffic into the shim until `tunnel-encap.set` succeeds again.

# Security Considerations {#security}

Source filtering:
: Without the check of {{routing}}, a peer could place in a delivery
packet an inner packet with a source address its tunnel would refuse,
bypassing the tunnel's source filtering. Endpoints MUST apply the same
filtering to decapsulated packets.

Unauthenticated signals:
: Measurement uses probes through the authenticated tunnel and ignores
ICMP errors from outside it. An off-path attacker cannot lower an
endpoint's tunnel path MTU. An on-path attacker that drops tunnel packets
can lower it to 1280 or below, which costs efficiency, not reachability:
it can already drop all of the tunnel's traffic.

Reassembly resources:
: A peer can send fragments that never complete. Endpoints bound the
memory and time they spend reassembling delivery packets per peer, as
for any IP reassembly. Only peers holding a grant reach reassembly at
all.

Misassembly:
: IPv4 delivery packets carry a 16-bit Identification. At high rates of
fragmented traffic, an identifier can recur before an earlier datagram's
fragments are reassembled, and fragments can be misassembled
{{RFC4963}}. Encapsulating endpoints SHOULD keep a separate
identification counter for each peer, and the gateway's TCP clamp
({{sizing}}) reduces TCP fragmentation. Inner transport checksums detect
most misassembly.

Loss amplification:
: Loss of any fragment loses the whole delivery packet. On a lossy path,
fragmented traffic sees a higher loss rate than unfragmented traffic.

Probing:
: Probes reveal no more than the tunnel's traffic already reveals: their
outer packets are encrypted tunnel packets of the probed size.

# IANA Considerations {#iana}

This document has no IANA actions.

--- back

# Relationship to Existing Mechanisms {#related}

This section is informative.

GRE over IPsec with fragmentation before encryption:
: IPsec gateways commonly carry GRE inside IPsec and fragment before
encrypting, so that fragments are never exposed outside the
encryption. This extension applies the same arrangement to WireGuard,
whose protocol has no means to negotiate it.

Datagram PLPMTUD {{RFC8899}}:
: The measurement of {{measurement}} follows its principles, probing with
the packets whose delivery matters and relying on no ICMP. It is
specified here with ICMP Echo because the endpoints probe the tunnel, not
a transport protocol of their own.

Packetization-layer PMTUD for TCP {{RFC4821}}:
: Recovers TCP connections that end at a tunnel endpoint, but not
connections the gateway forwards between other hosts, nor UDP.

GRE-in-UDP {{RFC8086}}:
: Traverses NAT. A future extension could carry the shim to a host behind
a third party's router.
