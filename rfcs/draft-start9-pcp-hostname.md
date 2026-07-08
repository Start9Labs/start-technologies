---
v: 3
title: PCP Hostname Extension for SNI-Demultiplexed Port Mappings
abbrev: PCP Hostname Extension
docname: draft-start9-pcp-hostname-00
category: std
submissiontype: IETF
consensus: true
ipr: trust200902
area: Internet
workgroup: Network Working Group
keyword:
  - PCP
  - SNI
  - NAT
  - port mapping
  - QUIC
date: 2026-06-23
author:
  - ins: A. McClelland
    name: Aiden McClelland
    org: Start9
    email: me@drbonez.dev
normative:
  RFC6887:
  RFC6066:
  RFC5890:
  RFC8999:
  RFC9000:
  RFC9001:
  RFC7753:
informative:
  RFC7652:
  I-D.ietf-tls-esni:
  I-D.ietf-quic-load-balancers:
---

abstract

This document defines a Port Control Protocol (PCP) option, HOSTNAME, that
associates one or more fully qualified domain names with a MAP request. A
PCP server implementing this option demultiplexes inbound connections on a
shared external port by inspecting the Server Name Indication presented in
the TLS ClientHello and forwards each connection to the internal host that
holds the binding for the presented name. This allows multiple internal
hosts behind a single external IP address to share well-known ports
(e.g. 443) provided their hostnames differ, while each host retains the
ability to self-provision its mappings via ordinary PCP semantics.
Demultiplexing is defined for TCP-carried TLS, with optional support for
QUIC.

--- middle

# Introduction {#intro}

## Motivation {#motivation}

A NAT gateway exposes one external IP address, or at most a few. A
conventional PCP MAP grants an entire (protocol, external IP, external
port) tuple to a single internal host, so the number of hosts that can
serve HTTPS on port 443 is limited to the number of external addresses,
typically one. Operators work around this with a manually configured
reverse proxy or SNI proxy on the gateway, which:

1. requires central, out-of-band configuration rather than client
   self-provisioning, and
2. typically replaces the client source address with the proxy address,
   hiding the real peer from the backend.

Since hosts behind a NAT typically serve public Internet domains (the
names are registered in public DNS and resolve to the gateway's external
address), the hostname is a natural demultiplexing key that the internal
host already knows and can assert itself. This extension moves the SNI
routing table into PCP, so each internal host registers its own names, and
the gateway, which is already on the return path for all LAN traffic,
forwards with the original client source address preserved.

## Requirements Language {#conventions}

{::boilerplate bcp14-tagged}

## Terminology {#terminology}

This document uses the terminology of {{RFC6887}} (PCP client, PCP server,
internal host, external address, mapping, mapping nonce). In addition:

Hostname binding:
: an association, held by the PCP server, from (protocol, external IP
address, external port, hostname) to an internal address and internal
port, created by a MAP request carrying a HOSTNAME option.

Fallback mapping:
: a conventional mapping (created by a MAP request with no HOSTNAME
option) on a port that also has hostname bindings; it receives
connections that match no hostname binding (see {{validity}}).

Demultiplexed port:
: a (protocol, external IP address, external port) tuple holding at least
one hostname binding. Inbound connections to a demultiplexed port are
routed per {{tcp-demux}} or {{quic-demux}} rather than by conventional
destination NAT.

# Overview of Operation {#overview}

This section is informative; normative requirements appear in
{{hostname-option}} through {{security}}.

1. An internal host sends a PCP MAP request for protocol TCP, suggesting
   an external port (e.g. 443), and includes one or more HOSTNAME options,
   each carrying an FQDN such as `git.example.com`.
2. The PCP server checks each requested name against its hostname bindings
   for that (external IP, external port). If no conflicting binding
   exists, it creates bindings with the requested lifetime and returns
   success, echoing the HOSTNAME options.
3. Other internal hosts may map the same external port with different
   hostnames; the port is shared because the mapping key is extended to
   (protocol, external IP, external port, hostname).
4. For each inbound TCP connection to a demultiplexed port, the server
   accepts the connection, reads the TLS ClientHello, extracts the
   server_name extension, selects the matching binding, and splices the
   connection to the bound internal host.
5. Because the PCP server is the default gateway for the internal network,
   it originates the internal leg with the original client's source
   address and port (transparent mode, {{source-pres}}); return packets
   necessarily traverse the gateway, which steers them back into the
   spliced connection.

