# Changelog

All notable changes to StartTunnel are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.1.0]

### Added

- **Per-subnet IPv6.** A subnet can now carry a routed IPv6 prefix your VPS delegates, and every host on it — the tunnel and each device — gets one globally-routable `/128` with its tunnel IPv4 embedded (`prefix-network | tunnel-IPv4`), so a device's address is stable and derivable from its IPv4 alone. Configure it per subnet with `start-tunnel subnet <SUBNET> set-ipv6 --prefix <prefix>` or the subnet's Add/Edit dialog (disable by omitting the prefix). Configuring per subnet lets a server with multiple disjoint allocations point different subnets at different prefixes. On the common single-/64 case the tunnel answers Neighbor Discovery for each device's address on the VPS network; device IPv6 is carried full-tunnel (`AllowedIPs = ::/0`) so replies return through the tunnel. Devices can make outbound IPv6 connections, and — on a device running a current StartOS (0.4.0-beta.10+) — accept unsolicited inbound connections too, so a service can be hosted over IPv6. The subnets and devices tables show the prefix and each device's computed address. See the IPv6 page in the docs.
- **`subnet … set-ipv6` validates the server can route the prefix.** Because a device with an IPv6 assignment routes all its IPv6 full-tunnel (`AllowedIPs = ::/0`), a prefix delegated on a server without working IPv6 egress just blackholes. The command **hard-errors** (leaving the config unchanged) when the server has no IPv6 default route, and logs an actionable warning when the prefix is neither on-link on a WAN interface nor otherwise verifiable — so operators catch a misconfigured VPS at set-time instead of discovering dead IPv6 on their devices.
- **Port-range forwarding.** A manual port forward can now span a contiguous range of ports. Set "Number of Ports" in the Add Port Forward dialog — or `--count` on `start-tunnel port-forward add` — to forward that many consecutive ports counting up from both the external and internal port. Ranges are plain port forwards and cannot be combined with SNI demux. (Automatic PCP PORT_SET range forwarding requested by connected devices was already supported; this exposes it to manually-added forwards.)

## [1.0.0]

- **Independent versioning.** `start-tunnel` now carries its own version (starting at `1.0.0`) in its `Cargo.toml`, decoupled from the StartOS release line; its `.deb` is versioned from the manifest.

### Changed

- Relocated into the `start-os` monorepo as the `start-tunnel` product. The
  daemon and API now build from the shared `start-core` crate
  (`shared-libs/crates/start-core/src/tunnel/`); the Angular UI is a project in the
  shared `shared-libs/ts-modules` workspace.

## [0.4.0-beta.10]

StartTunnel is a WireGuard virtual private router with kernel-level clearnet port
forwarding, a web UI and `start-tunnel` CLI over one JSON-RPC API, and reactive
on-demand HTTPS. The headline feature this release is gateway autoconfiguration;
StartOS provides the client side — see
[`../start-os/CHANGELOG.md`](../start-os/CHANGELOG.md).

### Added

