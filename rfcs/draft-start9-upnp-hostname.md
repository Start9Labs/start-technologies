---
v: 3
title: UPnP WANIPConnection Actions for Hostname-Based Port Mappings
abbrev: UPnP Hostname Mappings
docname: draft-start9-upnp-hostname-00
category: std
submissiontype: IETF
consensus: false
ipr: trust200902
area: Internet
workgroup: Individual Submission
keyword:
  - UPnP
  - SNI
  - NAT
  - port mapping
  - WANIPConnection
date: 2026-08-31
author:
  - ins: A. McClelland
    name: Aiden McClelland
    org: Start9
    email: me@drbonez.dev
normative:
  RFC6066:
  UDA2:
    title: UPnP Device Architecture 2.0
    date: 2020-04-17
    author:
      - org: Open Connectivity Foundation
    target: https://openconnectivity.org/wp-content/uploads/2023/10/UPnP-arch-DeviceArchitecture-v2.0-20200417.pdf
  WANIPC1:
    title: WANIPConnection:1 Service
    date: 2001-11-12
    author:
      - org: UPnP Forum
    target: http://upnp.org/specs/gw/UPnP-gw-WANIPConnection-v1-Service.pdf
informative:
  RFC6887:
---

abstract

This document defines two vendor-defined actions for the UPnP
WANIPConnection:1 service. The actions create and delete finite-lived TCP port
mappings selected by the hostname in the TLS Server Name Indication (SNI)
extension. Multiple internal hosts can thereby share an external address and
port while retaining end-to-end TLS. The actions are discoverable through the
existing WANIPConnection service description and do not change the behavior of
standard WANIPConnection actions.

--- middle

# Introduction {#intro}

A conventional UPnP Internet Gateway Device (IGD) port mapping assigns an
external port to one internal endpoint. A gateway that can inspect the TLS
ClientHello without terminating TLS can instead use SNI to select among
multiple internal endpoints sharing that external port.

This document specifies vendor-defined WANIPConnection:1 actions for creating
and deleting such hostname mappings. The extension uses the action-extension
mechanism defined by {{UDA2}}. It does not modify `AddPortMapping`,
`DeletePortMapping`, or any other standard action in {{WANIPC1}}.

The actions provide a UPnP control path for the same class of mapping that can
be represented by a PCP hostname option. The two control protocols are
independent; support for one does not imply support for the other.

## Requirements Language {#conventions}

{::boilerplate bcp14-tagged}

## Terminology {#terminology}

This document uses the UPnP and WANIPConnection terminology of {{UDA2}} and
{{WANIPC1}}. In addition:

Control point:
: the UPnP client invoking a WANIPConnection action.

Hostname mapping:
: a finite-lived association from an external IPv4 address, TCP port, and
hostname to an internal IPv4 address and TCP port.

Mapping owner:
: the control point authorized to create, renew, and delete a hostname mapping.
Ownership is scoped by the control point's source address and the internal port
in addition to the externally visible mapping tuple.

SNI demultiplexer:
: the gateway function that reads the TLS ClientHello, selects a hostname
mapping using the SNI `HostName`, and forwards the TCP stream without
terminating TLS.

# Protocol Overview {#overview}

A supporting gateway adds two actions to the action list of its existing
`urn:schemas-upnp-org:service:WANIPConnection:1` service:

- `X_START9_AddHostnameMapping`
- `X_START9_DeleteHostnameMapping`

A control point discovers support by retrieving the WANIPConnection SCPD as
specified by {{UDA2}} and checking that both action names are present. A
control point that requires the complete mapping lifecycle MUST treat the
extension as unsupported unless both actions are advertised.

The control point invokes the actions using the WANIPConnection control URL and
SOAP rules already used for standard actions. A successful add creates or
renews a finite lease. A successful delete removes the mapping owned by the
requesting control point. Both actions apply only to TCP mappings.