A host requests forwards for multiple hostnames either by including
multiple HOSTNAME options in a single MAP request or by issuing multiple
MAP requests; see {{client}}.

# The HOSTNAME Option {#hostname-option}

## Format {#format}

The option follows the PCP option format ({{Section 7.3 of RFC6887}}):

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|  Option Code  |  Reserved     |       Option Length           |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
:                hostname (variable length)                     :
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

{: #fig-format title="HOSTNAME Option Format"}

Option Code:
: TBD1, in the optional-to-process range.

Option Length:
: length of the hostname field in octets (1 to 255). The option data is
zero-padded to a 32-bit boundary per {{RFC6887}}; padding is not
included in Option Length.

hostname:
: the name to bind, encoded exactly as it appears on the wire in the SNI
HostName field ({{Section 3 of RFC6066}}): ASCII, internationalized
names as A-labels {{RFC5890}}, no trailing dot, no NUL octets, total
length 1 to 255 octets. Literal IPv4 and IPv6 addresses MUST NOT be used
({{Section 3 of RFC6066}}). Comparison is case-insensitive; servers
SHOULD normalize names to lowercase on receipt. A syntactically invalid
hostname MUST be rejected with MALFORMED_OPTION.

Because the option code is in the optional-to-process range, a server that
does not implement this option ignores it and processes the request as a
conventional MAP, granting an unrestricted port forward. This degrades
gracefully: if the requesting host is the only one using the port,
observable behavior is identical, and a second host receives
CANNOT_PROVIDE_EXTERNAL (or an alternate port) per standard MAP
processing. Clients detect whether hostname scoping was applied by
checking for the echoed HOSTNAME option(s) in the response; see
{{client}}.

## Validity and the Fallback Mapping {#validity}

The HOSTNAME option is valid only for the MAP opcode. A server
implementing this option MUST reject any other opcode carrying it with
MALFORMED_OPTION.

The option is valid for protocol TCP, and for protocol UDP only on servers
implementing QUIC demultiplexing ({{quic-demux}}). A server implementing
this option MUST reject a HOSTNAME option for any other protocol, or for
UDP when QUIC demultiplexing is not implemented, with UNSUPP_HOSTNAME
({{result-codes}}). Clients MUST NOT request hostname bindings for
protocols whose servers send application data before the client
(e.g. SMTP, IMAP); a server cannot detect this misuse, and such
connections will stall waiting for a ClientHello and then be handled as in
{{tcp-demux}}, step 4.

A HOSTNAME option MAY appear multiple times in a single MAP request, each
occurrence creating one binding, subject to the atomic failure semantics
of {{conflicts}}.

There is no in-option fallback syntax. A conventional MAP request (no
HOSTNAME option) for the same (protocol, external IP, external port)
creates the fallback mapping: connections whose ClientHello carries no
server_name extension, presents a name with no matching binding, or whose
initial bytes do not parse as TLS (or as QUIC per {{quic-demux}}), are
forwarded to its holder. At most one fallback mapping exists per
(protocol, external IP, external port), enforced by normal MAP
exclusivity.

## Wildcards {#wildcards}

A hostname whose first label is `*` (e.g. `*.example.com`) requests a
wildcard binding matching exactly one left-most label, with the same
semantics as certificate wildcard matching. Wildcard support is OPTIONAL;
servers not implementing it respond with UNSUPP_HOSTNAME
({{result-codes}}). An exact binding takes precedence over a wildcard
binding.

# Server Behavior {#server}

## Mapping Table {#mapping-table}

The server maintains, per (protocol, external IP, external port), a set of
hostname bindings, each comprising: hostname, internal address, internal
port, lifetime, and the mapping nonce of the creating client. All standard
MAP processing of {{RFC6887}} applies (lifetime decay, renewal by matching
nonce, ANNOUNCE/epoch recovery).

## Conflicts {#conflicts}

If a requested hostname is already bound on that (protocol, external IP,
external port) under a different mapping nonce, the server MUST NOT create
or alter the binding and MUST return result code HOSTNAME_TAKEN
({{result-codes}}).

If any HOSTNAME option in a request fails validation or conflicts, the
entire request fails and no bindings from that request are created or
modified; PCP responses are atomic per request.

A conventional MAP for a (protocol, external IP, external port) that has
hostname bindings is permitted and serves as the fallback mapping
({{validity}}). It is subject to ordinary MAP exclusivity. Hostname
bindings and the fallback mapping have independent lifetimes; expiry of
one does not affect the others.

## Deletion {#deletion}

A MAP request with requested lifetime 0 carrying HOSTNAME options deletes
only the named bindings, subject to the nonce-matching rules of
{{Section 15 of RFC6887}}. A lifetime-0 request without HOSTNAME options
deletes only the requester's conventional (fallback) mapping, if any.
Other bindings on the port are unaffected in both cases.

## TCP Demultiplexing {#tcp-demux}

For TCP connections arriving at a demultiplexed port, the server:

1. Completes the TCP handshake with the external client.
2. Reads until it has the full ClientHello or a parse failure, subject to
   a buffer cap (16384 octets, the maximum TLS plaintext record fragment,
   is RECOMMENDED) and a timeout (5 seconds is RECOMMENDED). ClientHellos
   can span multiple TCP segments and, rarely, multiple TLS records;
   servers SHOULD handle multi-record ClientHellos.
3. Extracts server_name, selects the binding (exact match, then wildcard,
   then the fallback mapping if present), and connects to the bound
   internal host and port, replaying the buffered bytes before splicing.
4. If no binding matches and no fallback mapping exists, the server SHOULD
   close the connection. Whether to reset or send a TLS alert is
   implementation defined; the server MUST NOT forward the connection to
   an arbitrary binding.

Demultiplexing engages only on demultiplexed ports. A port holding only a
conventional mapping is forwarded as plain destination NAT per {{RFC6887}},
with no ClientHello inspection. When the first hostname binding is added to
a port with an existing conventional mapping, the port transitions to
demultiplexed mode; established connections are unaffected, and the
conventional mapping's holder begins receiving only fallback traffic for
new connections. The reverse transition occurs when the last hostname
binding expires. The server does not terminate TLS; it requires no keys
and forwards the ClientHello bytes verbatim.

## QUIC Demultiplexing (OPTIONAL) {#quic-demux}

A server MAY additionally implement demultiplexing of QUIC {{RFC9000}} for
hostname bindings with protocol UDP. Servers implementing this section
accept HOSTNAME options with protocol UDP; servers that do not MUST reject
them per {{validity}}, so a client can discover support by the result
code.

For UDP datagrams arriving at a demultiplexed port:

1. Initial packets.
   A datagram beginning with a QUIC long header ({{RFC8999}}) of a version
   the server supports is processed as a candidate Initial. The server
   derives the Initial secrets from the client's Destination Connection ID
   and the version-specific salt ({{Section 5.2 of RFC9001}}), decrypts
   the packet, and reassembles the CRYPTO stream to obtain the TLS
   ClientHello. The ClientHello can span multiple Initial packets; the
   server MUST buffer and reassemble across datagrams, subject to a cap
   (10 datagrams or 65535 octets of CRYPTO data is RECOMMENDED) and a
   timeout (5 seconds is RECOMMENDED).
2. Binding selection.
   The server extracts server_name and selects a binding exactly as in
   {{tcp-demux}}, step 3. It then creates flow state keyed on the
   connection's 4-tuple, forwards the buffered datagrams verbatim to the
   bound internal host, and forwards subsequent datagrams matching the
   flow in both directions. No decryption is performed after binding
   selection, and the server never holds handshake or 1-RTT keys.
3. Flow lifetime.
   Flow state is removed after an idle period consistent with the
   gateway's UDP mapping timeout. Version Negotiation and Retry exchanges
   initiated by the internal host traverse the established flow and require
   no special handling.
4. Non-matching datagrams.
   Datagrams that are not parseable Initials of a supported version
   (including short-header packets matching no flow, long headers of
   unknown versions, and non-QUIC UDP payloads) are forwarded to the
   fallback mapping if one exists and otherwise dropped.

Two consequences follow from routing on flow state. First, active
connection migration and NAT rebinding both move the connection to a
4-tuple the server has no state for; such packets carry connection IDs
opaque to the server and arrive as non-matching datagrams. Internal hosts
serving QUIC behind a demultiplexing gateway SHOULD send the
disable_active_migration transport parameter ({{Section 18.2 of RFC9000}})
to suppress client-initiated migration; this does not prevent NAT
rebinding ({{Section 9.3 of RFC9000}}), which can still strand a connection
on an unknown 4-tuple unless coordinated connection-ID routing is in use.
Coordinated connection-ID routing (e.g. QUIC-LB
{{I-D.ietf-quic-load-balancers}}) could lift this restriction and is out of
scope. Second, a client whose first flight uses a QUIC version unknown to
the server cannot be demultiplexed; servers SHOULD support all current
standard versions and treat unknown-version long headers as non-matching
rather than attempting Version Negotiation themselves.

## Source Address Preservation {#source-pres}

Servers SHOULD originate the internal leg of demultiplexed TCP connections
using the external client's source address and port (e.g. via
IP_TRANSPARENT on Linux), relying on their position as the internal
network's gateway to capture return traffic. When operating in this mode
the forwarded connection is, from the internal host's perspective,
indistinguishable from a direct destination-NAT forward. A server that
cannot operate transparently uses its own address on the internal leg;
this behavior is visible to internal hosts and operators should document
it. QUIC demultiplexing ({{quic-demux}}) forwards datagrams at the IP layer
and preserves the source address inherently.

