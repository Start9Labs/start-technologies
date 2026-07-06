//! Best-effort automatic port mapping on a public address's upstream gateway.
//!
//! Tries PCP (RFC 6887), then NAT-PMP, then UPnP IGD — one code path for a home
//! router and a StartTunnel gateway (PCP over WireGuard, see
//! [`crate::tunnel::forward::pcp`]). PCP/NAT-PMP via `crab_nat`, UPnP via
//! [`crate::net::port_map::upnp`].
//!
//! All best-effort: failures are logged, never surfaced to the nftables forward
//! reconcile, so a gateway with none of these just falls back to a manual
//! forward. `ensure`/`remove` are fire-and-forget sends so a slow or absent
//! gateway never blocks the forward path.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::num::NonZeroU16;
use std::time::Duration;

use crab_nat::{InternetProtocol, PortMapping, PortMappingOptions, TimeoutConfig, pcp};
use igd_next::aio::Gateway;
use igd_next::aio::tokio::Tokio;
use ipnet::IpNet;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Instant, interval, timeout};

use crate::db::model::public::{GatewayType, NetworkInterfaceInfo};
use crate::net::port_map::pcp::capability::has_start9_capability;
use crate::net::port_map::pcp::hostname::OPTION_HOSTNAME;
use crate::net::port_map::pcp::portset::{OPTION_PORT_SET, PortSet};
use crate::net::port_map::server::PCP_PORT;
use crate::net::port_map::upnp;
use crate::net::utils::ipv6_is_link_local;
use crate::prelude::*;

/// Re-assert/renew every desired mapping on this cadence (well under the PCP
/// lease, and enough to recover a UPnP mapping lost to a gateway reboot).
const REFRESH_INTERVAL: Duration = Duration::from_secs(180);
/// Retry floor for a desired-but-not-active mapping, so a reconcile burst can't
/// busy-loop a failing apply yet boot/tunnel-restart races still recover in
/// seconds rather than waiting for the 180s refresh.
const RETRY_INTERVAL: Duration = Duration::from_secs(15);
const GATEWAY_CACHE_TTL: Duration = Duration::from_secs(600);
const PCP_LIFETIME_SECONDS: u32 = 3600;
/// Fail fast onto UPnP instead of the crate's multi-minute RFC backoff when a
/// gateway doesn't speak PCP/NAT-PMP.
const PCP_TIMEOUTS: TimeoutConfig = TimeoutConfig {
    initial_timeout: Duration::from_millis(250),
    max_retries: 1,
    max_retry_timeout: Some(Duration::from_secs(1)),
};

/// (local IP, external port, optional SNI hostname). Hostname is part of the
/// identity: many hostnames share one external port via gateway SNI demux, each
/// an independent mapping, so adding/removing one never tears down the others.
type MappingKey = (IpAddr, u16, Option<String>);