The extension adds declarations to an existing SCPD but does not alter standard
action declarations or semantics. Control points that do not recognize these
actions continue to use the standard WANIPConnection actions. A gateway MUST
NOT infer hostname-mapping support from the presence of ordinary UPnP IGD
support.

# Common Processing Rules {#common}

## Transport and Lease Requirements {#transport-lease}

Both actions defined by this document operate exclusively on TCP hostname
mappings. A control point invoking either action MUST set `NewProtocol` to
`TCP`. A gateway MUST reject any other value with error 402, `Invalid Args`.

Mappings managed by either action MUST be finite-lived. A gateway MUST NOT use
these actions to create, identify, renew, or delete a permanent mapping. The
maximum granted lifetime is 3600 seconds. A control point MUST renew a mapping
before the granted lifetime expires if continued operation is required.

## Hostname Syntax and Matching {#hostname}

`NewHostname` carries an ASCII hostname from 1 through 255 octets. It consists
of non-empty labels separated by dots. Each label contains only ASCII letters,
digits, or hyphens. The name has no trailing dot. Internationalized names are
carried as A-labels. Comparison is case-insensitive, and a gateway SHOULD store
the lowercase form.

A leading label consisting only of `*` is permitted and matches exactly one
left-most label. An exact hostname mapping takes precedence over a wildcard
mapping. A `*` appearing anywhere else is invalid.

Literal IPv4 and IPv6 addresses, empty labels, non-ASCII characters, leading or
trailing dots, and names longer than 255 octets are invalid. A gateway MUST
reject an invalid `NewHostname` with error 402, `Invalid Args`.

## Mapping Identity and Ownership {#ownership}

For conflict detection, a hostname mapping is selected by the external IPv4
address, external TCP port, and normalized hostname. Only one internal target
can hold that selection at a time.

The gateway determines the external IPv4 address associated with the control
point. The gateway MUST use the control point's source IPv4 address as the
internal address of the mapping, regardless of the value supplied in
`NewInternalClient`. This prevents a control point from publishing another
internal host through this action.

The gateway MUST scope renewal and deletion to the mapping owner. The
`NewInternalPort` argument is part of that ownership check. A control point
MUST NOT be permitted to renew, delete, or probe another control point's
mapping by changing only the internal address or port.

# `X_START9_AddHostnameMapping` {#add}

## Input Arguments {#add-args}

The action has the following required input arguments, in this order:

| Argument                    | UPnP data type | Semantics                                                                                   |
| --------------------------- | -------------- | ------------------------------------------------------------------------------------------- |
| `NewRemoteHost`             | string         | MUST be empty; non-empty source filters are not supported by this extension.                |
| `NewExternalPort`           | `ui2`          | External TCP port. Zero is invalid.                                                         |
| `NewProtocol`               | string         | MUST be `TCP`.                                                                              |
| `NewInternalPort`           | `ui2`          | Internal TCP port. Zero is invalid.                                                         |
| `NewInternalClient`         | string         | Address asserted by the control point; the gateway uses the request source address instead. |
| `NewEnabled`                | boolean        | MUST indicate enabled.                                                                      |
| `NewPortMappingDescription` | string         | Control-point-provided description.                                                         |
| `NewLeaseDuration`          | `ui4`          | Requested lifetime in seconds. Zero requests the gateway's finite default.                  |
| `NewHostname`               | string         | Hostname demultiplexing key, as specified in {{hostname}}.                                  |

All arguments MUST appear even when their value is empty. A gateway MUST reject
a missing or malformed required argument with error 402, `Invalid Args`.

`NewRemoteHost`, `NewEnabled`, and `NewPortMappingDescription` retain the wire
shape of `AddPortMapping` for interoperability with WANIPConnection SOAP
stacks. A gateway implementing this specification MAY apply a non-empty
`NewRemoteHost` as an additional source filter. A gateway that does not support
that filter MUST reject a non-empty value with error 801,
`HostnameNotSupported`; it MUST NOT silently broaden the requested mapping.

