//! Best-effort automatic port mapping on a public address's upstream gateway.
//!
//! Tries PCP (RFC 6887), then NAT-PMP, then UPnP IGD — one code path for a home
//! router and a StartTunnel gateway (PCP over WireGuard, see
//! [`crate::tunnel::forward::pcp`]). PCP/NAT-PMP via `crab_nat`, UPnP via
//! [`crate::net::port_map::upnp`].
//!
//! A drain atomically and permanently closes admission before awaiting shard teardown.
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

use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv4Addr};
use std::num::NonZeroU16;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use crab_nat::{
    InternetProtocol, MappingFailure, PortMapping, PortMappingOptions, TimeoutConfig, pcp,
};
use futures::FutureExt;
use futures::future::{BoxFuture, Shared, join_all};
use igd_next::PortMappingProtocol;
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
use crate::util::future::NonDetachingJoinHandle;
use crate::util::sync::{SyncMutex, Watch};

/// Refresh cadence for active mappings and backoff-eligible retries.
const REFRESH_INTERVAL: Duration = Duration::from_secs(180);
/// Initial retry delay; doubles to [`BACKOFF_MAX`].
const RETRY_INTERVAL: Duration = Duration::from_secs(15);
const BACKOFF_MAX: Duration = Duration::from_secs(960);
const GATEWAY_CACHE_TTL: Duration = Duration::from_secs(600);
const PCP_LIFETIME_SECONDS: u32 = 3600;
const DRAIN_TIMEOUT: Duration = Duration::from_secs(90);
/// Short probe timeout before falling back to UPnP.
const PCP_TIMEOUTS: TimeoutConfig = TimeoutConfig {
    initial_timeout: Duration::from_millis(250),
    max_retries: 1,
    max_retry_timeout: Some(Duration::from_secs(1)),
};