/// Candidate PCP/NAT-PMP servers for a gateway interface: the NM default
/// gateways (router) that fall on one of this interface's own subnets, plus the
/// v6 link-local default gateway. A StartTunnel gateway is on-link (routed by
/// AllowedIPs, no next-hop), so NM reports no gateway — fall back per family to
/// the tunnel server's address, the subnet's first host, where its PCP server
/// listens.
pub fn candidate_gateways(info: &NetworkInterfaceInfo) -> Vec<(IpAddr, Option<u32>)> {
    fn push(out: &mut Vec<(IpAddr, Option<u32>)>, ip: IpAddr, scope_id: Option<u32>) {
        let bad = match ip {
            IpAddr::V4(v4) => v4.is_unspecified() || v4.is_loopback() || v4.is_broadcast(),
            IpAddr::V6(v6) => v6.is_unspecified() || v6.is_loopback(),
        };
        if !bad && !out.iter().any(|(g, _)| *g == ip) {
            out.push((ip, scope_id));
        }
    }

    let mut out: Vec<(IpAddr, Option<u32>)> = Vec::new();
    let Some(ip_info) = &info.ip_info else {
        return out;
    };

    for ip in &ip_info.lan_ip {
        // v4: the gateway must sit within one of our own subnets. v6: accept the
        // link-local default gateway (fe80::, the common case) or one in our
        // subnets.
        match ip {
            IpAddr::V4(_) => {
                if ip_info.subnets.iter().any(|s| s.contains(ip)) {
                    push(&mut out, *ip, None);
                }
            }
            IpAddr::V6(v6) => {
                // A StartTunnel peer owns no link-local on the wg link (WireGuard
                // interfaces carry none), so an NM-reported fe80:: gateway there
                // can never answer PCP — skip it so the subnet-derived server
                // address below fills the v6 slot instead.
                let ll_ok = ipv6_is_link_local(*v6)
                    && info.gateway_type != Some(GatewayType::InboundOutbound);
                if ll_ok || ip_info.subnets.iter().any(|s| s.contains(ip)) {
                    push(&mut out, *ip, Some(ip_info.scope_id));
                }
            }
        }
    }

    // StartTunnel fallback: the tunnel is on-link (routed by AllowedIPs, no
    // next-hop) so NM reports no gateway and `lan_ip` is empty. The server's PCP
    // listener is at the subnet's first host — `.1` for v4, and the v6 that host
    // takes under the delegated prefix, which the client now carries on its own
    // /prefix v6 so `host_v6` is exact. Fill a family only when NM gave none, so
    // a real gateway always wins.
    if info.gateway_type == Some(GatewayType::InboundOutbound) {
        let have_v4 = out.iter().any(|(g, _)| g.is_ipv4());
        let have_v6 = out.iter().any(|(g, _)| g.is_ipv6());
        let server_v4 = ip_info.subnets.iter().find_map(|s| match s {
            IpNet::V4(n) => n.hosts().next(),
            IpNet::V6(_) => None,
        });
        if let Some(server_v4) = server_v4 {
            if !have_v4 {
                push(&mut out, IpAddr::V4(server_v4), None);
            }
            // Skip a bare /128 (a legacy config with no prefix): `host_v6` would
            // resolve to the client's own address, not the server's.
            if !have_v6 {
                if let Some(prefix) = ip_info.subnets.iter().find_map(|s| match s {
                    IpNet::V6(n) if n.prefix_len() < 128 => Some(*n),
                    _ => None,
                }) {
                    let server_v6 = crate::tunnel::wg6::host_v6(prefix, server_v4);
                    push(&mut out, IpAddr::V6(server_v6), Some(ip_info.scope_id));
                }
            }
        }
    }

    out
}

#[derive(Clone)]
struct Spec {
    internal_port: u16,
    gateways: Vec<(IpAddr, Option<u32>)>,
    /// Contiguous ports to map via PCP PORT_SET (RFC 7753); `1` is single-port.
    /// `> 1` is PCP-only and skipped where the gateway won't grant the full
    /// range (UPnP/NAT-PMP can't map ranges). Always `1` for HOSTNAME mappings.
    count: u16,
}

enum Active {
    Pcp(PortMapping),
    Upnp { external_ip: Option<Ipv4Addr> },
}

enum Command {
    Ensure {
        key: MappingKey,
        spec: Spec,
    },
    Remove {
        key: MappingKey,
    },
    /// Gateway-assigned external IP for an active mapping on
    /// `(local_ip, external_port)`, to confirm reachability without a remote
    /// echo. `None` if not mapped or the external IP is unknown.
    ExternalIp {
        local_ip: IpAddr,
        external_port: u16,
        resp: oneshot::Sender<Option<IpAddr>>,
    },
}

#[derive(Clone)]
pub struct PortMapController {
    req: mpsc::UnboundedSender<Command>,
}

