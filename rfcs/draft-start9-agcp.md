---
v: 3
title: Authenticated Gateway Control Protocol (AGCP)
abbrev: AGCP
docname: draft-start9-agcp-00
category: info
submissiontype: independent
ipr: trust200902
area: Internet
workgroup: Independent Submission
keyword:
  - gateway
  - NAT
  - port mapping
  - firewall
  - JSON-RPC
  - PCP
date: 2026-09-24
author:
  - ins: A. McClelland
    name: Aiden McClelland
    org: Start9
    email: me@drbonez.dev
normative:
  RFC4632:
  RFC4648:
  RFC5280:
  RFC6762:
  RFC6763:
  RFC6887:
  RFC8259:
  RFC8410:
  RFC8446:
  RFC9110:
  JSONRPC:
    title: JSON-RPC 2.0 Specification
    date: 2013-01-04
    author:
      - org: JSON-RPC Working Group
    target: https://www.jsonrpc.org/specification
  PCP-HOSTNAME:
    title: PCP Hostname Extension for SNI-Demultiplexed Port Mappings
    seriesinfo:
      Internet-Draft: draft-start9-pcp-hostname-00
    date: 2026-06-23
    author:
      - ins: A. McClelland
        name: Aiden McClelland
        org: Start9
    target: https://github.com/Start9Labs/start-technologies/blob/master/rfcs/draft-start9-pcp-hostname.md
informative:
  RFC6335:
  RFC6970:
  RFC7250:
  RFC7652:
  RFC8040:
  RFC8512:
  RFC8519:
  RFC8783:
  RFC9132:
  UPNP-DP:
    title: DeviceProtection:1 Service
    date: 2011-02-24
    author:
      - org: UPnP Forum
    target: http://upnp.org/specs/gw/UPnP-gw-DeviceProtection-v1-Service.pdf
  OPENC2-SLPF:
    title: Specification for the OpenC2 Actuator Profile for Stateless Packet Filtering Version 1.0
    date: 2019-12-11
    author:
      - org: OASIS
    target: https://docs.oasis-open.org/openc2/oc2slpf/v1.0/oc2slpf-v1.0.html
  TR-369:
    title: 'TR-369: User Services Platform (USP)'
    author:
      - org: Broadband Forum
    target: https://usp.technology/
---

--- abstract

This document defines the Authenticated Gateway Control Protocol (AGCP),
with which a host asks the gateway it sits behind to provision inbound
reachability and filtering for the host's own addresses: port mappings,
TLS hostname routes on shared ports, IPv6 firewall pinholes, and
source-address filters. Hosts enroll with the gateway under a public-key
identity carried in mutually authenticated TLS, and every grant a host holds
is confined to addresses the gateway attributes to that host. Commands are
JSON-RPC 2.0 over HTTPS. AGCP carries the self-provisioning model of the
Port Control Protocol (PCP) to an authenticated transport, and a gateway
that implements both refuses PCP and UPnP requests from any host that has
enrolled with AGCP.

--- middle

# Introduction {#intro}

## Motivation {#motivation}

Hosts behind a gateway have long been able to request their own inbound
port mappings through PCP {{RFC6887}} and UPnP IGD, without an operator
configuring each one. Both protocols identify the requesting host by the
source address of its request, and trust that address:

- PCP runs over UDP. Any host on the same link segment can forge the
  source address of another, and so create mappings on its behalf.
  {{RFC6887}} assumes the internal network prevents this. PCP's
  authentication mechanism {{RFC7652}} requires EAP, typically backed by
  a AAA server, and a per-device credential; it has no deployed
  implementations among common PCP clients.
- UPnP IGD runs over TCP, which proves the requester can receive at its
  address, but has no identity beyond that address. UPnP DeviceProtection
  {{UPNP-DP}} added identity and access control and saw no adoption.

Address-based identity is tolerable when the worst a forged request can do
is open a port to the host whose address was forged. It is not tolerable
for operations whose effect on a host is disruptive rather than additive,
such as blocking remote peers from reaching it, or for operations a
gateway operator wants to grant to specific hosts rather than to whatever
occupies an address.

AGCP keeps the self-provisioning model (a host provisions only what
targets itself) and replaces address-based identity with an enrolled
public key, verified on every connection by mutually authenticated TLS.

## Design Goals {#goals}

