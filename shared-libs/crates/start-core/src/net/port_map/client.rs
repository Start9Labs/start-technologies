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
//!
//! Work is sharded per local IP (one task per gateway interface), so a gateway
//! that answers slowly or not at all never head-of-line-blocks mapping attempts
//! against another interface's gateway. Two cooperating mechanisms keep a
//! chronically uncooperative gateway from being retried forever:
//!
//! - Per-gateway capability verdicts
//!   ([`GatewayPortMapCapabilities`]) live on the network-interface watcher
//!   (and in the db): protocols the gateway is known not to speak are skipped
//!   here, and every attempt outcome feeds back as fresh evidence.
//! - Per-key exponential backoff: a mapping that keeps failing is retried at
//!   15s doubling to a 16-minute cap, reset on success or a spec change.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};
use std::num::NonZeroU16;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use crab_nat::{
    InternetProtocol, MappingFailure, PortMapping, PortMappingOptions, TimeoutConfig, pcp,
};
use igd_next::aio::Gateway;
use igd_next::aio::tokio::Tokio;
use imbl::OrdMap;
use ipnet::IpNet;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Instant, interval};

use crate::GatewayId;
use crate::db::model::public::{
    CapabilityVerdict, GatewayPortMapCapabilities, GatewayType, NetworkInterfaceInfo,
};
use crate::net::port_map::pcp::hostname::OPTION_HOSTNAME;
use crate::net::port_map::pcp::portset::{OPTION_PORT_SET, PortSet};
use crate::net::port_map::{probe, upnp};
use crate::net::utils::ipv6_is_link_local;
use crate::prelude::*;
use crate::util::collections::OrdMapIterMut;
use crate::util::sync::{SyncMutex, Watch};

/// Cadence for the refresh tick: re-assert UPnP and retry not-yet-active
/// mappings whose backoff has elapsed, and check whether each active PCP
/// mapping has crossed half its lease (the point it's renewed). Well under the
/// PCP lease so a renewal that's come due is caught with ample margin before
/// expiry.
const REFRESH_INTERVAL: Duration = Duration::from_secs(180);
/// Initial retry delay for a desired-but-not-active mapping; doubles per
/// consecutive failure up to [`BACKOFF_MAX`], so boot/tunnel-restart races
/// still recover in seconds while a permanently-failing mapping quiets down.
const RETRY_INTERVAL: Duration = Duration::from_secs(15);
const BACKOFF_MAX: Duration = Duration::from_secs(960);
const GATEWAY_CACHE_TTL: Duration = Duration::from_secs(600);
const PCP_LIFETIME_SECONDS: u32 = 3600;
/// Fail fast onto UPnP instead of the crate's multi-minute RFC backoff when a
/// gateway doesn't speak PCP/NAT-PMP.
const PCP_TIMEOUTS: TimeoutConfig = TimeoutConfig {
    initial_timeout: Duration::from_millis(250),
    max_retries: 1,
    max_retry_timeout: Some(Duration::from_secs(1)),
};