# Client Behavior {#client}

- Clients include one HOSTNAME option per name. Given the atomic failure
  semantics of {{conflicts}}, clients SHOULD use one MAP request per name
  when independent success matters, and MAY batch multiple options in one
  request when the names succeed or fail together (e.g. a certificate SAN
  set).
- Clients MUST examine the response for echoed HOSTNAME options. Absence
  means the server did not process the option and the granted mapping is
  unrestricted; clients that require hostname scoping MAY delete such a
  mapping. Against a server that ignores the option, both request patterns
  above degrade safely: a batched request yields one unrestricted mapping
  covering all names, and per-name requests for the same internal port
  resolve to that same mapping ({{RFC6887}} keys mappings on the internal
  tuple), acting as refreshes rather than conflicts.
- On gateways with multiple external addresses, hostname bindings are
  scoped per external address: the same name may be bound on two external
  IPs to different internal hosts. Clients SHOULD set the Suggested
  External IP Address field ({{Section 11.1 of RFC6887}}) to the address
  their public DNS resolves to and SHOULD include PREFER_FAILURE so the
  server fails rather than assigning a different address. The Assigned
  External IP Address in the response is authoritative; clients publishing
  DNS records from PCP state SHOULD use it rather than the suggested
  address.
- Clients renew exactly as in {{RFC6887}}, re-sending the same MAP request
  (same nonce, same options) before lifetime expiry.