1. Self-scoped authority. A host can create, alter, or remove only grants
   that target addresses the gateway attributes to it ({{ownership}}). A
   compromised host can damage only its own reachability.
2. Cryptographic identity. A host's authority follows its key, not its
   address; forging a host's address grants nothing.
3. Implementability. Every mechanism is available in common libraries:
   TLS 1.3 with client certificates, HTTP, JSON, JSON-RPC 2.0. No
   AAA infrastructure is needed.
4. PCP compatibility. The mapping model is a superset of PCP MAP's, so a
   gateway can serve both protocols from one set of mapping state, and a
   host can fall back to PCP on gateways that do not implement AGCP.
5. No downgrade. Once a host enrolls, neither it nor its gateway accepts a
   weaker protocol for the same operations ({{coexistence}}).

Administration of the gateway itself (configuring its interfaces, its
users, or grants on behalf of other hosts) is out of scope.

## Requirements Language {#conventions}

{::boilerplate bcp14-tagged}

## Terminology {#terminology}

Gateway:
: the device that forwards traffic between a host and external networks
and implements the AGCP server.

Host:
: a device that implements the AGCP client and holds grants on a gateway.

Binding:
: the attachment through which the gateway reaches a host, as determined by
the gateway ({{bindings}}): a link-layer address on a directly attached
segment, or a tunnel peer.

Identity:
: the public key a host presents in its TLS client certificate.

Fingerprint:
: the SHA-256 digest of an identity's DER-encoded SubjectPublicKeyInfo
({{fingerprints}}).

Enrollment:
: the gateway's record that an identity is authorized to act for a binding.

Owned address:
: an address the gateway attributes to a binding ({{ownership}}).

Grant:
: state a host holds on the gateway: a mapping ({{mappings}}), a route
({{routes}}), a pinhole ({{pinholes}}), or a filter ({{filters}}). Each
grant has a lease ({{leases}}).

Epoch:
: an opaque value that changes whenever the gateway may have lost grant
state.

# Overview {#overview}

This section is informative.

1. The host discovers the gateway's AGCP endpoint ({{discovery}}) and
   connects with TLS 1.3, presenting a self-signed certificate for its
   identity. The gateway presents its own self-signed certificate.
2. The gateway determines the connection's binding from its own view of
   the network ({{bindings}}): the neighbor-table entry for the source
   address on a link, or the tunnel peer that carried it.
3. The host calls `enrollment.enroll`. If the binding is permitted to use
   AGCP and has no enrolled identity, the gateway enrolls the presented
   identity (trust on first use), records it, and releases any PCP or UPnP
   mappings the binding held. The host pins the gateway's identity.
4. The host provisions grants with `mapping.set`, `route.set`,
   `pinhole.set`, and `filter.set`, each targeting an owned address and
   carrying a lease, and renews them before expiry.
5. The host follows `event.wait` to learn of external address changes,
   revoked grants, and gateway state loss, and re-provisions as needed.

# Transport {#transport}

## HTTPS {#https}

AGCP messages are carried in HTTP {{RFC9110}} over TLS. The client sends
each JSON-RPC request ({{jsonrpc}}) as the body of a POST request to the
path `/agcp/v1`, with media type `application/json`. The server returns the
JSON-RPC response as the body of a 200 (OK) response with the same media
type, including when the response is a JSON-RPC error. Requests with any
other method or path receive an HTTP error response and no JSON-RPC
processing.

Clients and servers MUST support HTTP/1.1 and MAY support HTTP/2. HTTP
authentication schemes and cookies are not used; identity comes from TLS
alone ({{identity}}).

## TLS {#tls}

Clients and servers MUST use TLS 1.3 {{RFC8446}} and MUST NOT negotiate an
earlier version. The server MUST request a client certificate and MUST
abort the handshake if the client presents none. Each side presents an
X.509 certificate {{RFC5280}}, normally self-signed, whose only role is to
carry the sender's identity key:

- Each side MUST verify that its peer possesses the private key for the
  certificate (TLS CertificateVerify), and MUST NOT otherwise validate the
  certificate: its issuer, subject, names, extensions, and validity period
  are ignored. A peer's identity is its SubjectPublicKeyInfo.
- Implementations MUST support Ed25519 keys {{RFC8410}} and SHOULD support
  ECDSA with P-256.

