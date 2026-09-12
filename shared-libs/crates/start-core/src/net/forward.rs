use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddrV4, SocketAddrV6};
use std::sync::{Arc, Weak};
use std::time::Duration;

use futures::channel::oneshot;
use iddqd::{IdOrdItem, IdOrdMap};
use imbl::OrdMap;
use ipnet::IpNet;
use rand::RngExt;
use rpc_toolkit::{Context, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::GatewayId;
use crate::context::{CliContext, RpcContext};
use crate::db::model::public::NetworkInterfaceInfo;
use crate::net::port_map::{PortMapController, candidate_gateways};
use crate::prelude::*;
use crate::util::Invoke;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::serde::{HandlerExtSerde, display_serializable};
use crate::util::sync::Watch;

pub const START9_BRIDGE_IFACE: &str = "lxcbr0";
const EPHEMERAL_PORT_START: u16 = 49152;
const PORT_FORWARD_GC_INTERVAL: Duration = Duration::from_secs(30);
// Reserved by/for host daemons (mDNS 5353, LLMNR 5355, postgres 5432, X11
// forwarding 6010). 9050/9051 are claimable on purpose: they were the 0.3.x
// host tor daemon's reservation (gone in 0.4.x), and the tor service now binds
// 9050 without exporting an interface so its SOCKS proxy sits at a stable
// 10.0.3.1:9050 on the bridge — do not re-restrict them.
const RESTRICTED_PORTS: &[u16] = &[5353, 5355, 5432, 6010];

/// Only a privileged claimant — StartOS itself, which runs as root — may take a
/// port below 1024. A host daemon already holds RESTRICTED_PORTS, whoever asks.
fn may_claim(port: u16, privileged: bool) -> bool {
    !RESTRICTED_PORTS.contains(&port) && (privileged || port > 1024)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ForwardRequirements {
    pub public_gateways: BTreeSet<GatewayId>,
    pub private_ips: BTreeSet<IpAddr>,
    pub secure: bool,
}

impl std::fmt::Display for ForwardRequirements {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ForwardRequirements {{ public: {:?}, private: {:?}, secure: {} }}",
            self.public_gateways, self.private_ips, self.secure
        )
    }
}

/// Allocated external ports. The flag marks the ports one of our own TLS
/// listeners answers on — terminating (`add_ssl`) or SNI-passthrough (self-TLS)
/// — so a domain can be advertised there and SNI-routed.
#[derive(Debug, Deserialize, Serialize)]
pub struct AvailablePorts(BTreeMap<u16, bool>);
impl AvailablePorts {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }
    pub fn alloc(&mut self, ssl: bool) -> Result<u16, Error> {
        let mut rng = crate::util::crypto::os_rng();
        for _ in 0..1000 {
            let port = rng.random_range(EPHEMERAL_PORT_START..u16::MAX);
            if !self.0.contains_key(&port) {
                self.0.insert(port, ssl);
                return Ok(port);
            }
        }
        Err(Error::new(
            eyre!("{}", t!("net.forward.no-dynamic-ports-available")),
            ErrorKind::Network,
        ))
    }
    /// Allocate a specific port; `None` if taken or not the caller's to claim.
    pub fn try_alloc(&mut self, port: u16, ssl: bool, privileged: bool) -> Option<u16> {
        if !may_claim(port, privileged) || self.0.contains_key(&port) {
            return None;
        }
        self.0.insert(port, ssl);
        Some(port)
    }

    /// Allocate `count` contiguous non-ssl ports from `start`. All-or-nothing: if
    /// any port is taken or not the caller's to claim, allocates none and `Err`s
    /// on the first offender.
    pub fn try_alloc_range(
        &mut self,
        start: u16,
        count: u16,
        privileged: bool,
    ) -> Result<(), Error> {
        if count == 0 {
            return Err(Error::new(
                eyre!("port range must contain at least one port"),
                ErrorKind::InvalidRequest,
            ));
        }
        let end = start.checked_add(count - 1).ok_or_else(|| {
            Error::new(
                eyre!("port range {start}+{count} overflows u16"),
                ErrorKind::InvalidRequest,
            )
        })?;
        for port in start..=end {
            if !may_claim(port, privileged) {
                return Err(Error::new(
                    eyre!("port {port} in range {start}-{end} is restricted"),
                    ErrorKind::InvalidRequest,
                ));
            }
            if self.0.contains_key(&port) {
                return Err(Error::new(
                    eyre!("port {port} in range {start}-{end} is already allocated"),
                    ErrorKind::InvalidRequest,
                ));
            }
        }
        for port in start..=end {
            self.0.insert(port, false);
        }
        Ok(())
    }

    pub fn set_ssl(&mut self, port: u16, ssl: bool) {
        self.0.insert(port, ssl);
    }

    pub fn is_ssl(&self, port: u16) -> bool {
        self.0.get(&port).copied().unwrap_or(false)
    }
    pub fn free(&mut self, ports: impl IntoIterator<Item = u16>) {
        for port in ports {
            self.0.remove(&port);
        }
    }
}

pub fn forward_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new().subcommand(
        "dump-table",
        from_fn_async(
            |ctx: RpcContext| async move { ctx.net_controller.forward.dump_table().await },
        )
        .with_display_serializable()
        .with_custom_display_fn(|HandlerArgs { params, .. }, res| {
            use prettytable::*;

            if let Some(format) = params.format {
                return display_serializable(format, res);
            }

            let mut table = Table::new();
            table.add_row(row![bc => "FROM", "TO", "REQS"]);

            for (external, target) in res.0 {
                table.add_row(row![external, target.target, target.reqs]);
            }

            table.print_tty(false)?;

            Ok(())
        })
        .with_about("about.dump-port-forward-table")
        .with_call_remote::<CliContext>(),
    )
}

struct ForwardMapping {
    source: SocketAddrV4,
    target: SocketAddrV4,
    /// Contiguous ports forwarded from `source.port()` / `target.port()`. `> 1`
    /// becomes one nft rule for the range (port-preserving when the bases match,
    /// else an offset verdict map).
    count: u16,
    target_prefix: u8,
    src_filter: Option<IpNet>,
    rc: Weak<()>,
}