- Clients MUST be prepared to receive HOSTNAME_TAKEN at any renewal
  (e.g. after server state loss and a race with another host) and SHOULD
  surface the condition rather than silently retrying.

# Interaction with Other PCP Options {#interactions}

FILTER ({{RFC6887}}):
: composes normally; filters restrict which remote peers may connect and
are evaluated before demultiplexing.

PREFER_FAILURE ({{RFC6887}}):
: composes normally; see {{client}} for its use on multihomed gateways.

THIRD_PARTY ({{RFC6887}}):
: a request combining THIRD_PARTY and HOSTNAME options creates hostname
bindings on behalf of the third party, subject to the server's
THIRD_PARTY authorization policy; the conflict rules of {{conflicts}}
apply to the third party's bindings identically.

PORT_SET ({{RFC7753}}):
: a request combining PORT_SET and HOSTNAME options is not meaningful (a
hostname binding targets a single internal port) and MUST be rejected
with MALFORMED_OPTION.

# Interaction with Encrypted ClientHello {#ech}

ECH {{I-D.ietf-tls-esni}} encrypts the true server name; the outer
ClientHello carries the public name of the client-facing server. A
demultiplexing server sees only the outer SNI, for both TLS over TCP and
QUIC. Deployments using this extension with ECH MUST bind the public name
(the name published in the HTTPS/SVCB ech parameter), and the internal
host performs inner-name routing itself if needed. Names hidden by ECH
cannot serve as demultiplexing keys by design; this is a property of ECH,
not a defect of this extension.

# New Result Codes {#result-codes}

HOSTNAME_TAKEN (TBD2):
: the requested hostname is already bound on this external IP address and
port by another client. This is a short lifetime error; the condition
clears when the conflicting binding expires or is deleted.