The server authenticates the client against its enrollments
({{enrollment}}). The client authenticates the server against the identity
it pinned at enrollment ({{client-pinning}}); before enrollment, the
client accepts any server identity for the purpose of calling
`gateway.info`, `enrollment.enroll`, and `enrollment.status`.

## JSON-RPC {#jsonrpc}

Commands are JSON-RPC 2.0 {{JSONRPC}} requests encoded as JSON {{RFC8259}}.
Every request MUST carry an `id`; clients MUST NOT send notifications.
Parameters are passed by name. Member names use lowerCamelCase; enumerated
string values use lowercase with hyphens.

A request body MAY be a JSON-RPC batch. The server MAY process the calls of
a batch in any order and concurrently; a client needing ordering issues
the calls in separate requests. Each call in a batch succeeds or fails
independently. Servers MUST accept batches of at least 100 calls and
report their limit in `gateway.info` ({{gateway-info}}).

Errors are reported as JSON-RPC error objects. The error codes JSON-RPC
defines apply to malformed requests, unknown methods, and invalid
parameters; AGCP errors use the codes in {{errors}}.

# Identity and Enrollment {#identity}

## Fingerprints {#fingerprints}

An identity's fingerprint is the SHA-256 digest of its DER-encoded
SubjectPublicKeyInfo. In protocol messages a fingerprint is the unpadded
base64url encoding ({{Section 5 of RFC4648}}) of the 32-octet digest.

For display to users, implementations MUST render a fingerprint as the
base32 encoding ({{Section 6 of RFC4648}}) of the digest's first 20
octets, as 32 uppercase characters in eight groups of four separated by
spaces (for example `MZXW 6YTB OI3G 4ZLB MFRG 64TB OVSW 45DP`). Gateways
and hosts that display fingerprints MUST use this form, so that a user can
compare a fingerprint shown on one device against the other.

## Bindings {#bindings}

The gateway determines the binding of each connection from its own state,
never from anything the client asserts:

Link binding:
: for a connection from a directly attached link, the link-layer address
the gateway's neighbor table holds for the connection's source address,
on the interface the connection arrived on. The gateway MUST reject the
connection if the source address has no neighbor entry on the arrival
interface.

Tunnel binding:
: for a connection carried by a tunnel whose encapsulation authenticates
its peers (e.g. WireGuard), the peer that carried it.

A connection with neither binding (for example, one routed to the gateway
from beyond its directly attached networks) MUST be refused with
NOT_AUTHORIZED ({{errors}}).

A link binding is spoofable in isolation. It serves AGCP as a key under
which the gateway records enrollment and ownership, and as the identifier
by which the gateway recognizes the same host in PCP and UPnP requests
({{coexistence}}); it confers no authority without the enrolled identity.

## Permission {#permission}

A gateway maintains, per binding, whether that binding is permitted to
enroll. On shared links, gateways SHOULD default this to not permitted and
let an operator grant it per host. A gateway MAY treat a tunnel peer that
an operator admitted as permitted.

## Enrollment {#enrollment}

A binding has at most one enrolled identity. `enrollment.enroll`
({{m-enroll}}) proceeds as follows:

1. If the binding is not permitted, the call fails with NOT_AUTHORIZED.
2. If the binding has no enrolled identity, the gateway either enrolls the
   presented identity immediately (trust on first use) or, if its policy
   requires operator approval, records the identity as pending.
3. If the presented identity is already enrolled for the binding, the call
   succeeds without change.
4. If a different identity is enrolled for the binding, the gateway MUST
   NOT replace it; it records the presented identity as pending and the
   operator decides. Gateways SHOULD tell the operator that the binding
   was previously enrolled under a different identity.

On enrolling an identity, the gateway MUST, before returning, remove every
mapping the binding holds through PCP, UPnP IGD, or any other protocol
that authenticates by address, and MUST begin refusing those protocols for
the binding ({{coexistence}}). Enrollment MUST be stored durably.

An enrollment ends only when the operator removes it or the enrolled host
calls `enrollment.leave` ({{m-leave}}). When an enrollment ends, the
gateway MUST remove all grants held under it.

Gateways SHOULD display each enrollment's fingerprint to the operator, so
that it can be compared against the fingerprint the host displays.

A gateway MAY enroll one identity for several bindings (a host with
several interfaces); each is enrolled independently.

## Client Pinning {#client-pinning}

On its first successful `enrollment.enroll` call with a gateway, whether
the result is enrolled or pending, the client records a gateway record:
the server's identity and the gateway's attachment identifier, which is
the link-layer address of the gateway on the link the client reached it
through, or the tunnel peer identity of the gateway.