fn retry_delay(failures: u32) -> Duration {
    (RETRY_INTERVAL * 2u32.pow(failures.saturating_sub(1).min(6))).min(BACKOFF_MAX)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum TransportProtocol {
    Tcp,
    Udp,
}

impl TransportProtocol {
    fn internet(self) -> InternetProtocol {
        match self {
            Self::Tcp => InternetProtocol::Tcp,
            Self::Udp => InternetProtocol::Udp,
        }
    }

    fn upnp(self) -> PortMappingProtocol {
        match self {
            Self::Tcp => PortMappingProtocol::TCP,
            Self::Udp => PortMappingProtocol::UDP,
        }
    }
}

/// Mapping identity, including independent hostname and transport bindings.
type MappingKey = (IpAddr, u16, Option<String>, TransportProtocol);

/// Candidate PCP/NAT-PMP servers for a gateway interface: the NM default
/// gateways (router) that fall on one of this interface's own subnets, plus the
/// v6 link-local default gateway. A StartTunnel gateway is on-link (routed by
/// AllowedIPs, no next-hop), so NM reports no gateway — fall back per family to
/// the tunnel server's address, the subnet's first host, where its PCP server
/// listens.
pub fn candidate_gateways(info: &NetworkInterfaceInfo) -> Vec<(IpAddr, Option<u32>)> {
    // Outbound-only gateways cannot accept inbound mappings.
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
        // StartTunnel derives its IPv6 server from the routed prefix, not fe80::/64.
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

    // StartTunnel has no next-hop; derive its server from each routed prefix.
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
            // A bare /128 identifies the client rather than the server.
            if !have_v6 {
                if let Some(prefix) = ip_info.subnets.iter().find_map(|s| match s {
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
    Upnp {
        external_ip: Option<Ipv4Addr>,
        /// Needed by owner-scoped hostname deletion.
        internal_port: u16,
        gateway: Gateway<Tokio>,
    },
}

enum Command {
    Ensure {
        key: MappingKey,
        spec: Spec,
    },
    Remove {
        key: MappingKey,
    },
    /// Gateway-assigned external IP for an active TCP mapping on
    /// `external_port`, to confirm TCP reachability without a remote echo.
    /// `None` if not mapped or the external IP is unknown.
    ExternalIp {
        external_port: u16,
        resp: oneshot::Sender<Option<IpAddr>>,
    },
}

struct DrainRequest {
    deadline: Instant,
}

struct Shard {
    commands: mpsc::UnboundedSender<Command>,
    drain: mpsc::UnboundedSender<DrainRequest>,
    task: NonDetachingJoinHandle<Result<(), Error>>,
}

type DrainFuture = Shared<BoxFuture<'static, Result<(), Arc<Error>>>>;

enum ControllerState {
    Accepting(BTreeMap<IpAddr, Shard>),
    Draining(DrainFuture),
}

/// Fire-and-forget port-map requests, sharded per local IP so one interface's
/// gateway can never delay another interface's mapping work.
#[derive(Clone)]
pub struct PortMapController {
    interfaces: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    state: Arc<SyncMutex<ControllerState>>,
    footprints: Arc<SyncMutex<BTreeMap<MappingKey, Footprint>>>,
}

impl PortMapController {
    pub fn new(interfaces: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>) -> Self {
        Self {
            interfaces,
            state: Arc::new(SyncMutex::new(ControllerState::Accepting(BTreeMap::new()))),
            footprints: Default::default(),
        }
    }

    fn send(&self, local_ip: IpAddr, command: Command) -> bool {
        self.state.mutate(|state| match state {
            ControllerState::Accepting(shards) => shards
                .entry(local_ip)
                .or_insert_with(|| spawn_shard(self.interfaces.clone(), self.footprints.clone()))
                .commands
                .send(command)
                .is_ok(),
            ControllerState::Draining(_) => false,
        })
    }

    pub fn ensure(
        &self,
        local_ip: IpAddr,
        external_port: u16,
        internal_port: u16,
        gateways: Vec<(IpAddr, Option<u32>)>,
    ) {
        for protocol in [TransportProtocol::Tcp, TransportProtocol::Udp] {
            self.send_ensure(
                local_ip,
                external_port,
                internal_port,
                gateways.clone(),
                None,
                1,
                protocol,
            );
        }
    }

    /// Binds one FQDN through the gateway's hostname mapping extension.
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
            TransportProtocol::Tcp,
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
        for protocol in [TransportProtocol::Tcp, TransportProtocol::Udp] {
            self.send_ensure(
                local_ip,
                external_port,
                internal_port,
                gateways.clone(),
                None,
                count,
                protocol,
            );
        }
    }

    fn send_ensure(
        &self,
        local_ip: IpAddr,
        external_port: u16,
        internal_port: u16,
        gateways: Vec<(IpAddr, Option<u32>)>,
        hostname: Option<String>,
        count: u16,
        protocol: TransportProtocol,
    ) {
        self.send(
            local_ip,
            Command::Ensure {
                key: (local_ip, external_port, hostname, protocol),
                spec: Spec {
                    internal_port,
                    gateways,
                    count,
                },
            },
        );
    }

    pub fn remove(&self, local_ip: IpAddr, external_port: u16) {
        for protocol in [TransportProtocol::Tcp, TransportProtocol::Udp] {
            self.send(
                local_ip,
                Command::Remove {
                    key: (local_ip, external_port, None, protocol),
                },
            );
        }
    }

    /// Remove the SNI HOSTNAME mapping for `hostname` on
    /// `(local_ip, external_port)`, leaving any other hostnames on that port.
    pub fn remove_hostname(&self, local_ip: IpAddr, external_port: u16, hostname: String) {
        self.send(
            local_ip,
            Command::Remove {
                key: (
                    local_ip,
                    external_port,
                    Some(hostname),
                    TransportProtocol::Tcp,
                ),
            },
        );
    }

    pub(crate) async fn drain(&self) -> Result<(), Error> {
        self.drain_until(Instant::now() + DRAIN_TIMEOUT).await
    }

    pub(crate) async fn drain_until(&self, deadline: Instant) -> Result<(), Error> {
        let completion = self.state.mutate(|state| match state {
            ControllerState::Accepting(shards) => {
                let drains = std::mem::take(shards)
                    .into_iter()
                    .map(|(local_ip, shard)| drain_shard(local_ip, shard, deadline))
                    .collect::<Vec<_>>();
                let completion = tokio::spawn(async move {
                    let mut first_error = None;
                    for result in join_all(drains).await {
                        if let Err(error) = result {
                            first_error.get_or_insert(error);
                        }
                    }
                    first_error.map_or(Ok(()), Err)
                })
                .map(|result| {
                    result
                        .map_err(|error| {
                            Error::new(
                                eyre!("port-map drain task panicked: {error}"),
                                ErrorKind::Unknown,
                            )
                        })
                        .and_then(|result| result)
                        .map_err(Arc::new)
                })
                .boxed()
                .shared();
                *state = ControllerState::Draining(completion.clone());
                completion
            }
            ControllerState::Draining(completion) => completion.clone(),
        });
        completion.await.map_err(|error| error.clone_output())
    }

    /// Gateway-assigned external IP if a TCP mapping is active for
    /// `(local_ip, external_port)`, else `None`. `Some` means the TCP port was
    /// forwarded automatically, so a remote reachability check can be skipped.
    /// A v4 address the gateway reports from outside publicly routable space
    /// yields `None`: the gateway is itself behind a NAT, and only a probe can
    /// say whether anything reaches it.
    pub async fn mapped_external_ip(&self, local_ip: IpAddr, external_port: u16) -> Option<IpAddr> {
        let (resp, rx) = oneshot::channel();
        self.send(
            local_ip,
            Command::ExternalIp {
                external_port,
                resp,
            },
        )
        .then_some(())?;
        rx.await.ok().flatten()
    }
}

/// Gateway-reported external address of the active TCP mapping on
/// `external_port`, kept only where the public Internet can reach it.
fn external_ip_of(
    desired: &BTreeMap<MappingKey, Spec>,
    active: &BTreeMap<MappingKey, Active>,
    stale: &BTreeSet<MappingKey>,
    external_port: u16,
) -> Option<IpAddr> {
    active
        .iter()
        .find(|(key, _)| {
            key.1 == external_port
                && key.3 == TransportProtocol::Tcp
                && desired.contains_key(*key)
                && !stale.contains(*key)
        })
        .and_then(|(_, a)| {
            routable_external_ip(match a {
                Active::Pcp(m) => m.external_ip(),
                Active::Upnp { external_ip, .. } => external_ip.map(IpAddr::V4),
            })
        })
}

/// Discards a v4 address outside publicly routable space: the gateway is itself
/// behind a NAT, and only a probe can say whether anything reaches it. A v6
/// mapping is on the host's own address and is kept as reported.
fn routable_external_ip(reported: Option<IpAddr>) -> Option<IpAddr> {
    reported.filter(|ip| match ip {
        IpAddr::V4(v4) => upnp::is_wan_candidate(*v4),
        IpAddr::V6(_) => true,
    })
}

fn controller_exited() -> Error {
    Error::new(eyre!("port-map controller exited"), ErrorKind::Network)
}

async fn drain_shard(local_ip: IpAddr, mut shard: Shard, deadline: Instant) -> Result<(), Error> {
    if Instant::now() >= deadline {
        shard.task.abort();
        let _ = (&mut shard.task).await;
        return Err(Error::new(
            eyre!("port-map cleanup deadline expired"),
            ErrorKind::Network,
        ));
    }
    let requested = shard.drain.send(DrainRequest { deadline }).is_ok();
    match tokio::time::timeout_at(deadline, &mut shard.task).await {
        Ok(result) => {
            result.map_err(|_| controller_exited())??;
            if requested {
                Ok(())
            } else {
                Err(controller_exited())
            }
        }
        Err(_) => {
            shard.task.abort();
            let _ = (&mut shard.task).await;
            Err(Error::new(
                eyre!("port-map cleanup for {local_ip} incomplete at shutdown deadline"),
                ErrorKind::Network,
            ))
        }
    }
}

fn spawn_shard(
    interfaces: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    footprints: Arc<SyncMutex<BTreeMap<MappingKey, Footprint>>>,
) -> Shard {
    let (commands, recv) = mpsc::unbounded_channel();
    let (drain, drain_recv) = mpsc::unbounded_channel();
    let state = State {
        footprints,
        ..Default::default()
    };
    let task = tokio::spawn(run_shard(interfaces, state, recv, drain_recv)).into();
    Shard {
        commands,
        drain,
        task,
    }
}

async fn wait_for_retry(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

async fn drain_to_completion(state: &mut State, deadline: Instant) -> Result<(), Error> {
    let mut failures: u32 = 0;
    loop {
        if Instant::now() >= deadline {
            return Err(Error::new(
                eyre!("port-map cleanup deadline expired"),
                ErrorKind::Network,
            ));
        }
        match state.drain_until(deadline).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                failures = failures.saturating_add(1);
                let delay = retry_delay(failures);
                tracing::warn!(
                    "port-map cleanup failed on attempt {failures}; retrying in {delay:?}: {error}"
                );
                tokio::time::sleep_until((Instant::now() + delay).min(deadline)).await;
            }
        }
    }
}

async fn run_shard(
    interfaces: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    mut state: State,
    mut recv: mpsc::UnboundedReceiver<Command>,
    mut drain_recv: mpsc::UnboundedReceiver<DrainRequest>,
) -> Result<(), Error> {
    let mut refresh = interval(REFRESH_INTERVAL);
    refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    refresh.tick().await;
    loop {
        let removal_retry = state.removal_retry_deadline();
        tokio::select! {
            biased;
            request = drain_recv.recv() => {
                recv.close();
                while recv.try_recv().is_ok() {}
                let deadline = request.map_or_else(|| Instant::now() + DRAIN_TIMEOUT, |r| r.deadline);
                return drain_to_completion(&mut state, deadline).await;
            },
            _ = wait_for_retry(removal_retry) => state.retry_removals().await,
            cmd = recv.recv() => match cmd {
                Some(Command::Ensure { key, spec }) => state.ensure(&interfaces, key, spec).await,
                Some(Command::Remove { key }) => { state.remove(key).await.log_err(); }
                Some(Command::ExternalIp { external_port, resp }) => {
                    let _ = resp.send(external_ip_of(
                        &state.desired, &state.active, &state.stale, external_port,
                    ));
                }
                None => return drain_to_completion(&mut state, Instant::now() + DRAIN_TIMEOUT).await,
            },
            _ = refresh.tick() => state.refresh(&interfaces).await,
        }
    }
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

#[derive(Clone)]
struct Footprint {
    gateway: (IpAddr, Option<u32>),
    address: Option<IpAddr>,
    protocol: TransportProtocol,
    hostname: Option<String>,
    start: u16,
    count: u16,
}

impl Footprint {
    fn gateway(ip: IpAddr, scope: Option<u32>) -> (IpAddr, Option<u32>) {
        (
            ip,
            match ip {
                IpAddr::V6(ip) if ipv6_is_link_local(ip) => scope,
                _ => None,
            },
        )
    }

    fn requested(key: &MappingKey, spec: &Spec, gateway: (IpAddr, Option<u32>)) -> Self {
        Self {
            gateway: Self::gateway(gateway.0, gateway.1),
            address: key.0.is_ipv6().then_some(key.0),
            protocol: key.3,
            hostname: key.2.as_ref().map(|name| name.to_ascii_lowercase()),
            start: key.1,
            count: spec.count.max(1),
        }
    }

    fn conflicts(&self, other: &Self) -> bool {
        self.gateway == other.gateway
            && self.address == other.address
            && self.protocol == other.protocol
            && (self.hostname.is_none()
                || other.hostname.is_none()
                || self.hostname == other.hostname)
            && u32::from(self.start) < u32::from(other.start) + u32::from(other.count)
            && u32::from(other.start) < u32::from(self.start) + u32::from(self.count)
    }

    fn granted(key: &MappingKey, mapping: &PortMapping) -> Self {
        Self {
            gateway: Self::gateway(mapping.gateway(), mapping.gateway_scope_id()),
            address: key
                .0
                .is_ipv6()
                .then_some(mapping.external_ip().unwrap_or(key.0)),
            protocol: key.3,
            hostname: mapping
                .response_options()
                .iter()
                .find(|o| o.code == OPTION_HOSTNAME)
                .and_then(|o| String::from_utf8(o.data.clone()).ok())
                .map(|name| name.to_ascii_lowercase()),
            start: mapping.external_port().get(),
            count: mapping
                .response_options()
                .iter()
                .find(|o| o.code == OPTION_PORT_SET)
                .and_then(|o| PortSet::from_payload(&o.data))
                .map_or(1, |ps| ps.size.max(1)),
        }
    }
}

#[derive(Default)]
struct State {
    desired: BTreeMap<MappingKey, Spec>,
    active: BTreeMap<MappingKey, Active>,
    upnp_cache: BTreeMap<Ipv4Addr, (Gateway<Tokio>, Instant)>,
    failures: BTreeMap<MappingKey, (u32, Instant)>,
    stale: BTreeSet<MappingKey>,
    footprints: Arc<SyncMutex<BTreeMap<MappingKey, Footprint>>>,
}

impl State {
    fn reserve(&self, key: &MappingKey, footprint: Footprint) -> bool {
        self.footprints.mutate(|owned| {
            if owned
                .iter()
                .any(|(owner, old)| owner != key && old.conflicts(&footprint))
            {
                return false;
            }
            owned.insert(key.clone(), footprint);
            true
        })
    }

    fn release(&self, key: &MappingKey) {
        self.footprints.mutate(|owned| {
            owned.remove(key);
        });
    }

    fn retain_pcp(&mut self, key: &MappingKey, mapping: PortMapping) {
        self.footprints.mutate(|owned| {
            owned.insert(key.clone(), Footprint::granted(key, &mapping));
        });
        self.active.insert(key.clone(), Active::Pcp(mapping));
    }

    async fn reject_pcp(&mut self, key: &MappingKey, mapping: PortMapping) -> bool {
        self.retain_pcp(key, mapping);
        self.stale.insert(key.clone());
        if let Err(error) = self.teardown(key.clone()).await {
            Err::<(), _>(error).log_err();
            true
        } else {
            self.stale.remove(key);
            false
        }
    }

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
        // A changed request bypasses accumulated backoff.
        if changed {
            self.failures.remove(&key);
        }
        let replace = changed || self.stale.contains(&key);
        if replace && (changed || self.backoff_elapsed(&key)) {
            self.replace_mapping(interfaces, key).await;
        } else if !self.active.contains_key(&key) && self.backoff_elapsed(&key) {
            self.apply(interfaces, key).await;
        }
    }

    async fn remove(&mut self, key: MappingKey) -> Result<(), Error> {
        self.desired.remove(&key);
        self.stale.remove(&key);
        let result = self.teardown(key.clone()).await;
        self.finish_removal_attempt(key, &result);
        result
    }

    fn finish_removal_attempt(&mut self, key: MappingKey, result: &Result<(), Error>) {
        if result.is_ok() {
            self.failures.remove(&key);
        } else {
            self.record_failure(key);
        }
    }

    fn removal_retry_deadline(&self) -> Option<Instant> {
        self.active
            .keys()
            .filter(|key| !self.desired.contains_key(*key))
            .filter_map(|key| {
                self.failures
                    .get(key)
                    .map(|(failures, at)| *at + retry_delay(*failures))
            })
            .min()
    }

    async fn retry_removals(&mut self) {
        let keys = self
            .active
            .keys()
            .filter(|key| !self.desired.contains_key(*key) && self.backoff_elapsed(key))
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            let result = self.teardown(key.clone()).await;
            self.finish_removal_attempt(key, &result);
            result.log_err();
        }
    }

    #[cfg(test)]
    async fn drain(&mut self) -> Result<(), Error> {
        self.drain_until(Instant::now() + DRAIN_TIMEOUT).await
    }

    async fn drain_until(&mut self, deadline: Instant) -> Result<(), Error> {
        self.desired.clear();
        self.failures.clear();
        self.stale.clear();
        let keys = self.active.keys().cloned().collect::<Vec<_>>();
        let mut first_error = None;
        for key in keys {
            if Instant::now() >= deadline {
                return Err(Error::new(
                    eyre!("port-map cleanup deadline expired"),
                    ErrorKind::Network,
                ));
            }
            if let Err(error) = self.teardown(key).await {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn backoff_elapsed(&self, key: &MappingKey) -> bool {
        self.failures
            .get(key)
            .map_or(true, |(n, at)| at.elapsed() >= retry_delay(*n))
    }

    async fn refresh(&mut self, interfaces: &Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>) {
        for key in self.desired.keys().cloned().collect::<Vec<_>>() {
            let retry_ready = self.backoff_elapsed(&key);
            if self.stale.contains(&key) {
                if retry_ready {
                    self.replace_mapping(interfaces, key).await;
                }
                continue;
            }
            match self.active.get_mut(&key) {
                // Renew against the lifetime granted by the gateway.
                Some(Active::Pcp(m))
                    if renew_due(std::time::Instant::now(), m.expiration(), m.lifetime()) =>
                {
                    match m.renew().await {
                        Ok(()) => {
                            let granted = Footprint::granted(&key, m);
                            let spec = &self.desired[&key];
                            let accepted = granted.start == key.1
                                && granted.count >= spec.count
                                && granted.hostname
                                    == key.2.as_ref().map(|name| name.to_ascii_lowercase());
                            self.footprints.mutate(|owned| {
                                owned.insert(key.clone(), granted);
                            });
                            if !accepted {
                                self.stale.insert(key.clone());
                                self.replace_mapping(interfaces, key).await;
                            }
                        }
                        Err(e) => {
                            crate::dev_log!(debug, "PCP/NAT-PMP renew for {key:?} failed: {e}");
                            self.replace_mapping(interfaces, key).await;
                        }
                    }
                }
                Some(Active::Pcp(_)) => {}
                Some(Active::Upnp {
                    gateway,
                    internal_port,
                    ..
                }) if key.2.is_some() && retry_ready => {
                    let IpAddr::V4(local_ip) = key.0 else {
                        continue;
                    };
                    match upnp::add_hostname_mapping(
                        gateway,
                        key.1,
                        local_ip,
                        *internal_port,
                        key.2.as_deref().unwrap(),
                    )
                    .await
                    {
                        Ok(()) => {
                            self.failures.remove(&key);
                        }
                        Err(error) => {
                            self.stale.insert(key.clone());
                            self.record_failure(key);
                            Err::<(), _>(error).log_err();
                        }
                    }
                }
                Some(Active::Upnp { .. }) if key.2.is_some() => {}
                Some(Active::Upnp { .. }) => {
                    self.replace_mapping(interfaces, key).await;
                }
                None => {
                    if self.backoff_elapsed(&key) {
                        self.apply(interfaces, key).await;
                    }
                }
            }
        }
        self.retry_removals().await;
        self.upnp_cache
            .retain(|_, (_, at)| at.elapsed() < GATEWAY_CACHE_TTL);
        self.failures
            .retain(|k, _| self.desired.contains_key(k) || self.active.contains_key(k));
        self.stale.retain(|key| self.desired.contains_key(key));
    }

    async fn teardown(&mut self, key: MappingKey) -> Result<(), Error> {
        let result = match self.active.get(&key) {
            Some(Active::Pcp(mapping)) => mapping.clone().try_drop().await.map_err(|(error, _)| {
                Error::new(
                    eyre!("PCP/NAT-PMP unmap for {key:?} failed: {error}"),
                    ErrorKind::Network,
                )
            }),
            Some(Active::Upnp {
                internal_port,
                gateway,
                ..
            }) => match &key.2 {
                Some(hostname) => {
                    upnp::remove_hostname_mapping(gateway, key.1, *internal_port, hostname).await
                }
                None => upnp::remove_port(gateway, key.3.upnp(), key.1).await,
            },
            None => Ok(()),
        };
        if result.is_ok() {
            self.active.remove(&key);
            self.release(&key);
        }
        result
    }

    async fn replace_mapping(
        &mut self,
        interfaces: &Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
        key: MappingKey,
    ) {
        let result = self.teardown(key.clone()).await;
        if result.is_ok() {
            self.stale.remove(&key);
            self.apply(interfaces, key).await;
        } else {
            self.stale.insert(key.clone());
            self.record_failure(key);
        }
        result.log_err();
    }

    fn record_failure(&mut self, key: MappingKey) {
        let (failures, _) = self
            .failures
            .get(&key)
            .copied()
            .unwrap_or((0, Instant::now()));
        self.failures
            .insert(key, (failures.saturating_add(1), Instant::now()));
    }

    /// Updates retry backoff after a mapping attempt.
    async fn apply(
        &mut self,
        interfaces: &Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
        key: MappingKey,
    ) {
        let attempted = self.try_apply(interfaces, &key).await;
        if self.active.contains_key(&key) && !self.stale.contains(&key) {
            self.failures.remove(&key);
        } else if attempted {
            self.record_failure(key);
        }
    }

    /// Whether a failed request needs retry backoff.
    async fn try_apply(
        &mut self,
        interfaces: &Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
        key: &MappingKey,
    ) -> bool {
        let mut attempted = false;
        let Some(spec) = self.desired.get(key).cloned() else {
            return false;
        };
        let (local_ip, external_port, hostname, protocol) = (key.0, key.1, key.2.clone(), key.3);
        let (Some(ext), Some(intl)) = (
            NonZeroU16::new(external_port),
            NonZeroU16::new(spec.internal_port),
        ) else {
            return false;
        };
        let now = Utc::now();

        // Try PCP HOSTNAME before the UPnP vendor action.
        if let Some(hostname) = &hostname {
            let options = [pcp::PcpOption {
                code: OPTION_HOSTNAME,
                data: hostname.as_bytes().to_vec(),
            }];
            for (gw, scope_id) in &spec.gateways {
                if gw.is_ipv4() != local_ip.is_ipv4() {
                    continue;
                }
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
                if !self.reserve(key, Footprint::requested(key, &spec, (*gw, *scope_id))) {
                    return true;
                }
                attempted = true;
                match pcp::port_mapping(
                    pcp::BaseMapRequest::new(*gw, local_ip, protocol.internet(), intl),
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
                    // The echoed option confirms the gateway applied HOSTNAME.
                    Ok(m)
                        if m.external_port() == ext
                            && m.response_options().iter().any(|o| {
                                o.code == OPTION_HOSTNAME
                                    && o.data.eq_ignore_ascii_case(hostname.as_bytes())
                            }) =>
                    {
                        tracing::debug!(
                            "PCP HOSTNAME mapped {external_port}->{local_ip}:{} {hostname} via {gw}",
                            spec.internal_port,
                        );
                        self.retain_pcp(key, m);
                        return true;
                    }
                    Ok(m) => {
                        report(interfaces, *gw, |caps, now| {
                            set_verdict(&mut caps.pcp, true, now)
                                | set_verdict(&mut caps.pcp_hostname, false, now)
                        });
                        if self.reject_pcp(key, m).await {
                            return true;
                        }
                    }
                    Err(e) => {
                        self.release(key);
                        report_pcp_failure(interfaces, *gw, &e);
                        crate::dev_log!(
                            debug,
                            "PCP HOSTNAME map {local_ip}:{external_port} {hostname} via {gw} failed: {e}"
                        )
                    }
                }
            }

            if let IpAddr::V4(local_v4) = local_ip {
                let upnp_dead = capabilities_for_local(interfaces, local_ip)
                    .and_then(|c| c.upnp.fresh(now))
                    == Some(false);
                if upnp_dead {
                    crate::dev_log!(
                        debug,
                        "UPnP HOSTNAME skip on {local_ip}: known to have no IGD"
                    );
                    return attempted;
                }
                attempted = true;
                let (gateway, invalidate_upnp_cache) = match self
                    .gateway_for(local_v4)
                    .await
                    .cloned()
                {
                    Some(gateway) => {
                        report_local(interfaces, local_ip, true);
                        if !upnp::supports_hostname(&gateway) {
                            crate::dev_log!(
                                debug,
                                "UPnP HOSTNAME skip on {local_ip}: IGD doesn't advertise the vendor action"
                            );
                            (None, false)
                        } else {
                            if !self.reserve(
                                key,
                                Footprint::requested(key, &spec, (gateway.addr.ip(), None)),
                            ) {
                                return true;
                            }
                            match upnp::add_hostname_mapping(
                                &gateway,
                                external_port,
                                local_v4,
                                spec.internal_port,
                                hostname,
                            )
                            .await
                            {
                                Ok(()) => {
                                    tracing::debug!(
                                        "UPnP HOSTNAME mapped {external_port}->{local_v4}:{} {hostname}",
                                        spec.internal_port
                                    );
                                    (Some(gateway), false)
                                }
                                Err(e) => {
                                    crate::dev_log!(
                                        debug,
                                        "UPnP HOSTNAME map {local_v4}:{external_port} {hostname} failed: {e}"
                                    );
                                    self.release(key);
                                    (None, true)
                                }
                            }
                        }
                    }
                    None => {
                        report_local(interfaces, local_ip, false);
                        (None, true)
                    }
                };
                if let Some(gateway) = gateway {
                    let external_ip = None;
                    self.active.insert(
                        key.clone(),
                        Active::Upnp {
                            external_ip,
                            internal_port: spec.internal_port,
                            gateway,
                        },
                    );
                    if let Some(Active::Upnp {
                        gateway,
                        external_ip,
                        ..
                    }) = self.active.get_mut(key)
                    {
                        *external_ip = upnp::external_ipv4(gateway).await.ok().flatten();
                    }
                } else if invalidate_upnp_cache {
                    self.upnp_cache.remove(&local_v4);
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
                if !self.reserve(key, Footprint::requested(key, &spec, (*gw, *scope_id))) {
                    return true;
                }
                attempted = true;
                match pcp::port_mapping(
                    pcp::BaseMapRequest::new(*gw, local_ip, protocol.internet(), intl),
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
                                "PCP PORT_SET {protocol:?} mapped {external_port}+{range_size}->{local_ip}:{} via {gw}",
                                spec.internal_port
                            );
                            self.retain_pcp(key, m);
                            return true;
                        }
                        crate::dev_log!(
                            debug,
                            "gateway {gw} granted {granted}/{range_size} PORT_SET ports for {local_ip}:{external_port}; skipping range"
                        );
                        if self.reject_pcp(key, m).await {
                            return true;
                        }
                    }
                    Ok(m) => {
                        if self.reject_pcp(key, m).await {
                            return true;
                        }
                    }
                    Err(e) => {
                        self.release(key);
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
            if !self.reserve(key, Footprint::requested(key, &spec, (*gw, *scope_id))) {
                return true;
            }
            attempted = true;
            match PortMapping::new(
                *gw,
                local_ip,
                protocol.internet(),
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
                        "{} {protocol:?} mapped {external_port}->{local_ip}:{} via {gw}",
                        m.mapping_type(),
                        spec.internal_port,
                    );
                    let nat_pmp = matches!(m.mapping_type(), crab_nat::PortMappingType::NatPmp);
                    report(interfaces, *gw, |caps, now| {
                        set_verdict(&mut caps.pcp, !nat_pmp, now)
                            | set_verdict(&mut caps.nat_pmp, nat_pmp, now)
                    });
                    self.retain_pcp(key, m);
                    return true;
                }
                // A different external port is useless for a fixed public port.
                Ok(m) => {
                    if self.reject_pcp(key, m).await {
                        return true;
                    }
                }
                Err(e) => {
                    self.release(key);
                    report_crab_nat_failure(interfaces, *gw, &e);
                    crate::dev_log!(
                        debug,
                        "PCP/NAT-PMP {protocol:?} map {local_ip}:{external_port} via {gw} failed: {e}"
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
            let gateway = match self.gateway_for(local_v4).await.cloned() {
                Some(gateway) => {
                    // Discovery alone proves the IGD, whatever the SOAP call says.
                    report_local(interfaces, local_ip, true);
                    if !self.reserve(
                        key,
                        Footprint::requested(key, &spec, (gateway.addr.ip(), None)),
                    ) {
                        return true;
                    }
                    match upnp::add_port(
                        &gateway,
                        protocol.upnp(),
                        external_port,
                        local_v4,
                        spec.internal_port,
                    )
                    .await
                    {
                        Ok(()) => {
                            tracing::debug!(
                                "UPnP {protocol:?} mapped {external_port}->{local_v4}:{}",
                                spec.internal_port
                            );
                            Some(gateway)
                        }
                        Err(e) => {
                            crate::dev_log!(
                                debug,
                                "UPnP {protocol:?} map {local_v4}:{external_port} failed: {e}"
                            );
                            self.release(key);
                            None
                        }
                    }
                }
                None => {
                    report_local(interfaces, local_ip, false);
                    None
                }
            };
            if let Some(gateway) = gateway {
                let external_ip = None;
                self.active.insert(
                    key.clone(),
                    Active::Upnp {
                        external_ip,
                        internal_port: spec.internal_port,
                        gateway,
                    },
                );
                if let Some(Active::Upnp {
                    gateway,
                    external_ip,
                    ..
                }) = self.active.get_mut(key)
                {
                    *external_ip = upnp::external_ipv4(gateway).await.ok().flatten();
                }
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

    struct RouterFixture {
        ip: IpAddr,
        requests: Arc<SyncMutex<Vec<Vec<u8>>>>,
        reject_delete: Arc<std::sync::atomic::AtomicBool>,
        silence_delete: Arc<std::sync::atomic::AtomicBool>,
        barriers: mpsc::UnboundedSender<oneshot::Sender<usize>>,
        task: NonDetachingJoinHandle<()>,
    }

    impl RouterFixture {
        async fn new(nat_pmp: bool, shift: u16, count: Option<u16>, hostname: bool) -> Self {
            use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
            static NEXT: AtomicU32 = AtomicU32::new(1);
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let ip: IpAddr = Ipv4Addr::from(0x7f4d0000 + n).into();
            let socket = tokio::net::UdpSocket::bind((ip, 5351)).await.unwrap();
            let requests = Arc::new(SyncMutex::new(Vec::new()));
            let reject_delete = Arc::new(AtomicBool::new(true));
            let silence_delete = Arc::new(AtomicBool::new(false));
            let (barriers, mut barrier_recv) = mpsc::unbounded_channel::<oneshot::Sender<usize>>();
            let captured = requests.clone();
            let reject = reject_delete.clone();
            let silence = silence_delete.clone();
            let task = tokio::spawn(async move {
                let mut bytes = [0; 1500];
                loop {
                    let (n, peer) = tokio::select! {
                        biased;
                        packet = socket.recv_from(&mut bytes) => packet.unwrap(),
                        Some(barrier) = barrier_recv.recv() => {
                            let _ = barrier.send(captured.peek(|requests| requests.len()));
                            continue;
                        }
                    };
                    let request = &bytes[..n];
                    captured.mutate(|requests| requests.push(request.to_vec()));
                    if nat_pmp && request[0] == 2 {
                        socket.send_to(&[0, 0x81, 0, 1], peer).await.unwrap();
                        continue;
                    }
                    if request[1] == 0 {
                        let mut response = if request[0] == 2 {
                            let mut response = vec![0; 24];
                            response[0] = 2;
                            crate::net::port_map::pcp::capability::encode_start9_capability_option(
                                &mut response,
                            );
                            response
                        } else {
                            let mut response = vec![0; 12];
                            response[3] = if nat_pmp { 0 } else { 1 };
                            response[8..12].copy_from_slice(&[1, 2, 3, 4]);
                            response
                        };
                        response[1] = 0x80;
                        socket.send_to(&response, peer).await.unwrap();
                        continue;
                    }
                    let deleting = if nat_pmp {
                        &request[8..12]
                    } else {
                        &request[4..8]
                    } == [0; 4];
                    if deleting && silence.load(Ordering::SeqCst) {
                        continue;
                    }
                    let denied = deleting && reject.load(Ordering::SeqCst);
                    let response = if nat_pmp {
                        let mut response = vec![0; 16];
                        response[1] = request[1] | 0x80;
                        response[3] = if denied { 2 } else { 0 };
                        response[8..10].copy_from_slice(&request[4..6]);
                        let port = u16::from_be_bytes(request[6..8].try_into().unwrap());
                        response[10..12].copy_from_slice(
                            &if deleting {
                                0
                            } else {
                                port.saturating_add(shift)
                            }
                            .to_be_bytes(),
                        );
                        response[12..16].copy_from_slice(&request[8..12]);
                        response
                    } else {
                        let mut response = vec![0; 60];
                        response[0] = 2;
                        response[1] = request[1] | 0x80;
                        response[3] = if denied { 2 } else { 0 };
                        response[4..8].copy_from_slice(&request[4..8]);
                        response[24..60].copy_from_slice(&request[24..60]);
                        let port = u16::from_be_bytes(request[42..44].try_into().unwrap());
                        response[42..44].copy_from_slice(
                            &if deleting {
                                0
                            } else {
                                port.saturating_add(shift)
                            }
                            .to_be_bytes(),
                        );
                        response[44..60]
                            .copy_from_slice(&Ipv4Addr::new(1, 2, 3, 4).to_ipv6_mapped().octets());
                        for option in crate::net::port_map::pcp::pcp_options(&request[60..]) {
                            let (code, data) = option.unwrap();
                            if code == OPTION_HOSTNAME && !hostname {
                                continue;
                            }
                            let data = if code == OPTION_PORT_SET {
                                count
                                    .map(|size| {
                                        PortSet {
                                            size,
                                            first_internal_port: u16::from_be_bytes(
                                                request[40..42].try_into().unwrap(),
                                            ),
                                            parity: false,
                                        }
                                        .to_payload()
                                    })
                                    .unwrap_or_else(|| data.to_vec())
                            } else {
                                data.to_vec()
                            };
                            crate::net::port_map::pcp::encode_pcp_option(
                                &mut response,
                                code,
                                &data,
                            );
                        }
                        response
                    };
                    socket.send_to(&response, peer).await.unwrap();
                }
            })
            .into();
            Self {
                ip,
                requests,
                reject_delete,
                silence_delete,
                barriers,
                task,
            }
        }

        fn interfaces(&self) -> Watch<OrdMap<GatewayId, NetworkInterfaceInfo>> {
            Watch::new(OrdMap::from_iter([(
                GatewayId::from(imbl_value::InternedString::intern("fixture")),
                NetworkInterfaceInfo {
                    port_map: GatewayPortMapCapabilities {
                        pcp: CapabilityVerdict::supported(true),
                        pcp_hostname: CapabilityVerdict::supported(true),
                        upnp: CapabilityVerdict::supported(false),
                        ..Default::default()
                    },
                    ..iface(
                        &["127.0.0.2/8"],
                        &[&self.ip.to_string()],
                        GatewayType::InboundOutbound,
                    )
                },
            )]))
        }

        fn spec(&self, count: u16) -> Spec {
            Spec {
                internal_port: 8443,
                gateways: vec![(self.ip, None)],
                count,
            }
        }

        async fn received_through_barrier(&self) -> usize {
            let (send, recv) = oneshot::channel();
            self.barriers.send(send).unwrap();
            recv.await.unwrap()
        }

        async fn finish(mut self) {
            self.task.abort();
            let _ = (&mut self.task).await;
        }
    }

    #[tokio::test]
    async fn rejected_grants_retain_exact_cleanup_and_block_fallback() {
        use std::sync::atomic::Ordering;
        for (nat_pmp, shift, count, hostname) in [
            (false, 7, 1, false),
            (true, 7, 1, false),
            (false, 0, 1, true),
            (false, 0, 8, false),
        ] {
            let router = RouterFixture::new(nat_pmp, shift, Some(2), false).await;
            let fallback = RouterFixture::new(false, 0, None, true).await;
            let key = (
                "127.0.0.2".parse().unwrap(),
                443,
                hostname.then(|| "a.example.com".to_owned()),
                TransportProtocol::Tcp,
            );
            let mut requested = router.spec(count);
            requested.gateways.push((fallback.ip, None));
            let mut state = State::default();
            state
                .ensure(&router.interfaces(), key.clone(), requested)
                .await;
            assert!(state.active.contains_key(&key));
            assert!(state.stale.contains(&key));
            assert_eq!(
                external_ip_of(&state.desired, &state.active, &state.stale, 443),
                None
            );
            assert!(fallback.requests.peek(|requests| requests.is_empty()));
            let footprint = state.footprints.peek(|owned| owned[&key].clone());
            assert_eq!(footprint.start, 443 + shift);
            assert_eq!(footprint.count, if count > 1 { 2 } else { 1 });
            assert!(footprint.hostname.is_none());
            let mut overlapping = State {
                footprints: state.footprints.clone(),
                ..Default::default()
            };
            let overlap_key = (
                "127.0.0.3".parse().unwrap(),
                footprint.start,
                None,
                TransportProtocol::Tcp,
            );
            let calls = router.requests.peek(|requests| requests.len());
            overlapping
                .ensure(&router.interfaces(), overlap_key.clone(), router.spec(1))
                .await;
            assert!(!overlapping.active.contains_key(&overlap_key));
            assert_eq!(router.requests.peek(|requests| requests.len()), calls);
            let first = router.requests.peek(|requests| {
                requests
                    .iter()
                    .find(|r| r[1] != 0 && r[0] == if nat_pmp { 0 } else { 2 })
                    .unwrap()
                    .clone()
            });
            state.remove(key.clone()).await.unwrap_err();
            assert!(state.active.contains_key(&key));
            router.reject_delete.store(false, Ordering::SeqCst);
            state.drain().await.unwrap();
            assert!(state.active.is_empty());
            assert!(state.footprints.peek(|owned| owned.is_empty()));
            router.requests.peek(|requests| {
                for delete in requests
                    .iter()
                    .filter(|r| r[1] != 0 && r[0] == if nat_pmp { 0 } else { 2 })
                    .skip(1)
                {
                    if nat_pmp {
                        assert_eq!(&delete[4..6], &first[4..6]);
                        assert_eq!(&delete[8..12], &[0; 4]);
                    } else {
                        assert_eq!(&delete[24..42], &first[24..42]);
                        assert_eq!(&delete[60..], &first[60..]);
                        assert_eq!(&delete[4..8], &[0; 4]);
                    }
                }
            });
            router.finish().await;
            fallback.finish().await;
        }
    }

    #[tokio::test]
    async fn retained_footprints_fence_overlapping_keys_and_shards() {
        use std::sync::atomic::Ordering;
        let router = RouterFixture::new(false, 0, None, true).await;
        let key = (
            "127.0.0.2".parse().unwrap(),
            4000,
            None,
            TransportProtocol::Tcp,
        );
        let mut old = State::default();
        old.ensure(&router.interfaces(), key.clone(), router.spec(10))
            .await;
        assert!(old.active.contains_key(&key));
        let mut changed = router.spec(1);
        changed.internal_port = 9999;
        old.ensure(&router.interfaces(), key.clone(), changed).await;
        assert!(old.stale.contains(&key));
        assert_eq!(old.footprints.peek(|owned| owned[&key].count), 10);
        let mut next = State {
            footprints: old.footprints.clone(),
            ..Default::default()
        };
        let sibling = (
            "127.0.0.3".parse().unwrap(),
            4005,
            None,
            TransportProtocol::Tcp,
        );
        let calls = router.requests.peek(|requests| requests.len());
        next.ensure(&router.interfaces(), sibling.clone(), router.spec(1))
            .await;
        assert!(!next.active.contains_key(&sibling));
        assert_eq!(router.requests.peek(|requests| requests.len()), calls);
        let same_shard = (key.0, 4008, None, TransportProtocol::Tcp);
        old.ensure(&router.interfaces(), same_shard.clone(), router.spec(1))
            .await;
        assert!(!old.active.contains_key(&same_shard));
        let udp = (sibling.0, sibling.1, None, TransportProtocol::Udp);
        next.ensure(&router.interfaces(), udp.clone(), router.spec(1))
            .await;
        assert!(next.active.contains_key(&udp));
        router.reject_delete.store(false, Ordering::SeqCst);
        old.remove(key).await.unwrap();
        next.failures.remove(&sibling);
        next.ensure(&router.interfaces(), sibling.clone(), router.spec(1))
            .await;
        assert!(next.active.contains_key(&sibling));
        old.drain().await.unwrap();
        next.drain().await.unwrap();
        router.finish().await;
    }

    #[tokio::test]
    async fn cancelling_teardown_preserves_pcp_identity() {
        use std::sync::atomic::Ordering;
        let router = RouterFixture::new(false, 0, None, true).await;
        let key = (
            "127.0.0.2".parse().unwrap(),
            443,
            None,
            TransportProtocol::Tcp,
        );
        let mut state = State::default();
        state
            .ensure(&router.interfaces(), key.clone(), router.spec(1))
            .await;
        router.silence_delete.store(true, Ordering::SeqCst);
        assert!(
            tokio::time::timeout(Duration::from_millis(30), state.teardown(key.clone()))
                .await
                .is_err()
        );
        assert!(state.active.contains_key(&key));
        assert!(state.footprints.peek(|owned| owned.contains_key(&key)));
        router.silence_delete.store(false, Ordering::SeqCst);
        router.reject_delete.store(false, Ordering::SeqCst);
        state.teardown(key).await.unwrap();
        router.finish().await;
    }

    #[tokio::test]
    async fn retained_hostname_does_not_block_distinct_sni() {
        use std::sync::atomic::Ordering;
        let router = RouterFixture::new(false, 0, None, true).await;
        let key = (
            "127.0.0.2".parse().unwrap(),
            443,
            Some("a.example.com".into()),
            TransportProtocol::Tcp,
        );
        let mut first = State::default();
        first
            .ensure(&router.interfaces(), key.clone(), router.spec(1))
            .await;
        first.remove(key.clone()).await.unwrap_err();
        let mut second = State {
            footprints: first.footprints.clone(),
            ..Default::default()
        };
        let other = (
            "127.0.0.3".parse().unwrap(),
            443,
            Some("b.example.com".into()),
            TransportProtocol::Tcp,
        );
        second
            .ensure(&router.interfaces(), other.clone(), router.spec(1))
            .await;
        assert!(second.active.contains_key(&other));
        let same = (other.0, 443, key.2.clone(), TransportProtocol::Tcp);
        second
            .ensure(&router.interfaces(), same.clone(), router.spec(1))
            .await;
        assert!(!second.active.contains_key(&same));
        router.reject_delete.store(false, Ordering::SeqCst);
        first.drain().await.unwrap();
        second.drain().await.unwrap();
        router.finish().await;
    }

    async fn soap_fixture() -> (
        Gateway<Tokio>,
        Arc<SyncMutex<Vec<String>>>,
        Arc<std::sync::atomic::AtomicBool>,
        NonDetachingJoinHandle<()>,
    ) {
        use std::sync::atomic::{AtomicBool, Ordering};

        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        use crate::net::port_map::server::igd::{ADD_HOSTNAME_ACTION, DELETE_HOSTNAME_ACTION};
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let gateway = Gateway {
            addr: listener.local_addr().unwrap(),
            control_url: "/control".into(),
            control_schema: [ADD_HOSTNAME_ACTION, DELETE_HOSTNAME_ACTION]
                .into_iter()
                .map(|action| (action.to_owned(), Vec::new()))
                .collect(),
            ..test_gateway()
        };
        let requests = Arc::new(SyncMutex::new(Vec::new()));
        let fail = Arc::new(AtomicBool::new(false));
        let captured = requests.clone();
        let failing = fail.clone();
        let task = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut bytes = [0; 4096];
                loop {
                    let n = stream.read(&mut bytes).await.unwrap();
                    if n == 0 { break; }
                    request.extend_from_slice(&bytes[..n]);
                    if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                        let header = String::from_utf8_lossy(&request[..end]);
                        let length = header.lines().find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<usize>().unwrap())
                        }).unwrap_or(0);
                        if request.len() >= end + 4 + length { break; }
                    }
                }
                let request = String::from_utf8(request).unwrap();
                let action = if request.contains(DELETE_HOSTNAME_ACTION) { DELETE_HOSTNAME_ACTION } else { ADD_HOSTNAME_ACTION };
                captured.mutate(|requests| requests.push(request));
                let (status, body) = if failing.load(Ordering::SeqCst) {
                    (500, "<errorCode>501</errorCode>".to_owned())
                } else {
                    (200, format!("<{action}Response/>"))
                };
                let response = format!("HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                let _ = stream.write_all(response.as_bytes()).await;
            }
        }).into();
        (gateway, requests, fail, task)
    }

    #[tokio::test]
    async fn hostname_refresh_keeps_gateway_and_owner_until_withdrawn() {
        use std::sync::atomic::Ordering;

        use crate::net::port_map::server::igd::{ADD_HOSTNAME_ACTION, DELETE_HOSTNAME_ACTION};
        let (old_gateway, old_calls, old_fail, mut old_task) = soap_fixture().await;
        let (new_gateway, new_calls, _, mut new_task) = soap_fixture().await;
        let local = Ipv4Addr::new(127, 0, 0, 2);
        let key = (
            local.into(),
            443,
            Some("a.example.com".into()),
            TransportProtocol::Tcp,
        );
        let mut state = State::default();
        state.desired.insert(
            key.clone(),
            Spec {
                internal_port: 9443,
                ..spec()
            },
        );
        state.active.insert(
            key.clone(),
            Active::Upnp {
                external_ip: None,
                internal_port: 8443,
                gateway: old_gateway,
            },
        );
        state
            .upnp_cache
            .insert(local, (new_gateway, Instant::now()));
        let ifaces = Watch::new(OrdMap::new());
        state.refresh(&ifaces).await;
        old_calls.peek(|calls| {
            assert_eq!(calls.len(), 1);
            assert!(calls[0].contains(ADD_HOSTNAME_ACTION));
            assert!(calls[0].contains("<NewInternalPort>8443</NewInternalPort>"));
        });
        assert!(new_calls.peek(|calls| calls.is_empty()));
        old_fail.store(true, Ordering::SeqCst);
        state.refresh(&ifaces).await;
        assert!(state.stale.contains(&key));
        state.failures.remove(&key);
        state.refresh(&ifaces).await;
        assert!(new_calls.peek(|calls| calls.is_empty()));
        old_calls.peek(|calls| assert!(calls.last().unwrap().contains(DELETE_HOSTNAME_ACTION)));
        old_fail.store(false, Ordering::SeqCst);
        state.failures.remove(&key);
        state.refresh(&ifaces).await;
        assert!(!state.stale.contains(&key));
        new_calls.peek(|calls| {
            assert_eq!(calls.len(), 2);
            assert!(calls[1].contains("GetExternalIPAddress"));
            assert!(calls[0].contains(ADD_HOSTNAME_ACTION));
            assert!(calls[0].contains("<NewInternalPort>9443</NewInternalPort>"));
        });
        state.drain().await.unwrap();
        old_task.abort();
        new_task.abort();
        let _ = (&mut old_task).await;
        let _ = (&mut new_task).await;
    }

    #[tokio::test]
    async fn drain_deadline_joins_shard_and_stops_router_calls() {
        use std::sync::atomic::Ordering;
        let router = RouterFixture::new(false, 0, None, true).await;
        let key = (
            "127.0.0.2".parse().unwrap(),
            443,
            None,
            TransportProtocol::Tcp,
        );
        let mut state = State::default();
        state
            .ensure(&router.interfaces(), key.clone(), router.spec(1))
            .await;
        router.silence_delete.store(true, Ordering::SeqCst);
        let controller = PortMapController::new(router.interfaces());
        let (commands, recv) = mpsc::unbounded_channel();
        let (drain, drain_recv) = mpsc::unbounded_channel();
        let task: NonDetachingJoinHandle<_> =
            tokio::spawn(run_shard(router.interfaces(), state, recv, drain_recv)).into();
        let abort = task.abort_handle();
        controller.state.mutate(|state| {
            let ControllerState::Accepting(shards) = state else {
                unreachable!()
            };
            shards.insert(
                key.0,
                Shard {
                    commands,
                    drain,
                    task,
                },
            );
        });
        let first_controller = controller.clone();
        let deadline = Instant::now() + Duration::from_millis(60);
        let first = tokio::spawn(async move { first_controller.drain_until(deadline).await });
        tokio::time::sleep(Duration::from_millis(10)).await;
        first.abort();
        assert!(first.await.unwrap_err().is_cancelled());
        controller
            .drain_until(deadline + Duration::from_secs(60))
            .await
            .unwrap_err();
        assert!(abort.is_finished());
        // Flush datagrams queued before the shard joined before counting new sends.
        let calls = router.received_through_barrier().await;
        tokio::time::sleep(Duration::from_millis(350)).await;
        assert_eq!(router.received_through_barrier().await, calls);
        controller.drain().await.unwrap_err();
        router.finish().await;
    }

    #[tokio::test]
    async fn slow_gateway_does_not_block_another_interface() {
        use std::sync::atomic::Ordering;
        let slow = RouterFixture::new(false, 0, None, true).await;
        let fast = RouterFixture::new(false, 0, None, true).await;
        let controller = PortMapController::new(slow.interfaces());
        let local: IpAddr = "127.0.0.2".parse().unwrap();
        controller.ensure(local, 443, 8443, vec![(slow.ip, None)]);
        assert_eq!(
            controller.mapped_external_ip(local, 443).await,
            Some("1.2.3.4".parse().unwrap())
        );
        slow.silence_delete.store(true, Ordering::SeqCst);
        controller.remove(local, 443);
        let other: IpAddr = "127.0.0.3".parse().unwrap();
        controller.ensure(other, 443, 8443, vec![(fast.ip, None)]);
        assert_eq!(
            tokio::time::timeout(
                Duration::from_millis(150),
                controller.mapped_external_ip(other, 443)
            )
            .await
            .unwrap(),
            Some("1.2.3.4".parse().unwrap())
        );
        slow.silence_delete.store(false, Ordering::SeqCst);
        slow.reject_delete.store(false, Ordering::SeqCst);
        fast.reject_delete.store(false, Ordering::SeqCst);
        controller
            .drain_until(Instant::now() + Duration::from_secs(3))
            .await
            .unwrap();
        slow.finish().await;
        fast.finish().await;
    }

    #[tokio::test]
    async fn expired_drain_never_starts_withdrawal() {
        let router = RouterFixture::new(false, 0, None, true).await;
        let key = (
            "127.0.0.2".parse().unwrap(),
            443,
            None,
            TransportProtocol::Tcp,
        );
        let mut state = State::default();
        state
            .ensure(&router.interfaces(), key, router.spec(1))
            .await;
        let calls = router.requests.peek(|calls| calls.len());
        let (commands, recv) = mpsc::unbounded_channel();
        let (drain, drain_recv) = mpsc::unbounded_channel();
        let task = tokio::spawn(run_shard(router.interfaces(), state, recv, drain_recv)).into();
        drain_shard(
            router.ip,
            Shard {
                commands,
                drain,
                task,
            },
            Instant::now(),
        )
        .await
        .unwrap_err();
        assert_eq!(router.requests.peek(|calls| calls.len()), calls);
        router.finish().await;
    }

    fn spec() -> Spec {
        Spec {
            internal_port: 443,
            gateways: Vec::new(),
            count: 1,
        }
    }

    fn interfaces() -> Watch<OrdMap<GatewayId, NetworkInterfaceInfo>> {
        Watch::new(OrdMap::from_iter([(
            GatewayId::from(imbl_value::InternedString::intern("eno0")),
            NetworkInterfaceInfo {
                port_map: GatewayPortMapCapabilities {
                    upnp: CapabilityVerdict::supported(false),
                    ..Default::default()
                },
                ..iface(&["10.59.0.2/24"], &[], GatewayType::InboundOutbound)
            },
        )]))
    }

    fn test_gateway() -> Gateway<Tokio> {
        Gateway {
            addr: "127.0.0.1:49001".parse().unwrap(),
            root_url: String::new(),
            control_url: String::new(),
            control_schema_url: String::new(),
            control_schema: Default::default(),
            provider: Tokio,
        }
    }

    #[test]
    fn a_gateway_behind_another_nat_reports_no_usable_address() {
        for ip in ["192.168.1.1", "10.0.0.1", "172.16.0.1"] {
            let reported: IpAddr = ip.parse::<Ipv4Addr>().unwrap().into();
            assert_eq!(
                routable_external_ip(Some(reported)),
                None,
                "{ip} is not reachable from the public Internet"
            );
        }
    }

    #[test]
    fn a_routable_address_is_reported_as_given() {
        for ip in ["1.2.3.4", "93.184.216.34"] {
            let reported: IpAddr = ip.parse::<Ipv4Addr>().unwrap().into();
            assert_eq!(routable_external_ip(Some(reported)), Some(reported), "{ip}");
        }
    }

    // v6 has no NAT: the mapping is on the host's own GUA, and `check_gua_port`
    // only asks whether a pinhole exists.
    #[test]
    fn a_v6_pinhole_is_kept_as_reported() {
        let gua: IpAddr = "2001:470:1f0b:1::1".parse().unwrap();
        assert_eq!(routable_external_ip(Some(gua)), Some(gua));
    }

    // NAT-PMP grants carry no external address.
    #[test]
    fn an_addressless_grant_has_no_answer() {
        assert_eq!(routable_external_ip(None), None);
    }

    #[test]
    fn a_upnp_mapping_answers_only_for_its_own_tcp_port() {
        let ip: IpAddr = Ipv4Addr::new(10, 59, 0, 2).into();
        let public = Ipv4Addr::new(1, 2, 3, 4);
        let mut active = BTreeMap::new();
        active.insert(
            (ip, 443, Some("example.com".into()), TransportProtocol::Tcp),
            Active::Upnp {
                external_ip: Some(public),
                internal_port: 443,
                gateway: test_gateway(),
            },
        );
        active.insert(
            (ip, 8080, None, TransportProtocol::Udp),
            Active::Upnp {
                external_ip: Some(public),
                internal_port: 8080,
                gateway: test_gateway(),
            },
        );

        assert_eq!(
            external_ip_of(
                &active.keys().cloned().map(|key| (key, spec())).collect(),
                &active,
                &BTreeSet::new(),
                443
            ),
            Some(IpAddr::V4(public))
        );
        assert_eq!(
            external_ip_of(
                &active.keys().cloned().map(|key| (key, spec())).collect(),
                &active,
                &BTreeSet::new(),
                8080
            ),
            None,
            "UDP is not TCP"
        );
        assert_eq!(
            external_ip_of(
                &active.keys().cloned().map(|key| (key, spec())).collect(),
                &active,
                &BTreeSet::new(),
                444
            ),
            None,
            "no mapping on that port"
        );
    }

    #[test]
    fn a_stale_mapping_answers_nothing() {
        let ip: IpAddr = Ipv4Addr::new(10, 59, 0, 2).into();
        let key: MappingKey = (ip, 443, None, TransportProtocol::Tcp);
        let active = BTreeMap::from([(
            key.clone(),
            Active::Upnp {
                external_ip: Some(Ipv4Addr::new(1, 2, 3, 4)),
                internal_port: 443,
                gateway: test_gateway(),
            },
        )]);

        assert_eq!(
            external_ip_of(
                &active.keys().cloned().map(|key| (key, spec())).collect(),
                &active,
                &BTreeSet::from([key]),
                443
            ),
            None
        );
    }

    #[tokio::test]
    async fn a_hostname_refresh_without_replacement_restores_a_stale_mapping() {
        let ip: IpAddr = Ipv4Addr::new(10, 59, 0, 2).into();
        let key: MappingKey = (ip, 443, Some("example.com".into()), TransportProtocol::Tcp);
        let mut state = State::default();
        state.desired.insert(key.clone(), spec());
        state.active.insert(
            key.clone(),
            Active::Upnp {
                external_ip: Some(Ipv4Addr::new(1, 2, 3, 4)),
                internal_port: 443,
                gateway: test_gateway(),
            },
        );

        state.refresh(&interfaces()).await;

        assert!(state.active.contains_key(&key));
        assert!(state.stale.contains(&key));
        assert_eq!(
            external_ip_of(&state.desired, &state.active, &state.stale, 443),
            None
        );
    }

    #[test]
    fn an_active_undesired_mapping_answers_nothing() {
        let ip: IpAddr = Ipv4Addr::new(10, 59, 0, 2).into();
        let key: MappingKey = (ip, 443, None, TransportProtocol::Tcp);
        let active = BTreeMap::from([(
            key,
            Active::Upnp {
                external_ip: Some(Ipv4Addr::new(1, 2, 3, 4)),
                internal_port: 443,
                gateway: test_gateway(),
            },
        )]);

        assert_eq!(
            external_ip_of(&BTreeMap::new(), &active, &BTreeSet::new(), 443),
            None
        );
    }

    #[test]
    fn a_upnp_mapping_behind_another_nat_answers_nothing() {
        let ip: IpAddr = Ipv4Addr::new(10, 59, 0, 2).into();
        let mut active = BTreeMap::new();
        active.insert(
            (ip, 443, None, TransportProtocol::Tcp),
            Active::Upnp {
                external_ip: Some(Ipv4Addr::new(192, 168, 8, 1)),
                internal_port: 443,
                gateway: test_gateway(),
            },
        );
        assert_eq!(
            external_ip_of(
                &active.keys().cloned().map(|key| (key, spec())).collect(),
                &active,
                &BTreeSet::new(),
                443
            ),
            None
        );
    }

    #[tokio::test]
    async fn distinct_hostnames_share_a_port_without_clobbering() {
        let ip: IpAddr = Ipv4Addr::new(10, 59, 0, 2).into();
        let a: MappingKey = (
            ip,
            443,
            Some("a.example.com".into()),
            TransportProtocol::Tcp,
        );
        let b: MappingKey = (
            ip,
            443,
            Some("b.example.com".into()),
            TransportProtocol::Tcp,
        );
        let plain: MappingKey = (ip, 443, None, TransportProtocol::Tcp);

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

        state.remove(a.clone()).await.unwrap();
        assert!(!state.desired.contains_key(&a));
        assert!(state.desired.contains_key(&b), "removing a dropped b");
        assert!(state.desired.contains_key(&plain));
    }

    // Raw interface forwards need separate TCP and UDP mappings on the same
    // external port; removing one protocol must not tear down the other.
    #[tokio::test]
    async fn tcp_and_udp_share_a_port_without_clobbering() {
        let ip: IpAddr = Ipv4Addr::new(10, 59, 0, 2).into();
        let tcp: MappingKey = (ip, 51820, None, TransportProtocol::Tcp);
        let udp: MappingKey = (ip, 51820, None, TransportProtocol::Udp);

        let mut state = State::default();
        state.ensure(&interfaces(), tcp.clone(), spec()).await;
        state.ensure(&interfaces(), udp.clone(), spec()).await;
        assert!(state.desired.contains_key(&tcp));
        assert!(state.desired.contains_key(&udp));
        assert_eq!(state.desired.len(), 2);

        state.remove(tcp.clone()).await.unwrap();
        assert!(!state.desired.contains_key(&tcp));
        assert!(state.desired.contains_key(&udp), "removing TCP dropped UDP");
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
        let key: MappingKey = (local, 443, None, TransportProtocol::Tcp);
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
        let key: MappingKey = (ip, 443, None, TransportProtocol::Tcp);
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

    #[tokio::test]
    async fn controller_rejects_new_shards_after_drain() {
        let controller = PortMapController::new(interfaces());
        controller.drain().await.unwrap();

        let ip: IpAddr = "fd00:59::2".parse().unwrap();
        controller.ensure(ip, 443, 443, Vec::new());

        assert!(
            controller
                .state
                .peek(|state| matches!(state, ControllerState::Draining(_)))
        );
        assert_eq!(controller.mapped_external_ip(ip, 443).await, None);
        controller.drain().await.unwrap();
    }

    #[tokio::test]
    async fn drain_bypasses_queued_commands() {
        let (commands, recv) = mpsc::unbounded_channel();
        let (drain, drain_recv) = mpsc::unbounded_channel();
        let (external_ip, external_ip_rx) = oneshot::channel();
        commands
            .send(Command::ExternalIp {
                external_port: 443,
                resp: external_ip,
            })
            .unwrap();
        drain
            .send(DrainRequest {
                deadline: Instant::now() + DRAIN_TIMEOUT,
            })
            .unwrap();

        tokio::spawn(run_shard(interfaces(), State::default(), recv, drain_recv))
            .await
            .unwrap()
            .unwrap();
        assert!(
            external_ip_rx.await.is_err(),
            "the queued ordinary command ran before drain"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn failed_explicit_removal_uses_bounded_retry_state() {
        let ip: IpAddr = Ipv4Addr::LOCALHOST.into();
        let key: MappingKey = (ip, 443, None, TransportProtocol::Tcp);
        let mut state = State::default();
        state.desired.insert(key.clone(), spec());
        state.active.insert(
            key.clone(),
            Active::Upnp {
                external_ip: None,
                internal_port: 443,
                gateway: test_gateway(),
            },
        );

        state.remove(key.clone()).await.unwrap_err();
        let first_retry = state.removal_retry_deadline().unwrap();
        assert!(!state.desired.contains_key(&key));
        assert!(state.active.contains_key(&key));
        assert_eq!(state.failures.get(&key).map(|(n, _)| *n), Some(1));
        assert_eq!(first_retry.duration_since(Instant::now()), RETRY_INTERVAL);
        assert!(!state.backoff_elapsed(&key));

        tokio::time::advance(RETRY_INTERVAL).await;
        assert!(state.backoff_elapsed(&key));
        state.retry_removals().await;
        let second_retry = state.removal_retry_deadline().unwrap();
        assert!(state.active.contains_key(&key));
        assert_eq!(state.failures.get(&key).map(|(n, _)| *n), Some(2));
        assert_eq!(
            second_retry.duration_since(Instant::now()),
            RETRY_INTERVAL * 2
        );
        assert!(second_retry.duration_since(first_retry) < REFRESH_INTERVAL);

        state.active.remove(&key);
        state.finish_removal_attempt(key.clone(), &Ok(()));
        assert!(!state.failures.contains_key(&key));
        assert_eq!(state.removal_retry_deadline(), None);
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_drains_share_completion_after_cancellation() {
        let controller = PortMapController::new(interfaces());
        let ip: IpAddr = "fd00:59::2".parse().unwrap();
        let (commands, mut command_rx) = mpsc::unbounded_channel();
        let (drain, mut drain_requests) = mpsc::unbounded_channel();
        let (started, start_rx) = oneshot::channel();
        let (finish, finish_rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            drain_requests.recv().await.unwrap();
            started.send(()).unwrap();
            finish_rx.await.unwrap();
            Ok(())
        })
        .into();
        controller.state.mutate(|state| match state {
            ControllerState::Accepting(shards) => {
                shards.insert(
                    ip,
                    Shard {
                        commands,
                        drain,
                        task,
                    },
                );
            }
            ControllerState::Draining(_) => panic!("controller already draining"),
        });

        let first_controller = controller.clone();
        let first = tokio::spawn(async move { first_controller.drain().await });
        start_rx.await.unwrap();
        tokio::task::yield_now().await;
        controller.ensure(ip, 443, 443, Vec::new());
        assert!(command_rx.try_recv().is_err());

        first.abort();
        assert!(first.await.unwrap_err().is_cancelled());
        let second_controller = controller.clone();
        let second = tokio::spawn(async move { second_controller.drain().await });
        let third_controller = controller.clone();
        let third = tokio::spawn(async move { third_controller.drain().await });
        tokio::task::yield_now().await;
        assert!(!second.is_finished());
        assert!(!third.is_finished());

        finish.send(()).unwrap();

        second.await.unwrap().unwrap();
        third.await.unwrap().unwrap();
        controller.drain().await.unwrap();
        controller.drain().await.unwrap();
    }
}