impl PortMapController {
    pub fn new() -> Self {
        let (req, mut recv) = mpsc::unbounded_channel::<Command>();
        // Detached: `tokio::spawn` won't abort on drop; loop exits when all
        // senders are gone.
        tokio::spawn(async move {
            let mut state = State::default();
            let mut refresh = interval(REFRESH_INTERVAL);
            refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    cmd = recv.recv() => match cmd {
                        Some(Command::Ensure { key, spec }) => state.ensure(key, spec).await,
                        Some(Command::Remove { key }) => state.remove(key).await,
                        Some(Command::ExternalIp { local_ip, external_port, resp }) => {
                            let ip = state
                                .active
                                .iter()
                                .find(|(k, _)| k.0 == local_ip && k.1 == external_port)
                                .and_then(|(_, a)| match a {
                                    Active::Pcp(m) => m.external_ip(),
                                    Active::Upnp { external_ip } => external_ip.map(IpAddr::V4),
                                });
                            let _ = resp.send(ip);
                        }
                        None => break,
                    },
                    _ = refresh.tick() => state.refresh().await,
                }
            }
        });
        Self { req }
    }

    pub fn ensure(
        &self,
        local_ip: IpAddr,
        external_port: u16,
        internal_port: u16,
        gateways: Vec<(IpAddr, Option<u32>)>,
    ) {
        self.send_ensure(local_ip, external_port, internal_port, gateways, None, 1);
    }

    /// Like [`ensure`](Self::ensure) but binds one FQDN via PCP HOSTNAME so the
    /// gateway SNI-demuxes this external port. PCP-only; each hostname is an
    /// independent mapping sharing the port.
    pub fn ensure_hostname(
        &self,
        local_ip: IpAddr,
        external_port: u16,
        internal_port: u16,
        gateways: Vec<(IpAddr, Option<u32>)>,
        hostname: String,
    ) {
        self.send_ensure(local_ip, external_port, internal_port, gateways, Some(hostname), 1);
    }

    /// Map `count` contiguous ports starting at `external_port` via the PCP
    /// PORT_SET option (RFC 7753). PCP-only; skipped on gateways that don't
    /// grant the full range.
    pub fn ensure_range(
        &self,
        local_ip: IpAddr,
        external_port: u16,
        internal_port: u16,
        count: u16,
        gateways: Vec<(IpAddr, Option<u32>)>,
    ) {
        self.send_ensure(local_ip, external_port, internal_port, gateways, None, count);
    }

    fn send_ensure(
        &self,
        local_ip: IpAddr,
        external_port: u16,
        internal_port: u16,
        gateways: Vec<(IpAddr, Option<u32>)>,
        hostname: Option<String>,
        count: u16,
    ) {
        self.req
            .send(Command::Ensure {
                key: (local_ip, external_port, hostname),
                spec: Spec {
                    internal_port,
                    gateways,
                    count,
                },
            })
            .ok();
    }

    pub fn remove(&self, local_ip: IpAddr, external_port: u16) {
        self.req
            .send(Command::Remove {
                key: (local_ip, external_port, None),
            })
            .ok();
    }

    /// Remove the SNI HOSTNAME mapping for `hostname` on
    /// `(local_ip, external_port)`, leaving any other hostnames on that port.
    pub fn remove_hostname(&self, local_ip: IpAddr, external_port: u16, hostname: String) {
        self.req
            .send(Command::Remove {
                key: (local_ip, external_port, Some(hostname)),
            })
            .ok();
    }

    /// Gateway-assigned external IP if a mapping is active for
    /// `(local_ip, external_port)`, else `None`. `Some` means the port was
    /// forwarded automatically, so a remote reachability check can be skipped.
    pub async fn mapped_external_ip(
        &self,
        local_ip: IpAddr,
        external_port: u16,
    ) -> Option<IpAddr> {
        let (resp, rx) = oneshot::channel();
        self.req
            .send(Command::ExternalIp {
                local_ip,
                external_port,
                resp,
            })
            .ok()?;
        rx.await.ok().flatten()
    }
}

impl Default for PortMapController {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
struct State {
    desired: BTreeMap<MappingKey, Spec>,
    active: BTreeMap<MappingKey, Active>,
    upnp_cache: BTreeMap<Ipv4Addr, (Gateway<Tokio>, Instant)>,
    /// Last apply() attempt per key, to rate-limit on-demand retries of
    /// not-yet-active mappings (see RETRY_INTERVAL).
    last_attempt: BTreeMap<MappingKey, Instant>,
    /// Per-gateway PCP ANNOUNCE result ("speaks the Start9 HOSTNAME
    /// extension"); a positive verdict is trusted longer than a negative one.
    hostname_caps: BTreeMap<IpAddr, (bool, Instant)>,
}

impl State {
    async fn ensure(&mut self, key: MappingKey, spec: Spec) {
        let changed = self.desired.get(&key).map_or(true, |s| {
            s.internal_port != spec.internal_port
                || s.gateways != spec.gateways
                || s.count != spec.count
        });
        self.desired.insert(key.clone(), spec);
        // Reapply on a spec change, or to retry a not-yet-active mapping — but
        // rate-limited to RETRY_INTERVAL per key so a reconcile burst can't
        // busy-loop one the gateway can't satisfy.
        let stale = !self.active.contains_key(&key)
            && self
                .last_attempt
                .get(&key)
                .map_or(true, |t| t.elapsed() >= RETRY_INTERVAL);
        if changed || stale {
            self.teardown(key.clone()).await;
            self.apply(key).await;
        }
    }

