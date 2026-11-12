use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
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
use crate::net::transport::{Transport, TransportProtocol};
use crate::prelude::*;
use crate::util::Invoke;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::serde::{HandlerExtSerde, display_serializable};
use crate::util::sync::Watch;

pub const START9_BRIDGE_IFACE: &str = "lxcbr0";
/// The host's `ip_local_port_range` ends below this; see `projects/start-os/debian/postinst`.
const DYNAMIC_PORT_START: u16 = 49152;
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
    pub transport: Transport,
}

impl ForwardRequirements {
    pub fn allows_gateway(&self, gateway: &NetworkInterfaceInfo) -> bool {
        self.secure || gateway.secure()
    }

    pub fn public_on(&self, gateway: &GatewayId) -> bool {
        self.secure && self.public_gateways.contains(gateway)
    }
}

impl std::fmt::Display for ForwardRequirements {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ForwardRequirements {{ public: {:?}, private: {:?}, secure: {}, transport: {} }}",
            self.public_gateways,
            self.private_ips,
            self.secure,
            self.transport.as_str()
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
            let port = rng.random_range(DYNAMIC_PORT_START..u16::MAX);
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

            print_table(&table)?;

            Ok(())
        })
        .with_about("about.dump-port-forward-table")
        .with_call_remote::<CliContext>(),
    )
}

struct ForwardMapping {
    source: SocketAddrV4,
    target: SocketAddrV4,
    count: u16,
    target_prefix: u8,
    src_filter: Option<IpNet>,
    owners: Vec<Weak<()>>,
}

impl ForwardMapping {
    fn add_owner(&mut self, owner: &Arc<()>) {
        self.owners.retain(|rc| rc.strong_count() > 0);
        self.owners.push(Arc::downgrade(owner));
    }

    fn is_live(&self) -> bool {
        self.owners.iter().any(|rc| rc.strong_count() > 0)
    }
}

#[derive(Default)]
struct PortForwardState {
    // Leases retain exactly their requested protocols.
    mappings: BTreeMap<(SocketAddrV4, TransportProtocol), ForwardMapping>,
}

impl PortForwardState {
    async fn add_forward(
        &mut self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        count: u16,
        target_prefix: u8,
        src_filter: Option<IpNet>,
        transport: Transport,
    ) -> Result<Arc<()>, Error> {
        let rc = Arc::new(());
        for &protocol in transport.protocols() {
            let key = (source, protocol);
            if let Some(existing) = self.mappings.get_mut(&key) {
                if existing.target == target
                    && existing.count == count
                    && existing.target_prefix == target_prefix
                    && existing.src_filter == src_filter
                {
                    existing.add_owner(&rc);
                    continue;
                }
                unforward(
                    existing.source,
                    existing.target,
                    existing.count,
                    existing.target_prefix,
                    existing.src_filter.as_ref(),
                    protocol.into(),
                )
                .await?;
                self.mappings.remove(&key);
            }

            forward(
                source,
                target,
                count,
                target_prefix,
                src_filter.as_ref(),
                protocol.into(),
            )
            .await?;
            self.mappings.insert(
                key,
                ForwardMapping {
                    source,
                    target,
                    count,
                    target_prefix,
                    src_filter,
                    owners: vec![Arc::downgrade(&rc)],
                },
            );
        }
        Ok(rc)
    }

    async fn gc(&mut self) -> Result<(), Error> {
        let to_remove: Vec<_> = self
            .mappings
            .iter()
            .filter(|(_, mapping)| !mapping.is_live())
            .map(|(key, _)| *key)
            .collect();

        for key in to_remove {
            let mapping = &self.mappings[&key];
            unforward(
                mapping.source,
                mapping.target,
                mapping.count,
                mapping.target_prefix,
                mapping.src_filter.as_ref(),
                key.1.into(),
            )
            .await?;
            self.mappings.remove(&key);
        }
        Ok(())
    }

    fn dump(&self) -> BTreeMap<SocketAddrV4, SocketAddrV4> {
        self.mappings
            .iter()
            .filter(|(_, mapping)| mapping.is_live())
            .map(|((source, _), mapping)| (*source, mapping.target))
            .collect()
    }
}