/// Delay before the next apply after `failures` consecutive failures: 15s,
/// 30s, 60s, … capped at 16 minutes.
fn retry_delay(failures: u32) -> Duration {
    (RETRY_INTERVAL * 2u32.pow(failures.saturating_sub(1).min(6))).min(BACKOFF_MAX)
}

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
    // Port mapping is inbound-only: an OutboundOnly gateway (e.g. a commercial
    // VPN) exposes no PCP/NAT-PMP server we'd ever ask for a pinhole. Return no
    // candidates so every port-map call site — this is the one they all funnel
    // through — never attempts PCP against it.
    if info.gateway_type == GatewayType::OutboundOnly {
        return Vec::new();
    }

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
        // The gateway must sit within one of our own subnets. A link-local v6
        // gateway is never a PCP server we can reach — a StartTunnel peer owns
        // none on the wg link, and its own fe80::/64 nominally "contains" any
        // fe80::, so a subnet check can't distinguish it — so skip it. On a
        // tunnel the host_v6-derived server fills the v6 slot below.
        match ip {
            IpAddr::V4(_) => {
                if ip_info.subnets.iter().any(|s| s.contains(ip)) {
                    push(&mut out, *ip, None);
                }
            }
            IpAddr::V6(v6) => {
                if ipv6_is_link_local(*v6) {
                    continue;
                }
                if ip_info.subnets.iter().any(|s| s.contains(ip)) {
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
    if info.gateway_type == GatewayType::InboundOutbound {
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
                    // Derive the server from the routed prefix, never the wg
                    // iface's own fe80::/64 — that would yield a link-local server.
                    IpNet::V6(n) if n.prefix_len() < 128 && !ipv6_is_link_local(n.network()) => {
                        Some(*n)
                    }
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
    /// Gateway-assigned external IP for an active mapping on `external_port`,
    /// to confirm reachability without a remote echo. `None` if not mapped or
    /// the external IP is unknown.
    ExternalIp {
        external_port: u16,
        resp: oneshot::Sender<Option<IpAddr>>,
    },
}

/// Fire-and-forget port-map requests, sharded per local IP so one interface's
/// gateway can never delay another interface's mapping work.
#[derive(Clone)]
pub struct PortMapController {
    interfaces: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    shards: Arc<SyncMutex<BTreeMap<IpAddr, mpsc::UnboundedSender<Command>>>>,
}

impl PortMapController {
    pub fn new(interfaces: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>) -> Self {
        Self {
            interfaces,
            shards: Arc::new(SyncMutex::new(BTreeMap::new())),
        }
    }

    fn shard(&self, local_ip: IpAddr) -> mpsc::UnboundedSender<Command> {
        self.shards.mutate(|shards| {
            shards
                .entry(local_ip)
                .or_insert_with(|| spawn_shard(self.interfaces.clone()))
                .clone()
        })
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
        self.send_ensure(
            local_ip,
            external_port,
            internal_port,
            gateways,
            Some(hostname),
            1,
        );
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
        self.send_ensure(
            local_ip,
            external_port,
            internal_port,
            gateways,
            None,
            count,
        );
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
        self.shard(local_ip)
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
        self.shard(local_ip)
            .send(Command::Remove {
                key: (local_ip, external_port, None),
            })
            .ok();
    }

    /// Remove the SNI HOSTNAME mapping for `hostname` on
    /// `(local_ip, external_port)`, leaving any other hostnames on that port.
    pub fn remove_hostname(&self, local_ip: IpAddr, external_port: u16, hostname: String) {
        self.shard(local_ip)
            .send(Command::Remove {
                key: (local_ip, external_port, Some(hostname)),
            })
            .ok();
    }

    /// Gateway-assigned external IP if a mapping is active for
    /// `(local_ip, external_port)`, else `None`. `Some` means the port was
    /// forwarded automatically, so a remote reachability check can be skipped.
    pub async fn mapped_external_ip(&self, local_ip: IpAddr, external_port: u16) -> Option<IpAddr> {
        let (resp, rx) = oneshot::channel();
        self.shard(local_ip)
            .send(Command::ExternalIp {
                external_port,
                resp,
            })
            .ok()?;
        rx.await.ok().flatten()
    }
}

fn spawn_shard(
    interfaces: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
) -> mpsc::UnboundedSender<Command> {
    let (req, mut recv) = mpsc::unbounded_channel::<Command>();
    // Detached: `tokio::spawn` won't abort on drop; the loop exits when all
    // senders are gone.
    tokio::spawn(async move {
        let mut state = State::default();
        let mut refresh = interval(REFRESH_INTERVAL);
        refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                cmd = recv.recv() => match cmd {
                    Some(Command::Ensure { key, spec }) => state.ensure(&interfaces, key, spec).await,
                    Some(Command::Remove { key }) => state.remove(key).await,
                    Some(Command::ExternalIp { external_port, resp }) => {
                        let ip = state
                            .active
                            .iter()
                            .find(|(k, _)| k.1 == external_port)
                            .and_then(|(_, a)| match a {
                                Active::Pcp(m) => m.external_ip(),
                                Active::Upnp { external_ip } => external_ip.map(IpAddr::V4),
                            });
                        let _ = resp.send(ip);
                    }
                    None => break,
                },
                _ = refresh.tick() => state.refresh(&interfaces).await,
            }
        }
    });
    req
}

/// Capability verdicts for the interface whose candidate list contains `gw`.
fn capabilities_for(
    interfaces: &Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    gw: IpAddr,
) -> Option<GatewayPortMapCapabilities> {
    interfaces
        .read()
        .iter()
        .find(|(_, info)| candidate_gateways(info).iter().any(|(g, _)| *g == gw))
        .map(|(_, info)| info.port_map)
}

/// Capability verdicts for the interface that owns `local` — the granularity
/// UPnP discovery works at (no gateway address involved).
fn capabilities_for_local(
    interfaces: &Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    local: IpAddr,
) -> Option<GatewayPortMapCapabilities> {
    interfaces
        .read()
        .values()
        .find(|info| {
            info.ip_info
                .as_ref()
                .map_or(false, |i| i.subnets.iter().any(|s| s.addr() == local))
        })
        .map(|info| info.port_map)
}

/// Feed an attempt outcome back into the interface's capability state. `update`
/// mutates verdicts and reports whether anything changed, so a fresh identical
/// verdict doesn't churn the watch (and the db sync behind it).
fn report(
    interfaces: &Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    gw: IpAddr,
    update: impl Fn(&mut GatewayPortMapCapabilities, DateTime<Utc>) -> bool,
) {
    let now = Utc::now();
    interfaces.send_if_modified(|m| {
        let mut changed = false;
        for (_, info) in OrdMapIterMut::from(m) {
            if candidate_gateways(info).iter().any(|(g, _)| *g == gw) {
                changed |= update(&mut info.port_map, now);
            }
        }
        changed
    });
}

fn report_local(
    interfaces: &Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    local: IpAddr,
    supported: bool,
) {
    let now = Utc::now();
    interfaces.send_if_modified(|m| {
        let mut changed = false;
        for (_, info) in OrdMapIterMut::from(m) {
            let owns = info
                .ip_info
                .as_ref()
                .map_or(false, |i| i.subnets.iter().any(|s| s.addr() == local));
            if owns {
                changed |= set_verdict(&mut info.port_map.upnp, supported, now);
            }
        }
        changed
    });
}

pub(crate) fn set_verdict(v: &mut CapabilityVerdict, supported: bool, now: DateTime<Utc>) -> bool {
    if v.fresh(now) == Some(supported) {
        false
    } else {
        *v = CapabilityVerdict::supported(supported);
        true
    }
}

/// What a crab_nat failure implies about the gateway: a refusal/timeout means
/// the protocol is dead there; any protocol-level response (even a rejection)
/// means it's spoken.
fn report_crab_nat_failure(
    interfaces: &Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    gw: IpAddr,
    failure: &MappingFailure,
) {
    use crab_nat::{natpmp, pcp};
    let refused = |e: &std::io::Error| e.kind() == std::io::ErrorKind::ConnectionRefused;
    report(interfaces, gw, |caps, now| match failure {
        // A socket refusal or silence on the PCP attempt: nothing on 5351.
        MappingFailure::Pcp(pcp::Failure::Socket(e)) if refused(e) => {
            set_verdict(&mut caps.pcp, false, now)
        }
        MappingFailure::Pcp(pcp::Failure::Timeout) => set_verdict(&mut caps.pcp, false, now),
        // Any other PCP failure is a protocol-level answer — PCP is spoken.
        MappingFailure::Pcp(_) => set_verdict(&mut caps.pcp, true, now),
        // crab_nat only attempts NAT-PMP after PCP answers UNSUPP_VERSION, so a
        // NAT-PMP-level failure also settles PCP (unsupported) either way.
        MappingFailure::NatPmp(natpmp::Failure::Socket(e)) if refused(e) => {
            set_verdict(&mut caps.pcp, false, now) | set_verdict(&mut caps.nat_pmp, false, now)
        }
        MappingFailure::NatPmp(natpmp::Failure::Timeout) => {
            set_verdict(&mut caps.pcp, false, now) | set_verdict(&mut caps.nat_pmp, false, now)
        }
        MappingFailure::NatPmp(_) => {
            set_verdict(&mut caps.pcp, false, now) | set_verdict(&mut caps.nat_pmp, true, now)
        }
    });
}

fn report_pcp_failure(
    interfaces: &Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    gw: IpAddr,
    failure: &pcp::Failure,
) {
    let refused = |e: &std::io::Error| e.kind() == std::io::ErrorKind::ConnectionRefused;
    report(interfaces, gw, |caps, now| match failure {
        pcp::Failure::Socket(e) if refused(e) => set_verdict(&mut caps.pcp, false, now),
        pcp::Failure::Timeout => set_verdict(&mut caps.pcp, false, now),
        _ => set_verdict(&mut caps.pcp, true, now),
    });
}

#[derive(Default)]
struct State {
    desired: BTreeMap<MappingKey, Spec>,
    active: BTreeMap<MappingKey, Active>,
    upnp_cache: BTreeMap<Ipv4Addr, (Gateway<Tokio>, Instant)>,
    /// Consecutive apply failures per key and when the latest attempt ran —
    /// drives the exponential backoff between retries.
    failures: BTreeMap<MappingKey, (u32, Instant)>,
}

impl State {
    async fn ensure(
        &mut self,
        interfaces: &Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
        key: MappingKey,
        spec: Spec,
    ) {
        let changed = self.desired.get(&key).map_or(true, |s| {
            s.internal_port != spec.internal_port
                || s.gateways != spec.gateways
                || s.count != spec.count
        });
        self.desired.insert(key.clone(), spec);
        // A spec change is new information (operator or config) — retry
        // promptly, ignoring any accumulated backoff.
        if changed {
            self.failures.remove(&key);
        }
        if changed || (!self.active.contains_key(&key) && self.backoff_elapsed(&key)) {
            self.teardown(key.clone()).await;
            self.apply(interfaces, key).await;
        }
    }

    async fn remove(&mut self, key: MappingKey) {
        self.desired.remove(&key);
        self.failures.remove(&key);
        self.teardown(key).await;
    }

    fn backoff_elapsed(&self, key: &MappingKey) -> bool {
        self.failures
            .get(key)
            .map_or(true, |(n, at)| at.elapsed() >= retry_delay(*n))
    }

    async fn refresh(&mut self, interfaces: &Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>) {
        for key in self.desired.keys().cloned().collect::<Vec<_>>() {
            match self.active.get_mut(&key) {
                // expiration()/lifetime() reflect the gateway's last grant
                // (crab_nat uses std::time::Instant), so renewal self-corrects if
                // the gateway caps the lease below what we asked for, and the
                // ticks before it's due are skipped.
                Some(Active::Pcp(m))
                    if renew_due(std::time::Instant::now(), m.expiration(), m.lifetime()) =>
                {
                    if let Err(e) = m.renew().await {
                        crate::dev_log!(
                            debug,
                            "PCP/NAT-PMP renew for {key:?} failed, re-mapping: {e}"
                        );
                        self.teardown(key.clone()).await;
                        self.apply(interfaces, key).await;
                    }
                }
                // A PCP mapping not yet at its renewal point: leave it be.
                Some(Active::Pcp(_)) => {}
                // UPnP has no lease; re-assert in case a gateway reboot dropped
                // it.
                Some(Active::Upnp { .. }) => {
                    self.teardown(key.clone()).await;
                    self.apply(interfaces, key).await;
                }
                // A prior failure: retry once its backoff has elapsed.
                None => {
                    if self.backoff_elapsed(&key) {
                        self.apply(interfaces, key).await;
                    }
                }
            }
        }
        self.upnp_cache
            .retain(|_, (_, at)| at.elapsed() < GATEWAY_CACHE_TTL);
        self.failures.retain(|k, _| self.desired.contains_key(k));
    }

    async fn teardown(&mut self, key: MappingKey) {
        match self.active.remove(&key) {
            Some(Active::Pcp(m)) => {
                if let Err((e, _)) = m.try_drop().await {
                    crate::dev_log!(debug, "PCP/NAT-PMP unmap for {key:?} failed: {e}");
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

    /// Wrapper around the attempt paths: on success any backoff is cleared;
    /// after a real (network) attempt that left the key inactive the failure
    /// count grows. An attempt fully short-circuited by capability verdicts
    /// does no I/O and counts neither as success nor failure.
    async fn apply(
        &mut self,
        interfaces: &Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
        key: MappingKey,
    ) {
        let attempted = self.try_apply(interfaces, &key).await;
        if self.active.contains_key(&key) {
            self.failures.remove(&key);
        } else if attempted {
            let (n, _) = self
                .failures
                .get(&key)
                .copied()
                .unwrap_or((0, Instant::now()));
            self.failures.insert(key, (n + 1, Instant::now()));
        }
    }

    /// Returns `true` if any network I/O was attempted.
    async fn try_apply(
        &mut self,
        interfaces: &Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
        key: &MappingKey,
    ) -> bool {
        let mut attempted = false;
        let Some(spec) = self.desired.get(key).cloned() else {
            return false;
        };
        let (local_ip, external_port, hostname) = (key.0, key.1, key.2.clone());
        let (Some(ext), Some(intl)) = (
            NonZeroU16::new(external_port),
            NonZeroU16::new(spec.internal_port),
        ) else {
            return false;
        };
        let now = Utc::now();

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
                // Skip a gateway whose HOSTNAME support is fresh-known-absent;
                // probe only when the verdict is unknown or stale, and feed the
                // result back to the interface's capability state.
                let caps = capabilities_for(interfaces, *gw);
                match caps.and_then(|c| c.pcp_hostname.fresh(now)) {
                    Some(false) => {
                        crate::dev_log!(
                            debug,
                            "PCP HOSTNAME skip {gw}: known not to support the HOSTNAME extension"
                        );
                        continue;
                    }
                    Some(true) => {}
                    None => {
                        let probe = probe::probe_gateway(local_ip, *gw, *scope_id).await;
                        report(interfaces, *gw, |caps, now| {
                            set_verdict(&mut caps.pcp, probe.pcp, now)
                                | set_verdict(&mut caps.pcp_hostname, probe.pcp_hostname, now)
                                | set_verdict(&mut caps.nat_pmp, probe.nat_pmp, now)
                        });
                        if !probe.pcp_hostname {
                            crate::dev_log!(
                                debug,
                                "PCP HOSTNAME skip {gw}: no ANNOUNCE confirmation of support"
                            );
                            continue;
                        }
                    }
                }
                attempted = true;
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
                            && m.response_options()
                                .iter()
                                .any(|o| o.code == OPTION_HOSTNAME) =>
                    {
                        tracing::debug!(
                            "PCP HOSTNAME mapped {external_port}->{local_ip}:{} {hostname} via {gw}",
                            spec.internal_port,
                        );
                        self.active.insert(key.clone(), Active::Pcp(m));
                        return true;
                    }
                    // Answered but didn't echo HOSTNAME: doesn't honor it.
                    Ok(m) => {
                        report(interfaces, *gw, |caps, now| {
                            set_verdict(&mut caps.pcp, true, now)
                                | set_verdict(&mut caps.pcp_hostname, false, now)
                        });
                        let _ = m.try_drop().await;
                    }
                    Err(e) => {
                        report_pcp_failure(interfaces, *gw, &e);
                        crate::dev_log!(
                            debug,
                            "PCP HOSTNAME map {local_ip}:{external_port} {hostname} via {gw} failed: {e}"
                        )
                    }
                }
            }
            return attempted;
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
                // PORT_SET is PCP-only — a live NAT-PMP verdict can't save it.
                if capabilities_for(interfaces, *gw).and_then(|c| c.pcp.fresh(now)) == Some(false) {
                    crate::dev_log!(debug, "PCP PORT_SET skip {gw}: known not to support PCP");
                    continue;
                }
                attempted = true;
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
                            self.active.insert(key.clone(), Active::Pcp(m));
                            return true;
                        }
                        crate::dev_log!(
                            debug,
                            "gateway {gw} granted {granted}/{range_size} PORT_SET ports for {local_ip}:{external_port}; skipping range"
                        );
                        let _ = m.try_drop().await;
                    }
                    Ok(m) => {
                        let _ = m.try_drop().await;
                    }
                    Err(e) => {
                        report_pcp_failure(interfaces, *gw, &e);
                        crate::dev_log!(
                            debug,
                            "PCP PORT_SET map {local_ip}:{external_port} via {gw} failed: {e}"
                        )
                    }
                }
            }
            return attempted;
        }

        // PCP first, NAT-PMP fallback (crab_nat), against each candidate gateway.
        for (gw, scope_id) in &spec.gateways {
            if gw.is_ipv4() != local_ip.is_ipv4() {
                continue;
            }
            if pcp_fresh_dead(interfaces, *gw, now) {
                crate::dev_log!(
                    debug,
                    "PCP/NAT-PMP skip {gw}: known not to support port mapping"
                );
                continue;
            }
            attempted = true;
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
                        spec.internal_port,
                    );
                    let nat_pmp = matches!(m.mapping_type(), crab_nat::PortMappingType::NatPmp);
                    report(interfaces, *gw, |caps, now| {
                        set_verdict(&mut caps.pcp, !nat_pmp, now)
                            | set_verdict(&mut caps.nat_pmp, nat_pmp, now)
                    });
                    self.active.insert(key.clone(), Active::Pcp(m));
                    return true;
                }
                // A different external port is useless for a fixed public port.
                Ok(m) => {
                    let _ = m.try_drop().await;
                }
                Err(e) => {
                    report_crab_nat_failure(interfaces, *gw, &e);
                    crate::dev_log!(
                        debug,
                        "PCP/NAT-PMP map {local_ip}:{external_port} via {gw} failed: {e}"
                    )
                }
            }
        }

        // Fall back to UPnP (IPv4 only), unless the interface's gateway is
        // fresh-known to have no IGD.
        if let IpAddr::V4(local_v4) = local_ip {
            let upnp_dead = capabilities_for_local(interfaces, local_ip)
                .and_then(|c| c.upnp.fresh(now))
                == Some(false);
            if upnp_dead {
                crate::dev_log!(debug, "UPnP skip on {local_ip}: known to have no IGD");
                return attempted;
            }
            attempted = true;
            let added = match self.gateway_for(local_v4).await {
                Some(gw) => {
                    // Discovery alone proves the IGD, whatever the SOAP call says.
                    report_local(interfaces, local_ip, true);
                    match upnp::add_port(gw, external_port, local_v4, spec.internal_port).await {
                        Ok(()) => {
                            tracing::debug!(
                                "UPnP mapped {external_port}->{local_v4}:{}",
                                spec.internal_port
                            );
                            true
                        }
                        Err(e) => {
                            crate::dev_log!(
                                debug,
                                "UPnP map {local_v4}:{external_port} failed: {e}"
                            );
                            false
                        }
                    }
                }
                None => {
                    report_local(interfaces, local_ip, false);
                    false
                }
            };
            if added {
                // Best-effort external IP (local IGD query) so a reachability check
                // can short-circuit; `get_external_ipv4` discards private/CGNAT.
                let external_ip = upnp::get_external_ipv4(local_v4).await.ok().flatten();
                self.active
                    .insert(key.clone(), Active::Upnp { external_ip });
            } else {
                // Re-discover next time in case the gateway went away.
                self.upnp_cache.remove(&local_v4);
            }
        }
        attempted
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
                    crate::dev_log!(debug, "no UPnP gateway on {local_ip}: {e}");
                    self.upnp_cache.remove(&local_ip);
                    return None;
                }
            }
        }
        self.upnp_cache.get(&local_ip).map(|(g, _)| g)
    }
}