    async fn remove(&mut self, key: MappingKey) {
        self.desired.remove(&key);
        self.last_attempt.remove(&key);
        self.teardown(key).await;
    }

    async fn refresh(&mut self) {
        for key in self.desired.keys().cloned().collect::<Vec<_>>() {
            match self.active.get_mut(&key) {
                Some(Active::Pcp(m)) => {
                    if let Err(e) = m.renew().await {
                        tracing::debug!("PCP/NAT-PMP renew for {key:?} failed, re-mapping: {e}");
                        self.teardown(key.clone()).await;
                        self.apply(key).await;
                    }
                }
                // UPnP has no lease; re-assert in case a gateway reboot dropped
                // it. `None` retries a prior failure.
                Some(Active::Upnp { .. }) | None => {
                    self.teardown(key.clone()).await;
                    self.apply(key).await;
                }
            }
        }
        self.upnp_cache
            .retain(|_, (_, at)| at.elapsed() < GATEWAY_CACHE_TTL);
        self.hostname_caps.retain(|_, (ok, at)| {
            at.elapsed() < if *ok { GATEWAY_CACHE_TTL } else { RETRY_INTERVAL }
        });
        self.last_attempt.retain(|k, _| self.desired.contains_key(k));
    }

    async fn teardown(&mut self, key: MappingKey) {
        match self.active.remove(&key) {
            Some(Active::Pcp(m)) => {
                if let Err((e, _)) = m.try_drop().await {
                    tracing::debug!("PCP/NAT-PMP unmap for {key:?} failed: {e}");
                }
            }
            Some(Active::Upnp { .. }) => {
                let (local_ip, external_port, _) = key;
                if let IpAddr::V4(local_v4) = local_ip {
                    if let Some(gw) = self.gateway_for(local_v4).await {
                        upnp::remove_port(gw, external_port).await.log_err();
                    }
                }
            }
            None => {}
        }
    }

