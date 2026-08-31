# UPnP Vendor-Defined Action for SNI Hostname Mappings

Status: accepted as a Start9-internal design; implemented (server + StartOS
client, branch `wrt/upnp-hostname-action`). Not a standards submission: unlike
the PCP companion (`draft-start9-pcp-hostname`), a UPnP vendor-defined action
has no standards venue — the SCPD is the external documentation.

A second front door onto the existing SNI demux: today a client binds a hostname
to a shared external port only over PCP (the `HOSTNAME` private-use option,
`rfcs/draft-start9-pcp-hostname.md`). This adds a UPnP IGD vendor-defined action
that does the same thing, so a client whose gateway is reachable over UPnP but
not PCP can still get an SNI-demuxed mapping.

Almost all of it lands in already-shared code, so StartTunnel and StartWRT get
the server side from one change.

## Background

### What exists

**Server.** `shared-libs/crates/start-core/src/net/port_map/server/igd.rs` is the
shared UPnP IGD server. `handle_control` (`:183`) dispatches four standard SOAP actions —
`GetExternalIPAddress`, `AddPortMapping`, `AddAnyPortMapping`,
`DeletePortMapping` — and falls through to `fault(401, "Invalid Action")`. It is
used by StartTunnel today and by StartWRT as of PR #3634.

**SNI backend.** `GatewayBackend` (`server/mod.rs`) already carries
`add_sni_forward` / `remove_sni_forward`, driven by the PCP `HOSTNAME` path.
PR #3634 makes the dataplane optional — `fn sni(&self) -> Option<&Arc<SniDemux>>`
— with `add_sni_forward` early-returning when it is `None`.

**Client.** `net/port_map/client.rs:695` short-circuits: _"HOSTNAME (SNI-demux)
mapping: PCP-only, since NAT-PMP/UPnP can't demux by SNI."_ Support is confirmed
per-gateway via a PCP `ANNOUNCE` capability marker, cached as
`pcp_hostname: CapabilityVerdict` (`db/model/public.rs:292`) with a negative
trust window.

### Why a vendor action is legal UPnP

The UPnP Device Architecture permits a vendor to add non-standard actions to a
service, named `X_<VENDOR>_<Action>` and declared in the SCPD beside the standard
ones. Clients ignore actions they do not recognize. AVM ships `X_AVM-DE_*` on
FRITZ!Box WANIPConnection in volume, which is the deployment evidence that
extending a standard service this way does not upset third-party clients.

### Why a second transport, given PCP already works

The feature still requires a Start9 gateway on the other end — a third-party
router has no SNI demux dataplane regardless of how it is asked. So this adds no
reach on the _gateway_ side; every Start9 gateway speaks PCP already.

It adds reach on the **client** side, in two ways:

1. **The existing fallback chain.** The port-mapping client tries PCP, NAT-PMP,
   and UPnP against a gateway. Hostname mappings are the one capability with no
   UPnP rung, so a client that reaches a Start9 gateway over UPnP but not PCP —
   UDP 5351 filtered by an intermediate device, a source-address binding the
   PCP path cannot satisfy — silently loses SNI demux while ordinary forwards
   keep working. This closes that asymmetry.
2. **Third-party client implementers.** A `HOSTNAME` mapping over PCP means
   implementing a private-use option over raw UDP. The same thing over UPnP is a
   SOAP POST against a documented, discoverable action. For anyone outside Start9
   writing a client, that is a large difference in cost.

Note that "vendor-defined action" is UPnP terminology with no PCP counterpart to
add: PCP's extension mechanism is the private-use option range, and it is already
implemented and shipping as `OPTION_HOSTNAME = 224`. This proposal brings the
UPnP side up to parity with a PCP capability that already exists.

## Goals

- A client that can reach the gateway's UPnP IGD control endpoint can create and
  delete SNI hostname mappings.
- One implementation serves both products. StartWRT must require no
  product-specific code in this change.
- Capability discovery with no extra round trip.
- Identical authorization and ownership semantics to the PCP path — no new
  trust granted by choosing a different transport.
- Standard IGD clients are unaffected by the extended SCPD.

## Non-goals

