use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write;
use std::future::Future;
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
use tokio_util::sync::CancellationToken;

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
const FORWARD_SCRIPT_TIMEOUT: Duration = Duration::from_secs(30);
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

#[derive(Clone)]
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
    fn next_operation(&self) -> Option<Ipv6ForwardOperation> {
        let live = self.rc.strong_count() > 0;
        match &self.applied {
            Some(applied) if !live || &self.desired != applied => {
                Some(Ipv6ForwardOperation::Remove(applied.clone()))
            }
            None if live => Some(Ipv6ForwardOperation::Add(self.desired.clone())),
            _ => None,
        }
    }

    fn operation_started(&mut self, operation: &Ipv6ForwardOperation) {
        if let Ipv6ForwardOperation::Add(spec) = operation {
            self.applied = Some(spec.clone());
        }
    }

    fn operation_failed(&mut self, operation: &Ipv6ForwardOperation) {
        if matches!(operation, Ipv6ForwardOperation::Add(_)) {
            self.applied = None;
        }
    }

    fn operation_succeeded(&mut self, operation: Ipv6ForwardOperation) {
        if matches!(operation, Ipv6ForwardOperation::Remove(_)) {
            self.applied = None;
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

    fn dump(&self) -> BTreeMap<SocketAddrV4, ForwardMapping> {
        self.mappings
            .iter()
            .filter(|(_, mapping)| mapping.rc.strong_count() > 0)
            .map(|(source, mapping)| (*source, mapping.clone()))
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
        respond: oneshot::Sender<BTreeMap<SocketAddrV4, ForwardMapping>>,
    },
    Drain {
        respond: oneshot::Sender<Result<(), Error>>,
    },
}

impl PortForwardCommand {
    fn response_is_closed(&self) -> bool {
        match self {
            Self::AddForward { respond, .. } => respond.is_canceled(),
            Self::Gc { respond } | Self::Drain { respond } => respond.is_canceled(),
            Self::Dump { respond } => respond.is_canceled(),
        }
    }
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

/// Converges the tagged rule, retrying stale handles.
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

async fn initialize_port_forwarding() -> Result<(), Error> {
    nft_ensure_base().await?;
    nft_rule(
        "forward",
        "base-established",
        false,
        false,
        "ct state established,related accept",
    )
    .await?;
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
    Ok(())
}

impl PortForwardController {
    pub fn new() -> Self {
        Self::spawn(initialize_port_forwarding)
    }

    fn spawn<F, Fut>(mut initialize: F) -> Self
    where
        F: FnMut() -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), Error>> + Send + 'static,
    {
        let (req_send, mut req_recv) = mpsc::unbounded_channel::<PortForwardCommand>();
        let thread = NonDetachingJoinHandle::from(tokio::spawn(async move {
            let mut pending = VecDeque::new();
            'initialize: loop {
                let initialization = initialize();
                tokio::pin!(initialization);
                let error = loop {
                    tokio::select! {
                        result = &mut initialization => match result {
                            Ok(()) => break 'initialize,
                            Err(error) => break error,
                        },
                        cmd = req_recv.recv() => match cmd {
                            Some(PortForwardCommand::Drain { respond }) => {
                                respond.send(Ok(())).ok();
                            }
                            Some(cmd) => pending.push_back(cmd),
                            None => return,
                        },
                    }
                };
                tracing::error!(
                    "{}",
                    t!(
                        "net.forward.error-initializing-controller",
                        error = format!("{error:#}")
                    )
                );
                tracing::debug!("{error:?}");
                let retry = tokio::time::sleep(Duration::from_secs(5));
                tokio::pin!(retry);
                loop {
                    tokio::select! {
                        _ = &mut retry => break,
                        cmd = req_recv.recv() => match cmd {
                            Some(PortForwardCommand::Drain { respond }) => {
                                respond.send(Ok(())).ok();
                            }
                            Some(cmd) => pending.push_back(cmd),
                            None => return,
                        },
                    }
                }
            }

            let mut state = PortForwardState::default();
            let mut gc_interval = tokio::time::interval_at(
                tokio::time::Instant::now() + PORT_FORWARD_GC_INTERVAL,
                PORT_FORWARD_GC_INTERVAL,
            );
            gc_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                let cmd = if let Some(cmd) = pending.pop_front() {
                    Some(cmd)
                } else {
                    tokio::select! {
                        cmd = req_recv.recv() => cmd,
                        _ = gc_interval.tick() => {
                            state.gc().await.log_err();
                            continue;
                        }
                    }
                };
                let Some(cmd) = cmd else {
                    break;
                };
                if cmd.response_is_closed() {
                    continue;
                }
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
                    PortForwardCommand::Gc { respond } | PortForwardCommand::Drain { respond } => {
                        respond.send(state.gc().await).ok();
                    }
                    PortForwardCommand::Dump { respond } => {
                        respond.send(state.dump()).ok();
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

    async fn drain(&self) -> Result<(), Error> {
        let (send, recv) = oneshot::channel();
        self.req
            .send(PortForwardCommand::Drain { respond: send })
            .map_err(err_has_exited)?;

        recv.await.map_err(err_has_exited)?
    }

    async fn dump(&self) -> Result<BTreeMap<SocketAddrV4, ForwardMapping>, Error> {
        let (send, recv) = oneshot::channel();
        self.req
            .send(PortForwardCommand::Dump { respond: send })
            .map_err(err_has_exited)?;

        recv.await.map_err(err_has_exited)
    }
}

fn ipv6_candidate_gateways(
    ip_info: &OrdMap<GatewayId, NetworkInterfaceInfo>,
    source: std::net::Ipv6Addr,
) -> Vec<(IpAddr, Option<u32>)> {
    ip_info
        .values()
        .find(|info| {
            info.ip_info.as_ref().is_some_and(|interface| {
                interface
                    .subnets
                    .iter()
                    .any(|subnet| subnet.addr() == IpAddr::V6(source))
            })
        })
        .map(candidate_gateways)
        .unwrap_or_default()
        .into_iter()
        .filter(|(gateway, _)| gateway.is_ipv6())
        .collect()
}

fn target_prefix_for(
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
    target_prefix_fallback: u8,
    reqs: ForwardRequirements,
    rc: Arc<()>,
}

pub(super) struct ForwardLease {
    pub(super) lease: Arc<()>,
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
            target_prefix_fallback,
            reqs,
            rc,
        }: InterfaceForwardRequest,
    ) -> Result<ForwardLease, Error> {
        if external != self.external {
            return Err(Error::new(
                eyre!("{}", t!("net.forward.mismatched-external-port")),
                ErrorKind::InvalidRequest,
            ));
        }
        if count != self.count {
            // The nft chain name encodes the count.
            self.count = count;
            self.targets.clear();
            self.forwards.clear();
        }

        let rc = self.cache_target(reqs, target, target_prefix_fallback, rc);

        Ok(ForwardLease { lease: rc })
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
    ) -> Result<ForwardLease, Error> {
        let count = request.count;
        self.state
            .entry(request.external)
            .or_insert_with(|| InterfaceForwardEntry::new(request.external, count))
            .update_request(request)
            .await
    }

    async fn add_forward6(&mut self, source: SocketAddrV6, spec: Ipv6ForwardSpec) -> ForwardLease {
        let rc = self
            .ipv6
            .get(&source)
            .filter(|mapping| mapping.desired == spec)
            .and_then(|mapping| mapping.rc.upgrade())
            .unwrap_or_else(|| Arc::new(()));
        match self.ipv6.entry(source) {
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                entry.get_mut().desired = spec;
                entry.get_mut().rc = Arc::downgrade(&rc);
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(Ipv6ForwardMapping {
                    desired: spec,
                    applied: None,
                    rc: Arc::downgrade(&rc),
                });
            }
        }
        ForwardLease { lease: rc }
    }

    async fn reconcile_forward6(
        &mut self,
        source: SocketAddrV6,
        ip_info: &OrdMap<GatewayId, NetworkInterfaceInfo>,
    ) -> Result<(), Error> {
        let Some(mapping) = self.ipv6.get_mut(&source) else {
            return Ok(());
        };
        while let Some(operation) = mapping.next_operation() {
            mapping.operation_started(&operation);
            let result = match &operation {
                Ipv6ForwardOperation::Add(spec) => {
                    forward6(
                        source,
                        spec.target,
                        spec.target_prefix,
                        spec.src_filter.as_ref(),
                    )
                    .await
                }
                Ipv6ForwardOperation::Remove(spec) => {
                    unforward6(
                        source,
                        spec.target,
                        spec.target_prefix,
                        spec.src_filter.as_ref(),
                    )
                    .await
                }
            };
            if let Err(error) = result {
                mapping.operation_failed(&operation);
                return Err(error);
            }
            if matches!(&operation, Ipv6ForwardOperation::Remove(spec) if spec.src_filter.is_none())
            {
                self.pmap.remove(IpAddr::V6(*source.ip()), source.port());
            }
            mapping.operation_succeeded(operation);
        }
        if mapping
            .applied
            .as_ref()
            .is_some_and(|spec| spec.src_filter.is_none())
        {
            self.pmap.ensure(
                IpAddr::V6(*source.ip()),
                source.port(),
                source.port(),
                ipv6_candidate_gateways(ip_info, *source.ip()),
            );
        }
        Ok(())
    }

    async fn reconcile_forwards6(
        &mut self,
        ip_info: &OrdMap<GatewayId, NetworkInterfaceInfo>,
    ) -> Result<(), Error> {
        let sources: Vec<_> = self.ipv6.keys().copied().collect();
        let mut first_error = None;
        for source in sources {
            if let Err(error) = self.reconcile_forward6(source, ip_info).await {
                first_error.get_or_insert(error);
            }
        }
        self.ipv6
            .retain(|_, mapping| mapping.rc.strong_count() > 0 || mapping.applied.is_some());
        first_error.map_or(Ok(()), Err)
    }

    async fn reconcile(
        &mut self,
        ip_info: &OrdMap<GatewayId, NetworkInterfaceInfo>,
    ) -> Result<(), Error> {
        let mut first_error = None;
        let mut empty = Vec::new();
        for mut entry in self.state.iter_mut() {
            match entry.gc(ip_info, &self.port_forward, &self.pmap).await {
                Ok(())
                    if entry.targets.is_empty()
                        && entry.forwards.is_empty()
                        && entry.mapped.is_empty() =>
                {
                    empty.push(entry.external);
                }
                Ok(()) => {}
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }
        for external in empty {
            self.state.remove(&external);
        }
        if let Err(error) = self.reconcile_forwards6(ip_info).await {
            first_error.get_or_insert(error);
        }
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

impl InterfaceForwardState {
    async fn drain(&mut self) -> Result<(), Error> {
        self.state.clear();
        let mut first_error = self.port_forward.drain().await.err();
        for mapping in self.ipv6.values_mut() {
            mapping.rc = Weak::new();
        }
        let sources = self.ipv6.keys().copied().collect::<Vec<_>>();
        for source in sources {
            let Some(spec) = self
                .ipv6
                .get(&source)
                .and_then(|mapping| mapping.applied.clone())
            else {
                continue;
            };
            match unforward6(
                source,
                spec.target,
                spec.target_prefix,
                spec.src_filter.as_ref(),
            )
            .await
            {
                Ok(()) => self.ipv6.get_mut(&source).unwrap().applied = None,
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }
        self.ipv6.retain(|_, mapping| mapping.applied.is_some());
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

impl ForwardTable {
    fn from_state(
        state: &InterfaceForwardState,
        applied: &BTreeMap<SocketAddrV4, ForwardMapping>,
    ) -> Self {
        Self(
            state
                .state
                .iter()
                .flat_map(|entry| {
                    entry.targets.iter().filter_map(|(reqs, (target, _, rc))| {
                        let applied = applied.values().find(|mapping| {
                            mapping.source.port() == entry.external && mapping.target == *target
                        })?;
                        (rc.strong_count() > 0).then(|| {
                            (
                                entry.external,
                                ForwardTarget {
                                    target: *target,
                                    target_prefix: applied.target_prefix,
                                    reqs: format!("{reqs}"),
                                },
                            )
                        })
                    })
                })
                .collect(),
        )
    }
}

enum InterfaceForwardCommand {
    Forward(
        InterfaceForwardRequest,
        oneshot::Sender<Result<ForwardLease, Error>>,
    ),
    Forward6 {
        source: SocketAddrV6,
        spec: Ipv6ForwardSpec,
        respond: oneshot::Sender<ForwardLease>,
    },
    Sync(oneshot::Sender<Result<(), Error>>),
    DumpTable(oneshot::Sender<Result<ForwardTable, Error>>),
    Drain(oneshot::Sender<Result<(), Error>>),
}

pub struct InterfacePortForwardController {
    req: mpsc::UnboundedSender<InterfaceForwardCommand>,
    cancel: CancellationToken,
    _thread: NonDetachingJoinHandle<()>,
}

impl InterfacePortForwardController {
    pub fn new(
        ip_info: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
        pmap: PortMapController,
    ) -> Self {
        Self::with_port_forward(ip_info, pmap, PortForwardController::new())
    }

    fn with_port_forward(
        mut ip_info: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
        pmap: PortMapController,
        port_forward: PortForwardController,
    ) -> Self {
        let cancel = CancellationToken::new();
        let actor_cancel = cancel.clone();
        let (req_send, mut req_recv) = mpsc::unbounded_channel::<InterfaceForwardCommand>();
        let thread = NonDetachingJoinHandle::from(tokio::spawn(async move {
            let mut state = InterfaceForwardState::new(port_forward, pmap);
            let mut interfaces = ip_info.read_and_mark_seen();
            let mut reconcile_interval = tokio::time::interval_at(
                tokio::time::Instant::now() + PORT_FORWARD_GC_INTERVAL,
                PORT_FORWARD_GC_INTERVAL,
            );
            reconcile_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            'active: loop {
                tokio::select! {
                    biased;
                    _ = actor_cancel.cancelled() => break,
                    msg = req_recv.recv() => {
                        let Some(cmd) = msg else {
                            return;
                        };
                        match cmd {
                            InterfaceForwardCommand::Forward(req, re) => {
                                let result = tokio::select! {
                                    biased;
                                    _ = actor_cancel.cancelled() => break 'active,
                                    result = state.handle_request(req) => result,
                                };
                                re.send(result).ok()
                            }
                            InterfaceForwardCommand::Forward6 { source, spec, respond } => {
                                let result = tokio::select! {
                                    biased;
                                    _ = actor_cancel.cancelled() => break 'active,
                                    result = state.add_forward6(source, spec) => result,
                                };
                                respond.send(result).ok()
                            }
                            InterfaceForwardCommand::Sync(re) => {
                                let result = tokio::select! {
                                    biased;
                                    _ = actor_cancel.cancelled() => break 'active,
                                    result = state.sync(&interfaces) => result,
                                };
                                re.send(result).ok()
                            }
                            InterfaceForwardCommand::DumpTable(re) => {
                                let result = tokio::select! {
                                    biased;
                                    _ = actor_cancel.cancelled() => break 'active,
                                    result = state.port_forward.dump() => result.map(|applied| {
                                        ForwardTable::from_state(&state, &applied)
                                    }),
                                };
                                re.send(result).ok()
                            }
                            InterfaceForwardCommand::Drain(respond) => {
                                respond.send(state.drain().await).ok();
                                break 'active;
                            }
                        };
                    }
                    _ = ip_info.changed() => {
                        interfaces = ip_info.read();
                        tokio::select! {
                            biased;
                            _ = actor_cancel.cancelled() => break,
                            result = state.sync(&interfaces) => result.log_err(),
                        };
                    }
                    _ = reconcile_interval.tick() => {
                        tokio::select! {
                            biased;
                            _ = actor_cancel.cancelled() => break,
                            result = state.reconcile(&interfaces) => result.log_err(),
                        };
                    }
                }
            }
            while let Some(cmd) = req_recv.recv().await {
                if let InterfaceForwardCommand::Drain(respond) = cmd {
                    respond.send(state.drain().await).ok();
                }
            }
        }));

        Self {
            req: req_send,
            cancel,
            _thread: thread,
        }
    }

    pub(super) async fn forward6(
        &self,
        source: SocketAddrV6,
        target: SocketAddrV6,
        target_prefix: u8,
        src_filter: Option<IpNet>,
    ) -> Result<ForwardLease, Error> {
        let (respond, receive) = oneshot::channel();
        self.req
            .send(InterfaceForwardCommand::Forward6 {
                source,
                spec: Ipv6ForwardSpec {
                    target,
                    target_prefix,
                    src_filter,
                },
                respond,
            })
            .map_err(err_has_exited)?;
        receive.await.map_err(err_has_exited)
    }

    pub(super) async fn add_range(
        &self,
        external: u16,
        count: u16,
        reqs: ForwardRequirements,
        target: SocketAddrV4,
    ) -> Result<ForwardLease, Error> {
        let rc = Arc::new(());
        let (send, recv) = oneshot::channel();
        self.req
            .send(InterfaceForwardCommand::Forward(
                InterfaceForwardRequest {
                    external,
                    target,
                    count,
                    target_prefix_fallback: 32,
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
        res.await.map_err(err_has_exited)?
    }

    pub async fn drain(&self) -> Result<(), Error> {
        self.cancel.cancel();
        let (req, res) = oneshot::channel();
        self.req
            .send(InterfaceForwardCommand::Drain(req))
            .map_err(err_has_exited)?;
        res.await.map_err(err_has_exited)?
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
    cmd.kill_process_group_on_drop()
        .timeout(Some(FORWARD_SCRIPT_TIMEOUT))
        .invoke(ErrorKind::Network)
        .await?;
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
    cmd.kill_process_group_on_drop()
        .timeout(Some(FORWARD_SCRIPT_TIMEOUT))
        .invoke(ErrorKind::Network)
        .await?;
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
    cmd.kill_process_group_on_drop()
        .timeout(Some(FORWARD_SCRIPT_TIMEOUT))
        .invoke(ErrorKind::Network)
        .await?;
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
    cmd.kill_process_group_on_drop()
        .timeout(Some(FORWARD_SCRIPT_TIMEOUT))
        .invoke(ErrorKind::Network)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ipv6_spec() -> Ipv6ForwardSpec {
        Ipv6ForwardSpec {
            target: SocketAddrV6::new("fd00:3::2".parse().unwrap(), 8080, 0, 0),
            target_prefix: 64,
            src_filter: None,
        }
    }

    #[test]
    fn ipv6_add_and_replacement_advance_only_after_success() {
        let lease = Arc::new(());
        let original = ipv6_spec();
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
        mapping.operation_started(&add);
        mapping.operation_succeeded(add);
        assert_eq!(mapping.next_operation(), None);

        mapping.desired = replacement.clone();
        let remove = Ipv6ForwardOperation::Remove(original);
        assert_eq!(mapping.next_operation(), Some(remove.clone()));
        mapping.operation_succeeded(remove);

        let add_replacement = Ipv6ForwardOperation::Add(replacement);
        assert_eq!(mapping.next_operation(), Some(add_replacement.clone()));
        mapping.operation_started(&add_replacement);
        mapping.operation_succeeded(add_replacement);
        assert_eq!(mapping.next_operation(), None);
    }

    #[test]
    fn failed_ipv6_add_clears_provisional_applied_state() {
        let lease = Arc::new(());
        let spec = ipv6_spec();
        let mut mapping = Ipv6ForwardMapping {
            desired: spec.clone(),
            applied: None,
            rc: Arc::downgrade(&lease),
        };
        let operation = Ipv6ForwardOperation::Add(spec.clone());

        mapping.operation_started(&operation);
        assert_eq!(mapping.applied, Some(spec.clone()));
        mapping.operation_failed(&operation);

        assert_eq!(mapping.applied, None);
        assert_eq!(
            mapping.next_operation(),
            Some(Ipv6ForwardOperation::Add(spec))
        );
    }

    #[test]
    fn gateway_refresh_does_not_change_ipv6_nft_identity() {
        use imbl::OrdSet;
        use imbl_value::InternedString;

        use crate::db::model::public::{GatewayType, IpInfo};

        let source: std::net::Ipv6Addr = "2001:db8::2".parse().unwrap();
        let interfaces = |gateway: &str| {
            OrdMap::from_iter([(
                GatewayId::from(InternedString::intern("eth0")),
                NetworkInterfaceInfo {
                    gateway_type: GatewayType::InboundOutbound,
                    ip_info: Some(Arc::new(IpInfo {
                        subnets: OrdSet::from_iter(["2001:db8::2/64".parse::<IpNet>().unwrap()]),
                        lan_ip: OrdSet::from_iter([gateway.parse::<IpAddr>().unwrap()]),
                        ..Default::default()
                    })),
                    ..Default::default()
                },
            )])
        };
        let lease = Arc::new(());
        let spec = ipv6_spec();
        let mapping = Ipv6ForwardMapping {
            desired: spec.clone(),
            applied: Some(spec),
            rc: Arc::downgrade(&lease),
        };

        assert_eq!(mapping.next_operation(), None);
        assert_ne!(
            ipv6_candidate_gateways(&interfaces("2001:db8::1"), source),
            ipv6_candidate_gateways(&interfaces("2001:db8::ff"), source)
        );
        assert_eq!(mapping.next_operation(), None);
    }

    #[tokio::test]
    async fn dump_table_includes_only_applied_ipv4_forwards() {
        let interfaces = Watch::new(OrdMap::new());
        let mut state = InterfaceForwardState::new(
            PortForwardController::new(),
            PortMapController::new(interfaces),
        );
        let requirements = ForwardRequirements {
            public_gateways: BTreeSet::new(),
            private_ips: BTreeSet::new(),
            secure: true,
        };
        let target = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 2), 8080);
        let lease = Arc::new(());
        let mut entry = InterfaceForwardEntry::new(80, 1);
        entry
            .targets
            .insert(requirements, (target, 32, Arc::downgrade(&lease)));
        state.state.entry(80).or_insert(entry);

        assert!(
            ForwardTable::from_state(&state, &BTreeMap::new())
                .0
                .is_empty()
        );
        let source = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 80);
        let applied = BTreeMap::from([(
            source,
            ForwardMapping {
                source,
                target,
                count: 1,
                target_prefix: 24,
                src_filter: None,
                rc: Arc::downgrade(&lease),
            },
        )]);
        assert_eq!(
            ForwardTable::from_state(&state, &applied).0[&80].target_prefix,
            24
        );
    }

    #[test]
    fn dropped_ipv6_lease_keeps_teardown_pending_until_success() {
        let lease = Arc::new(());
        let spec = ipv6_spec();
        let mut mapping = Ipv6ForwardMapping {
            desired: spec.clone(),
            applied: Some(spec.clone()),
            rc: Arc::downgrade(&lease),
        };
        assert_eq!(mapping.next_operation(), None);

        drop(lease);

        let remove = Ipv6ForwardOperation::Remove(spec);
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

    #[test]
    fn failed_ipv6_retirement_keeps_applied_state() {
        let spec = ipv6_spec();
        let mut mapping = Ipv6ForwardMapping {
            desired: spec.clone(),
            applied: Some(spec.clone()),
            rc: Weak::new(),
        };
        let operation = Ipv6ForwardOperation::Remove(spec.clone());

        mapping.operation_failed(&operation);

        assert_eq!(mapping.applied, Some(spec));
        assert_eq!(mapping.next_operation(), Some(operation));
    }

    #[tokio::test]
    async fn a_preinitialization_request_resumes_after_initialization() {
        let initialize = Arc::new(tokio::sync::Notify::new());
        let controller = PortForwardController::spawn({
            let initialize = initialize.clone();
            move || {
                let initialize = initialize.clone();
                async move {
                    initialize.notified().await;
                    Ok(())
                }
            }
        });
        let request = controller.dump();
        tokio::pin!(request);

        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut request)
                .await
                .is_err()
        );
        initialize.notify_one();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), &mut request)
                .await
                .expect("dump stayed blocked after initialization")
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn drain_interrupts_a_forward_waiting_for_initialization() {
        use imbl::OrdSet;
        use imbl_value::InternedString;

        use crate::db::model::public::IpInfo;

        let gateway = GatewayId::from(InternedString::intern("eth0"));
        let interfaces = Watch::new(OrdMap::from_iter([(
            gateway.clone(),
            NetworkInterfaceInfo {
                ip_info: Some(Arc::new(IpInfo {
                    subnets: OrdSet::from_iter(["192.168.1.2/24".parse::<IpNet>().unwrap()]),
                    ..Default::default()
                })),
                ..Default::default()
            },
        )]));
        let port_forward =
            PortForwardController::spawn(|| std::future::pending::<Result<(), Error>>());
        let port_forward_req = port_forward.req.clone();
        let controller = InterfacePortForwardController::with_port_forward(
            interfaces.clone(),
            PortMapController::new(interfaces),
            port_forward,
        );
        let _lease = controller
            .add_range(
                8080,
                1,
                ForwardRequirements {
                    public_gateways: BTreeSet::from([gateway]),
                    private_ips: BTreeSet::new(),
                    secure: true,
                },
                SocketAddrV4::new(Ipv4Addr::new(10, 0, 3, 2), 8080),
            )
            .await
            .unwrap();
        let gc = controller.gc();
        tokio::pin!(gc);

        assert!(futures::poll!(&mut gc).is_pending());
        let (barrier_send, barrier_receive) = oneshot::channel();
        port_forward_req
            .send(PortForwardCommand::Drain {
                respond: barrier_send,
            })
            .unwrap();
        barrier_receive.await.unwrap().unwrap();

        tokio::time::timeout(Duration::from_secs(1), controller.drain())
            .await
            .expect("drain timed out")
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), &mut gc)
                .await
                .expect("gc stayed blocked")
                .is_err()
        );
    }
}