    async fn apply(&mut self, key: MappingKey) {
        let Some(spec) = self.desired.get(&key).cloned() else {
            return;
        };
        // Stamp before attempting so a permanently-failing mapping is also
        // rate-limited, not just successful ones.
        self.last_attempt.insert(key.clone(), Instant::now());
        let (local_ip, external_port, hostname) = (key.0, key.1, key.2.clone());
        let (Some(ext), Some(intl)) = (
            NonZeroU16::new(external_port),
            NonZeroU16::new(spec.internal_port),
        ) else {
            return;
        };

        // HOSTNAME (SNI-demux) mapping: PCP-only, since NAT-PMP/UPnP can't demux
        // by SNI. Other hostnames on the same port are separate mappings.
        if let Some(hostname) = &hostname {
            let options = [pcp::PcpOption {
                code: OPTION_HOSTNAME,
                data: hostname.as_bytes().to_vec(),
            }];
            for (gw, scope_id) in &spec.gateways {
                if gw.is_ipv4() != local_ip.is_ipv4() {
                    continue;
                }
                // Never hand a Private-Use OPTION_HOSTNAME to a gateway that
                // hasn't confirmed it speaks the extension via ANNOUNCE.
                if !self.gateway_supports_hostname(local_ip, *gw, *scope_id).await {
                    tracing::debug!("PCP HOSTNAME skip {gw}: no ANNOUNCE confirmation of support");
                    continue;
                }
                match pcp::port_mapping(
                    pcp::BaseMapRequest::new(*gw, local_ip, InternetProtocol::Tcp, intl),
                    None,
                    None,
                    PortMappingOptions {
                        external_port: Some(ext),
                        lifetime_seconds: Some(PCP_LIFETIME_SECONDS),
                        timeout_config: Some(PCP_TIMEOUTS),
                        gateway_scope_id: *scope_id,
                    },
                    &options,
                )
                .await
                {
                    // Require the gateway to echo the HOSTNAME option too: it
                    // confirms the binding took, independent of the ANNOUNCE marker.
                    Ok(m)
                        if m.external_port() == ext
                            && m.response_options().iter().any(|o| o.code == OPTION_HOSTNAME) =>
                    {
                        tracing::debug!(
                            "PCP HOSTNAME mapped {external_port}->{local_ip}:{} {hostname} via {gw}",
                            spec.internal_port,
                        );
                        self.active.insert(key, Active::Pcp(m));
                        return;
                    }
                    Ok(m) => {
                        let _ = m.try_drop().await;
                    }
                    Err(e) => tracing::debug!(
                        "PCP HOSTNAME map {local_ip}:{external_port} {hostname} via {gw} failed: {e}"
                    ),
                }
            }
            return;
        }

        // Range mapping via PCP PORT_SET (RFC 7753), PCP-only. A gateway lacking
        // PORT_SET silently maps a single port; detect the missing/short grant
        // and skip rather than forward a partial range.
        if spec.count > 1 {
            let range_size = spec.count;
            let option = pcp::PcpOption {
                code: OPTION_PORT_SET,
                data: PortSet {
                    size: range_size,
                    first_internal_port: spec.internal_port,
                    parity: false,
                }
                .to_payload(),
            };
            for (gw, scope_id) in &spec.gateways {
                if gw.is_ipv4() != local_ip.is_ipv4() {
                    continue;
                }
                match pcp::port_mapping(
                    pcp::BaseMapRequest::new(*gw, local_ip, InternetProtocol::Tcp, intl),
                    None,
                    None,
                    PortMappingOptions {
                        external_port: Some(ext),
                        lifetime_seconds: Some(PCP_LIFETIME_SECONDS),
                        timeout_config: Some(PCP_TIMEOUTS),
                        gateway_scope_id: *scope_id,
                    },
                    std::slice::from_ref(&option),
                )
                .await
                {
                    Ok(m) if m.external_port() == ext => {
                        let granted = m
                            .response_options()
                            .iter()
                            .find(|o| o.code == OPTION_PORT_SET)
                            .and_then(|o| PortSet::from_payload(&o.data))
                            .map_or(1, |ps| ps.size);
                        if granted >= range_size {
                            tracing::debug!(
                                "PCP PORT_SET mapped {external_port}+{range_size}->{local_ip}:{} via {gw}",
                                spec.internal_port
                            );
                            self.active.insert(key, Active::Pcp(m));
                            return;
                        }
                        tracing::debug!(
                            "gateway {gw} granted {granted}/{range_size} PORT_SET ports for {local_ip}:{external_port}; skipping range"
                        );
                        let _ = m.try_drop().await;
                    }
                    Ok(m) => {
                        let _ = m.try_drop().await;
                    }
                    Err(e) => tracing::debug!(
                        "PCP PORT_SET map {local_ip}:{external_port} via {gw} failed: {e}"
                    ),
                }
            }
            return;
        }

        // PCP first, NAT-PMP fallback (crab_nat), against each candidate gateway.
        for (gw, scope_id) in &spec.gateways {
            if gw.is_ipv4() != local_ip.is_ipv4() {
                continue;
            }
            match PortMapping::new(
                *gw,
                local_ip,
                InternetProtocol::Tcp,
                intl,
                PortMappingOptions {
                    external_port: Some(ext),
                    lifetime_seconds: Some(PCP_LIFETIME_SECONDS),
                    timeout_config: Some(PCP_TIMEOUTS),
                    gateway_scope_id: *scope_id,
                },
            )
            .await
            {
                Ok(m) if m.external_port() == ext => {
                    tracing::debug!(
                        "{} mapped {external_port}->{local_ip}:{} via {gw}",
                        m.mapping_type(),
                        spec.internal_port
                    );
                    self.active.insert(key, Active::Pcp(m));
                    return;
                }
                // A different external port is useless for a fixed public port.
                Ok(m) => {
                    let _ = m.try_drop().await;
                }
                Err(e) => {
                    tracing::debug!("PCP/NAT-PMP map {local_ip}:{external_port} via {gw} failed: {e}")
                }
            }
        }

        // Fall back to UPnP (IPv4 only).
        if let IpAddr::V4(local_v4) = local_ip {
            let added = match self.gateway_for(local_v4).await {
                Some(gw) => match upnp::add_port(gw, external_port, local_v4, spec.internal_port).await {
                    Ok(()) => {
                        tracing::debug!("UPnP mapped {external_port}->{local_v4}:{}", spec.internal_port);
                        true
                    }
                    Err(e) => {
                        tracing::debug!("UPnP map {local_v4}:{external_port} failed: {e}");
                        false
                    }
                },
                None => false,
            };
            if added {
                // Best-effort external IP (local IGD query) so a reachability check
                // can short-circuit; `get_external_ipv4` discards private/CGNAT.
                let external_ip = upnp::get_external_ipv4(local_v4).await.ok().flatten();
                self.active.insert(key, Active::Upnp { external_ip });
            } else {
                // Re-discover next time in case the gateway went away.
                self.upnp_cache.remove(&local_v4);
            }
        }
    }