- Changing the PCP `HOSTNAME` path, which stays the preferred transport.
- ~~Building StartWRT's SNI dataplane.~~ Originally out of scope; the dataplane
  was subsequently folded into the same branch (divert infra as an fw4 include
  - `DivertConfig`, `Via::sni() -> Some`, WAN-admit rules, WAN re-key), so the
    vendor action ships _working_ on StartWRT, not inert.
- IPv6 (the demux is v4-only today).
- NAT-PMP, which has no extension mechanism to carry a hostname.

## Design

### Naming and placement

Extend the existing `WANIPConnection:1` service rather than defining a new one:
one control URL, one SCPD the client already fetches, and the AVM precedent.

```
X_START9_AddHostnameMapping
X_START9_DeleteHostnameMapping
```

Arguments mirror `AddPortMapping` exactly, plus `NewHostname` — so the handler is
a near-copy of `add_mapping` and the existing `soap_u16` helper is reused
unchanged:

| Action                           | In-args                                                                                                                                                                 |
| -------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `X_START9_AddHostnameMapping`    | `NewRemoteHost`, `NewExternalPort`, `NewProtocol`, `NewInternalPort`, `NewInternalClient`, `NewEnabled`, `NewPortMappingDescription`, `NewLeaseDuration`, `NewHostname` |
| `X_START9_DeleteHostnameMapping` | `NewRemoteHost`, `NewExternalPort`, `NewProtocol`, `NewInternalPort`, `NewHostname`                                                                                     |

Delete carries `NewInternalPort` — unlike `DeletePortMapping`, which identifies
a mapping by external port alone — because an SNI route's identity and
ownership is its full target `(peer, internal port)`: both the demux and the
tunnel's persistence match on it exactly, and the PCP delete (a lifetime-0 MAP)
carries the internal port the same way.

The alternative — a separate `urn:start9-com:service:HostnameMapping:1` — is
cleaner in principle but costs a second service block in the root description, a
second control endpoint, and a second SCPD fetch, to avoid a risk AVM's
deployment suggests is not real. See Open questions.

### Error codes

UPnP reserves errors 800–899 for vendor use. Map the PCP result codes across so
both transports report the same conditions:

| Condition                                         | PCP                            | UPnP                           |
| ------------------------------------------------- | ------------------------------ | ------------------------------ |
| Hostname already bound on this `(extIP, extPort)` | `RESULT_HOSTNAME_TAKEN` (192)  | **800** `HostnameTaken`        |
| Gateway has no SNI dataplane                      | `RESULT_UNSUPP_HOSTNAME` (193) | **801** `HostnameNotSupported` |
| Malformed hostname                                | malformed-option               | 402 `Invalid Args`             |
| Peer not an authorized device                     | —                              | 606 `Action not authorized`    |

**801 is the honest answer from any gateway without an SNI dataplane.** Both
handlers check `backend.sni().is_none()` before doing anything else and fault
801 (possible because this branch stacks on PR #3634's `Option`-ification of
`sni()`), so a backend returning `None` advertises-but-refuses rather than
"succeeding" into a demux nothing listens on. StartWRT's backend originally
returned `None`; its dataplane now rides this same branch, so on both Start9
gateways `sni()` is `Some` and 801 remains for future/partial backends.
`sni_fault` additionally maps a backend `RESULT_UNSUPP_HOSTNAME` to the same 801.

### Capability discovery — free, via `control_schema`

`igd_next::Gateway` (0.17.1, `gateway.rs:13`) carries
`pub control_schema: HashMap<String, Vec<String>>` — "Control schema for all
actions" — populated during `discover()`. The client therefore already holds the
gateway's parsed action list in memory the moment it is on the UPnP path at all.

Detecting support is `control_schema.contains_key("X_START9_AddHostnameMapping")`.
No probe, no marker option, no extra request, no XML parsing to write. This is
strictly simpler than the PCP `ANNOUNCE` marker it parallels, and it is why no
persisted capability verdict is needed (see Client, below).

### Lease semantics — the one real semantic gap

`add_mapping` comments that _"UPnP IGD leases are permanent here (StartOS
requests lease 0); PCP is the lease-bearing path."_ Left alone, a UPnP-created
SNI route would be permanent, and StartTunnel's lease-expiry sweep — which
exists precisely so an automatic mapping dies when its device stops renewing —
would never reap it. That is a behavior regression relative to the PCP path.