impl Drop for PortForwardState {
    fn drop(&mut self) {
        if !self.mappings.is_empty() {
            let mappings = std::mem::take(&mut self.mappings);
            tokio::spawn(async move {
                for ((_, protocol), mapping) in mappings {
                    unforward(
                        mapping.source,
                        mapping.target,
                        mapping.count,
                        mapping.target_prefix,
                        mapping.src_filter.as_ref(),
                        protocol.into(),
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
        transport: Transport,
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

/// `nft -a list chain <family> startos <chain>` output, empty on error.
/// `family` is `ip` (IPv4) or `ip6`.
async fn nft_list_chain(family: &str, chain: &str) -> String {
    let out = Command::new("nft")
        .arg("-a")
        .arg("list")
        .arg("chain")
        .arg(family)
        .arg("startos")
        .arg(chain)
        .invoke(ErrorKind::Network)
        .await
        .unwrap_or_default();
    String::from_utf8_lossy(&out).into_owned()
}

/// Rules in `chain` tagged with `comment`, as `(handle, body)` where `body` is
/// the rule text preceding the `comment "..."` token.
async fn nft_rules_with_comment(family: &str, chain: &str, comment: &str) -> Vec<(u32, String)> {
    let needle = format!("comment \"{comment}\"");
    nft_list_chain(family, chain)
        .await
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
        .collect()
}

/// Comment tags in `chain` of `table ip startos` beginning with `prefix`. Used
/// to prune orphaned per-device/per-subnet rules whose owner no longer exists.
pub(crate) async fn nft_comments_with_prefix(chain: &str, prefix: &str) -> Vec<String> {
    nft_list_chain("ip", chain)
        .await
        .lines()
        .filter_map(|line| {
            let after = line.split_once("comment \"")?.1;
            let tag = after.split_once('"')?.0;
            tag.starts_with(prefix).then(|| tag.to_owned())
        })
        .collect()
}

/// The comment tag `forward-port` (`F4-` + digest) or `forward-port6` (`F6-` +
/// digest) stamps on its rules.
fn is_forward_script_tag(tag: &str) -> bool {
    tag.strip_prefix("F4-")
        .or_else(|| tag.strip_prefix("F6-"))
        .is_some_and(|hex| hex.len() == 16 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Deletes every rule a previous run's forward scripts left in the kernel.
async fn remove_stale_forward_rules() -> Result<(), Error> {
    let mut script = String::new();
    for family in ["ip", "ip6"] {
        for chain in [
            "prerouting",
            "output",
            "postrouting",
            "forward",
            "forward_snat",
        ] {
            for line in nft_list_chain(family, chain).await.lines() {
                let Some(tag) = line
                    .split_once("comment \"")
                    .and_then(|(_, after)| after.split_once('"'))
                    .map(|(tag, _)| tag)
                else {
                    continue;
                };
                let Some(handle) = line
                    .rsplit_once("# handle ")
                    .and_then(|(_, handle)| handle.trim().parse::<u32>().ok())
                else {
                    continue;
                };
                if is_forward_script_tag(tag) {
                    writeln!(
                        script,
                        "delete rule {family} startos {chain} handle {handle}"
                    )
                    .unwrap();
                }
            }
        }
    }
    if script.is_empty() {
        return Ok(());
    }
    Command::new("nft")
        .arg(&script)
        .invoke(ErrorKind::Network)
        .await?;
    Ok(())
}

/// Idempotently install (or, with `undo`, remove) the rule tagged `comment` in
/// `chain` of `table ip startos`, via one atomic nft transaction that drops
/// every prior copy of this comment and adds the desired rule. No-op when the
/// chain already holds exactly that rule. `prepend` inserts at the chain top
/// (needed for the mark-restore rule, which must precede the set-mark rules).
///
/// Lock-free: the only failure is a stale handle — a concurrent reconcile of
/// the *same* comment replaced the rule between our list and delete. Benign (nft
/// commits atomically), so we warn, re-read, and retry to convergence. Only
/// arises for callers without a single-writer guarantee (e.g. tunnel masq).
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
        let existing = nft_rules_with_comment(family, chain, comment).await;

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
                remove_stale_forward_rules().await?;
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
            while let Some(cmd) = req_recv.recv().await {
                match cmd {
                    PortForwardCommand::AddForward {
                        source,
                        target,
                        count,
                        target_prefix,
                        src_filter,
                        transport,
                        respond,
                    } => {
                        let result = state
                            .add_forward(
                                source,
                                target,
                                count,
                                target_prefix,
                                src_filter,
                                transport,
                            )
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
        transport: Transport,
    ) -> Result<Arc<()>, Error> {
        self.add_forward_range(source, target, 1, target_prefix, src_filter, transport)
            .await
    }

    /// Like [`add_forward`] but covers `count` contiguous ports per protocol
    /// from `source.port()` / `target.port()`, mapped by offset
    /// (the two bases may differ).
    pub async fn add_forward_range(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        count: u16,
        target_prefix: u8,
        src_filter: Option<IpNet>,
        transport: Transport,
    ) -> Result<Arc<()>, Error> {
        let (send, recv) = oneshot::channel();
        self.req
            .send(PortForwardCommand::AddForward {
                source,
                target,
                count,
                target_prefix,
                src_filter,
                transport,
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
    forwards: BTreeMap<(SocketAddrV4, TransportProtocol), Arc<()>>,
    // Includes the protocol to withdraw upstream mappings when a binding narrows.
    mapped: BTreeSet<(Ipv4Addr, u16, TransportProtocol)>,
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
        let mut keep = BTreeSet::new();
        let mut want = BTreeMap::new();

        for (gw_id, info) in ip_info.iter() {
            if let Some(ip_info) = &info.ip_info {
                for subnet in ip_info.subnets.iter() {
                    if let IpAddr::V4(ip) = subnet.addr() {
                        let addr = SocketAddrV4::new(ip, self.external);
                        for (reqs, (target, target_prefix, rc)) in self.targets.iter() {
                            if rc.strong_count() == 0 || !reqs.allows_gateway(info) {
                                continue;
                            }
                            let public = reqs.public_on(gw_id);
                            let src_filter = if public {
                                None
                            } else if reqs.private_ips.contains(&IpAddr::V4(ip)) {
                                Some(subnet.trunc())
                            } else {
                                continue;
                            };

                            for &protocol in reqs.transport.protocols() {
                                if !keep.insert((addr, protocol)) {
                                    continue;
                                }
                                if public {
                                    // Upstream maps to this host's listener, before local DNAT.
                                    want.entry((ip, self.external, protocol)).or_insert_with(|| {
                                        let gateways = candidate_gateways(info);
                                        tracing::debug!(
                                            "auto-port-mapping {ip}:{} {protocol:?} on gateway {gw_id} via {gateways:?} (reqs {reqs})",
                                            self.external,
                                        );
                                        (self.count, gateways)
                                    });
                                }
                                let fwd_rc = port_forward
                                    .add_forward_range(
                                        addr,
                                        *target,
                                        self.count,
                                        *target_prefix,
                                        src_filter,
                                        protocol.into(),
                                    )
                                    .await?;
                                self.forwards.insert((addr, protocol), fwd_rc);
                            }
                        }
                    }
                }
            }
        }

        self.forwards.retain(|key, _| keep.contains(key));
        port_forward.gc().await?;

        for (ip, port, protocol) in self.mapped.iter().filter(|key| !want.contains_key(key)) {
            pmap.remove(IpAddr::V4(*ip), *port, (*protocol).into());
        }
        for ((ip, external, protocol), (count, gateways)) in &want {
            if *count > 1 {
                pmap.ensure_range(
                    IpAddr::V4(*ip),
                    *external,
                    *external,
                    *count,
                    gateways.clone(),
                    (*protocol).into(),
                );
            } else {
                pmap.ensure(
                    IpAddr::V4(*ip),
                    *external,
                    *external,
                    gateways.clone(),
                    (*protocol).into(),
                );
            }
        }
        self.mapped = want.into_keys().collect();

        Ok(())
    }

    async fn update_request(
        &mut self,
        InterfaceForwardRequest {
            external,
            target,
            count,
            target_prefix,
            reqs,
            mut rc,
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

        self.update(ip_info, port_forward, pmap).await?;

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
}

impl InterfaceForwardState {
    fn new(port_forward: PortForwardController, pmap: PortMapController) -> Self {
        Self {
            port_forward,
            pmap,
            state: IdOrdMap::new(),
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

    async fn sync(
        &mut self,
        ip_info: &OrdMap<GatewayId, NetworkInterfaceInfo>,
    ) -> Result<(), Error> {
        for mut entry in self.state.iter_mut() {
            entry.gc(ip_info, &self.port_forward, &self.pmap).await?;
        }

        self.port_forward.gc().await
    }
}

fn err_has_exited<T>(_: T) -> Error {
    Error::new(
        eyre!("{}", t!("net.forward.controller-thread-exited")),
        ErrorKind::Unknown,
    )
}

#[derive(Debug, Clone, Deserialize, Serialize, VisitFields)]
pub struct ForwardTable(pub BTreeMap<u16, ForwardTarget>);

rpc_toolkit::reflect_ts!(ForwardTable);

#[derive(Debug, Clone, Deserialize, Serialize, VisitFields)]
pub struct ForwardTarget {
    pub target: SocketAddrV4,
    pub target_prefix: u8,
    pub reqs: String,
}

rpc_toolkit::reflect_ts!(ForwardTarget);

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
    Sync(oneshot::Sender<Result<(), Error>>),
    DumpTable(oneshot::Sender<ForwardTable>),
}

pub struct InterfacePortForwardController {
    req: mpsc::UnboundedSender<InterfaceForwardCommand>,
    /// A clone of the shared `PortMapController`, so this controller owns the
    /// upstream pinhole of a v6 GUA DNAT ([`Self::forward6`]) the same way the v4
    /// path does inside [`InterfaceForwardEntry::update`].
    pmap: PortMapController,
    _thread: NonDetachingJoinHandle<()>,
}

impl InterfacePortForwardController {
    pub fn new(
        mut ip_info: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
        pmap: PortMapController,
    ) -> Self {
        let port_forward = PortForwardController::new();
        let v6_pmap = pmap.clone();

        let (req_send, mut req_recv) = mpsc::unbounded_channel::<InterfaceForwardCommand>();
        let thread = NonDetachingJoinHandle::from(tokio::spawn(async move {
            let mut state = InterfaceForwardState::new(port_forward, pmap);
            let mut interfaces = ip_info.read_and_mark_seen();
            loop {
                tokio::select! {
                    msg = req_recv.recv() => {
                        if let Some(cmd) = msg {
                            match cmd {
                                InterfaceForwardCommand::Forward(req, re) => {
                                    re.send(state.handle_request(req, &interfaces).await).ok()
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
                }
            }
        }));

        Self {
            req: req_send,
            pmap: v6_pmap,
            _thread: thread,
        }
    }

    /// DNAT a host GUA to a container ULA (v6 counterpart of a v4 forward) and, for
    /// a WAN forward (`src_filter == None`), open its upstream firewall pinhole —
    /// so the DNAT and its pinhole are owned together, as in the v4 path.
    /// `gateways` are the PCP candidates for the pinhole (ignored for a LAN-only
    /// forward). Best-effort pinhole: it is a fire-and-forget port-map request.
    pub async fn forward6(
        &self,
        source: SocketAddrV6,
        target: SocketAddrV6,
        target_prefix: u8,
        src_filter: Option<IpNet>,
        gateways: Vec<(IpAddr, Option<u32>)>,
        transport: Transport,
    ) -> Result<(), Error> {
        forward6(
            source,
            target,
            target_prefix,
            src_filter.as_ref(),
            transport,
        )
        .await?;
        if src_filter.is_none() && !gateways.is_empty() {
            self.pmap.ensure(
                IpAddr::V6(*source.ip()),
                source.port(),
                source.port(),
                gateways,
                transport,
            );
        }
        Ok(())
    }

    /// Tear down a [`Self::forward6`] — the nft DNAT and, for a WAN forward, its
    /// upstream pinhole.
    pub async fn unforward6(
        &self,
        source: SocketAddrV6,
        target: SocketAddrV6,
        target_prefix: u8,
        src_filter: Option<IpNet>,
        transport: Transport,
    ) -> Result<(), Error> {
        unforward6(
            source,
            target,
            target_prefix,
            src_filter.as_ref(),
            transport,
        )
        .await?;
        if src_filter.is_none() {
            self.pmap
                .remove(IpAddr::V6(*source.ip()), source.port(), transport);
        }
        Ok(())
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
    transport: Transport,
) -> Result<(), Error> {
    let mut cmd = Command::new("/usr/lib/startos/scripts/forward-port");
    cmd.env("sip", source.ip().to_string())
        .env("dip", target.ip().to_string())
        .env("dprefix", target_prefix.to_string())
        .env("sport", source.port().to_string())
        .env("dport", target.port().to_string())
        .env("transport", transport.as_str())
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
    transport: Transport,
) -> Result<(), Error> {
    let mut cmd = Command::new("/usr/lib/startos/scripts/forward-port");
    cmd.env("UNDO", "1")
        .env("sip", source.ip().to_string())
        .env("dip", target.ip().to_string())
        .env("dprefix", target_prefix.to_string())
        .env("sport", source.port().to_string())
        .env("dport", target.port().to_string())
        .env("transport", transport.as_str())
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

/// The host's own address on the lxcbr0 bridge, pinned in `debian/postinst`
/// beside the subnet above. The IPv6 counterpart of [`crate::HOST_IP`].
pub(crate) const START9_BRIDGE_V6_ADDR: Ipv6Addr = Ipv6Addr::new(0xfd00, 3, 0, 0, 0, 0, 0, 1);

/// IPv6 counterpart of [`forward`]: DNAT `source` (a host GUA:port) to `target`
/// (the container's ULA:port) via the `forward-port6` script. `src_filter`
/// restricts inbound to a LAN v6 subnet (a LAN-only GUA); `None` is WAN.
pub(crate) async fn forward6(
    source: SocketAddrV6,
    target: SocketAddrV6,
    target_prefix: u8,
    src_filter: Option<&IpNet>,
    transport: Transport,
) -> Result<(), Error> {
    let mut cmd = Command::new("/usr/lib/startos/scripts/forward-port6");
    cmd.env("sip", source.ip().to_string())
        .env("dip", target.ip().to_string())
        .env("dprefix", target_prefix.to_string())
        .env("sport", source.port().to_string())
        .env("dport", target.port().to_string())
        .env("transport", transport.as_str())
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
    transport: Transport,
) -> Result<(), Error> {
    let mut cmd = Command::new("/usr/lib/startos/scripts/forward-port6");
    cmd.env("UNDO", "1")
        .env("sip", source.ip().to_string())
        .env("dip", target.ip().to_string())
        .env("dprefix", target_prefix.to_string())
        .env("sport", source.port().to_string())
        .env("dport", target.port().to_string())
        .env("transport", transport.as_str());
    if let Some(subnet) = src_filter {
        cmd.env("src_subnet", subnet.to_string());
    }
    cmd.invoke(ErrorKind::Network).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwarding_scripts_enforce_transport() {
        let test = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../projects/start-os/build/tests/forward-transport-test.sh");
        let output = std::process::Command::new("bash")
            .arg(test)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    #[tokio::test]
    async fn narrower_lease_does_not_keep_other_protocol_alive() {
        let both = Arc::new(());
        let source = "192.0.2.1:5000".parse().unwrap();
        let target = "10.0.3.2:5000".parse().unwrap();
        let mut state = PortForwardState::default();
        for &protocol in Transport::Both.protocols() {
            state.mappings.insert(
                (source, protocol),
                ForwardMapping {
                    source,
                    target,
                    count: 1,
                    target_prefix: 24,
                    src_filter: None,
                    owners: vec![Arc::downgrade(&both)],
                },
            );
        }
        let tcp = state
            .add_forward(source, target, 1, 24, None, Transport::Tcp)
            .await
            .unwrap();
        drop(both);
        assert!(state.mappings[&(source, TransportProtocol::Tcp)].is_live());
        assert!(!state.mappings[&(source, TransportProtocol::Udp)].is_live());
        drop(tcp);
        assert!(!state.mappings[&(source, TransportProtocol::Tcp)].is_live());
        state.mappings.clear();
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
    fn forward_script_tags_are_a_family_prefix_and_sixteen_hex_digits() {
        assert!(is_forward_script_tag("F4-ac286185d01ca612"));
        assert!(is_forward_script_tag("F6-ac286185d01ca612"));
        assert!(!is_forward_script_tag("Fac286185d01ca612"));
        assert!(!is_forward_script_tag("F4-ac286185d01ca61"));
        assert!(!is_forward_script_tag("F4-ac286185d01ca612a"));
        assert!(!is_forward_script_tag("F4-ac286185d01cg612"));
        assert!(!is_forward_script_tag("base-established"));
        assert!(!is_forward_script_tag("pinhole:2001:db8::1:443"));
        assert!(!is_forward_script_tag("sni-divert"));
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