Thereafter the client MUST verify the server's identity against the
record on every connection. A mismatch MUST be treated as a failure and
surfaced to the user; the client MUST NOT re-enroll with, or fall back to
another protocol toward, a gateway whose attachment identifier matches a
record but whose identity does not.

A client MAY remove a gateway record on user action, after
`enrollment.leave`, or on receiving NOT_ENROLLED ({{errors}}) from a server
that authenticated with the recorded identity.

Clients SHOULD use a distinct identity key for each gateway record
({{privacy}}).

# Authorization {#authorization}

## Owned Addresses {#ownership}

The gateway attributes owned addresses to each binding from its own state:

- for a link binding, the IPv4 addresses the gateway has leased or
  reserved for the binding's link-layer address, and the IPv6 addresses
  within prefixes the gateway routes whose neighbor entries resolve to
  that link-layer address;
- for a tunnel binding, the addresses the gateway routes to that peer.

Every grant targets one or more owned addresses. A request naming a target
address that is not owned by the caller's binding MUST fail with
NOT_OWNED. When an address ceases to be owned (a lease released or
reassigned, a neighbor entry resolving elsewhere), the gateway MUST remove
the grants that target it, as PCP requires for mappings whose internal
address is released ({{Section 15 of RFC6887}}, {{Section 5.10 of RFC6970}}).

## Quotas {#quotas}

A gateway MAY limit the number of grants of each kind per enrollment and
the maximum lease it grants, and reports its limits in `gateway.info`. A
request that would exceed a limit fails with QUOTA_EXCEEDED.

# Grants {#grants}

## Identifiers and Replacement {#grant-ids}

Each grant is named by a client-chosen `id`: a string of 1 to 64
characters from `A-Z`, `a-z`, `0-9`, `.`, `_`, and `-`. Identifiers are
scoped to the enrollment and to the grant kind; a mapping and a filter may
share an identifier.

A `*.set` call with an identifier the enrollment already holds for that
kind replaces the grant atomically: on success the new parameters and
lease are in effect; on failure the existing grant is unchanged. A
`*.set` call repeating a grant's parameters renews its lease. `*.remove`
removes a grant; removing an absent grant succeeds.

## Leases {#leases}

Every `*.set` call carries a requested `lifetime` in seconds, from 1 to
the gateway's maximum. The gateway grants a lifetime no greater than
requested and returns it. The gateway MUST remove the grant when its
granted lifetime elapses without renewal. Clients SHOULD renew when half
the granted lifetime has elapsed.

There are no permanent grants; persistent state is the host's to maintain
by renewal. A gateway MAY retain grants across a restart; if it may have
lost any, it MUST change its epoch.

Every `*.set` result and every `gateway.info` and `event.wait` result
carries the current `epoch`. A client that observes a change of epoch
MUST re-issue `*.set` for every grant it intends to hold.

# Methods {#methods}

In the method descriptions, a parameter marked optional may be omitted.
Addresses are strings in their standard textual form; prefixes add a
`/length` suffix {{RFC4632}}. `protocol` is `"tcp"` or `"udp"`.

## gateway.info {#gateway-info}

Callable without enrollment. No parameters. Result:

`protocolVersion`:
: the integer 1.

`epoch`:
: the current epoch, a string.

`fingerprint`:
: the gateway's identity fingerprint.

`capabilities`:
: an array of strings naming the optional features the gateway supports:
`"mapping-port-range"`, `"pinhole"`, `"pinhole-port-translation"`,
`"route-tcp"`, `"route-quic"`, `"route-wildcard"`, `"filter-deny"`,
`"filter-allow"`. The methods of {{mappings}} and {{events}} are
mandatory and are not listed.

`enrollment`:
: the caller's enrollment state, as for `enrollment.status`.

For an enrolled caller, the result also carries:

`externalAddresses`:
: an array of the external IPv4 addresses the gateway can map for the
caller.

`limits`:
: an object with `maxLifetime` (seconds), `maxBatch`, and the per-kind
quotas `maxMappings`, `maxRoutes`, `maxPinholes`, and `maxFilters`.

## enrollment.enroll {#m-enroll}

Callable without enrollment. No parameters. Performs {{enrollment}} for
the connection's binding and identity. Result: `status`, one of
`"enrolled"` or `"pending"`, and `fingerprint`, the gateway's identity
fingerprint.