    async fn gateway_for(&mut self, local_ip: Ipv4Addr) -> Option<&Gateway<Tokio>> {
        let fresh = self
            .upnp_cache
            .get(&local_ip)
            .map_or(false, |(_, at)| at.elapsed() < GATEWAY_CACHE_TTL);
        if !fresh {
            match upnp::discover(local_ip).await {
                Ok(g) => {
                    self.upnp_cache.insert(local_ip, (g, Instant::now()));
                }
                Err(e) => {
                    tracing::debug!("no UPnP gateway on {local_ip}: {e}");
                    self.upnp_cache.remove(&local_ip);
                    return None;
                }
            }
        }
        self.upnp_cache.get(&local_ip).map(|(g, _)| g)
    }

    /// Whether `gw` answered a PCP ANNOUNCE with the Start9 capability marker,
    /// cached per gateway (a yes trusted for GATEWAY_CACHE_TTL, a no re-probed
    /// after RETRY_INTERVAL). Gates OPTION_HOSTNAME while we ride a Private-Use
    /// option code.
    async fn gateway_supports_hostname(
        &mut self,
        local_ip: IpAddr,
        gw: IpAddr,
        scope_id: Option<u32>,
    ) -> bool {
        if let Some((ok, at)) = self.hostname_caps.get(&gw) {
            let ttl = if *ok { GATEWAY_CACHE_TTL } else { RETRY_INTERVAL };
            if at.elapsed() < ttl {
                return *ok;
            }
        }
        let ok = probe_announce(local_ip, gw, scope_id).await;
        self.hostname_caps.insert(gw, (ok, Instant::now()));
        ok
    }
}

/// A datagram is a Start9 ANNOUNCE response iff it's a SUCCESS ANNOUNCE reply
/// (version 2, response bit set on opcode 0) carrying the capability marker.
fn announce_marker_ok(resp: &[u8]) -> bool {
    resp.len() >= 24
        && resp[0] == 2
        && resp[1] == 0x80
        && resp[3] == 0
        && has_start9_capability(&resp[24..])
}

/// Send a PCP ANNOUNCE to `gw:5351` and report whether it answers with the
/// Start9 capability marker. Raw UDP since crab_nat exposes no ANNOUNCE;
/// best-effort — any error/timeout/non-marker reply is "not supported".
async fn probe_announce(local_ip: IpAddr, gw: IpAddr, scope_id: Option<u32>) -> bool {
    // Bind the gateway-facing source IP (as the crab_nat MAP path does) so the
    // ANNOUNCE egresses the right interface — e.g. the WireGuard tunnel to a
    // StartTunnel gateway — and the reply routes back to us.
    let Ok(sock) = UdpSocket::bind((local_ip, 0)).await else {
        return false;
    };
    // A link-local (fe80::) gateway needs the interface zone/scope id to connect.
    let dst = match gw {
        IpAddr::V4(v4) => SocketAddr::V4(SocketAddrV4::new(v4, PCP_PORT)),
        IpAddr::V6(v6) => SocketAddr::V6(SocketAddrV6::new(v6, PCP_PORT, 0, scope_id.unwrap_or(0))),
    };
    if sock.connect(dst).await.is_err() {
        return false;
    }
    // Bare 24-byte PCP header: version 2, opcode 0 (ANNOUNCE), client IP.
    let mut req = [0u8; 24];
    req[0] = 2;
    let client_octets = match local_ip {
        IpAddr::V4(v4) => v4.to_ipv6_mapped().octets(),
        IpAddr::V6(v6) => v6.octets(),
    };
    req[8..24].copy_from_slice(&client_octets);
    let mut buf = [0u8; 1100];
    let attempts = PCP_TIMEOUTS.max_retries as u32 + 1;
    for attempt in 0..attempts {
        if sock.send(&req).await.is_err() {
            return false;
        }
        let dur = if attempt == 0 {
            PCP_TIMEOUTS.initial_timeout
        } else {
            PCP_TIMEOUTS
                .max_retry_timeout
                .unwrap_or(Duration::from_secs(1))
        };
        // Retransmit on a lost or garbled reply (RFC 6887 §8.3) rather than
        // giving up on the first delivered-but-non-marker datagram.
        if let Ok(Ok(n)) = timeout(dur, sock.recv(&mut buf)).await {
            if announce_marker_ok(&buf[..n]) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> Spec {
        // No gateways: apply() does no network I/O, so these tests exercise the
        // keying/identity logic only.
        Spec {
            internal_port: 443,
            gateways: Vec::new(),
            count: 1,
        }
    }

    // Distinct hostnames on the same external port are independent mappings;
    // removing one (or adding a plain mapping) never clobbers the others.
    #[tokio::test]
    async fn distinct_hostnames_share_a_port_without_clobbering() {
        let ip: IpAddr = Ipv4Addr::new(10, 59, 0, 2).into();
        let a: MappingKey = (ip, 443, Some("a.example.com".into()));
        let b: MappingKey = (ip, 443, Some("b.example.com".into()));
        let plain: MappingKey = (ip, 443, None);

        let mut state = State::default();
        state.ensure(a.clone(), spec()).await;
        state.ensure(b.clone(), spec()).await;
        assert!(state.desired.contains_key(&a));
        assert!(state.desired.contains_key(&b), "adding b clobbered a's siblings");

        state.ensure(plain.clone(), spec()).await;
        assert_eq!(state.desired.len(), 3, "plain mapping is a distinct identity");

        state.remove(a.clone()).await;
        assert!(!state.desired.contains_key(&a));
        assert!(state.desired.contains_key(&b), "removing a dropped b");
        assert!(state.desired.contains_key(&plain));
    }

    // The client accepts only a SUCCESS ANNOUNCE reply carrying the exact marker.
    #[test]
    fn announce_marker_recognized() {
        use crate::net::port_map::pcp::capability::encode_start9_capability_option;
        let mut resp = vec![0u8; 24];
        resp[0] = 2;
        resp[1] = 0x80; // RESPONSE_BIT | opcode 0 (ANNOUNCE)
        encode_start9_capability_option(&mut resp);
        assert!(announce_marker_ok(&resp));

        let mut not_response = resp.clone();
        not_response[1] = 0x00;
        assert!(!announce_marker_ok(&not_response));

        let mut not_success = resp.clone();
        not_success[3] = 4;
        assert!(!announce_marker_ok(&not_success));

        assert!(!announce_marker_ok(&resp[..24]), "no marker option");
    }

    fn iface(
        subnets: &[&str],
        lan_ip: &[&str],
        gateway_type: Option<GatewayType>,
    ) -> NetworkInterfaceInfo {
        use crate::db::model::public::IpInfo;
        NetworkInterfaceInfo {
            ip_info: Some(std::sync::Arc::new(IpInfo {
                scope_id: 42,
                subnets: subnets.iter().map(|s| s.parse::<IpNet>().unwrap()).collect(),
                lan_ip: lan_ip.iter().map(|s| s.parse::<IpAddr>().unwrap()).collect(),
                ..Default::default()
            })),
            gateway_type,
            ..Default::default()
        }
    }

    // A StartTunnel gateway has no NM gateway (on-link), so we fall back to the
    // subnet's first host per family: v4 `.1` and the v6 that host maps to under
    // the delegated /prefix the client now carries.
    #[test]
    fn tunnel_fallback_derives_server_v4_and_v6() {
        let gws = candidate_gateways(&iface(
            &["10.59.0.2/24", "2001:db8:abcd::a3b:2/64"],
            &[],
            Some(GatewayType::InboundOutbound),
        ));
        assert!(gws.contains(&(Ipv4Addr::new(10, 59, 0, 1).into(), None)));
        let server_v6: IpAddr = "2001:db8:abcd::a3b:1".parse().unwrap();
        assert!(gws.iter().any(|(g, _)| *g == server_v6), "got {gws:?}");
    }

    // On a /124 the client carries its /128 at /124, so `host_v6` stays exact
    // where a naive v4-bit XOR would corrupt the prefix.
    #[test]
    fn tunnel_fallback_v6_exact_on_a_small_prefix() {
        let gws = candidate_gateways(&iface(
            &["10.59.0.2/24", "2001:db8:abcd:1::f2/124"],
            &[],
            Some(GatewayType::InboundOutbound),
        ));
        let server_v6: IpAddr = "2001:db8:abcd:1::f1".parse().unwrap();
        assert!(gws.iter().any(|(g, _)| *g == server_v6), "got {gws:?}");
    }

    // A real NM gateway always wins; the fallback fills only the missing family.
    #[test]
    fn tunnel_fallback_is_per_family() {
        let gws = candidate_gateways(&iface(
            &["10.59.0.2/24", "2001:db8:abcd::a3b:2/64"],
            &["10.59.0.1"], // NM has v4 but no v6
            Some(GatewayType::InboundOutbound),
        ));
        assert_eq!(gws.iter().filter(|(g, _)| g.is_ipv4()).count(), 1);
        let server_v6: IpAddr = "2001:db8:abcd::a3b:1".parse().unwrap();
        assert!(gws.iter().any(|(g, _)| *g == server_v6), "got {gws:?}");
    }

    // The fallback is scoped to StartTunnel; a plain interface with no NM gateway
    // yields nothing (no bogus `.1`).
    #[test]
    fn non_tunnel_no_gateway_yields_nothing() {
        assert!(candidate_gateways(&iface(&["192.168.1.5/24"], &[], None)).is_empty());
    }

    // A legacy /128 client (pre-/prefix config) can't derive the server v6, so
    // the v6 fallback is skipped rather than resolving to the client's own addr.
    #[test]
    fn tunnel_fallback_skips_bare_128_v6() {
        let gws = candidate_gateways(&iface(
            &["10.59.0.2/24", "2001:db8:abcd::a3b:2/128"],
            &[],
            Some(GatewayType::InboundOutbound),
        ));
        assert!(gws.contains(&(Ipv4Addr::new(10, 59, 0, 1).into(), None)));
        assert!(!gws.iter().any(|(g, _)| g.is_ipv6()), "no v6 from a bare /128");
    }

    // NM can report a link-local v6 gateway for the wg connection, but the
    // tunnel server owns no link-local on the wg link — it must be skipped so
    // the subnet-derived server v6 fills the slot (else every v6 map times out).
    #[test]
    fn tunnel_skips_link_local_nm_gateway() {
        let gws = candidate_gateways(&iface(
            &["10.59.0.2/24", "2001:db8:abcd:1::f2/124"],
            &["fe80::a3b:1"],
            Some(GatewayType::InboundOutbound),
        ));
        assert!(!gws.iter().any(|(g, _)| match g {
            IpAddr::V6(v6) => ipv6_is_link_local(*v6),
            _ => false,
        }));
        let server_v6: IpAddr = "2001:db8:abcd:1::f1".parse().unwrap();
        assert!(gws.iter().any(|(g, _)| *g == server_v6), "got {gws:?}");
    }

    // A real router's link-local v6 gateway (the common home case) stays.
    #[test]
    fn router_link_local_gateway_is_kept() {
        let gws = candidate_gateways(&iface(
            &["192.168.1.5/24"],
            &["192.168.1.1", "fe80::1"],
            None,
        ));
        assert!(gws.contains(&("fe80::1".parse().unwrap(), Some(42))));
    }
}