UNSUPP_HOSTNAME (TBD3):
: the hostname is syntactically valid but the request uses a feature this
server does not support (e.g. wildcard bindings, or protocol UDP without
QUIC demultiplexing support). This is a long lifetime error.

Syntactically invalid hostnames are rejected with the existing
MALFORMED_OPTION ({{RFC6887}}).

# Security Considerations {#security}

The security model is that of {{RFC6887}}: any host on the internal
network may create mappings, so any host may claim any hostname, first come
first served. This is equivalent in power to the existing ability of any
internal host to claim an entire external port; the granularity is finer
but the trust assumption is unchanged. Deployments requiring stronger
guarantees SHOULD deploy PCP authentication {{RFC7652}} or restrict the
option via policy (e.g. per-host hostname allowlists).

A server MAY verify, at binding time, that the requested name resolves in
public DNS to one of its external addresses, rejecting names that do not.
DNS can change after binding and split-horizon deployments may legitimately
fail such a check, so it MUST be possible to disable; it is a policy
mechanism, not a protocol requirement.

ClientHello collection ({{tcp-demux}} and {{quic-demux}}) creates state per
pending connection. Servers MUST cap buffered bytes and the number of
pending connections to bound memory under handshake-and-stall attacks, and
SHOULD subject pending connections to the same limits as their existing
connection tracking.

The hostname presented in SNI is attacker controlled and unauthenticated.
The server uses it only to select among pre-registered bindings; it MUST
NOT be used to construct file paths or commands, and MUST be sanitized
before inclusion in log output.

For QUIC demultiplexing, an attacker can cheaply elicit Initial-secret
derivation and decryption work with spoofed datagrams. The per-version key
derivation uses no secret state and the cost is bounded per datagram, but
servers SHOULD rate-limit Initial processing per source address.

# Applicability and Limitations {#applicability}

Demultiplexing requires a client-first protocol carrying SNI: TLS over TCP
({{tcp-demux}}) and QUIC ({{quic-demux}}). Non-TLS protocols with a
cleartext hostname early in the stream (e.g. HTTP/1.1 Host) are out of
scope; the fallback mapping covers them coarsely. Server-first protocols
cannot be demultiplexed at all ({{validity}}).

# IANA Considerations {#iana}

## PCP Option Code {#iana-option}

IANA is requested to assign the following option code in the "PCP
Options" registry of the "Port Control Protocol (PCP) Parameters" group,
from the Specification Required range (192-223); values in this range are
optional-to-process:

| Field               | Value                                                                                                                                                                                                                   |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Value               | TBD1                                                                                                                                                                                                                    |
| Name                | HOSTNAME                                                                                                                                                                                                                |
| Purpose             | Associates a fully qualified domain name with a MAP request; inbound connections on the mapped external port are forwarded to the internal host whose binding matches the SNI presented in the TLS or QUIC ClientHello. |
| Valid for Opcodes   | MAP                                                                                                                                                                                                                     |
| Length              | variable; 1 to 255 octets                                                                                                                                                                                               |
| May Appear in       | Request. May appear in response only if it appeared in the associated request.                                                                                                                                          |
| Maximum Occurrences | As many as fit within maximum PCP message size.                                                                                                                                                                         |
| Reference           | This document                                                                                                                                                                                                           |

## PCP Result Codes {#iana-results}

IANA is requested to assign two result codes in the "PCP Result Codes"
registry of the same group, from the Specification Required range
(128-191):

| Value | Name            | Description                                                                                                                          | Reference     |
| ----- | --------------- | ------------------------------------------------------------------------------------------------------------------------------------ | ------------- |
| TBD2  | HOSTNAME_TAKEN  | The requested hostname is already bound on the assigned external address and port by another client. This is a short lifetime error. | This document |
| TBD3  | UNSUPP_HOSTNAME | The HOSTNAME option requests a feature not supported by this server. This is a long lifetime error.                                  | This document |

--- back

# Implementation Status {#impl-status}

An open-source implementation exists in StartOS and its StartTunnel
gateway. Pending IANA assignment, it uses values from the PCP Private Use
ranges: option code 224, and result codes 192 (HOSTNAME_TAKEN) and 193
(UNSUPP_HOSTNAME). These are placeholders and are expected to be replaced
by the IANA-assigned values (TBD1, TBD2, TBD3) once allocated.