## enrollment.status {#m-status}

Callable without enrollment. No parameters. Result: `status`, one of
`"enrolled"`, `"pending"`, `"not-enrolled"`, or `"not-permitted"`, for the
connection's binding and identity.

## enrollment.leave {#m-leave}

No parameters. Ends the caller's enrollment ({{enrollment}}). Result: an
empty object.

## Mappings {#mappings}

A mapping forwards inbound connections or datagrams on an external IPv4
address and port range to an owned IPv4 address, as a PCP MAP does
{{RFC6887}}.

### mapping.set

Parameters:

`id`:
: the grant identifier.

`protocol`:
: the transport protocol.

`internalAddress` (optional):
: the owned IPv4 address to forward to. Required when the binding owns
more than one IPv4 address.

`internalPort`:
: the first internal port.

`portCount` (optional):
: the number of consecutive ports, default 1. Values above 1 require the
`mapping-port-range` capability; the gateway MAY grant fewer.

`externalAddress` (optional):
: the suggested external address.

`externalPort` (optional):
: the suggested first external port. When omitted, the gateway chooses.

`preferFailure` (optional):
: a boolean, default false. When true, the call fails with
CANNOT_PROVIDE_EXTERNAL rather than granting an external address or port
other than the suggested one, as with PCP's PREFER_FAILURE option.

`lifetime`:
: the requested lease in seconds.

Result: `id`, `externalAddress`, `externalPort`, `portCount`, `lifetime`,
and `epoch`.

An external port range held by a mapping of another enrollment, or by
another protocol's mapping on the gateway, is unavailable. A mapping on an
external port that also carries routes is that port's fallback mapping
({{routes}}).

### mapping.remove

Parameters: `id`. Result: an empty object.

## Routes {#routes}

A route binds one or more TLS server names, on a shared external port, to
an owned address. The gateway demultiplexes inbound connections on that
port by the server name in the TLS ClientHello, with the semantics that
{{PCP-HOSTNAME}} defines for hostname bindings: its rules for hostname
syntax and matching, wildcards, conflicts, TCP and QUIC demultiplexing,
the fallback mapping, and source address preservation apply, with a route
in the role of a hostname binding and a mapping ({{mappings}}) in the
role of the fallback mapping.

### route.set

Parameters:

`id`:
: the grant identifier.

`protocol`:
: `"tcp"`, or `"udp"` for QUIC, which requires the `route-quic`
capability.

`hostnames`:
: a non-empty array of server names. Names with a leading `*` label
require the `route-wildcard` capability.

`internalAddress` (optional):
: as for `mapping.set`.

`internalPort`:
: the internal port.

`externalAddress` (optional):
: the external address; required when the gateway has more than one.

`externalPort`:
: the external port. Routes are never moved to another external port.

`lifetime`:
: the requested lease in seconds.

Result: `id`, `externalAddress`, `externalPort`, `lifetime`, and `epoch`.

If any name is held on the same external address, port, and protocol by
another enrollment, the call fails with HOSTNAME_TAKEN and no part of the
route is created or changed.

### route.remove

Parameters: `id`. Result: an empty object.

## Pinholes {#pinholes}

A pinhole admits inbound traffic to an owned IPv6 address through the
gateway's firewall. Requires the `pinhole` capability.

### pinhole.set

Parameters:

`id`:
: the grant identifier.

`protocol`:
: the transport protocol.

`internalAddress`:
: the owned IPv6 address.

`internalPort`:
: the first port.

`portCount` (optional):
: as for `mapping.set`.

`externalPort` (optional):
: the port external peers connect to, default `internalPort`. A different
value requires the `pinhole-port-translation` capability, and the gateway
translates the destination port only.

`lifetime`:
: the requested lease in seconds.

Result: `id`, `internalAddress`, `externalPort`, `portCount`, `lifetime`,
and `epoch`.

### pinhole.remove

Parameters: `id`. Result: an empty object.

## Filters {#filters}

A filter restricts which remote sources may reach an owned address through
the gateway. Its vocabulary follows the ACL model of {{RFC8519}} and the
deny and allow actions of {{OPENC2-SLPF}}.

### filter.set

Parameters:

`id`:
: the grant identifier.

`action`:
: `"deny"`, requiring the `filter-deny` capability, or `"allow"`,
requiring the `filter-allow` capability.