## Lease Processing {#add-lease}

A nonzero `NewLeaseDuration` requests that number of seconds. The gateway MUST
grant a finite lifetime no greater than the requested lifetime and no greater
than 3600 seconds. A zero value requests a server-selected finite lifetime no
greater than 3600 seconds; it does not request a permanent lease for this
action.

The action has no output argument for the granted lifetime. A control point
therefore MUST assume that the mapping expires no later than 3600 seconds after
a successful response and SHOULD renew it sufficiently before that time. A
renewal uses the same external port, internal port, hostname, and owner.

## Server Behavior {#add-behavior}

After validating and authorizing the request, the gateway determines its
external IPv4 address and creates a mapping from that address,
`NewExternalPort`, and `NewHostname` to the request source address and
`NewInternalPort`.

If the same owner and target already hold the mapping, the gateway MUST treat
the request as a renewal and replace its remaining lifetime according to
{{add-lease}}. If another target holds the hostname on the same external
address and port, the gateway MUST leave the existing mapping unchanged and
return error 800, `HostnameTaken`.

On success, the gateway returns an
`X_START9_AddHostnameMappingResponse` element with no output arguments.

# `X_START9_DeleteHostnameMapping` {#delete}

## Input Arguments {#delete-args}

The action has the following required input arguments, in this order:

| Argument          | UPnP data type | Semantics                                          |
| ----------------- | -------------- | -------------------------------------------------- |
| `NewRemoteHost`   | string         | MUST be empty.                                     |
| `NewExternalPort` | `ui2`          | External TCP port.                                 |
| `NewProtocol`     | string         | MUST be `TCP`.                                     |
| `NewInternalPort` | `ui2`          | Internal TCP port and part of the ownership check. |
| `NewHostname`     | string         | Hostname key, as specified in {{hostname}}.        |

All arguments MUST appear even when their value is empty. A gateway MUST reject
a missing or malformed required argument with error 402, `Invalid Args`.

Unlike `DeletePortMapping`, this action carries `NewInternalPort`. A hostname
mapping's target and ownership include the requesting control point and its
internal port, while multiple targets can share one external TCP port under
different hostnames.

## Server Behavior {#delete-behavior}

The gateway reconstructs the target from the request source address and
`NewInternalPort`, then removes the matching finite-lived TCP hostname mapping.
The action MUST NOT remove a conventional port mapping, a permanent
operator-created hostname mapping, another hostname on the same port, or a
mapping owned by another control point.

Deletion SHOULD be idempotent. To avoid exposing another control point's
mapping state, a gateway MAY return success when no owner-matching mapping
exists. On success, the gateway returns an
`X_START9_DeleteHostnameMappingResponse` element with no output arguments.

# Error Responses {#errors}

Errors use the SOAP fault format defined by {{UDA2}}. The following codes apply:

| Code | Description             | Condition                                                                                     |
| ---- | ----------------------- | --------------------------------------------------------------------------------------------- |
| 402  | `Invalid Args`          | A required argument is absent or malformed, a port is invalid, or `NewProtocol` is not `TCP`. |
| 501  | `Action Failed`         | The gateway cannot complete the action for an otherwise valid request.                        |
| 606  | `Action not authorized` | The control point is not authorized to manage hostname mappings.                              |
| 800  | `HostnameTaken`         | Another target holds the hostname on the selected external address and port.                  |
| 801  | `HostnameNotSupported`  | The gateway lacks SNI demultiplexing or an optional requested behavior.                       |

Codes 800 and 801 are vendor-defined UPnP errors from the 800--899 range. A
malformed hostname is reported as 402 rather than 800 or 801.

A gateway that advertises both actions but cannot provide an SNI
demultiplexer MUST return 801. Advertising the actions does not permit a
gateway to accept a mapping that its data plane cannot serve.

# Discovery {#discovery}

Discovery follows the existing UPnP IGD process:

1. The control point discovers the InternetGatewayDevice and its
   WANIPConnection:1 service according to {{UDA2}}.
2. The control point retrieves that service's SCPD.
3. The control point checks the SCPD action list for both action names in
   {{overview}}.
4. The control point invokes the advertised actions at that service's control
   URL.

No separate capability probe, service, control URL, or evented state variable is
required. The action declarations MUST reference the existing WANIPConnection
state variables where their semantics match. `NewHostname` references a
vendor-defined, non-evented string state variable named `X_START9_Hostname`.

A control point MUST NOT invoke either action merely because the gateway's
manufacturer or model is recognized. The SCPD is the authoritative capability
signal for the current service instance.

# Data-Plane Behavior {#data-plane}

For an inbound TCP connection to an external port with hostname mappings, the
gateway reads enough of the TLS ClientHello to obtain the `server_name`
extension defined by {{RFC6066}}. It selects an exact hostname mapping first,
then a wildcard mapping. The gateway forwards the buffered bytes and subsequent
stream without terminating TLS.

If no mapping matches, the gateway MAY forward to a conventional mapping on the
same external port if one exists. Otherwise, it MUST NOT select an arbitrary
hostname mapping and SHOULD close the connection.

The gateway MUST bound the bytes, time, and concurrent state used while waiting
for a ClientHello. The gateway SHOULD preserve the external client's source
address and port when forwarding to the internal endpoint. If it does not,
operators and internal endpoints need to account for the gateway address as the
apparent peer.

This specification does not define hostname demultiplexing for UDP or QUIC.
Both actions reject `UDP` even if the gateway independently supports QUIC
demultiplexing through another protocol.

# Security Considerations {#security}

The actions create externally reachable mappings and therefore MUST be subject
to the gateway's authorization policy for port mapping. A gateway SHOULD limit
the actions to control points on an authorized internal network. Where stronger
identity is available, ownership SHOULD be bound to that identity rather than
source address alone.

Forcing the target address to the control request's source address prevents one
internal host from directing traffic to another. Owner-scoped deletion prevents
a control point from removing another control point's mapping. An implementation
MUST perform these checks before changing mapping state.

The hostname in a request and the SNI in an inbound ClientHello are untrusted.
An implementation MUST NOT use either value to construct file paths or commands
and SHOULD sanitize them before logging. Hostname comparison is
case-insensitive, but authorization and ownership comparisons MUST NOT depend on
hostname casing.

Waiting for a ClientHello creates state before an internal target is selected.
An implementation MUST impose buffer, timeout, and concurrency limits to
mitigate handshake-and-stall attacks.

A compromised authorized control point can claim an unbound hostname first.
Deployments requiring stronger policy MAY restrict the hostnames each control
point can claim or verify that a hostname resolves to the gateway's external
address. DNS verification is policy, not proof of control, and does not replace
control-point authorization.

# Privacy Considerations {#privacy}

A hostname mapping reveals the requested hostname to the gateway and to any
party authorized to inspect its mapping state. Implementations SHOULD minimize
retention of expired mappings and SHOULD avoid logging hostnames unless needed
for operation or auditing.

The data plane observes the SNI already visible in an unencrypted TLS
ClientHello. This mechanism does not decrypt TLS application data. Encrypted
ClientHello can hide the origin hostname; a gateway can route only on a name
visible in the outer ClientHello and cannot demultiplex on an encrypted name.

Advertising the actions in the SCPD reveals that the gateway supports hostname
mappings, but it does not reveal configured hostnames.

# IANA Considerations {#iana}

This document requests no IANA actions. The action names and error codes are in
vendor-defined namespaces provided by {{UDA2}} and {{WANIPC1}}. Publication of
this document would not assign these names to the UPnP or IANA standards
namespace and would not change the status of the underlying WANIPConnection
service.

--- back

# Acknowledgements

The authors thank implementers and reviewers of UPnP IGD port-mapping software
whose interoperability feedback informed this specification.
