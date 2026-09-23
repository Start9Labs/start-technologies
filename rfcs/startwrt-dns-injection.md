# StartWRT: RFC 2136 Private-DNS Injection

Status: design settled; ready to implement. Owner: unassigned.

The ingress and answer-plane mechanisms in this document were validated on K1
hardware on 2026-08-21 — the opcode match, the NAT include, the chain priority,
the local-vs-transit discrimination, the dnsmasq signalling path, and end-to-end
resolution from a LAN client. Points still awaiting a bench result are marked
inline and none of them fork the design.

Phase 2 of gateway autoconfiguration on StartWRT. Phase 1 (PCP/UPnP automatic
port forwarding, `projects/start-wrt/backend/ctrl/src/port_control.rs`) **merged
to master** as `81da14da8` (#3634) and is a hard prerequisite — this design
reuses its device-authorization machinery wholesale, and can now cite it as
shipped code rather than as a sibling branch.

Phase 3 has since **overtaken this document**: its SNI hostname-route dataplane
landed on `wrt/upnp-hostname-action` and was hardware-validated on the K1 on
2026-08-17. Only Phase 3's IPv6 pinholes remain unbuilt. That reordering does not
invalidate anything below — the shared `dns_update` module is untouched since
`eb4809b7b`, and both problem statements still hold verbatim — but it does supply
a second, sharper motivation for this work (see "The SNI hairpin" below).

## Background

### What the feature is

A StartOS server that enables a **private domain** on a gateway pushes an
`A`/`AAAA` record for that domain — pointing at its own address on that
gateway's subnet — into the gateway's DNS server via RFC 2136 DNS UPDATE, and
withdraws it when the domain is disabled. The point is stated in
`net_controller.rs:898`: _"so LAN devices not using StartOS's resolver can
resolve it"_. Without it, a private domain resolves only on the StartOS box's
own resolver; every other device on the LAN gets NXDOMAIN unless the user hand-
enters a record in their router.

It landed for StartTunnel in #3306 (`49ea23e3b`), with `.local` auto-injection
added in #3523 (`eb4809b7b`).

### The three halves that already exist

**Client (`shared-libs/crates/start-core/src/net/dns_update/mod.rs`).**
`DnsUpdateController` mirrors `DnsController`'s `add`/`gc` API so the net
reconcile drives both in lock-step. Per FQDN it computes targets from the live
gateways — `targets_for()` (`dns_update/mod.rs:263`) picks each subnet's NM
default gateway as the resolver and this host's address on that subnet as the
rdata — then sends a delete-rrset + append pair over **UDP only** (the client;
StartTunnel's own listener binds UDP _and_ TCP, `tunnel/dns.rs:164-186`), bound
to its own source address so the server can authorize by source IP. Re-asserted
every `REFRESH_INTERVAL` (180s) with a 300s record TTL, so the injected state is
self-healing and converges within three minutes of any gateway restart. Outcomes
feed the gateway's `dns_update` capability verdict; a fresh failure suppresses
retries for the negative trust window via `recently_failed()` —
`CapabilityVerdict::fresh` (`db/model/public.rs:317-325`) gives a failure 5
minutes and a _success_ 1 hour.

**A silent no-target case worth knowing before debugging one.** `targets_for()`
routes through PCP's `candidate_gateways()` (`net/port_map/client.rs:116`), which
returns nothing for an `OutboundOnly` gateway, and it then requires a
**same-family resolver inside the subnet it is publishing**. A StartWRT profile
whose gateway IP is not within the subnet it advertises therefore gets no
injection attempt at all — and no log line, because the empty result is not an
error path.

**Shared server handler
(`shared-libs/crates/start-core/src/net/dns_update/rfc2136.rs`).** Written to be
shared from day one — its module doc says _"used by both StartTunnel (in this
crate) and StartWRT's `startwrt-ctrld` (which imports this crate)"_ and names
_"an addn-hosts file on StartWRT"_ as the intended persistence hook.
`DnsInjector` is an in-memory `BTreeMap<LowerName, Vec<InjectedRecord>>` plus
three plug-in closures — `authorize(IpAddr) -> bool`, `tsig_key(IpAddr) ->
Option<[u8;32]>`, `on_change(Vec<InjectedRecord>)`. `InjectingHandler` wraps a
forwarding `Catalog`: an injected name is answered authoritatively (including
NODATA for a held name, so an upstream NXDOMAIN can't poison it per RFC 8020), a
TSIG-verified UPDATE mutates the store, everything else forwards unchanged.

**StartTunnel wiring
(`shared-libs/crates/start-core/src/tunnel/{context,dns,wg,db,api}.rs` — the
tunnel backend lives in the shared crate, not under `projects/start-tunnel/`).** Each per-subnet
DNS proxy is a hickory `Server` on `<subnet>.1:53` fronting a
`ForwardZoneHandler`. `WgConfig` gains `allow_dns_injection` (default off for a
Client, on for a Server); the authorizer reads a live allowed-IP set so a toggle
change applies without a rebuild; `on_change` persists to `db.dns_records`. A
DNS Records page plus `tunnel dns list/add/remove` and `device
set-dns-injection` expose it.

### Authentication, and why it doesn't port

UPDATEs are authenticated with **TSIG** (RFC 8945, HMAC-SHA256). The key is
`HKDF-SHA256(psk, "startos-dns-update-v1")` where `psk` is the **WireGuard
preshared key** the two ends already share. Source IP alone is explicitly
rejected as insufficient — the module doc notes it is forgeable by any
co-located service that can emit on the tunnel interface — and the PSK is
root-only in NetworkManager, so a sandboxed service cannot forge a signature.

**On a plain LAN there is no such shared secret.** `wireguard_psk()` returns
`None` for a non-WireGuard gateway; `signer_for()` then sends the UPDATE
**unsigned** (the client's doc comment: _"a gateway with no PSK (e.g. a plain
router) gets an unsigned, best-effort update"_). The server side hard-refuses
it: `verify_tsig()` returns `false` when the key lookup yields `None`, and
`InjectingHandler` answers `Refused`.

So the primary StartWRT use case — a StartOS box plugged into the LAN — is,
with the shared code exactly as it stands today, guaranteed to fail closed.

### The SNI hairpin — a second motivation, added by Phase 3

Phase 3's SNI hostname routes let several devices share one external port: the
demux reads each ClientHello on the WAN address and splices the connection to
whichever device owns the hostname, opening the internal leg **source-preserving**
via `transparent_connect` (`tunnel/forward/sni.rs:1-8, 573`).

That source preservation is what makes the feature correct from the WAN and what
breaks it from the LAN. A device _inside_ the LAN that resolves the hostname to
the public address reaches the demux (the router owns that address), the demux
dials the internal host from the LAN client's own source address, and the
internal host — a neighbor on the same segment — replies **directly** to the
client with its own address as the source. The client's stack drops the reply as
belonging to no connection it opened, so the connection hangs. This is the
classic NAT hairpin, and source preservation is the reason the usual
loopback-NAT workaround does not apply. _(Derived from the code path, not from a
bench observation — confirm on hardware before quoting it in docs.)_

**Split-horizon DNS is the LAN-side answer**, which makes this RFC a dependency
of Phase 3 being usable from inside the house rather than a parallel nicety. Four
things must be stated precisely, because three plausible readings are wrong:

- **Never inject the gateway's address.** The demux binds `(wan_ipv4, port)`
  _specifically_ — `ensure_listener` binds `SocketAddrV4::new(key.0, key.1)`
  where `key.0` is the external IP from `PortControl::wan_ipv4`
  (`sni.rs:470-494`, `port_control.rs:315-324`). It never binds a wildcard and
  never binds a LAN gateway address. A record pointing at the gateway would land
  LAN clients on **the router's own wildcard UI listener**
  (`ctrl/src/bins/daemon.rs:457-476`) — a worse outcome than the hairpin, since
  it fails as a wrong answer rather than as a timeout.
- **The correct record is the owning device's own LAN address** — exactly what
  `targets_for()` already produces, and exactly what Problem 3's
  rdata-equals-source constraint permits. Split horizon resolves each name to
  that name's own owner, so several hostnames sharing one external port each
  resolve correctly and independently. **The SNI feature and the constrained
  unsigned tier are compatible**; it reads like a conflict and is not.
- **A DNS record carries no port.** Split horizon therefore only works where the
  device serves the same port on its LAN address as the external port it was
  granted. An SNI route mapping external 443 → internal 8443 still hairpins for
  LAN clients. StartOS private domains on 443 — the case this RFC exists for —
  are the case that works.
- **The two features do not connect themselves.** Injection is driven solely by
  `HostnameMetadata::PrivateDomain` (`net/net_controller.rs:388-407, 680-700`,
  fed by `add_private_domain`), so registering an SNI hostname route does **not**
  inject anything. Today the user must additionally enable that domain as a
  private domain on the StartWRT gateway. That is a docs obligation and an open
  question (below), not something this design fixes for free.

## Goals

- A StartOS server on a StartWRT LAN can publish its private domains to the
  router's DNS, so every other device on the LAN resolves them.
- Default-off, per-device, matching the Phase 1 permission model.
- Correct under StartWRT's multi-profile model: a record is resolvable exactly
  where the profile system says the underlying host is reachable.
- No new failure mode for LAN DNS. dnsmasq must remain the DNS dataplane; a
  `startwrt-ctrld` crash must not take name resolution down.
- No steady-state flash writes.

## Non-goals

- IPv6 UPDATE ingress (v4 only to start — see Open questions), and IPv6
  pinholes, the remaining half of Phase 3. Phase 3's SNI/HOSTNAME demux has
  already landed; see "The SNI hairpin" above for how it interacts.
- Changing StartOS's client. This is a strong preference, not an impossibility:
  Phase 3 _did_ change the client (the UPnP vendor-action fallback in
  `net/port_map/client.rs`, with its own StartOS changelog entry). The reason to
  hold the line here is different — a client change to accommodate injection
  would have to weaken or complicate the client's own trust derivation, which is
  the part of this system with real security weight. Treat a design that needs
  one as suspect and justify it explicitly.

## Problem 1: dnsmasq owns port 53

On StartWRT, DNS is dnsmasq, arranged per profile
(`rewrite_dns_forwarding`, `ctrl/src/profiles.rs:344-441`): each profile with
effective DNS gets its own
`config dnsmasq 'dns_<iface>'` section bound to that profile's gateway IP
(`listen_address`, `nonwildcard`), forwarding to a SmartDNS group on
`127.0.0.1#<5300+vlan>` or to VPN DNS; a `DNS-Override-<profile>` fw4 redirect
DNATs all port-53 traffic in the zone to that gateway IP so devices cannot
bypass it. Profiles in ISP mode get **no** per-profile section and fall through
to the main dnsmasq instance.

dnsmasq does not implement RFC 2136 at all; it refuses or drops opcode 5.
Something else must handle UPDATE, and the client sends to `<gateway>:53`
(`DNS_PORT` is a hard-coded constant) — so the router must answer opcode 5 on
the address dnsmasq is already listening on.

### Option A — front dnsmasq with `InjectingHandler` (the StartTunnel shape)

Move each per-profile dnsmasq to an alternate port, bind hickory's
`InjectingHandler` on `<gateway>:53`, forward the miss path to dnsmasq.

Maximal code reuse (this is literally `tunnel/dns.rs`), all four record types.
But it puts the management daemon in the path of **every LAN DNS query**: a
daemon restart — which happens on config apply and on update — becomes a LAN-
wide DNS outage, on a device whose entire value proposition is being the
reliable piece of the house. It also adds a second resolver cache and loses
dnsmasq's rebind protection on the fronted path, and ISP-mode profiles share the
main dnsmasq instance so they cannot be fronted selectively. **Rejected.**

### Option B — divert only UPDATE, in nftables (recommended)

Leave dnsmasq on `:53` untouched. Add one static fw4 include rule that diverts
**only DNS UPDATE packets** to the daemon on an alternate port:

```
iifname "wg_*" meta nfproto ipv4 fib daddr type local udp dport 53 @th,81,4 == 5 \
	limit rate 20/second burst 50 packets counter redirect to :9554
meta nfproto ipv4 fib daddr type local udp dport 53 @th,81,4 == 5 \
	limit rate 20/second burst 50 packets counter redirect to :9553
```

Two rules, not one, because arrival scoping is structural (see Problem 3): each
listener is `SO_BINDTODEVICE`-bound to exactly one kernel device, and an
inbound-WireGuard peer's UPDATE arrives on `wg_<iface>`, not the bridge — the
VPN client config points DNS at the same profile gateway IP. Splitting by
arrival interface in nft gives the wg-bound and bridge-bound listeners their
own ports, so no two sockets ever share an `(addr, port)` pair and delivery
never depends on reuseport-group subtleties. `redirect` is terminal, so the
first matching rule wins.

`@th,64,16` is the DNS ID (the UDP header is 64 bits); the flags byte begins at
`@th,80`, so the 4-bit opcode is `@th,81,4`, and `5` is UPDATE. The client is
UDP-only, so no TCP variant is needed (TCP DNS's 2-byte length prefix plus a
variable TCP header length makes the offset non-constant anyway).

Every clause earns its place:

- **`fib daddr type local` confines the divert to UPDATEs addressed to the
  router.** Without it the rule also catches UPDATEs merely _transiting_ the
  router to an external authoritative server and redirects them into the
  injector. That is not hypothetical: StartWRT ships `ddns-scripts`, whose
  nsupdate backend does exactly that, and a bench run on 2026-08-21 confirmed the
  unqualified rule matching a transit packet.
- **`redirect to :<port>` rather than `dnat to <gateway>:<port>`.** `redirect`
  preserves the destination address and rewrites only the port, so **one static
  rule is correct for every profile**. A `dnat` rule must name a gateway IP, and
  each profile has its own — which would drag the ingress back into per-profile
  generated firewall rules. `redirect` lands the packet on exactly the address
  the daemon's per-profile socket is already bound to.
- **The base chain is `type nat hook prerouting priority -101`.** All three
  shipped includes (`backend/nftables/10-startwrt-dnat-mark.nft`,
  `11-startwrt-inbound6-mark.nft`, `12-startwrt-sni-divert.nft`) are
  filter/mangle marking chains, so this is the tree's first NAT include. `-101`
  is one ahead of fw4's own `dstnat` (`-100`) and well above `conntrack`
  (`-200`); netfilter orders base chains by declared priority, not by position in
  the ruleset. nft echoes it back as `priority dstnat - 1`.
- **`meta nfproto ipv4`** is required in the `inet` family. The client is v4-only
  today; v6 ingress is a non-goal (see Open questions).
- **The rate limit is the only place to shed an UPDATE flood cheaply.** One
  static rule means every device's UPDATE-shaped packet reaches the daemon, and
  authorization happens there — parse plus TSIG plus a set lookup per packet.
  Dropping the excess in nft costs nothing.
- **`@th,64,16` is the DNS ID** (the UDP header is 64 bits); the flags byte
  begins at `@th,80`, so the 4-bit opcode is `@th,81,4`, and `5` is UPDATE. The
  client is UDP-only, so no TCP variant is needed — TCP DNS's 2-byte length
  prefix plus a variable header length makes the offset non-constant anyway.
- **Not port 5354 — or 5454.** `5300 + vlan_tag` is SmartDNS's per-profile
  port namespace (`ctrl/src/dns.rs:18,22`), which spans **5300–9394**; 5354 is
  VLAN tag 54 inside it, and an earlier draft's 5454 is tag 154 — inside it
  too. Not a hard bind conflict — SmartDNS binds `127.0.0.1` — but the
  namespace is reserved by convention, so the implementation uses **9553**
  (bridge listeners) and **9554** (WireGuard listeners), both above it.
- **Replies un-NAT through conntrack**, so the client still sees its answer from
  `<gateway>:53` and needs no awareness of the diversion. This is precisely why
  a port redirect works here where TPROXY would not.

Consequences, all good:

- **The query dataplane is completely unchanged.** Zero added latency, zero new
  outage mode: if `startwrt-ctrld` is down, UPDATEs fail (the client retries in
  180s and marks the gateway unsupported for 5 min) and nothing else does.
- **One static rule, no per-device firewall churn.** Authorization stays in the
  daemon's `authorize` closure where it belongs, so toggling a device does not
  touch `/etc/config/firewall` or restart fw4.
- `InjectingHandler`'s forwarder catalog still points at the profile's dnsmasq,
  so an UPDATE-shaped probe that isn't authorized still gets sane behavior.

**This was the design's largest unknown and it is now settled on hardware.** The
raw-payload expression had no precedent anywhere in this tree; on 2026-08-21 it
compiled, loaded, and matched real client traffic on the K1, and the kernel
accepted the NAT chain at `-101`. The earlier draft's fallback (per-device
`src_ip` redirects managed like Phase 1's `_apf_*` sections) is **no longer
needed and has been dropped**.

The file goes in `backend/nftables/` alongside the existing includes; build
wiring is free, because `build/stage-files.sh:146-152` globs
`backend/nftables/*.nft` into `/etc/nftables.d/` and `build.mk` already lists the
directory as a prerequisite. `kmod-nft-nat` is already enabled, but heed
`12-startwrt-sni-divert.nft`'s lesson: an include whose expressions need a kmod
must ship with its `build/openwrt.diffconfig` entry, or fw4 refuses to load the
**entire** ruleset — a failure that takes the firewall down, not just the
feature.

Note when validating by hand that `prerouting` sees only _arriving_ packets.
A query generated on the router itself never traverses the chain, so a negative
control must be run from a LAN client, not from the router's own shell.

## Problem 2: getting answers back out through dnsmasq

The daemon holds the records, but every other LAN device asks dnsmasq. Two ways
to bridge, and the choice should be made by record type:

**A/AAAA — per-profile `addn-hosts` file in tmpfs.** `on_change` renders
`/tmp/startwrt-dns-inject.<section>` and signals that profile's dnsmasq. dnsmasq
re-reads `/etc/hosts` and every `--addn-hosts` file on SIGHUP and clears its
cache (so a previously cached NXDOMAIN for the name is dropped) without
restarting, so there is no DNS gap. The `list addnhosts` line is written into the
profile's `config dnsmasq` section once, when injection is first enabled for that
profile; after that, steady-state updates are pure tmpfs writes. **No flash
writes at all in the steady state.** This is the mechanism the shared module doc
anticipated, and it was validated end to end on the K1 on 2026-08-21: a file
written to `/tmp`, a signal, and the name resolving from a LAN client.

What this needs, and two things the earlier draft got wrong:

- **`ProfileDnsmasq` has no `addnhosts` field.** The typed section
  (`backend/uciedit/src/openwrt.rs:456-490`) models `server`, `noresolv`,
  `listen_address`, `dhcpscript` and the rest, but not this; add
  `pub addnhosts: Vec<String>`.
- **Signal through procd, never through the pidfile.** dnsmasq runs under
  `ujail` in its own PID namespace, so `/var/run/dnsmasq/dnsmasq.<section>.pid`
  contains `1` — dnsmasq's PID _inside_ the jail. `kill -HUP $(cat …)` from the
  host signals init and silently does nothing. Use procd, which knows the real
  PID: `ubus call service signal '{"name":"dnsmasq","instance":"<section>",
"signal":1}'`.
- **`/etc/init.d/dnsmasq reload` is not a re-exec, so the no-gap path already
  exists.** `reload_service()` (`dnsmasq.init:1377-1380`) regenerates the config
  and then calls `procd_send_signal`; procd restarts the instance only if the
  generated command line changed, and otherwise just delivers SIGHUP. So
  `devices::reload_dnsmasq()` (`ctrl/src/devices.rs:671`) sits on the right
  primitive already — it only needs to take an instance name. No new signalling
  machinery is required.
- **There is no bind-mount inode trap.** `ujail` exposes the host's `/tmp`
  wholesale rather than bind-mounting individual files, so writing the rendered
  file by temp-plus-rename is safe and the renderer may use the ordinary atomic
  idiom.
- **The one-time UCI write has a precedent to copy.**
  `device_ident::ensure_uci_dhcpscript` / `ensure_dhcp_fingerprint_hook`
  (`ctrl/src/device_ident.rs:225-303`) is the same shape: a `TypedSection` that
  touches only its declared field, returns whether anything changed, and reloads
  solely on change. Follow it rather than inventing a second idiom.

**Leave `local-ttl` at its default of 0.** dnsmasq serves hosts-file names with
`--local-ttl`, and 0 means downstream clients don't cache them. The extra query
load on the router is trivial; what it buys is that a withdrawn record disappears
immediately instead of lingering in client caches for the TTL — which matters
because records are also reaped by the sweep (decision 5) rather than by expiry.

**The renderer must select record types deliberately.** `parse_rdata`
(`rfc2136.rs:90`) restricts the _text/admin_ path to A/AAAA/CNAME/TXT, but
`apply_update` (`:239-260`) stores whatever arrives on the wire with **no type
filter at all**. An addn-hosts renderer must therefore filter to A/AAAA itself
and must not assume the store only ever holds representable types.

**CNAME/TXT — `server=/<name>/127.0.0.1#9553` delegation.** A hosts file cannot
express them. Delegating the specific name to the daemon can, and covers all
types uniformly, but it is a `/etc/config/dhcp` write (flash) and it changes the
generated command line — which is exactly the case where procd _does_ restart the
instance, i.e. a brief DNS gap for that profile. Names change rarely (only when a
user adds or removes a private domain), so this is acceptable — but it should be
added only when a CNAME/TXT actually needs serving, not unconditionally.

Phase 1 of the implementation can ship A/AAAA only and defer delegation.

> **The single most likely bug.** `DnsInjector::apply_update()` calls
> `self.notify()` unconditionally (`rfc2136.rs:285`) — including for the 180s re-assert that
> changes nothing. StartTunnel gets away with it because PatchDb diffs. On
> StartWRT, `on_change` **must** diff the rendered output and return early when
> unchanged — and it is also the dataplane rather than a display cache, so it
> must be serialized (see "on_change is the dataplane" below). Without the diff,
> a delegation-based implementation rewrites
> `/etc/config/dhcp` and reloads dnsmasq every 180 seconds per injecting device:
> flash wear plus a DNS gap, forever.

## Problem 3: the trust model

The client sends unsigned to a keyless gateway and the shared server refuses
unsigned. Something has to give. Three candidate resolutions:

1. **Accept unsigned from the LAN, authorized by source IP + the per-device
   toggle.** Requires a core change (a TSIG policy knob). Security: identical
   in kind to the LAN source-address spoofing already documented and
   deliberately accepted for Phase 1's PCP — but **strictly worse in degree**.
   PCP's shared cores force the DNAT target to the claimed source address, so a
   spoofer can only expose _the victim_; DNS injection as written lets a spoofer
   point _any name_ at _any address_, including redirecting a victim's private
   domain to the attacker's own box.

2. **A pairing flow** — user copies a key from StartWRT into StartOS. Secure,
   but StartOS's client has no per-gateway key configuration; it derives from
   the WG PSK and nothing else. Violates the "no client change" non-goal.

3. **Signed-only, over StartWRT's inbound WireGuard.** `vpn_server.rs:1288`
   already generates a PSK for **every** inbound peer, so a StartOS server that
   joins StartWRT over the inbound VPN has exactly the StartTunnel setup and
   works today with zero security compromise. It just doesn't cover the primary
   case (a server on the LAN).

**Recommendation: a tiered model — 1 constrained, plus 3 unconstrained.**

| Injector                            | Capability                                                            |
| ----------------------------------- | --------------------------------------------------------------------- |
| **Signed** (WG peer; TSIG verifies) | Any name, any rdata, A/AAAA/CNAME/TXT                                 |
| **Unsigned** (plain LAN, toggle on) | A/AAAA only, **and the rdata must equal the UPDATE's source address** |

The rdata-equals-source constraint is the load-bearing mitigation: it reduces
the unsigned capability from "point any name anywhere" to "publish a name for
yourself", which is exactly what the StartOS client actually does
(`targets_for()` returns the host's own subnet address as the rdata). A spoofer
is left with name-squatting and denial of service against a specific neighbor —
the same blast radius as Phase 1's accepted PCP exposure, no longer worse. It
costs one comparison and is worth stating as an invariant in the module doc.

> **Amended in review: unsigned updates arrive over TCP.** A spoofed UDP
> source could still delete a victim's records, since a delete carries no
> rdata to compare. A TCP handshake proves the source address, so the unsigned
> tier requires TCP. The divert takes every TCP connection to the router's
> port 53; the daemon answers UPDATEs and relays everything else to the
> profile's dnsmasq. `pre_update` receives the transport alongside the TSIG
> verdict.

**Enforce it inside `apply_update`, not in the `authorize` closure.** The
handler verifies TSIG _before_ calling into the store (`rfc2136.rs:332-336`), and
`authorize` receives only an `IpAddr` — it never sees the records. `apply_update`
(`:239`) has both `src` and `updates` in scope, so that is the only place the
comparison can be made. Required-change 1 below is scoped accordingly.

### The arrival-interface hole, and how to close it without a core change

Phase 1 does **not** merely accept LAN source-address spoofing — it closes the
cross-segment half of it. `serve_pcp` reads the receiving interface from
`IP_PKTINFO` and `arrival_matches` (`port_control.rs:804-843`, gated by
`is_known_client` at `:879`) requires it to be the interface the neighbor table
places the claimed source on, so a datagram arriving on `br-lan` cannot act as a
device located on `br-lan.101`. Only the same-segment case stays open, and the
module doc names that boundary honestly.

**An unsigned DNS tier as sketched cannot express that check**, because hickory's
`Request` exposes only `src()` (`hickory-server-0.26.1/src/server/request_handler.rs:88`)
— there is no arrival ifindex to compare against. Left as-is, the unsigned path
would be worse than PCP in a _second_ dimension, which breaks the "no worse than
Phase 1" claim the recommendation rests on.

**The fix needs no shared-core change.** `ServerFuture::register_socket` takes a
pre-bound `tokio::net::UdpSocket` (`hickory-server-0.26.1/src/server/mod.rs:102`),
so bind **one `SO_BINDTODEVICE`-scoped socket per profile bridge** and register
each with its own handler. The kernel then guarantees the arrival interface
structurally and the closure never needs to know about it — the same device-scoped
binding StartTunnel already relies on for its WireGuard sockets. This is settled
by reading, not a bench item: `register_socket` spawns `handle_udp` with the
socket, which hands it to `UdpStream::with_bound`
(`hickory-net-0.26.1/src/udp/udp_stream.rs:126`) — the fd is moved into the
struct and used for both directions, never rebound or duplicated, so a sockopt
set before registration survives.

**Frame this as preserving StartTunnel's property, not adding a new one.**
StartTunnel's whole authorization model rests on the source address being
unforgeable: it binds `<subnet>.1:53`, an address reachable only through the
tunnel, and WireGuard binds each in-tunnel IP to the peer key that decrypted the
packet. That is why a bare `BTreeSet<IpAddr>` (`context.rs:79-94`) suffices
there. A per-bridge `SO_BINDTODEVICE` socket is the closest a LAN can come to the
same guarantee: unforgeable across segments, and openly not within one.

**One more wrinkle for the security argument: the PSK lookup is tri-state.**
`signer_for` (`dns_update/mod.rs:296-312`) returns `None` both for a gateway with
no PSK _and_ for a transient NM/D-Bus error, so a WireGuard peer that does have a
PSK can be demoted into the constrained tier for a single attempt. It is not
cached, so the next tick recovers, and under the rdata-equals-source constraint
the demotion is harmless — but the argument should say so rather than assume
`None` means "plain router".

### Why the unsigned tier is defensible: what this device already does

There is no stock precedent to inherit — **dnsmasq does not implement RFC 2136
at all**, so OpenWrt has taken no position on these risks. The useful comparison
is with the equivalent capability the router already grants.

**DHCP-derived names are the same thing with fewer controls, enabled by
default.** Every OpenWrt box publishes DHCP lease hostnames into DNS; StartWRT's
per-profile sections set `expandhosts` and `domain=lan` (`profiles.rs:413-414`),
so a device that puts `nas` in DHCP option 12 becomes `nas.lan` for its whole
segment. That is unauthenticated name injection by any LAN device, on by default,
first-come, with no per-device permission. Its one constraint is that the address
is the one the DHCP server assigned — structurally the same property as
rdata-equals-source, reached by a different mechanism. The tier proposed here is
therefore **stricter than what the router already does**: default-off per device,
plus ownership, plus arrival-interface scoping, plus reserved names. What it adds
is scope — a name outside `.lan`.

**Real RFC 2136 servers take the opposite tack, and this design offers that too.**
BIND's `allow-update` documentation discourages address-based authorization as
spoofable and steers operators to `allow-update { key … }`; Windows AD ships
GSS-TSIG "secure dynamic update" precisely because the insecure mode produced a
known family of record-hijack attacks. That is exactly the posture of the
**signed tier**, which a StartOS server joined over inbound WireGuard gets for
free (`vpn_server.rs:1289-1290`).

**And mDNS, which every LAN already runs, is weaker than either.** Avahi/Bonjour
name claiming is unauthenticated, first-come, with no permission model at all.

So the unsigned tier sits between mDNS and DHCP names on one side and BIND-style
TSIG on the other, and TSIG stays the default: the unsigned path is a per-device
opt-in with a confirm dialog naming the trust granted, mirroring
`WgConfig::allow_dns_injection` being default-off for a tunnel Client.

## StartTunnel parity: `on_change` is the dataplane

StartTunnel's implementation is the model to follow, but one structural
difference drives most of what is StartWRT-specific, and it is worth stating
before the design decisions below.

**On StartTunnel the injector _is_ the resolver.** `bind_proxy`
(`tunnel/dns.rs:157-204`) puts `InjectingHandler` on `<subnet>.1:53` and answers
injected names straight out of the in-memory `BTreeMap`; `on_change` → PatchDb
exists only for the UI and for restart seeding. **On StartWRT the injector is not
the resolver**, so the file `on_change` writes is the only path by which a record
reaches a querier. Consequences:

- **`on_change` must be serialized.** Its type is `Fn(Vec<InjectedRecord>)`
  (`rfc2136.rs:135`), called synchronously from hickory's request task and
  returning nothing. StartTunnel spawns and logs errors — a lost write costs a
  stale UI row. StartWRT must also spawn (a file write cannot block the DNS
  task), so two rapid updates can finish out of order and the loser silently
  wins. Push the latest list through a `tokio::sync::watch` to a single render
  task rather than spawning per notification.
- **`authorize` is synchronous and StartWRT's identity lookup is not.**
  `Authorizer = Fn(IpAddr) -> bool`, satisfied on the tunnel from a set computed
  off the DB. `PortControl::authorized_client()` is `async` and hits the neighbor
  table, so it cannot be called from the closure. Use the same shape as
  `dns_allowed` — a `SyncMutex<BTreeSet<IpAddr>>` peeked synchronously — with a
  refresher driven by the neighbor table and UCI instead of a DB write.
- **`tsig_key` conflates "no key" with "not allowed"** (`rfc2136.rs:122-124`).
  Every tunnel client has a PSK, so the conflation never bites there. On StartWRT
  allowed-but-unsigned is a legitimate state and the two must separate.
- **The store is flat and shared across every subnet** (`context.rs:319` passes
  one `Arc<DnsInjector>` to every listener), which is correct on a tunnel whose
  subnets are mutually reachable. StartWRT must not do that. Render per profile
  by filtering the flat list on each record's `source` against an
  `IpAddr → profile` map the sweep maintains; the store itself stays flat, so no
  core change is needed. Note this is a deliberate **strengthening** relative to
  StartTunnel, justified by profiles being an isolation boundary.
- **Seeding inverts.** Tunnel seeds from PatchDb (`seed_records`); StartWRT
  persists nothing and passes `initial: vec![]`.

**What is actually shareable** is narrower than "reuse `tunnel/dns.rs`":
`DnsInjector`, `InjectingHandler`, `InjectedRecord`, `derive_tsig_key` and
`tsig_signer` are `pub`; `bind_proxy` and `forwarding_catalog` are private, and
`bind_proxy` hard-codes port 53 with no device binding. Two cheap moves buy real
reuse — move `forwarding_catalog` out of `tunnel/dns.rs` into `net/dns_update/`
(nothing about it is tunnel-specific), and generalize `bind_proxy` to take
`(addr, port, Option<&device>)` so StartTunnel keeps calling it with
`(addr, 53, None)`.

**One pattern to copy verbatim:** the shutdown discipline at `tunnel/dns.rs:30-37`
— tear listeners down through the `CancellationToken` and _wait_, because
aborting the task only schedules the abort of hickory's socket tasks and the
rebind then hits `EADDRINUSE`. StartWRT rebinds per-profile sockets on every
profile change, so it will meet this.

## Multi-profile design decisions

This is where StartWRT diverges from StartTunnel most sharply. StartTunnel's
subnets are flat and mutually reachable; StartWRT's profiles are a deliberate
isolation boundary with an explicit access policy (`LanAccess::{All,
SameProfile, OtherProfiles(set)}`).

**1. Visibility follows the existing access policy — not "all profiles".** A
record injected by a device in profile P is served to profile Q **iff** Q's
`lan_access` permits Q → P. Resolving a name you are firewalled away from is a
broken UX (a connection that hangs instead of failing fast) and a real
information leak — a guest device learning the name-to-address map of the
trusted subnet is precisely what the profile system exists to prevent.
`addn-hosts` files are per-dnsmasq-instance, so the mechanism is a natural fit —
but **it does not fall out for free**, because a default box has no per-profile
instances at all (see decision 2).

**2. Enabling injection on a profile forces it to have its own dnsmasq
instance — and on a default box that means creating the first one.** A profile
gets a per-profile section only when its DNS server list is non-empty
(`rewrite_dns_forwarding`, `profiles.rs:399`), so a stock StartWRT install runs a
**single** anonymous dnsmasq for everything. This was confirmed on hardware: one
`dhcp.@dnsmasq[0]` section, no per-profile instances, and consequently no
`DNS-Override` redirect anywhere. A shared instance cannot carry per-profile
visibility, so injection must create one. Three parts:

- **A sibling predicate, never a widened `has_effective_dns`.** That predicate
  (`profiles.rs:250`) also gates the `DNS-Override` redirect at `:326` and
  `:1928`; widening it would mean that switching on DNS injection silently starts
  hijacking every port-53 packet in the profile. Add instead:

  ```rust
  /// True when any device in `profile` may inject DNS records. Deliberately
  /// separate from `has_effective_dns`: that predicate also gates the
  /// `DNS-Override` redirect, and enabling injection must not start hijacking
  /// the profile's port-53 traffic.
  fn has_dns_injection(cfgs: &Configs, profile: &Profile) -> bool
  ```

  and widen only the section gate:

  ```rust
  let inject = has_dns_injection(cfgs, profile);
  if !servers.is_empty() || inject {
      cfgs["dhcp"].append(&ProfileDnsmasq {
          noresolv: (!servers.is_empty()).then(|| "1".to_string()),  // was hard-coded
          addnhosts: inject.then(|| vec![inject_path(profile)]).unwrap_or_default(),
          interface: vec![profile.id.interface.clone()],             // was vec![]
          server: servers,
          ..
      }, Some(&section_name))?;
  }
  ```

  With an empty `server` list and `noresolv` unset the instance falls back to
  `/tmp/resolv.conf.d/resolv.conf.auto`, the same upstreams the main instance
  uses, so ordinary queries behave identically.

- **Scope the new instance to its own interface, or it will serve every
  profile's DHCP.** A `config dhcp` pool with no `option instance` is picked up
  by **every** dnsmasq instance (`dnsmasq.init:239-246`), StartWRT creates one
  pool per profile (`profiles.rs:2205-2217`), and it sets `instance` nowhere.
  Setting `interface: vec![<profile iface>]` on the new section confines it to
  its own bridge; the existing fix-up that adds `notinterface` to the main
  instance (`profiles.rs:422-426`) keeps the main one off that bridge.
  **This looks like a pre-existing latent bug**: any install that already has a
  profile with VPN or custom DNS is running a second instance with
  `interface: vec![]` today. Worth checking separately on such a box with
  `grep -E 'dhcp-range|^interface' /var/etc/dnsmasq.conf.*`.

- **Verify no `dhcp-range` reaches the new instance** once this is implemented —
  the same grep, on a box where injection has created a section.

**3. Name ownership is first-come, keyed by MAC.** `DnsInjector::apply_update()`
has **no ownership check** — any authorized source may overwrite any name.
That is tolerable on StartTunnel, where every authorized device is trusted by an
explicit toggle and there is one flat trust domain. It is not tolerable on a
router with a guest VLAN: without it, an authorized device in a low-trust
profile can hijack a name a trusted server owns. Bind `(name, rtype)` to the
first injecting identity and refuse a different one's UPDATE for a held name.

**Keep the ownership map in memory; do not persist it.** Persisting would close
exactly one race — after a daemon restart the store is empty, every client
re-asserts within 180s, and whoever claims a name first in that window keeps it.
For that to be an attack the claimant must _already hold injection permission_:
an unpermitted device is refused outright, and a device spoofing a permitted
neighbour's source address can only publish records pointing at that neighbour,
because rdata is pinned to source. So the residue is "a device the user
explicitly granted injection to has been compromised" — the same trust boundary
already accepted for PCP — and for the primary case, StartOS private domains,
the claimant still cannot produce a valid certificate for the name. Document the
residue rather than adding a store. If it ever needs closing, the natural home is
a second option on the DHCP host section already being written for the
permission, not a new persistence layer.

**Identity is not a MAC on every transport.** A WireGuard peer has none, so the
owner key is `Owner::{Mac(..), WgPeer(pubkey)}`, resolved for the VPN side from
`devices::parse_wg_show_dump` (`devices.rs:640`) and each peer's `allowed_ips`.
Keyed on MAC alone, every VPN-injected name would be unowned.

**4. Reserved names.** Refuse injection of anything under `lan.` — dnsmasq is
authoritative there (`local=/lan/`, `domain=lan`) and delegation would conflict
with DHCP-derived hostnames — and under any profile-owned domain. Do **not**
refuse `.local`: see decision 7.

**5. Records are bound to the address assignment, not just a clock.** Devices
move between profiles (a WiFi password change, an Ethernet port re-tag) and
their address changes with them. Records keyed by source IP go stale
immediately. Reuse Phase 1's sweep invariant verbatim: drop a record whose
owning MAC no longer holds the address it points at. The 180s re-assert then
republishes it under the new profile. Phase 1's profile-move handling is the
citable precedent: `published_ports::remove_ports_for_macs` (`:321`) drops
stranded `_apf_*` forwards (`:336-346`) while _preserving_ `_allow_pcp` (`:370`),
and `PortControl::sweep` reaps `unbound_sections` against
`devices::current_lease_ips()` (`devices.rs:263`). Do the same here — drop
records, keep the permission.

**This is also the liveness answer, and no expiry timer is needed.** A device
that simply vanishes leaves a record behind; the harm is not that the record
exists but that DHCP eventually recycles its address to a _different_ device, at
which point the name resolves to the wrong machine. The sweep invariant catches
exactly that case. A record pointing at an address nobody holds is inert — the
same experience as a powered-off server. StartTunnel needs neither the sweep nor
a timer because it persists deliberately, exposes an operator-visible delete
(`remove_dns_record`, `tunnel/api.rs:565`), and assigns client addresses
statically rather than from a recycling pool; StartWRT has none of the three,
which is why the sweep does the work.

**6. Do not persist record data.** StartWRT has no database, and the client
re-asserts every 180s, so a cold store self-heals within three minutes of boot.
Persisting buys three minutes of faster convergence at the cost of flash writes
and a whole staleness problem. Persist only the per-device permission flag,
which is already UCI. (Note this diverges from StartTunnel, which persists to
`db.dns_records` — the right call there, where PatchDb is free and the UI reads
it live.)

**7. Injection over inbound WireGuard is a bonus, not an edge case.** StartWRT's
inbound VPN clients hit the same problem StartTunnel's do — Android
[excludes VPN connections from `.local` resolution](https://source.android.com/docs/core/ota/modular-system/dns-resolver),
so a phone on the VPN cannot mDNS-resolve `<server>.local`. `#3523`'s
`spawn_server_mdns_injection` injects `<hostname>.local` over **every WireGuard
gateway**, which includes a StartWRT inbound-VPN interface. A StartOS server
joined over inbound VPN therefore fixes StartWRT's `.local`-over-VPN problem
automatically, with full TSIG, the moment this lands. Worth calling out in the
docs, and worth a test.

**8. Rule ordering against `DNS-Override` — specify the priority, don't discover
it.** The UPDATE diversion must be evaluated before the `DNS-Override-<profile>`
redirect, which DNATs all port-53 traffic in the zone to the profile gateway.
This is settled by construction rather than by experiment: netfilter orders base
chains by **declared priority**, not by textual position in the ruleset, and
fw4's own `dstnat` chain sits at priority `-100`. Declare the divert chain at an
explicitly lower number — `-101` works and stays well above `conntrack` at
`-200`, so conntrack still sees the original tuple. `nf_nat` initializes a
mapping once per conntrack entry, so the first chain to act wins and the later
`DNS-Override` DNAT is skipped.

The chain loads and matches at `-101` on hardware. The race itself is still
unrun, because a default box has no `DNS-Override` rule to race against — create
one by setting custom system DNS servers (`dns::get_system_dns_servers`,
`dns.rs:77-88`) through the UI, then confirm the divert counter still increments.
_(Bench item; the design does not fork on the outcome.)_

## Required changes to shared core

These touch StartTunnel. Each needs to be verified behavior-preserving there,
the way Phase 1's two intentional shared-core changes were.

**Two changes, not three.** The unsigned-tier constraint, name ownership, and
record-type filtering all need the same thing — `(src, &[Record])` before the
store is mutated — and none can live in `authorize`, which receives only an
`IpAddr` and never sees the records. One hook covers all three, and gives a much
smaller diff to argue behavior-preserving than a `TsigPolicy` enum plus an
ownership flag plus a notify contract:

1. **A `pre_update` hook on `DnsInjector`**, called at the top of `apply_update`:

   ```rust
   pre_update: Fn(IpAddr, &[Record], bool /* tsig_verified */) -> ResponseCode
   ```

   StartWRT puts rdata-equals-source, ownership, and the A/AAAA filter here.
   StartTunnel passes an accept-all, so its behavior is unchanged by
   construction. This also subsumes the TSIG requirement currently hard-coded in
   `InjectingHandler::handle_request` (`rfc2136.rs:332-336`): pass the verification
   result through rather than refusing before the hook runs, and let policy live
   in one place.

2. **A change-diffing contract for `on_change`** — either document that it may
   fire without a change (and diff on the StartWRT side), or suppress the
   notify when the store is unmodified. The latter is better for both products.

Optionally, two visibility moves that cost nothing and buy real reuse: lift
`forwarding_catalog` out of `tunnel/dns.rs` into `net/dns_update/`, and
generalize `bind_proxy` to `(addr, port, Option<&device>)` so StartTunnel keeps
calling it with `(addr, 53, None)` and StartWRT can bind `<gateway>:9553` scoped
to a bridge.

No new dependencies: `hickory-server` is already an unconditional `start-core`
dep, and `startwrt-core` imports `start-core` aliased as `startos`.

## Implementation sketch

New module `ctrl/src/dns_inject.rs`, alongside `port_control.rs` and reusing its
helpers. **Every helper named here is private today** — that visibility work is
the sketch's largest unstated cost, so decide up front whether it becomes
`pub(crate)` promotions or a shared `ctrl/src/lan_client.rs` module, and say
which in the implementation PR:

- `authorized_client()` (`port_control.rs:268`) / `resolve_client()` (`:286`) are
  private **methods on `PortControl`**; `Client` (`:187`) is a private struct with
  private fields; `parse_neigh()` (`:1430`) and `uci_task()` (`:923`) are private
  free functions. Together they give neighbor-table IP→(MAC, iface) resolution;
  the brief positive-and-negative cache (`CLIENT_CACHE_TTL`, 10s, `:149`,
  `:272-281`) belongs to `authorized_client`, not to `parse_neigh`, which is a
  pure parser.
- The `_allow_pcp` lookup is **not** generalized — it is hard-coded at
  `port_control.rs:302`, `devices.rs:1575` and `published_ports.rs:370`, and
  `DhcpHost` is a `TypedSection` with a typed `_allow_pcp` field
  (`openwrt.rs:494-510`). So "take the option name" is not a string parameter: it
  means a second typed field plus a selector closure.
- `upsert_dhcp_host()` lives in `ctrl/src/devices.rs:1699` (already
  `pub(crate)`), not in `port_control.rs`, and takes a `_allow_dns_inject` option
  on `DhcpHost` mirroring `_allow_pcp` (an option fw4 and dnsmasq ignore).
- `uci_task()` for the `!Send` `Arena`/`Configs` boundary.

Wiring:

- Bind `InjectingHandler` per profile on a `SO_BINDTODEVICE`-scoped UDP socket
  for that profile's bridge, registered via `ServerFuture::register_socket`;
  forwarder catalog → that profile's dnsmasq. Reuse the existing supervisor —
  `supervise()` (`port_control.rs:1520`) already wraps Phase 1's tasks with 5s
  backoff, so the panic-kills-the-sweep failure that motivated it is historical,
  not something to re-solve.
- `authorize` closure → `authorized_client()`; the arrival interface comes from
  the scoped socket rather than from the request.
- `tsig_key` closure → the inbound-WG peer PSK for that source, else `None`
  (which, under the new policy, routes to the constrained unsigned path).
- `on_change` → push the list into a `tokio::sync::watch`; a single render task
  diffs, writes each profile's addn-hosts file (A/AAAA only, filtered by the
  `IpAddr → profile` map), and signals that instance via
  `ubus call service signal` — never the pidfile, which is namespace-local.
- **Purge every rendered file at daemon start**, before binding. Records live in
  memory and die with the daemon; the files do not, so dnsmasq would keep
  answering from stale ones — including for devices whose permission was revoked
  while the daemon was down. This is the invariant Phase 3 already enforces for
  `apf_sni_*` rules: state must not outlive the in-memory records that justify
  it. Cost is that names stop resolving for up to 180s after a daemon restart,
  which is the convergence budget the design accepts everywhere else.

RPC (update `API_CONTRACT.md`, `api.service.ts`, **and both** `live-api` and
`mock-api`):

- `devices.set-dns-injection { mac, allow }` — mirrors `devices.set-auto-forward`.
- `dns.injected-list` — read-only view for the UI, grouped by device.

**No manual CRUD.** StartTunnel exposes `add_dns_record` / `remove_dns_record`
(`tunnel/api.rs:481-499`) over `DnsInjector::upsert`/`delete`, which bypass the
authorizer and TSIG by design — and would bypass ownership too. StartWRT does not
copy that surface: injection is client-managed, and the router's manual
equivalent already exists as a static DHCP lease with a hostname. Not exposing it
means that bypass is never reachable here.

UI: a per-device toggle on the Devices page beside the automatic-port-forwarding
toggle, default off, with a confirm dialog naming the trust granted — matching
StartTunnel's pattern and Phase 1's. An injected-records table, read-only at
first.

## Phasing

Phase 1 (PCP/UPnP) and Phase 3's SNI dataplane are already built; what follows is
this document's own work, in order:

1. Core: the `pre_update` hook + notify suppression, with StartTunnel regression
   tests proving no behavior change.
2. StartWRT: UPDATE ingress (nft NAT include), the per-profile dnsmasq instance
   and its sibling predicate, `InjectingHandler` on a per-profile device-scoped
   socket, addn-hosts answer plane, A/AAAA only, per-device toggle + RPC + UI.
3. Multi-profile visibility filtering per `lan_access`; profile-move reaping.
4. CNAME/TXT via delegation; IPv6 ingress — sequenced together with Phase 3's
   remaining IPv6 pinhole work, since both hinge on the same question about what
   a StartWRT LAN actually presents (see Open question 3).

## Testing

Unit-testable in `startwrt-core` without hardware: rendered addn-hosts content,
the diff/no-op path, ownership refusal, the rdata-equals-source constraint,
visibility filtering against a synthetic `lan_access` matrix, profile-move
reaping. Note Phase 1's known gap — `TestContext.effectful()` is always false,
so effectful early-return paths are untestable in-process. One construction
detail: `DnsInjector` needs a tokio runtime to build at all (its `SyncMutex`
spawns a lock watchdog, `rfc2136.rs:420`), so these must be `#[tokio::test]`.

Validated on K1 hardware, 2026-08-21:

- The raw-payload opcode match compiles, loads, and matches real client traffic;
  the NAT include loads without costing the ruleset; the chain sits at `-101`
  (nft echoes `priority dstnat - 1`).
- **The three-case divert matrix passes.** Against the final rule, one client
  batch — an UPDATE to the gateway, an UPDATE transiting to an off-LAN address,
  and an ordinary query — moved the counter by exactly 1. Local UPDATEs divert,
  transit UPDATEs do not, ordinary queries do not.
- dnsmasq signalling through `ubus call service signal`, and `addn-hosts` picked
  up and resolving from a LAN client.

The unqualified rule was observed matching a _transit_ UPDATE before
`fib daddr type local` was added — which is how that clause was found. Note also
that `prerouting` sees only arriving packets, so a negative control run from the
router's own shell proves nothing.

Still requires hardware: ordering vs. `DNS-Override` (needs a profile with DNS
configured — none exists by default); no `dhcp-range` reaching a newly created
per-profile instance; the SNI hairpin failing as described and then resolving
once injection supplies the LAN record; end-to-end against a real StartOS box
with a private domain, both LAN-attached (unsigned) and inbound-VPN-attached
(signed).

## Open questions

**Settled.** Two questions the earlier draft left open are now decided and are
recorded here so they are not reopened:

- _Is the unsigned path acceptable at all?_ **Yes, as a per-device opt-in.** The
  device-scoped socket closes cross-segment spoofing, leaving the same
  same-segment residue the shipped PCP path already accepts, and the capability
  granted is strictly narrower than the DHCP-name injection this router performs
  for every device by default. Signed-only would also leave LAN clients hanging
  on the SNI hairpin.
- _Should visibility follow `lan_access` or the injecting profile only?_
  **Follow `lan_access`** (decision 1). It is the only option that cannot
  produce a name which resolves but is unreachable, and `lan_access` already
  encodes the user's reachability intent.

Still open:

1. **Manual records** — worth the UI surface later? Currently answered "no" (see
   the RPC section); this records that the door isn't nailed shut if users hit
   name-squatting in practice.
2. **IPv6.** StartWRT's LAN is SLAAC-only with no RA M-flag, and the shared
   client publishes AAAA only for a non-link-local address on a subnet with a
   same-family resolver. Confirm whether a StartWRT LAN ever presents one before
   building v6 ingress — and settle it **together with Phase 3's remaining IPv6
   pinhole work**, which turns on the same facts, rather than twice.
3. **Should registering an SNI hostname route auto-create the matching private
   domain record?** Today it does not: injection keys on
   `HostnameMetadata::PrivateDomain` alone, so a user who registers a hostname
   route must separately enable that domain as a private domain to get LAN
   resolution. Auto-creating it would make the hairpin fix invisible and correct
   by default; not auto-creating it keeps the two permissions independent, which
   matters because an SNI route is a _WAN_ exposure and a private domain is a
   _LAN_ disclosure. If the answer is "no", the docs must spell out the manual
   second step.

## Landing obligations

Per the root `AGENTS.md`: user-visible behavior means
`projects/start-wrt/docs/src/` (a section under `devices.md` or
`security-profiles.md`, plus an `faq.md` entry for `.local` over inbound VPN)
and a `projects/start-wrt/CHANGELOG.md` entry — in the same change, not a
follow-up. That entry goes under the existing **unreleased `## [1.1.0]`** block
(origin tags stop at `start-wrt/v1.0.1`), which already carries Phase 1's and the
SNI dataplane's entries; do not cut a new heading. `API_CONTRACT.md` and both API
service implementations move with the handler.

The workflow-`paths:` obligation is moot for this change:
`.github/workflows/start-wrt.yaml` already allowlists `projects/start-wrt/**`
(`:35`, `:56`), which covers `backend/nftables/` and `backend/hotplug/`. It would
only apply if the build gained an input _outside_ the project tree.