- **Gateway autoconfiguration — port-control servers (#3306).** A StartOS server behind a StartTunnel can now open its public ports automatically. The tunnel runs:
  - a **PCP server** (RFC 6887, UDP 5351) and **NAT-PMP** responder, plus a **UPnP IGD** fallback (SSDP discovery + device description + SCPD + SOAP), all `SO_BINDTODEVICE`-bound to the WireGuard interface and honoring only configured peers. PCP maps the requesting host, so a peer can only forward to itself; the IGD server enforces the same. Mappings land in the existing `port_forwards` table.
  - **PCP HOSTNAME (SNI demux).** A per-`(extIP, extPort)` hostname binding table fronts a TCP listener that reads the TLS ClientHello, extracts `server_name`, and splices to the bound internal host (exact → wildcard → fallback). The internal leg is opened from the client's real source address via `IP_TRANSPARENT` egress, so the backend sees the true peer IP. PCP `MAP` requests carrying HOSTNAME options register/refresh/delete named bindings instead of creating an nft DNAT.
  - **PCP `PORT_SET` (RFC 7753)** range mappings, capped at `MAX_PORT_SET`.
  - **PCP `ANNOUNCE` capability discovery** + a Start9 capability option, so clients confirm the tunnel speaks the HOSTNAME extension before sending the Private-Use option code.
- **SNI-demuxed forwards are a first-class, persistent forward type.** The port-forwards value is now an enum `PortForward = Dnat | Sni` (mutually exclusive per external address); SNI forwards hold per-hostname routes to backend targets, survive a restart, and appear in the dashboard. An operator can also create SNI routes manually. SNI hostname ownership is keyed to the target backend (a different target is rejected; the PCP layer forces `target = peer`).
- **Per-subnet DNS proxy.** Each WireGuard subnet picks a DNS mode — **default** (the VPS's own resolvers), **device** (a selected device on the subnet), or **custom** (up to 3 servers) — and clients receive `DNS = <subnet .1>`. A forward-only hickory proxy binds per subnet to the in-tunnel address (`10.59.x.1:53`), reachable only over WireGuard, and forwards to the configured upstreams.
- **RFC 2136 DNS injection from trusted devices (#3306).** A device with gateway autoconfiguration enabled may push `A`/`AAAA`/`CNAME`/`TXT` records to its subnet's DNS proxy via DNS UPDATE; the proxy answers them authoritatively and persists them to `db.dns_records`. UPDATEs are authenticated with **TSIG** (RFC 8945, HMAC-SHA256) keyed off a per-device key derived from the device's WireGuard PSK — fail-closed on any error. A DNS records page (view/add/delete) and the per-device toggle are exposed in the portal and via `start-cli tunnel dns …` / `tunnel device set-dns-injection`.
- **Per-subnet and per-device WAN-IP assignment.** A public WAN IP can be assigned per WireGuard subnet (`WgSubnetConfig.wan_ip`) with an optional per-device override (`WgConfig.wan_ip`), unifying the external-IP model so PCP MAP / UPnP `GetExternalIPAddress` report the right public IP. Egress SNATs each subnet to its assigned WAN (per-device `/32` overrides win; unassigned stays masquerade), reconciled by stable comment tags and serialized against concurrent reconciles. New `set-subnet-wan` / `set-device-wan` RPCs, surfaced in the subnet/device Add/Edit dialogs with WAN columns.
- **Device Client/Server kind.** `WgConfig.kind` (`Client`/`Server`) is stored and sticky; a Server defaults its autoconfiguration flags on, a Client off. The portal splits Devices into **Servers** and **Clients** tables (kind inferred from which Add button opened the dialog), and `set-device-kind` promotes/demotes.
- **Gateway-autoconfiguration toggle** (per device) gates both DNS injection and PCP/UPnP port forwarding, backed by two independent flags (`allow_dns_injection`, `allow_auto_port_forward`); an untrusted device can neither inject DNS nor request forwards.
- **Forwarded-protocols column** in the port-forwards table, surfacing that each forward exposes both TCP and UDP (fab7722a4).
- **Portal restructure.** Tables are wrapped in titled cards (mirroring the StartOS interfaces UI); port-forwards and DNS split into **Manual** and **Automatic** tables driven by an explicit `auto` flag; the WAN-IP selector renders context-specific default labels.

### Changed

- **Firewall rules migrated from iptables to native nftables (5b9cf7313).** The tunnel's WireGuard forward-accept, TCP MSS clamp, and per-subnet masquerade now emit nft rules.
- **A port forward's external IP is fixed to the target device's WAN.** A forward's inbound IP must equal the device's egress WAN or return traffic leaves the wrong interface (asymmetric routing breaks the forward), so the external IP is derived from the target device rather than chosen freely, and forwards are re-keyed when a device's or subnet's WAN assignment changes.
- **Admin portal upgraded** to Angular 22 / TypeScript 6 / Taiga UI 5.11 (08de8e1e8).
- **DNS proxy migrated to hickory 0.26** (f9f8ee8e1), clearing DNS RUSTSEC advisories.
- Auto-created (PCP/UPnP) forwards get a consistent `PCP` label.

### Fixed

- **Forwarded TCP MSS is clamped to the WireGuard path MTU (#3262, #3261).** A `mangle FORWARD … TCPMSS --clamp-mss-to-pmtu` rule fixes hung TLS handshakes where a large ClientHello (e.g. desktop Firefox/Chromium with the X25519MLKEM768 post-quantum key share) split into a segment that exceeded the tunnel's effective path MTU after WireGuard encapsulation and was silently dropped.
- **"Default" DNS mode falls back to `/etc/resolv.conf`** when systemd-resolved is absent, unreadable, or yields no usable servers (84e99f1e9).
- **Gateway type is auto-detected from the StartTunnel marker** when a pasted config declares no type, so outbound-only configs (e.g. Mullvad) are no longer mislabeled as inbound-capable (f76ac4bfe).
- **DNS injector serves `NODATA` for held names** instead of forwarding a missing-type query upstream (which returned `NXDOMAIN` for the private TLD and, per RFC 8020, poisoned the held record).
- **UPnP IGD control calls are bounded by a timeout**, and the SNI-divert nft rule is installed on the tunnel host itself, fixing SNI-demuxed forwards that previously timed out because the reply path was never marked.

### Security

- **DNS injection is TSIG-authenticated** (see _Added_), closing a forgery vector where any service emitting from the server's tunnel IP could forge DNS injections into the gateway's resolver.
- **Forward/DNS requests are honored only from devices that opted into gateway autoconfiguration** — the forward-authorization gate (`is_known_client`) and the DNS-injection authorizer each check their own per-device flag.

[Unreleased]: https://github.com/Start9Labs/start-technologies/compare/v0.4.0-beta.10...HEAD
[0.4.0-beta.10]: https://github.com/Start9Labs/start-technologies/releases/tag/v0.4.0-beta.10