`source`:
: an IPv4 or IPv6 prefix.

`protocol` (optional):
: the transport protocol. When omitted, the filter matches all protocols.

`destinationAddress` (optional):
: an owned address of the source's family. When omitted, the filter
applies to every owned address of that family.

`destinationPorts` (optional):
: an object with `start` and `end` ports, inclusive. Requires `protocol`.
When omitted, the filter matches all ports.

`lifetime`:
: the requested lease in seconds.

Result: `id`, `lifetime`, and `epoch`.

A filter's scope is the set of destination address, protocol, and port
combinations it matches. The gateway applies filters to traffic it
forwards, translates, or demultiplexes toward the owned address; it cannot
apply them to traffic that does not transit it.

- A connection or datagram whose source matches a deny filter in scope is
  dropped.
- Where one or more allow filters are in scope, a source that matches
  none of them is dropped, as with PCP's FILTER option. Deny takes
  precedence over allow.
- On creating a deny filter, the gateway SHOULD terminate established
  flows it matches, including connections it relays for routes.
- On a demultiplexed port, the gateway evaluates the filters of the
  enrollment whose route (or fallback mapping) the connection selects,
  before forwarding any data to the host.

A filter never affects traffic toward any address other than the
enrollment's owned addresses.

### filter.remove

Parameters: `id`. Result: an empty object.

## grant.list

Parameters: `kind` (optional), one of `"mapping"`, `"route"`, `"pinhole"`,
or `"filter"`. Result: `epoch`, and `grants`, an array of objects each
carrying `kind`, the parameters of the grant's most recent `*.set` call,
the external values granted, and `remaining`, its remaining lease in
seconds. Only the caller's own grants are listed.

## Events {#events}

### event.wait

Parameters:

`cursor` (optional):
: a cursor from a previous result. When omitted, the call returns
immediately with a current cursor and no events.

`timeout` (optional):
: seconds to wait for an event, from 1 to 300, default 60.

The call returns when at least one event after the cursor exists or the
timeout elapses. Result: `cursor`, `epoch`, `reset`, and `events`, an
array of event objects each carrying a `type`:

`"external-address"`:
: the gateway's external addresses changed. Carries `externalAddresses`.

`"grant-removed"`:
: the gateway removed a grant other than on request or expiry. Carries
`kind`, `id`, and `reason`, one of `"address-released"`, `"operator"`, or
`"conflict"`.

`"enrollment-ended"`:
: the enrollment ended. The client's next call fails with NOT_ENROLLED.

`reset` is true when the gateway could not deliver every event since the
cursor (an unknown or expired cursor, or an epoch change); the client
then MUST re-read its state with `grant.list` and `gateway.info`.
Servers MAY limit concurrent waits per enrollment.

# Errors {#errors}

AGCP errors use the following JSON-RPC error codes. The error object's
`data` member MUST be an object whose `kind` member carries the name
below, and MAY carry further detail.

| Code | Name                    | Meaning                                                                            |
| ---- | ----------------------- | ---------------------------------------------------------------------------------- |
| 1001 | NOT_AUTHORIZED          | The connection has no binding, or the binding is not permitted to enroll.          |
| 1002 | NOT_ENROLLED            | The identity is not enrolled for this binding.                                     |
| 1003 | ENROLLMENT_PENDING      | The identity awaits operator approval.                                             |
| 1004 | NOT_OWNED               | A target address is not owned by the binding.                                      |
| 1005 | CONFLICT                | The requested external address and port are held by another grant.                 |
| 1006 | CANNOT_PROVIDE_EXTERNAL | The suggested external address or port is unavailable and `preferFailure` was set. |
| 1007 | HOSTNAME_TAKEN          | A requested server name is held by another enrollment.                             |
| 1008 | QUOTA_EXCEEDED          | The request exceeds a limit reported in `gateway.info`.                            |
| 1009 | UNSUPPORTED             | The request uses a capability the gateway does not advertise.                      |

Every method other than `gateway.info`, `enrollment.enroll`, and
`enrollment.status` fails with NOT_ENROLLED or ENROLLMENT_PENDING for a
caller that is not enrolled.

# Discovery {#discovery}