It is worse than an ordinary stale forward. A stale DNAT wastes a port and is
visible and removable in the published-ports UI. A stale **SNI route holds a
name**: `add_sni_forward` answers `HOSTNAME_TAKEN` to anyone else asking for it,
so the legitimate owner is locked out with no path to recover, and the condition
never self-heals.

The decisive detail is in `sni.rs`'s `Binding`:

```rust
struct Binding {
    target: SocketAddrV4,
    /// `None` for a permanent (DB-backed/manual) binding that never expires.
    expiry: Option<Instant>,
}
```

`None` is **reserved for operator-created bindings**. A device-initiated route
passing `None` would be indistinguishable from one an admin added by hand — a
category error, not merely an expiry policy choice.

**Resolution: vendor-action routes are lease-bearing.** `add_sni_forward`
already takes `lifetime: Option<u32>` and the PCP path already passes
`Some(lifetime)`, so this is passing a different value at one call site — not
new machinery. Honor `NewLeaseDuration` when nonzero, clamped by
`MAX_LIFETIME_SECONDS` (3600); apply that clamp as the default when the client
sends 0. The existing sweep then reaps unrefreshed routes with no change.

Alternatives considered:

- **Permanent, reaped by device lifecycle only.** Relies on the existing "delete
  or demote a device clears its forwards, SNI routes, and pinholes" path.
  Rejected: it misses the common case — a device that goes offline or withdraws
  the exposure without being deleted — which is exactly what the lease sweep was
  added for.