impl ForwardMapping {
    fn matches(
        &self,
        target: SocketAddrV4,
        count: u16,
        target_prefix: u8,
        src_filter: Option<&IpNet>,
    ) -> bool {
        self.target == target
            && self.count == count
            && self.target_prefix == target_prefix
            && self.src_filter.as_ref() == src_filter
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Ipv6ForwardSpec {
    target: SocketAddrV6,
    target_prefix: u8,
    src_filter: Option<IpNet>,
    gateways: Vec<(IpAddr, Option<u32>)>,
}

struct Ipv6ForwardMapping {
    desired: Ipv6ForwardSpec,
    applied: Option<Ipv6ForwardSpec>,
    rc: Weak<()>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Ipv6ForwardOperation {
    Add(Ipv6ForwardSpec),
    Remove(Ipv6ForwardSpec),
}

impl Ipv6ForwardMapping {
    fn live_desired(&self) -> Option<Ipv6ForwardSpec> {
        (self.rc.strong_count() > 0).then(|| self.desired.clone())
    }

    fn next_operation(&self) -> Option<Ipv6ForwardOperation> {
        let desired = self.live_desired();
        match (&self.applied, desired) {
            (Some(applied), desired) if desired.as_ref() != Some(applied) => {
                Some(Ipv6ForwardOperation::Remove(applied.clone()))
            }
            (None, Some(desired)) => Some(Ipv6ForwardOperation::Add(desired)),
            _ => None,
        }
    }

    fn operation_succeeded(&mut self, operation: Ipv6ForwardOperation) {
        match operation {
            Ipv6ForwardOperation::Add(spec) => self.applied = Some(spec),
            Ipv6ForwardOperation::Remove(_) => self.applied = None,
        }
    }
}

#[derive(Default)]
struct PortForwardState {
    mappings: BTreeMap<SocketAddrV4, ForwardMapping>, // source -> target
}

impl PortForwardState {
    async fn add_forward(
        &mut self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        count: u16,
        target_prefix: u8,
        src_filter: Option<IpNet>,
    ) -> Result<Arc<()>, Error> {
        if let Some(existing) = self.mappings.get_mut(&source) {
            if existing.matches(target, count, target_prefix, src_filter.as_ref()) {
                if let Some(existing_rc) = existing.rc.upgrade() {
                    return Ok(existing_rc);
                } else {
                    let rc = Arc::new(());
                    existing.rc = Arc::downgrade(&rc);
                    return Ok(rc);
                }
            } else {
                self.remove_forward(source).await?;
            }
        }

        let rc = Arc::new(());
        forward(source, target, count, target_prefix, src_filter.as_ref()).await?;
        self.mappings.insert(
            source,
            ForwardMapping {
                source,
                target,
                count,
                target_prefix,
                src_filter,
                rc: Arc::downgrade(&rc),
            },
        );

        Ok(rc)
    }

    async fn gc(&mut self) -> Result<(), Error> {
        let to_remove: Vec<SocketAddrV4> = self
            .mappings
            .iter()
            .filter(|(_, mapping)| mapping.rc.strong_count() == 0)
            .map(|(source, _)| *source)
            .collect();

        let mut first_error = None;
        for source in to_remove {
            if let Err(error) = self.remove_forward(source).await {
                tracing::error!("failed to remove IPv4 forward {source}: {error}");
                tracing::debug!("{error:?}");
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    async fn remove_forward(&mut self, source: SocketAddrV4) -> Result<(), Error> {
        let Some(mapping) = self.mappings.get(&source) else {
            return Ok(());
        };
        unforward(
            mapping.source,
            mapping.target,
            mapping.count,
            mapping.target_prefix,
            mapping.src_filter.as_ref(),
        )
        .await?;
        self.mappings.remove(&source);
        Ok(())
    }

    fn dump(&self) -> BTreeMap<SocketAddrV4, SocketAddrV4> {
        self.mappings
            .iter()
            .filter(|(_, mapping)| mapping.rc.strong_count() > 0)
            .map(|(source, mapping)| (*source, mapping.target))
            .collect()
    }
}

impl Drop for PortForwardState {
    fn drop(&mut self) {
        if !self.mappings.is_empty() {
            let mappings = std::mem::take(&mut self.mappings);
            tokio::spawn(async move {
                for (_, mapping) in mappings {
                    unforward(
                        mapping.source,
                        mapping.target,
                        mapping.count,
                        mapping.target_prefix,
                        mapping.src_filter.as_ref(),
                    )
                    .await
                    .log_err();
                }
            });
        }
    }
}

enum PortForwardCommand {
    AddForward {
        source: SocketAddrV4,
        target: SocketAddrV4,
        count: u16,
        target_prefix: u8,
        src_filter: Option<IpNet>,
        respond: oneshot::Sender<Result<Arc<()>, Error>>,
    },
    Gc {
        respond: oneshot::Sender<Result<(), Error>>,
    },
    Dump {
        respond: oneshot::Sender<BTreeMap<SocketAddrV4, SocketAddrV4>>,
    },
}

pub struct PortForwardController {
    req: mpsc::UnboundedSender<PortForwardCommand>,
    _thread: NonDetachingJoinHandle<()>,
}

/// Native nftables table owning all of StartOS's packet-filter / NAT rules.
/// Coexists with lxc-net / wg-quick, which keep their own iptables-nft rules in
/// separate tables on the shared nf_tables datapath.
pub const NFT_TABLE: &str = "startos";

/// Ensure `table ip startos` and its base chains exist. Idempotent (nft's `add
/// table`/`add chain` are no-ops if present). The forward chain defaults to
/// `drop` (replacing `iptables -P FORWARD DROP`); callers add ACCEPT rules.
pub async fn nft_ensure_base() -> Result<(), Error> {
    Command::new("nft")
        .arg(include_str!("startos-base.nft"))
        .invoke(ErrorKind::Network)
        .await?;
    Command::new("nft")
        .arg(include_str!("startos-base-v6.nft"))
        .invoke(ErrorKind::Network)
        .await?;
    Ok(())
}

async fn nft_list_chain(family: &str, chain: &str) -> Result<String, Error> {
    let out = Command::new("nft")
        .arg("-a")
        .arg("list")
        .arg("chain")
        .arg(family)
        .arg("startos")
        .arg(chain)
        .invoke(ErrorKind::Network)
        .await?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// Rules in `chain` tagged with `comment`, as `(handle, body)` where `body` is
/// the rule text preceding the `comment "..."` token.
async fn nft_rules_with_comment(
    family: &str,
    chain: &str,
    comment: &str,
) -> Result<Vec<(u32, String)>, Error> {
    let needle = format!("comment \"{comment}\"");
    Ok(nft_list_chain(family, chain)
        .await?
        .lines()
        .filter_map(|line| {
            let handle = line
                .rsplit_once("# handle ")?
                .1
                .trim()
                .parse::<u32>()
                .ok()?;
            let body = line.split_once(&needle)?.0.trim().to_owned();
            Some((handle, body))
        })
        .collect())
}

/// Comment tags in `chain` of `table ip startos` beginning with `prefix`. Used
/// to prune orphaned per-device/per-subnet rules whose owner no longer exists.
pub(crate) async fn nft_comments_with_prefix(
    chain: &str,
    prefix: &str,
) -> Result<Vec<String>, Error> {
    Ok(nft_list_chain("ip", chain)
        .await?
        .lines()
        .filter_map(|line| {
            let after = line.split_once("comment \"")?.1;
            let tag = after.split_once('"')?.0;
            tag.starts_with(prefix).then(|| tag.to_owned())
        })
        .collect())
}

/// Idempotently install (or, with `undo`, remove) the rule tagged `comment` in
/// `chain` of `table ip startos`, via one atomic nft transaction that drops
/// every prior copy of this comment and adds the desired rule. No-op when the
/// chain already holds exactly that rule. `prepend` inserts at the chain top
/// (needed for the mark-restore rule, which must precede the set-mark rules).
///
/// Retries when concurrent reconciliation invalidates a listed handle.
pub async fn nft_rule(
    chain: &str,
    comment: &str,
    undo: bool,
    prepend: bool,
    rule: &str,
) -> Result<(), Error> {
    nft_rule_family("ip", chain, comment, undo, prepend, rule).await
}

/// Like [`nft_rule`] but against `table ip6 startos` (the IPv6 base). Used for
/// the v6 forward-chain filter rules (established-accept, bridge egress).
pub async fn nft_rule_v6(
    chain: &str,
    comment: &str,
    undo: bool,
    prepend: bool,
    rule: &str,
) -> Result<(), Error> {
    nft_rule_family("ip6", chain, comment, undo, prepend, rule).await
}

async fn nft_rule_family(
    family: &str,
    chain: &str,
    comment: &str,
    undo: bool,
    prepend: bool,
    rule: &str,
) -> Result<(), Error> {
    nft_ensure_base().await?;

    const MAX_ATTEMPTS: usize = 5;
    let mut last_err = None;
    for attempt in 1..=MAX_ATTEMPTS {
        let existing = nft_rules_with_comment(family, chain, comment).await?;

        // Already converged: nothing to undo, or exactly the desired rule present.
        if undo {
            if existing.is_empty() {
                return Ok(());
            }
        } else if let [(_, body)] = existing.as_slice() {
            if body == rule {
                return Ok(());
            }
        }

        // Drop every prior copy, then add the desired rule, in one transaction:
        // no window where the rule is missing or duplicated.
        let mut script = String::new();
        for (handle, _) in &existing {
            writeln!(
                script,
                "delete rule {family} startos {chain} handle {handle}"
            )
            .unwrap();
        }
        if !undo {
            let verb = if prepend { "insert" } else { "add" };
            writeln!(
                script,
                "{verb} rule {family} startos {chain} {rule} comment \"{comment}\""
            )
            .unwrap();
        }
        if script.is_empty() {
            return Ok(());
        }

        match Command::new("nft")
            .arg(&script)
            .invoke(ErrorKind::Network)
            .await
        {
            Ok(_) => return Ok(()),
            // Stale handle: a concurrent reconcile won the race; re-read and
            // retry. Any other error is real and surfaces immediately.
            Err(e) if e.source.to_string().contains("No such file or directory") => {
                tracing::warn!(
                    "nft_rule {chain}/{comment}: stale handle on attempt {attempt}/{MAX_ATTEMPTS}"
                );
                last_err = Some(e);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_err.expect("loop only exits here via the stale-handle path, which sets last_err"))
}

impl PortForwardController {
    pub fn new() -> Self {
        let (req_send, mut req_recv) = mpsc::unbounded_channel::<PortForwardCommand>();
        let thread = NonDetachingJoinHandle::from(tokio::spawn(async move {
            while let Err(e) = async {
                nft_ensure_base().await?;
                nft_rule(
                    "forward",
                    "base-established",
                    false,
                    false,
                    "ct state established,related accept",
                )
                .await?;
                // Same for the v6 forward chain (drop policy) so reply packets of
                // a non-SSL GUA forward aren't dropped.
                nft_rule_v6(
                    "forward",
                    "base-established",
                    false,
                    false,
                    "ct state established,related accept",
                )
                .await?;
                Command::new("sysctl")
                    .arg("-w")
                    .arg("net.ipv4.ip_forward=1")
                    .invoke(ErrorKind::Network)
                    .await?;
                Command::new("sysctl")
                    .arg("-w")
                    .arg("net.ipv6.conf.all.forwarding=1")
                    .invoke(ErrorKind::Network)
                    .await?;
                Ok::<_, Error>(())
            }
            .await
            {
                tracing::error!(
                    "{}",
                    t!(
                        "net.forward.error-initializing-controller",
                        error = format!("{e:#}")
                    )
                );
                tracing::debug!("{e:?}");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            let mut state = PortForwardState::default();
            let mut gc_interval = tokio::time::interval_at(
                tokio::time::Instant::now() + PORT_FORWARD_GC_INTERVAL,
                PORT_FORWARD_GC_INTERVAL,
            );
            gc_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    cmd = req_recv.recv() => {
                        let Some(cmd) = cmd else {
                            break;
                        };
                        match cmd {
                            PortForwardCommand::AddForward {
                                source,
                                target,
                                count,
                                target_prefix,
                                src_filter,
                                respond,
                            } => {
                                let result = state
                                    .add_forward(source, target, count, target_prefix, src_filter)
                                    .await;
                                respond.send(result).ok();
                            }
                            PortForwardCommand::Gc { respond } => {
                                let result = state.gc().await;
                                respond.send(result).ok();
                            }
                            PortForwardCommand::Dump { respond } => {
                                respond.send(state.dump()).ok();
                            }
                        }
                    }
                    _ = gc_interval.tick() => {
                        let _ = state.gc().await;
                    }
                }
            }
        }));

        Self {
            req: req_send,
            _thread: thread,
        }
    }

    pub async fn add_forward(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        target_prefix: u8,
        src_filter: Option<IpNet>,
    ) -> Result<Arc<()>, Error> {
        self.add_forward_range(source, target, 1, target_prefix, src_filter)
            .await
    }

    /// Like [`add_forward`] but covers `count` contiguous ports per protocol
    /// (TCP + UDP) from `source.port()` / `target.port()`, mapped by offset
    /// (the two bases may differ).
    pub async fn add_forward_range(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        count: u16,
        target_prefix: u8,
        src_filter: Option<IpNet>,
    ) -> Result<Arc<()>, Error> {
        let (send, recv) = oneshot::channel();
        self.req
            .send(PortForwardCommand::AddForward {
                source,
                target,
                count,
                target_prefix,
                src_filter,
                respond: send,
            })
            .map_err(err_has_exited)?;

        recv.await.map_err(err_has_exited)?
    }

    pub async fn gc(&self) -> Result<(), Error> {
        let (send, recv) = oneshot::channel();
        self.req
            .send(PortForwardCommand::Gc { respond: send })
            .map_err(err_has_exited)?;

        recv.await.map_err(err_has_exited)?
    }

    pub async fn dump(&self) -> Result<BTreeMap<SocketAddrV4, SocketAddrV4>, Error> {
        let (send, recv) = oneshot::channel();
        self.req
            .send(PortForwardCommand::Dump { respond: send })
            .map_err(err_has_exited)?;

        recv.await.map_err(err_has_exited)
    }
}

pub(super) fn target_prefix_for(
    ip_info: &OrdMap<GatewayId, NetworkInterfaceInfo>,
    target: Ipv4Addr,
    fallback: u8,
) -> u8 {
    ip_info
        .iter()
        .filter_map(|(_, info)| info.ip_info.as_ref())
        .flat_map(|ip_info| ip_info.subnets.iter())
        .filter(|subnet| subnet.contains(&IpAddr::V4(target)))
        .map(IpNet::prefix_len)
        .max()
        .unwrap_or(fallback)
}

struct InterfaceForwardRequest {
    external: u16,
    target: SocketAddrV4,
    /// Contiguous ports from `external` / `target.port()` (port-preserving when
    /// the bases are equal, else an offset map).
    count: u16,
    target_prefix: u8,
    reqs: ForwardRequirements,
    rc: Arc<()>,
}

#[derive(Clone)]
struct InterfaceForwardEntry {
    external: u16,
    /// Shared across all targets at this `external` start — `AvailablePorts`
    /// prevents overlap, so a range and a single-port forward can't coexist here.
    count: u16,
    targets: BTreeMap<ForwardRequirements, (SocketAddrV4, u8, Weak<()>)>,
    forwards: BTreeMap<SocketAddrV4, Arc<()>>,
    // (local IP, external port) pairs we've asked the upstream gateway (via
    // PCP/NAT-PMP/UPnP) to forward here. Tracked so the mapping is withdrawn
    // when the forward is dropped; a range contributes one entry per port.
    mapped: BTreeSet<(Ipv4Addr, u16)>,
}

impl IdOrdItem for InterfaceForwardEntry {
    type Key<'a> = u16;
    fn key(&self) -> Self::Key<'_> {
        self.external
    }

    iddqd::id_upcast!();
}

impl InterfaceForwardEntry {
    fn new(external: u16, count: u16) -> Self {
        Self {
            external,
            count,
            targets: BTreeMap::new(),
            forwards: BTreeMap::new(),
            mapped: BTreeSet::new(),
        }
    }

    async fn update(
        &mut self,
        ip_info: &OrdMap<GatewayId, NetworkInterfaceInfo>,
        port_forward: &PortForwardController,
        pmap: &PortMapController,
    ) -> Result<(), Error> {
        let mut keep = BTreeSet::<SocketAddrV4>::new();
        // (local IP, external start) -> (port count, internal start, candidate
        // upstream gateways) to open upstream. The internal port is the target's,
        // so the gateway maps external->internal faithfully (e.g. an 80->443
        // redirect); it equals the external for ordinary port-preserving forwards.
        // Only public (WAN-facing) forwards need this; private subnets are already
        // reachable. A `count > 1` range is one PCP PORT_SET request (RFC 7753),
        // skipped on gateways without it (UPnP/NAT-PMP can't map ranges).
        let mut want = BTreeMap::<(Ipv4Addr, u16), (u16, u16, Vec<(IpAddr, Option<u32>)>)>::new();

        for (gw_id, info) in ip_info.iter() {
            if let Some(interface_ip_info) = &info.ip_info {
                for subnet in interface_ip_info.subnets.iter() {
                    if let IpAddr::V4(ip) = subnet.addr() {
                        let addr = SocketAddrV4::new(ip, self.external);
                        if keep.contains(&addr) {
                            continue;
                        }

                        for (reqs, (target, target_prefix, rc)) in self.targets.iter() {
                            if rc.strong_count() == 0 {
                                continue;
                            }

                            // The WAN is never secure: an insecure exposure is never public,
                            // so it still serves the LAN but never the public internet.
                            let public = reqs.public_gateways.contains(gw_id) && reqs.secure;
                            if !reqs.secure && !info.secure() {
                                continue;
                            }
                            let src_filter = if public {
                                None
                            } else if reqs.private_ips.contains(&IpAddr::V4(ip)) {
                                Some(subnet.trunc())
                            } else {
                                continue;
                            };

                            keep.insert(addr);
                            if public {
                                // The gateway forwards to the port StartOS listens
                                // on at its LAN IP (== external); our own nftables
                                // rule DNATs that to the container target locally.
                                let internal = self.external;
                                want.entry((ip, self.external)).or_insert_with(|| {
                                    let gws = candidate_gateways(info);
                                    tracing::debug!(
                                        "auto-port-mapping {ip}:{}->{internal} on gateway {gw_id} via {gws:?} (reqs {reqs})",
                                        self.external,
                                    );
                                    (self.count, internal, gws)
                                });
                            }
                            let live_target_prefix =
                                target_prefix_for(ip_info, *target.ip(), *target_prefix);
                            let fwd_rc = port_forward
                                .add_forward_range(
                                    addr,
                                    *target,
                                    self.count,
                                    live_target_prefix,
                                    src_filter,
                                )
                                .await?;
                            self.forwards.insert(addr, fwd_rc);
                            break;
                        }
                    }
                }
            }
        }

        // Dropping the strong refs lets PortForwardController gc the rules.
        self.forwards.retain(|addr, _| keep.contains(addr));

        for (ip, port) in self.mapped.iter().filter(|key| !want.contains_key(key)) {
            pmap.remove(IpAddr::V4(*ip), *port);
        }
        for ((ip, external), (count, internal, gateways)) in &want {
            if *count > 1 {
                pmap.ensure_range(
                    IpAddr::V4(*ip),
                    *external,
                    *internal,
                    *count,
                    gateways.clone(),
                );
            } else {
                pmap.ensure(IpAddr::V4(*ip), *external, *internal, gateways.clone());
            }
        }
        self.mapped = want.into_keys().collect();

        Ok(())
    }

    fn cache_target(
        &mut self,
        reqs: ForwardRequirements,
        target: SocketAddrV4,
        target_prefix: u8,
        mut rc: Arc<()>,
    ) -> Arc<()> {
        let entry = self
            .targets
            .entry(reqs)
            .or_insert_with(|| (target, target_prefix, Arc::downgrade(&rc)));
        if entry.0 != target || entry.1 != target_prefix {
            entry.0 = target;
            entry.1 = target_prefix;
            entry.2 = Arc::downgrade(&rc);
        }
        if let Some(existing) = entry.2.upgrade() {
            rc = existing;
        } else {
            entry.2 = Arc::downgrade(&rc);
        }
        rc
    }

    async fn update_request(
        &mut self,
        InterfaceForwardRequest {
            external,
            target,
            count,
            target_prefix,
            reqs,
            rc,
        }: InterfaceForwardRequest,
        ip_info: &OrdMap<GatewayId, NetworkInterfaceInfo>,
        port_forward: &PortForwardController,
        pmap: &PortMapController,
    ) -> Result<Arc<()>, Error> {
        if external != self.external {
            return Err(Error::new(
                eyre!("{}", t!("net.forward.mismatched-external-port")),
                ErrorKind::InvalidRequest,
            ));
        }
        if count != self.count {
            // A resize, or a single-port forward and a range swapped at this
            // reused start port. The nft chain name encodes the count, so rebuild
            // from scratch; `state` entries are never evicted, so otherwise a
            // count change here would be a hard error until restart.
            self.count = count;
            self.targets.clear();
            self.forwards.clear();
        }

        let rc = self.cache_target(reqs, target, target_prefix, rc);

        self.update(ip_info, port_forward, pmap).await.log_err();

        Ok(rc)
    }

    async fn gc(
        &mut self,
        ip_info: &OrdMap<GatewayId, NetworkInterfaceInfo>,
        port_forward: &PortForwardController,
        pmap: &PortMapController,
    ) -> Result<(), Error> {
        self.targets.retain(|_, (_, _, rc)| rc.strong_count() > 0);

        self.update(ip_info, port_forward, pmap).await
    }
}

struct InterfaceForwardState {
    port_forward: PortForwardController,
    pmap: PortMapController,
    state: IdOrdMap<InterfaceForwardEntry>,
    ipv6: BTreeMap<SocketAddrV6, Ipv6ForwardMapping>,
}

impl InterfaceForwardState {
    fn new(port_forward: PortForwardController, pmap: PortMapController) -> Self {
        Self {
            port_forward,
            pmap,
            state: IdOrdMap::new(),
            ipv6: BTreeMap::new(),
        }
    }
}

impl InterfaceForwardState {
    async fn handle_request(
        &mut self,
        request: InterfaceForwardRequest,
        ip_info: &OrdMap<GatewayId, NetworkInterfaceInfo>,
    ) -> Result<Arc<()>, Error> {
        let count = request.count;
        self.state
            .entry(request.external)
            .or_insert_with(|| InterfaceForwardEntry::new(request.external, count))
            .update_request(request, ip_info, &self.port_forward, &self.pmap)
            .await
    }

    async fn add_forward6(&mut self, source: SocketAddrV6, spec: Ipv6ForwardSpec) -> Arc<()> {
        let rc = self
            .ipv6
            .get(&source)
            .filter(|mapping| mapping.desired == spec)
            .and_then(|mapping| mapping.rc.upgrade())
            .unwrap_or_else(|| Arc::new(()));
        let mapping = self
            .ipv6
            .entry(source)
            .or_insert_with(|| Ipv6ForwardMapping {
                desired: spec.clone(),
                applied: None,
                rc: Arc::downgrade(&rc),
            });
        mapping.desired = spec;
        mapping.rc = Arc::downgrade(&rc);
        if let Err(error) = self.reconcile_forward6(source).await {
            tracing::error!("failed to reconcile IPv6 forward {source}: {error}");
            tracing::debug!("{error:?}");
        }
        rc
    }

    async fn reconcile_forward6(&mut self, source: SocketAddrV6) -> Result<(), Error> {
        let Some(mapping) = self.ipv6.get_mut(&source) else {
            return Ok(());
        };
        while let Some(operation) = mapping.next_operation() {
            match &operation {
                Ipv6ForwardOperation::Add(spec) => {
                    forward6(
                        source,
                        spec.target,
                        spec.target_prefix,
                        spec.src_filter.as_ref(),
                    )
                    .await?;
                    if spec.src_filter.is_none() {
                        self.pmap.ensure(
                            IpAddr::V6(*source.ip()),
                            source.port(),
                            source.port(),
                            spec.gateways.clone(),
                        );
                    }
                }
                Ipv6ForwardOperation::Remove(spec) => {
                    unforward6(
                        source,
                        spec.target,
                        spec.target_prefix,
                        spec.src_filter.as_ref(),
                    )
                    .await?;
                    if spec.src_filter.is_none() {
                        self.pmap.remove(IpAddr::V6(*source.ip()), source.port());
                    }
                }
            }
            mapping.operation_succeeded(operation);
        }
        Ok(())
    }

    async fn reconcile_forwards6(&mut self) {
        let sources: Vec<_> = self.ipv6.keys().copied().collect();
        for source in sources {
            if let Err(error) = self.reconcile_forward6(source).await {
                tracing::error!("failed to reconcile IPv6 forward {source}: {error}");
                tracing::debug!("{error:?}");
            }
        }
        self.ipv6
            .retain(|_, mapping| mapping.rc.strong_count() > 0 || mapping.applied.is_some());
    }

    async fn reconcile(
        &mut self,
        ip_info: &OrdMap<GatewayId, NetworkInterfaceInfo>,
    ) -> Result<(), Error> {
        let mut first_error = None;
        for mut entry in self.state.iter_mut() {
            if let Err(error) = entry.gc(ip_info, &self.port_forward, &self.pmap).await {
                first_error.get_or_insert(error);
            }
        }
        self.reconcile_forwards6().await;
        first_error.map_or(Ok(()), Err)
    }

    async fn sync(
        &mut self,
        ip_info: &OrdMap<GatewayId, NetworkInterfaceInfo>,
    ) -> Result<(), Error> {
        let mut first_error = self.reconcile(ip_info).await.err();
        if let Err(error) = self.port_forward.gc().await {
            first_error.get_or_insert(error);
        }
        first_error.map_or(Ok(()), Err)
    }
}

impl Drop for InterfaceForwardState {
    fn drop(&mut self) {
        let applied = std::mem::take(&mut self.ipv6)
            .into_iter()
            .filter_map(|(source, mapping)| mapping.applied.map(|spec| (source, spec)))
            .collect::<Vec<_>>();
        if !applied.is_empty() {
            let pmap = self.pmap.clone();
            tokio::spawn(async move {
                for (source, spec) in applied {
                    if unforward6(
                        source,
                        spec.target,
                        spec.target_prefix,
                        spec.src_filter.as_ref(),
                    )
                    .await
                    .log_err()
                    .is_some()
                        && spec.src_filter.is_none()
                    {
                        pmap.remove(IpAddr::V6(*source.ip()), source.port());
                    }
                }
            });
        }
    }
}

fn err_has_exited<T>(_: T) -> Error {
    Error::new(
        eyre!("{}", t!("net.forward.controller-thread-exited")),
        ErrorKind::Unknown,
    )
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ForwardTable(pub BTreeMap<u16, ForwardTarget>);

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ForwardTarget {
    pub target: SocketAddrV4,
    pub target_prefix: u8,
    pub reqs: String,
}

impl From<&InterfaceForwardState> for ForwardTable {
    fn from(value: &InterfaceForwardState) -> Self {
        Self(
            value
                .state
                .iter()
                .flat_map(|entry| {
                    entry
                        .targets
                        .iter()
                        .filter(|(_, (_, _, rc))| rc.strong_count() > 0)
                        .map(|(reqs, (target, target_prefix, _))| {
                            (
                                entry.external,
                                ForwardTarget {
                                    target: *target,
                                    target_prefix: *target_prefix,
                                    reqs: format!("{reqs}"),
                                },
                            )
                        })
                })
                .collect(),
        )
    }
}

enum InterfaceForwardCommand {
    Forward(
        InterfaceForwardRequest,
        oneshot::Sender<Result<Arc<()>, Error>>,
    ),
    Forward6 {
        source: SocketAddrV6,
        spec: Ipv6ForwardSpec,
        respond: oneshot::Sender<Arc<()>>,
    },
    Sync(oneshot::Sender<Result<(), Error>>),
    DumpTable(oneshot::Sender<ForwardTable>),
}

pub struct InterfacePortForwardController {
    req: mpsc::UnboundedSender<InterfaceForwardCommand>,
    _thread: NonDetachingJoinHandle<()>,
}

impl InterfacePortForwardController {
    pub fn new(
        mut ip_info: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
        pmap: PortMapController,
    ) -> Self {
        let port_forward = PortForwardController::new();

        let (req_send, mut req_recv) = mpsc::unbounded_channel::<InterfaceForwardCommand>();
        let thread = NonDetachingJoinHandle::from(tokio::spawn(async move {
            let mut state = InterfaceForwardState::new(port_forward, pmap);
            let mut interfaces = ip_info.read_and_mark_seen();
            let mut reconcile_interval = tokio::time::interval_at(
                tokio::time::Instant::now() + PORT_FORWARD_GC_INTERVAL,
                PORT_FORWARD_GC_INTERVAL,
            );
            reconcile_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    msg = req_recv.recv() => {
                        if let Some(cmd) = msg {
                            match cmd {
                                InterfaceForwardCommand::Forward(req, re) => {
                                    re.send(state.handle_request(req, &interfaces).await).ok()
                                }
                                InterfaceForwardCommand::Forward6 { source, spec, respond } => {
                                    respond.send(state.add_forward6(source, spec).await).ok()
                                }
                                InterfaceForwardCommand::Sync(re) => {
                                    re.send(state.sync(&interfaces).await).ok()
                                }
                                InterfaceForwardCommand::DumpTable(re) => {
                                    re.send((&state).into()).ok()
                                }
                            };
                        } else {
                            break;
                        }
                    }
                    _ = ip_info.changed() => {
                        interfaces = ip_info.read();
                        state.sync(&interfaces).await.log_err();
                    }
                    _ = reconcile_interval.tick() => {
                        state.reconcile(&interfaces).await.log_err();
                    }
                }
            }
        }));

        Self {
            req: req_send,
            _thread: thread,
        }
    }

    pub async fn forward6(
        &self,
        source: SocketAddrV6,
        target: SocketAddrV6,
        target_prefix: u8,
        src_filter: Option<IpNet>,
        gateways: Vec<(IpAddr, Option<u32>)>,
    ) -> Result<Arc<()>, Error> {
        let (respond, receive) = oneshot::channel();
        self.req
            .send(InterfaceForwardCommand::Forward6 {
                source,
                spec: Ipv6ForwardSpec {
                    target,
                    target_prefix,
                    src_filter,
                    gateways,
                },
                respond,
            })
            .map_err(err_has_exited)?;
        receive.await.map_err(err_has_exited)
    }

    pub async fn add(
        &self,
        external: u16,
        reqs: ForwardRequirements,
        target: SocketAddrV4,
        target_prefix: u8,
    ) -> Result<Arc<()>, Error> {
        self.add_range(external, 1, reqs, target, target_prefix)
            .await
    }

    /// Add a `count`-port contiguous forward from `external` / `target.port()`.
    /// `count == 1` equals [`add`]; for `count > 1` the bases may differ
    /// (offset-mapped).
    pub async fn add_range(
        &self,
        external: u16,
        count: u16,
        reqs: ForwardRequirements,
        target: SocketAddrV4,
        target_prefix: u8,
    ) -> Result<Arc<()>, Error> {
        let rc = Arc::new(());
        let (send, recv) = oneshot::channel();
        self.req
            .send(InterfaceForwardCommand::Forward(
                InterfaceForwardRequest {
                    external,
                    target,
                    count,
                    target_prefix,
                    reqs,
                    rc,
                },
                send,
            ))
            .map_err(err_has_exited)?;

        recv.await.map_err(err_has_exited)?
    }

    pub async fn gc(&self) -> Result<(), Error> {
        let (send, recv) = oneshot::channel();
        self.req
            .send(InterfaceForwardCommand::Sync(send))
            .map_err(err_has_exited)?;

        recv.await.map_err(err_has_exited)?
    }

    pub async fn dump_table(&self) -> Result<ForwardTable, Error> {
        let (req, res) = oneshot::channel();
        self.req
            .send(InterfaceForwardCommand::DumpTable(req))
            .map_err(err_has_exited)?;
        res.await.map_err(err_has_exited)
    }
}

async fn forward(
    source: SocketAddrV4,
    target: SocketAddrV4,
    count: u16,
    target_prefix: u8,
    src_filter: Option<&IpNet>,
) -> Result<(), Error> {
    let mut cmd = Command::new("/usr/lib/startos/scripts/forward-port");
    cmd.env("sip", source.ip().to_string())
        .env("dip", target.ip().to_string())
        .env("dprefix", target_prefix.to_string())
        .env("sport", source.port().to_string())
        .env("dport", target.port().to_string())
        .env("count", count.to_string());
    if let Some(subnet) = src_filter {
        cmd.env("src_subnet", subnet.to_string());
    }
    cmd.invoke(ErrorKind::Network).await?;
    Ok(())
}

async fn unforward(
    source: SocketAddrV4,
    target: SocketAddrV4,
    count: u16,
    target_prefix: u8,
    src_filter: Option<&IpNet>,
) -> Result<(), Error> {
    let mut cmd = Command::new("/usr/lib/startos/scripts/forward-port");
    cmd.env("UNDO", "1")
        .env("sip", source.ip().to_string())
        .env("dip", target.ip().to_string())
        .env("dprefix", target_prefix.to_string())
        .env("sport", source.port().to_string())
        .env("dport", target.port().to_string())
        .env("count", count.to_string());
    if let Some(subnet) = src_filter {
        cmd.env("src_subnet", subnet.to_string());
    }
    cmd.invoke(ErrorKind::Network).await?;
    Ok(())
}

/// The lxcbr0 IPv6 bridge subnet (a ULA), assigned by lxc-net (see
/// `debian/postinst`). Containers get a SLAAC address in it.
pub(crate) const START9_BRIDGE_V6_SUBNET: &str = "fd00:3::/64";

/// IPv6 counterpart of [`forward`]: DNAT `source` (a host GUA:port) to `target`
/// (the container's ULA:port) via the `forward-port6` script. `src_filter`
/// restricts inbound to a LAN v6 subnet (a LAN-only GUA); `None` is WAN.
pub(crate) async fn forward6(
    source: SocketAddrV6,
    target: SocketAddrV6,
    target_prefix: u8,
    src_filter: Option<&IpNet>,
) -> Result<(), Error> {
    let mut cmd = Command::new("/usr/lib/startos/scripts/forward-port6");
    cmd.env("sip", source.ip().to_string())
        .env("dip", target.ip().to_string())
        .env("dprefix", target_prefix.to_string())
        .env("sport", source.port().to_string())
        .env("dport", target.port().to_string())
        .env("bridge_subnet", START9_BRIDGE_V6_SUBNET);
    if let Some(subnet) = src_filter {
        cmd.env("src_subnet", subnet.to_string());
    }
    cmd.invoke(ErrorKind::Network).await?;
    Ok(())
}

/// Tear down a forward created by [`forward6`]. Passes the same identifying env
/// so the script recomputes the matching comment tag.
pub(crate) async fn unforward6(
    source: SocketAddrV6,
    target: SocketAddrV6,
    target_prefix: u8,
    src_filter: Option<&IpNet>,
) -> Result<(), Error> {
    let mut cmd = Command::new("/usr/lib/startos/scripts/forward-port6");
    cmd.env("UNDO", "1")
        .env("sip", source.ip().to_string())
        .env("dip", target.ip().to_string())
        .env("dprefix", target_prefix.to_string())
        .env("sport", source.port().to_string())
        .env("dport", target.port().to_string());
    if let Some(subnet) = src_filter {
        cmd.env("src_subnet", subnet.to_string());
    }
    cmd.invoke(ErrorKind::Network).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ipv6_spec(gateways: Vec<(IpAddr, Option<u32>)>) -> Ipv6ForwardSpec {
        Ipv6ForwardSpec {
            target: SocketAddrV6::new("fd00:3::2".parse().unwrap(), 8080, 0, 0),
            target_prefix: 64,
            src_filter: None,
            gateways,
        }
    }

    #[test]
    fn ipv6_mapping_identity_includes_gateways() {
        let first = ipv6_spec(vec![("2001:db8::1".parse().unwrap(), None)]);
        let changed = ipv6_spec(vec![("2001:db8::2".parse().unwrap(), None)]);

        assert_ne!(first, changed);
    }

    #[test]
    fn ipv6_add_and_replacement_advance_only_after_success() {
        let lease = Arc::new(());
        let original = ipv6_spec(Vec::new());
        let replacement = Ipv6ForwardSpec {
            target: SocketAddrV6::new("fd00:3::3".parse().unwrap(), 8080, 0, 0),
            ..original.clone()
        };
        let mut mapping = Ipv6ForwardMapping {
            desired: original.clone(),
            applied: None,
            rc: Arc::downgrade(&lease),
        };

        let add = Ipv6ForwardOperation::Add(original.clone());
        assert_eq!(mapping.next_operation(), Some(add.clone()));
        assert_eq!(mapping.next_operation(), Some(add.clone()));
        mapping.operation_succeeded(add);
        assert_eq!(mapping.next_operation(), None);

        mapping.desired = replacement.clone();
        let remove = Ipv6ForwardOperation::Remove(original);
        assert_eq!(mapping.next_operation(), Some(remove.clone()));
        assert_eq!(mapping.next_operation(), Some(remove.clone()));
        mapping.operation_succeeded(remove);

        let add_replacement = Ipv6ForwardOperation::Add(replacement);
        assert_eq!(mapping.next_operation(), Some(add_replacement.clone()));
        mapping.operation_succeeded(add_replacement);
        assert_eq!(mapping.next_operation(), None);
    }

    #[test]
    fn dropped_ipv6_lease_keeps_teardown_pending_until_success() {
        let lease = Arc::new(());
        let spec = ipv6_spec(Vec::new());
        let mut mapping = Ipv6ForwardMapping {
            desired: spec.clone(),
            applied: Some(spec.clone()),
            rc: Arc::downgrade(&lease),
        };
        assert_eq!(mapping.next_operation(), None);

        drop(lease);

        let remove = Ipv6ForwardOperation::Remove(spec);
        assert_eq!(mapping.next_operation(), Some(remove.clone()));
        assert_eq!(mapping.next_operation(), Some(remove.clone()));
        mapping.operation_succeeded(remove);
        assert_eq!(mapping.next_operation(), None);
    }

    #[test]
    fn mapping_identity_includes_target_prefix() {
        let source = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 80);
        let target = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 2), 8080);
        let mapping = ForwardMapping {
            source,
            target,
            count: 1,
            target_prefix: 24,
            src_filter: None,
            rc: Weak::new(),
        };

        assert!(mapping.matches(target, 1, 24, None));
        assert!(!mapping.matches(target, 1, 32, None));
    }

    #[test]
    fn live_target_prefix_tracks_interface_changes() {
        use imbl::OrdSet;
        use imbl_value::InternedString;

        use crate::db::model::public::IpInfo;

        let interfaces = |entries: &[(&str, &str)]| {
            entries
                .iter()
                .map(|(gateway, subnet)| {
                    let subnets: OrdSet<IpNet> =
                        [subnet.parse::<IpNet>().unwrap()].into_iter().collect();
                    (
                        GatewayId::from(InternedString::intern(*gateway)),
                        NetworkInterfaceInfo {
                            ip_info: Some(Arc::new(IpInfo {
                                subnets,
                                ..Default::default()
                            })),
                            ..Default::default()
                        },
                    )
                })
                .collect()
        };
        let target = Ipv4Addr::new(10, 0, 0, 2);

        assert_eq!(
            target_prefix_for(&interfaces(&[("eth0", "10.0.0.0/24")]), target, 32),
            24
        );
        assert_eq!(
            target_prefix_for(&interfaces(&[("eth0", "10.0.0.0/16")]), target, 32),
            16
        );
        assert_eq!(
            target_prefix_for(
                &interfaces(&[("eth0", "10.0.0.0/16"), ("eth1", "10.0.0.0/24")]),
                target,
                32,
            ),
            24
        );
        assert_eq!(target_prefix_for(&OrdMap::new(), target, 32), 32);
    }

    #[test]
    fn cached_target_identity_includes_target_prefix() {
        let requirements = ForwardRequirements {
            public_gateways: BTreeSet::new(),
            private_ips: BTreeSet::new(),
            secure: true,
        };
        let target = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 2), 8080);
        let mut entry = InterfaceForwardEntry::new(80, 1);
        let first = entry.cache_target(requirements.clone(), target, 24, Arc::new(()));
        let replacement = Arc::new(());
        let updated = entry.cache_target(requirements.clone(), target, 32, replacement.clone());

        assert!(!Arc::ptr_eq(&first, &updated));
        assert!(Arc::ptr_eq(&replacement, &updated));
        assert_eq!(entry.targets[&requirements].1, 32);
    }

    #[test]
    fn try_alloc_range_basic() {
        let mut ports = AvailablePorts::new();
        assert!(ports.try_alloc_range(40000, 100, false).is_ok());
        // All 100 ports should now be allocated
        for p in 40000..40100 {
            assert!(
                ports.try_alloc(p, false, false).is_none(),
                "port {p} should be taken"
            );
        }
        assert!(ports.try_alloc(40100, false, false).is_some());
    }

    #[test]
    fn try_alloc_range_zero_count_is_error() {
        let mut ports = AvailablePorts::new();
        assert!(ports.try_alloc_range(40000, 0, false).is_err());
    }

    #[test]
    fn try_alloc_range_overflow_is_error() {
        let mut ports = AvailablePorts::new();
        assert!(ports.try_alloc_range(65500, 100, false).is_err());
    }

    #[test]
    fn try_alloc_range_restricted_port_is_error_and_atomic() {
        let mut ports = AvailablePorts::new();
        // Range straddling a restricted port (1024 and below) hard-fails…
        assert!(ports.try_alloc_range(1020, 10, false).is_err());
        // …and nothing was allocated.
        assert!(ports.try_alloc(2000, false, false).is_some());
        ports.free([2000]);
        assert!(ports.try_alloc_range(1020, 10, false).is_err());
        for p in 1020..1030 {
            // None of them are reserved either
            if may_claim(p, false) {
                assert!(
                    ports.try_alloc(p, false, false).is_some(),
                    "port {p} unexpectedly taken"
                );
            }
        }
    }

    #[test]
    fn try_alloc_range_collision_is_error_and_atomic() {
        let mut ports = AvailablePorts::new();
        ports.try_alloc(40050, false, false).unwrap();
        assert!(ports.try_alloc_range(40000, 100, false).is_err());
        // Other ports in the requested range were NOT allocated as a side effect.
        assert!(ports.try_alloc(40000, false, false).is_some());
        assert!(ports.try_alloc(40099, false, false).is_some());
    }

    #[test]
    fn only_the_os_may_claim_privileged_ports() {
        let mut ports = AvailablePorts::new();
        assert!(ports.try_alloc(443, true, false).is_none());
        assert_eq!(ports.try_alloc(443, true, true), Some(443));
        assert_eq!(ports.try_alloc(80, false, true), Some(80));
        // …but a claim is still a claim: the OS holds it against everyone.
        assert!(ports.try_alloc(443, true, true).is_none());

        // A host daemon's port is nobody's to take, root or not.
        assert!(ports.try_alloc(5432, false, true).is_none());
        assert!(ports.try_alloc_range(1020, 10, true).is_ok());
    }
}