/// PCP is fresh-known-dead on this gateway — and when NAT-PMP is too, any
/// crab_nat attempt is a guaranteed failure, so skip it.
fn pcp_fresh_dead(
    interfaces: &Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    gw: IpAddr,
    now: DateTime<Utc>,
) -> bool {
    capabilities_for(interfaces, gw).map_or(false, |c| {
        c.pcp.fresh(now) == Some(false) && c.nat_pmp.fresh(now) == Some(false)
    })
}

/// Whether a PCP mapping granted `lifetime` seconds and expiring at `expiration`
/// is due for renewal at `now` — RFC 6887 §11.2.1: renew once half the granted
/// lifetime has elapsed (i.e. remaining lifetime has dropped to ≤ half), well
/// before expiry. Saturates so a tiny grant or an already-lapsed mapping renews
/// immediately rather than underflowing the `Instant`.
fn renew_due(now: std::time::Instant, expiration: std::time::Instant, lifetime: u32) -> bool {
    let half = Duration::from_secs(u64::from(lifetime) / 2);
    now >= expiration.checked_sub(half).unwrap_or(now)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> Spec {
        // No gateways: try_apply() does no network I/O, so these tests exercise
        // the keying/identity logic only.
        Spec {
            internal_port: 443,
            gateways: Vec::new(),
            count: 1,
        }
    }

    fn interfaces() -> Watch<OrdMap<GatewayId, NetworkInterfaceInfo>> {
        Watch::new(OrdMap::new())
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
        state.ensure(&interfaces(), a.clone(), spec()).await;
        state.ensure(&interfaces(), b.clone(), spec()).await;
        assert!(state.desired.contains_key(&a));
        assert!(
            state.desired.contains_key(&b),
            "adding b clobbered a's siblings"
        );

        state.ensure(&interfaces(), plain.clone(), spec()).await;
        assert_eq!(
            state.desired.len(),
            3,
            "plain mapping is a distinct identity"
        );

        state.remove(a.clone()).await;
        assert!(!state.desired.contains_key(&a));
        assert!(state.desired.contains_key(&b), "removing a dropped b");
        assert!(state.desired.contains_key(&plain));
    }

    // Renewal fires at half the granted lifetime, not before — so a healthy
    // mapping renews with ~half its lease still to spare, well ahead of the
    // gateway's reap.
    #[test]
    fn renew_due_at_half_life() {
        let now = std::time::Instant::now();
        let lt = 3600; // half-life = 1800s
        let due = |remaining: u64| renew_due(now, now + Duration::from_secs(remaining), lt);
        assert!(!due(3600), "fresh grant: not due");
        assert!(!due(1801), "just before half-life: not due");
        assert!(due(1800), "at half-life: due");
        assert!(due(1), "near expiry: due");
        assert!(renew_due(now, now - Duration::from_secs(1), lt));
        assert!(renew_due(now, now, 0));
    }

    // Backoff schedule: 15s doubling per consecutive failure, capped.
    #[test]
    fn retry_delay_doubles_and_caps() {
        assert_eq!(retry_delay(0), Duration::from_secs(15));
        assert_eq!(retry_delay(1), Duration::from_secs(15));
        assert_eq!(retry_delay(2), Duration::from_secs(30));
        assert_eq!(retry_delay(3), Duration::from_secs(60));
        assert_eq!(retry_delay(7), BACKOFF_MAX);
        assert_eq!(retry_delay(100), BACKOFF_MAX);
    }

    // A fresh "not supported" verdict short-circuits the apply before any
    // network I/O, and counts as neither success nor failure.
    #[tokio::test]
    async fn dead_gateway_verdict_skips_attempt_without_backoff() {
        let gw: IpAddr = Ipv4Addr::new(192, 168, 8, 1).into();
        let local: IpAddr = Ipv4Addr::new(192, 168, 8, 101).into();
        let ifaces = Watch::new(OrdMap::from_iter([(
            GatewayId::from(imbl_value::InternedString::intern("eno0")),
            NetworkInterfaceInfo {
                port_map: GatewayPortMapCapabilities {
                    pcp: CapabilityVerdict::supported(false),
                    nat_pmp: CapabilityVerdict::supported(false),
                    upnp: CapabilityVerdict::supported(false),
                    ..Default::default()
                },
                ..iface(
                    &["192.168.8.101/24"],
                    &["192.168.8.1"],
                    GatewayType::InboundOutbound,
                )
            },
        )]));
        let key: MappingKey = (local, 443, None);
        let mut state = State::default();
        state
            .ensure(
                &ifaces,
                key.clone(),
                Spec {
                    internal_port: 443,
                    gateways: vec![(gw, None)],
                    count: 1,
                },
            )
            .await;
        assert!(state.desired.contains_key(&key));
        assert!(!state.active.contains_key(&key), "no mapping should exist");
        assert!(
            !state.failures.contains_key(&key),
            "a verdict-skipped apply must not grow the backoff"
        );
    }

    // A changed spec clears accumulated backoff so an operator's change is
    // retried promptly. (v6 local IP: no gateways in the spec and no UPnP
    // fallback for v6, so the apply does no network I/O.)
    #[tokio::test]
    async fn spec_change_resets_backoff() {
        let ip: IpAddr = "fd00:59::2".parse().unwrap();
        let key: MappingKey = (ip, 443, None);
        let mut state = State::default();
        state.failures.insert(key.clone(), (5, Instant::now()));
        assert!(!state.backoff_elapsed(&key));
        state.ensure(&interfaces(), key.clone(), spec()).await;
        assert!(!state.failures.contains_key(&key));
        assert!(state.backoff_elapsed(&key));
    }

    fn iface(subnets: &[&str], lan_ip: &[&str], gateway_type: GatewayType) -> NetworkInterfaceInfo {
        use crate::db::model::public::IpInfo;
        NetworkInterfaceInfo {
            ip_info: Some(std::sync::Arc::new(IpInfo {
                scope_id: 42,
                subnets: subnets
                    .iter()
                    .map(|s| s.parse::<IpNet>().unwrap())
                    .collect(),
                lan_ip: lan_ip
                    .iter()
                    .map(|s| s.parse::<IpAddr>().unwrap())
                    .collect(),
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
            GatewayType::InboundOutbound,
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
            GatewayType::InboundOutbound,
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
            GatewayType::InboundOutbound,
        ));
        assert_eq!(gws.iter().filter(|(g, _)| g.is_ipv4()).count(), 1);
        let server_v6: IpAddr = "2001:db8:abcd::a3b:1".parse().unwrap();
        assert!(gws.iter().any(|(g, _)| *g == server_v6), "got {gws:?}");
    }

    // Gateway type is a two-state default of inbound-outbound, so the
    // subnet-derived `.1` fallback now applies to any inbound-outbound gateway
    // with no NM gateway — not only explicit StartTunnel ones.
    #[test]
    fn inbound_outbound_no_nm_gateway_derives_first_host() {
        let gws = candidate_gateways(&iface(
            &["192.168.1.5/24"],
            &[],
            GatewayType::InboundOutbound,
        ));
        assert!(gws.contains(&(Ipv4Addr::new(192, 168, 1, 1).into(), None)));
    }

    // A legacy /128 client (pre-/prefix config) can't derive the server v6, so
    // the v6 fallback is skipped rather than resolving to the client's own addr.
    #[test]
    fn tunnel_fallback_skips_bare_128_v6() {
        let gws = candidate_gateways(&iface(
            &["10.59.0.2/24", "2001:db8:abcd::a3b:2/128"],
            &[],
            GatewayType::InboundOutbound,
        ));
        assert!(gws.contains(&(Ipv4Addr::new(10, 59, 0, 1).into(), None)));
        assert!(
            !gws.iter().any(|(g, _)| g.is_ipv6()),
            "no v6 from a bare /128"
        );
    }

    // NM can report a link-local v6 gateway for the wg connection, but the
    // tunnel server owns no link-local on the wg link — it must be skipped so
    // the subnet-derived server v6 fills the slot (else every v6 map times out).
    #[test]
    fn tunnel_skips_link_local_nm_gateway() {
        let gws = candidate_gateways(&iface(
            &["10.59.0.2/24", "2001:db8:abcd:1::f2/124"],
            &["fe80::a3b:1"],
            GatewayType::InboundOutbound,
        ));
        assert!(!gws.iter().any(|(g, _)| match g {
            IpAddr::V6(v6) => ipv6_is_link_local(*v6),
            _ => false,
        }));
        let server_v6: IpAddr = "2001:db8:abcd:1::f1".parse().unwrap();
        assert!(gws.iter().any(|(g, _)| *g == server_v6), "got {gws:?}");
    }

    // A link-local v6 gateway is never a reachable PCP server, so it's skipped
    // regardless of gateway_type — a home router still keeps its v4 gateway.
    #[test]
    fn link_local_v6_gateway_is_always_skipped() {
        let gws = candidate_gateways(&iface(
            &["192.168.1.5/24"],
            &["192.168.1.1", "fe80::1"],
            GatewayType::InboundOutbound,
        ));
        assert!(
            !gws.iter()
                .any(|(g, _)| matches!(g, IpAddr::V6(v6) if ipv6_is_link_local(*v6))),
            "link-local v6 gateway must be skipped: {gws:?}"
        );
        assert!(gws.contains(&(Ipv4Addr::new(192, 168, 1, 1).into(), None)));
    }

    // Regression for the live-box timeout: the wg iface carries its own
    // fe80::/64, so `subnets.contains(fe80::gw)` is true (every link-local shares
    // that /64) and re-admitted the NM link-local gateway past #3417's guard —
    // the gateway the tunnel server can't answer on. It must be rejected so the
    // host_v6-derived server (`.1` of the routed prefix) fills the v6 slot.
    #[test]
    fn tunnel_rejects_link_local_gateway_even_when_a_subnet_contains_it() {
        let gws = candidate_gateways(&iface(
            &[
                "10.59.0.2/24",
                "2604:a880:4:1d0::a3b:2/64",
                "fe80::1234:5678:9abc:def0/64",
            ],
            &["fe80::a3b:1"],
            GatewayType::InboundOutbound,
        ));
        assert!(
            !gws.iter().any(|(g, _)| match g {
                IpAddr::V6(v6) => ipv6_is_link_local(*v6),
                _ => false,
            }),
            "link-local gateway survived despite the fe80::/64 subnet: {gws:?}"
        );
        let server_v6: IpAddr = "2604:a880:4:1d0::a3b:1".parse().unwrap();
        assert!(
            gws.iter().any(|(g, _)| *g == server_v6),
            "expected host_v6-derived server, got {gws:?}"
        );
        assert!(gws.contains(&(Ipv4Addr::new(10, 59, 0, 1).into(), None)));
    }

    // Port mapping is inbound-only: an OutboundOnly gateway is never a PCP target,
    // so it yields no candidates regardless of what NM reports.
    #[test]
    fn outbound_only_gateway_has_no_candidates() {
        let gws = candidate_gateways(&iface(
            &["10.8.0.2/24", "2001:db8::2/64"],
            &["10.8.0.1", "fe80::1"],
            GatewayType::OutboundOnly,
        ));
        assert!(
            gws.is_empty(),
            "OutboundOnly must yield no candidates, got {gws:?}"
        );
    }
}