- **Permanent, recovered by owner re-registration.** Ownership is keyed to the
  target, so a returning device can overwrite its own route. Rejected: it fails
  precisely when the device's address changes, which on StartWRT happens
  routinely on a profile move (the DNS-injection RFC's decision 5). The stale
  route then squats the name against the device's own new address.

### Authorization

Unchanged from `add_mapping`: `is_known_client(peer)` gates the call, and the
target is forced to the requesting peer's own address (`target =
SocketAddrV4::new(peer, internal_port)`), so `NewInternalClient` cannot be used
to publish someone else. Delete is owner-scoped the same way `delete_mapping`
is, so a peer cannot remove or probe for another's route.

## Changes by layer

### Shared server — `net/port_map/server/igd.rs` (both products, one change)

- Two arms in `handle_control`'s match.
- `add_hostname_mapping()` / `delete_hostname_mapping()`, structured as
  `add_mapping` / `delete_mapping` but calling `backend.add_sni_forward()` /
  `remove_sni_forward()`.
- A `soap_str(body, tag)` extractor beside the existing `soap_u16`.
- Hostname validation reuses `pcp::hostname::validate_hostname`. It is no longer
  PCP-specific; consider lifting it to `port_map/hostname.rs`. Cosmetic — do it
  only if it stays a small diff.
- `igd_xml/scpd.xml`: two `<action>` blocks. The existing SCPD test asserts named
  actions rather than an exhaustive list, so it does not need loosening.

### StartTunnel

Nothing. `sni()` already returns `Some`, and the tunnel's `add_sni_forward`
override persists routes to PatchDb — so persistence, restart survival, and
dashboard visibility come free via the shared trait method.

### StartWRT

The SNI dataplane rides this branch: `kmod-nft-socket` in the image, the
reply-path divert as an fw4 include (`12-startwrt-sni-divert.nft`) plus a
`DivertConfig` for the iproute2 half (table 5344, masked fwmark), `sni()`
returning the shared demux, per-port WAN-admit ACCEPT rules (`apf_sni_<port>`,
which also make the port read as router-reserved), WAN re-key via a `wan`
hotplug hook + sweep backstop, and SNI rows in `published-ports.auto-list`.
Routes are demux-memory only (finite-lease, device-renewed); a daemon restart
drops them until the device re-asserts.

### Client — `net/port_map/{client,upnp}.rs`

- `upnp.rs`: SOAP calls for the two actions, alongside the existing `add_port` /
  `remove_port`.
- `client.rs:695`: replace the PCP-only short-circuit. PCP stays first; when a
  gateway's HOSTNAME verdict is known-absent, fall through to the UPnP path if
  `gateway.control_schema` advertises `X_START9_AddHostnameMapping`.

**No PatchDb change.** Support is read from `control_schema`, which `discover()`
already populated on the `Gateway` the UPnP path is holding — so there is no
probe to suppress and nothing worth persisting. Adding a
`upnp_hostname: CapabilityVerdict` beside `pcp_hostname` would be symmetric, but
it buys nothing here: the negative trust window exists to avoid re-probing, and
this costs no probe. It would also drag in the full cross-layer sequence
(`make start-core-ts-bindings` → SDK rebuild → web and container-runtime type
checks) for a field no UI reads — `pcpHostname` today appears only as a seed
value in `projects/start-tunnel/web/src/app/services/patch-db/data-model.ts:133`
and is rendered nowhere.

This keeps the whole proposal inside `start-core`, touching no product's UI,
bindings, or database.

## Phasing

1. **Shared server + SCPD + lease semantics**, with StartTunnel regression tests.
   Ships working on StartTunnel.
2. **Client UPnP hostname path + capability caching.** The cross-layer step.
3. **StartWRT SNI dataplane** — folded into the same branch (see the StartWRT
   section above), so the router serves the action rather than faulting 801.

Phase 1 is independently useful and independently reviewable: it makes the
gateway answer the action, which is what a manual `curl` or a third-party client
would exercise.

## Testing

Unit-testable in `start-core`:

- SOAP parse of both actions, including `NewHostname` extraction.
- SCPD advertises both actions; the standard action set is unchanged.
- Fault 801 when `sni()` is `None` — the StartWRT-shaped backend.
- Fault 800 when the hostname is held by a different target.
- Fault 606 for an unauthorized peer; 402 for a malformed hostname.
- Target forcing: `NewInternalClient` naming another host does not publish it.
- Owner-scoped delete: a different peer's delete does not remove the route.
- Lease: `NewLeaseDuration` 0 → clamped default; nonzero → honored and clamped;
  the sweep reaps an unrefreshed route.

Integration:

- StartOS client against StartTunnel with PCP blocked (drop UDP 5351) — the
  mapping still comes up over UPnP, and the SNI route serves.
- **Third-party regression:** `miniupnpc` and `igd-next` against the extended
  SCPD, confirming the added actions do not disturb standard `AddPortMapping` /
  `DeletePortMapping` flows. This is the check that validates the
  extend-WANIPConnection decision.

## Landing obligations

Per the root `AGENTS.md`:

- `projects/start-tunnel/CHANGELOG.md` under the prospective next version, and
  `projects/start-tunnel/docs/src/published-ports.md` if the behavior is
  user-visible.
- `projects/start-os/CHANGELOG.md` under the prospective next version — the
  client fallback changes StartOS behavior, and client-side port-map changes
  carry StartOS entries by precedent.
- `projects/start-wrt/CHANGELOG.md` — the dataplane rides this branch, so the
  unreleased automatic-port-forwarding entry describes hostname routes, and
  the StartWRT docs book's published-ports page documents them.
- If the capability field is added: TS bindings → SDK rebuild → web /
  container-runtime type checks, in that order, in the same change.
- `API_CONTRACT.md` is untouched; this is not a JSON-RPC surface.
- No CI `paths:` change — no new build inputs.

## Open questions

1. **Extend `WANIPConnection:1`, or define a separate vendor service?**
   Recommended: extend, per AVM precedent; the third-party regression test is
   what confirms it.
2. **Lease-bearing vendor-action routes, diverging from `AddPortMapping`'s
   permanence on the same server?** Recommended: yes — see Lease semantics. A
   client that assumes UPnP mappings are permanent would see a route expire, but
   the only clients are ours.
3. **One hostname per call, or several?** A PCP `MAP` can carry multiple
   `HOSTNAME` options in one request; SOAP has no natural framing for a repeated
   argument. One call per hostname is proposed. This is a wire-efficiency
   question only — the backend already registers hostnames one at a time.
4. **Does anything document the vendor action externally?** The PCP side has an
   IETF-style draft (`draft-start9-pcp-hostname`). A UPnP vendor action has no
   equivalent venue, so if third-party client implementers are part of the
   justification, the SCPD needs to be the documentation — which argues for
   precise `<argumentList>` entries and a short section in the tunnel's docs.