Gateways advertise AGCP by DNS-Based Service Discovery {{RFC6763}}, with
service type `_agcp._tcp`, over Multicast DNS {{RFC6762}} on their
attached links and MAY also answer it through their unicast DNS service.
The SRV record gives the port. The TXT record carries `v=1` and MAY carry
`fp=` with the gateway's fingerprint; a client MUST NOT treat the TXT
fingerprint as authentication.

A client selects the gateway that is its default router on the link, or
the tunnel peer that routes its traffic. Gateways MAY additionally provide
the endpoint by configuration, for example alongside a tunnel's peer
configuration.

A PCP server MAY also signal AGCP support in its responses by an
implementation-specific option. Neither the presence nor the absence of
any discovery signal overrides a gateway record ({{client-pinning}}).

# Coexistence with Address-Authenticated Protocols {#coexistence}

## Gateway Behavior {#coexistence-gateway}

A gateway that implements AGCP alongside PCP, UPnP IGD, or another protocol
that identifies hosts by address:

- MUST keep one mapping state across all of them, so that an external
  port held through one protocol is unavailable through another;
- MUST refuse, from a binding with an AGCP enrollment, every request of
  those protocols that creates, alters, or removes a mapping or other
  forwarding state, answering PCP requests with NOT_AUTHORIZED and UPnP
  actions with error 606 (Action not authorized). The gateway identifies
  the binding of such a request as in {{bindings}};
- MUST keep refusing them until the enrollment ends ({{enrollment}}).

Requests that change no state, such as PCP ANNOUNCE, are unaffected.
Mappings created by the operator are unaffected.

## Client Behavior {#coexistence-client}

A client that holds a gateway record ({{client-pinning}}) MUST NOT use PCP,
UPnP IGD, or another address-authenticated protocol toward that gateway.
When it cannot reach the gateway over AGCP, it MUST report the failure
rather than fall back. Toward gateways it holds no record for, a client
MAY use those protocols.

# Security Considerations {#security}

First contact:
: Enrollment by trust on first use accepts the first identity to enroll
for a permitted binding. An attacker must hold the binding's address
long enough to complete a TLS handshake, which on a shared link means
actively diverting the host's traffic by ARP or Neighbor Discovery
spoofing, and must do so before the genuine host first enrolls. The
genuine host's later enrollment then appears to the operator as a pending
identity for an already enrolled binding. The permission of
{{permission}} limits the exposure to hosts the operator chose to trust,
and gateways MAY require operator approval of every enrollment.

After enrollment:
: A forged address gains nothing: AGCP requires the enrolled key, and the
gateway refuses address-authenticated protocols for the binding
({{coexistence}}). Traffic diversion on the link yields an attacker only
TLS it cannot complete.

Downgrade:
: An attacker able to block AGCP cannot induce an enrolled client to use
PCP or UPnP ({{coexistence-client}}), and cannot use them itself for the
binding ({{coexistence-gateway}}). A client that has never enrolled with a
gateway has no such protection.

Blast radius:
: Every grant targets owned addresses, and filters apply only to traffic
toward them, so a compromised host or stolen key can affect only the
reachability of that host. Operators revoke a compromised identity by
ending its enrollment.

Link bindings on wired segments:
: Link-layer addresses are forgeable. A binding is used only to locate
enrollment and ownership; it never substitutes for the enrolled identity.

Resource exhaustion:
: Gateways bound per-enrollment state through the quotas of {{quotas}},
and SHOULD bound concurrent connections, pending enrollments, and
concurrent `event.wait` calls per binding. The resource considerations
of {{PCP-HOSTNAME}} apply to routes.

Server names:
: Server names received in ClientHellos are untrusted input and are
handled as {{PCP-HOSTNAME}} requires.

# Privacy Considerations {#privacy}

An identity is a stable identifier. A host that used one key with every
gateway would let gateways, or anyone who observes enrollments, correlate
the host across networks. Clients SHOULD use a distinct key per gateway
record.

A gateway's fingerprint in its DNS-SD TXT record identifies the gateway to
every host on the link; gateways MAY omit it.

# IANA Considerations {#iana}

IANA is requested to register the following in the "Service Name and
Transport Protocol Port Number Registry" {{RFC6335}}:

| Field              | Value                                  |
| ------------------ | -------------------------------------- |
| Service Name       | agcp                                   |
| Transport Protocol | tcp                                    |
| Assignee           | Start9                                 |
| Contact            | Aiden McClelland <me@drbonez.dev>      |
| Description        | Authenticated Gateway Control Protocol |
| Reference          | This document                          |
| Port Number        | none                                   |
| Assignment Notes   | Discovered through DNS-SD SRV records  |

--- back

# Relationship to Existing Protocols {#related}

This section is informative. It records why AGCP is a new protocol rather
than a profile of an existing one. No existing protocol combines
self-scoped authority with an enrolled cryptographic identity.

PCP {{RFC6887}} with authentication {{RFC7652}}:
: Self-scoped, but its identity is the request's source address unless
EAP authentication is deployed, which requires per-device credentials and
typically a AAA server, and which common PCP clients do not implement.
PCP messages are limited to 1100 octets, which bounds how many filters one
request can carry. AGCP adopts PCP's mapping model, lease semantics, and
epoch recovery.

UPnP IGD with DeviceProtection {{UPNP-DP}}:
: IGD is self-scoped by convention and authenticated only by address;
DeviceProtection adds identity and roles but was not adopted.

DOTS {{RFC9132}} {{RFC8783}}:
: Self-scoped with strong identity, and its data channel's drop and accept
lists inspired AGCP's filters, but its scope is DDoS mitigation; it does
not provision mappings, and its CoAP, DTLS, and RESTCONF layers exceed
what a home gateway and its hosts need.

OpenC2 {{OPENC2-SLPF}}:
: Its Stateless Packet Filtering profile expresses AGCP's filters, and
AGCP follows its vocabulary, but it has no mapping profile and leaves
authentication to its transports.

RESTCONF {{RFC8040}} with YANG NAT {{RFC8512}} and ACL {{RFC8519}} models:
: Provides identity and standard data models, but its access control
is expressed over data paths; it cannot confine a client to entries that
target that client's own addresses.

USP {{TR-369}}:
: The Broadband Forum's gateway management protocol has certificate
identity, role-based access control, notifications, and a data model with
port mappings and firewall rules. It is designed for a management
controller administering the gateway, not for a host provisioning its own
reachability, and its message, transport, and data-model layers are a
substantial undertaking for a host implementer.

TLS with raw public keys {{RFC7250}} would carry AGCP's identities more
directly than self-signed certificates; AGCP uses certificates because
client-certificate support is universal among TLS libraries.

# Example {#example}

This section is informative. A host enrolls, maps TCP port 8333, routes a
hostname on the shared port 443, and denies a prefix, on a gateway that
does not support deny filters. Each block shows a request body followed by
its response body.

```json
{"jsonrpc": "2.0", "id": 1, "method": "enrollment.enroll", "params": {}}

{"jsonrpc": "2.0", "id": 1,
 "result": {"status": "enrolled",
            "fingerprint": "q0W9JQf0c9mYq6o5nqj1kIhZcT0Hc0v8b2yF2d9x2cA"}}
```

```json
[{"jsonrpc": "2.0", "id": 2, "method": "mapping.set",
  "params": {"id": "bitcoin", "protocol": "tcp", "internalPort": 8333,
             "externalPort": 8333, "lifetime": 3600}},
 {"jsonrpc": "2.0", "id": 3, "method": "route.set",
  "params": {"id": "web", "protocol": "tcp",
             "hostnames": ["cloud.example.com"],
             "internalPort": 443, "externalPort": 443, "lifetime": 3600}},
 {"jsonrpc": "2.0", "id": 4, "method": "filter.set",
  "params": {"id": "ban-1", "action": "deny", "source": "198.51.100.0/24",
             "lifetime": 86400}}]

[{"jsonrpc": "2.0", "id": 2,
  "result": {"id": "bitcoin", "externalAddress": "203.0.113.7",
             "externalPort": 8333, "portCount": 1, "lifetime": 3600,
             "epoch": "c1"}},
 {"jsonrpc": "2.0", "id": 3,
  "result": {"id": "web", "externalAddress": "203.0.113.7",
             "externalPort": 443, "lifetime": 3600, "epoch": "c1"}},
 {"jsonrpc": "2.0", "id": 4,
  "error": {"code": 1009, "message": "unsupported",
            "data": {"kind": "UNSUPPORTED", "capability": "filter-deny"}}}]
```

# Implementation Status {#impl-status}

No implementation exists yet. Implementations are planned in StartOS
(client) and in its StartTunnel and StartWRT gateways, which already serve
PCP, UPnP IGD, and {{PCP-HOSTNAME}} from shared mapping state; AGCP is
planned as a further front end to that state.
