use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddrV4, SocketAddrV6};
use std::sync::{Arc, Weak};
use std::time::Duration;

use futures::channel::oneshot;
use futures::future::{BoxFuture, FutureExt, Shared};
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
use crate::util::sync::{SyncMutex, Watch};

pub const START9_BRIDGE_IFACE: &str = "lxcbr0";
const EPHEMERAL_PORT_START: u16 = 49152;
const PORT_FORWARD_GC_INTERVAL: Duration = Duration::from_secs(30);
const FORWARD_SCRIPT_TIMEOUT: Duration = Duration::from_secs(30);
const FORWARD_DRAIN_RETRY_INTERVAL: Duration = Duration::from_secs(1);
pub(crate) const FORWARD_DRAIN_TIMEOUT: Duration = Duration::from_secs(90);
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
        let end = source
            .port()
            .checked_add(count.checked_sub(1).ok_or_else(|| {
                Error::new(eyre!("empty forwarding range"), ErrorKind::InvalidRequest)
            })?)
            .ok_or_else(|| {
                Error::new(
                    eyre!("forwarding range overflow"),
                    ErrorKind::InvalidRequest,
                )
            })?;
        target.port().checked_add(count - 1).ok_or_else(|| {
            Error::new(
                eyre!("target forwarding range overflow"),
                ErrorKind::InvalidRequest,
            )
        })?;
        let conflicts = self
            .mappings
            .iter()
            .filter(|(key, mapping)| {
                **key != source
                    && key.ip() == source.ip()
                    && u32::from(key.port()) <= u32::from(end)
                    && u32::from(source.port()) < u32::from(key.port()) + u32::from(mapping.count)
            })
            .map(|(key, mapping)| (*key, mapping.rc.strong_count() > 0))
            .collect::<Vec<_>>();
        if conflicts.iter().any(|(_, live)| *live) {
            return Err(Error::new(
                eyre!("overlapping live forwarding range"),
                ErrorKind::Network,
            ));
        }
        for (key, _) in conflicts {
            self.remove_forward(key).await?;
        }
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

    async fn drain(&mut self) -> Result<(), Error> {
        let sources = self.mappings.keys().copied().collect::<Vec<_>>();
        let mut first_error = None;
        for source in sources {
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
            Self::Gc { respond } => respond.is_canceled(),
            Self::Dump { respond } => respond.is_canceled(),
            Self::Drain { .. } => false,
        }
    }
}

type PortForwardDrainFuture = Shared<BoxFuture<'static, Result<(), Arc<Error>>>>;

async fn join_forward_actor(
    mut thread: NonDetachingJoinHandle<()>,
    response: oneshot::Receiver<Result<(), Error>>,
    deadline: tokio::time::Instant,
) -> Result<(), Error> {
    match tokio::time::timeout_at(deadline, async {
        let response = response.await.map_err(err_has_exited);
        (&mut thread).await.map_err(err_has_exited)?;
        response?
    })
    .await
    {
        Ok(result) => result,
        Err(_) => {
            thread.wait_for_abort().await.ok();
            Err(Error::new(
                eyre!("forwarding teardown deadline expired; cleanup incomplete"),
                ErrorKind::Timeout,
            ))
        }
    }
}

pub struct PortForwardController {
    req: mpsc::UnboundedSender<PortForwardCommand>,
    drain_completion: Arc<SyncMutex<Option<PortForwardDrainFuture>>>,
    thread: SyncMutex<Option<NonDetachingJoinHandle<()>>>,
}

/// Native nftables table owning all of StartOS's packet-filter / NAT rules.
/// Coexists with lxc-net / wg-quick, which keep their own iptables-nft rules in
/// separate tables on the shared nf_tables datapath.
pub const NFT_TABLE: &str = "startos";

/// Ensure `table ip startos` and its base chains exist. Idempotent (nft's `add
/// table`/`add chain` are no-ops if present). The forward chain defaults to
/// `drop` (replacing `iptables -P FORWARD DROP`); callers add ACCEPT rules.
pub async fn nft_ensure_base() -> Result<(), Error> {
    nft_ensure_base_until("nft", nft_deadline()).await
}

async fn nft_ensure_base_until(program: &str, deadline: tokio::time::Instant) -> Result<(), Error> {
    for base in [
        include_str!("startos-base.nft"),
        include_str!("startos-base-v6.nft"),
    ] {
        nft_invoke_until(Command::new(program).arg(base), &[], deadline).await?;
    }
    Ok(())
}

async fn nft_list_chain_until(
    program: &str,
    family: &str,
    chain: &str,
    deadline: tokio::time::Instant,
) -> Result<String, Error> {
    let out = nft_invoke_until(
        Command::new(program).args(["-a", "list", "chain", family, "startos", chain]),
        &[],
        deadline,
    )
    .await?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// Rules in `chain` tagged with `comment`, as `(handle, body)` where `body` is
/// the rule text preceding the `comment "..."` token.
async fn nft_rules_with_comment(
    family: &str,
    chain: &str,
    comment: &str,
    deadline: tokio::time::Instant,
) -> Result<Vec<(u64, String)>, Error> {
    let needle = format!("comment \"{comment}\"");
    Ok(nft_list_chain_until("nft", family, chain, deadline)
        .await?
        .lines()
        .filter_map(|line| {
            let handle = line
                .rsplit_once("# handle ")?
                .1
                .trim()
                .parse::<u64>()
                .ok()?;
            let body = line.split_once(&needle)?.0.trim().to_owned();
            Some((handle, body))
        })
        .collect())
}

async fn nft_comments_with_prefix_family(
    family: &str,
    chain: &str,
    prefix: &str,
) -> Result<Vec<String>, Error> {
    Ok(nft_list_chain_until("nft", family, chain, nft_deadline())
        .await?
        .lines()
        .filter_map(|line| {
            let after = line.split_once("comment \"")?.1;
            let tag = after.split_once('"')?.0;
            tag.starts_with(prefix).then(|| tag.to_owned())
        })
        .collect())
}

pub(crate) async fn nft_comments_with_prefix(
    chain: &str,
    prefix: &str,
) -> Result<Vec<String>, Error> {
    nft_comments_with_prefix_family("ip", chain, prefix).await
}

const NFT_RULE_MAX_ATTEMPTS: usize = 5;

fn nft_deadline() -> tokio::time::Instant {
    tokio::time::Instant::now() + FORWARD_SCRIPT_TIMEOUT
}

/// Cancellation kills the inherited process group; deadline expiry also reaps the immediate child.
pub(crate) async fn nft_invoke_until(
    command: &mut Command,
    input: &[u8],
    deadline: tokio::time::Instant,
) -> Result<Vec<u8>, Error> {
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    if tokio::time::Instant::now() >= deadline {
        return Err(Error::new(
            eyre!("nft deadline expired"),
            ErrorKind::Timeout,
        ));
    }
    command.as_std_mut().process_group(0);
    command
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().with_kind(ErrorKind::Network)?;
    let pid = child.id().expect("spawned nft child has a pid");
    let group = crate::util::GeneralGuard::new(move || {
        let _ = nix::sys::signal::killpg(
            nix::unistd::Pid::from_raw(pid as i32),
            nix::sys::signal::Signal::SIGKILL,
        );
    });
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let result = tokio::time::timeout_at(deadline, async {
        let (input, stdout, stderr, status) = tokio::join!(
            async {
                stdin.write_all(input).await?;
                stdin.shutdown().await?;
                drop(stdin);
                Ok::<_, std::io::Error>(())
            },
            stdout.read_to_end(&mut output),
            stderr.read_to_end(&mut errors),
            child.wait(),
        );
        Ok::<_, std::io::Error>((status?, input, stdout, stderr))
    })
    .await;
    let (status, input_result, stdout_result, stderr_result) = match result {
        Ok(Ok(result)) => {
            group.drop_without_action();
            result
        }
        result => {
            drop(group);
            child.wait().await.with_kind(ErrorKind::Network)?;
            return Err(match result {
                Err(error) => Error::new(error, ErrorKind::Timeout),
                Ok(Err(error)) => Error::new(error, ErrorKind::Network),
                Ok(Ok(_)) => unreachable!(),
            });
        }
    };
    crate::ensure_code!(
        status.success(),
        ErrorKind::Network,
        "{}",
        String::from_utf8_lossy(&errors).trim()
    );
    input_result.with_kind(ErrorKind::Network)?;
    stdout_result.with_kind(ErrorKind::Network)?;
    stderr_result.with_kind(ErrorKind::Network)?;
    Ok(output)
}

fn nft_error_is_stale(error: &Error) -> bool {
    error
        .source
        .to_string()
        .contains("No such file or directory")
}

async fn nft_execute_transaction_until(
    program: &str,
    script: &str,
    deadline: tokio::time::Instant,
) -> Result<(), Error> {
    nft_invoke_until(
        Command::new(program).args(["-f", "-"]),
        script.as_bytes(),
        deadline,
    )
    .await?;
    Ok(())
}

fn nft_delete_rules_matching_script<F>(family: &str, chains: &[(&str, &str)], matches: &F) -> String
where
    F: Fn(&str) -> bool,
{
    let mut script = String::new();
    for (chain, listing) in chains {
        for line in listing.lines() {
            let Some((rule, handle)) = line.rsplit_once("# handle ") else {
                continue;
            };
            let Ok(handle) = handle.trim().parse::<u64>() else {
                continue;
            };
            let Some((_, comment)) = rule.split_once("comment \"") else {
                continue;
            };
            let Some((comment, _)) = comment.split_once('"') else {
                continue;
            };
            if matches(comment) {
                writeln!(
                    script,
                    "delete rule {family} startos {chain} handle {handle}"
                )
                .unwrap();
            }
        }
    }
    script
}

pub(crate) async fn nft_delete_rules_matching<F>(
    family: &str,
    chains: &[&str],
    matches: F,
) -> Result<(), Error>
where
    F: Fn(&str) -> bool,
{
    nft_delete_rules_matching_until(family, chains, matches, nft_deadline()).await
}

pub(crate) async fn nft_delete_rules_matching_until<F>(
    family: &str,
    chains: &[&str],
    matches: F,
    deadline: tokio::time::Instant,
) -> Result<(), Error>
where
    F: Fn(&str) -> bool,
{
    nft_delete_rules_matching_with_program_until("nft", family, chains, matches, deadline).await
}

async fn nft_delete_rules_matching_with_program_until<F>(
    program: &str,
    family: &str,
    chains: &[&str],
    matches: F,
    deadline: tokio::time::Instant,
) -> Result<(), Error>
where
    F: Fn(&str) -> bool,
{
    nft_ensure_base_until(program, deadline).await?;

    let mut last_err = None;
    for attempt in 1..=NFT_RULE_MAX_ATTEMPTS {
        let listings = futures::future::join_all(chains.iter().map(|chain| async move {
            Ok::<_, Error>((
                *chain,
                nft_list_chain_until(program, family, chain, deadline).await?,
            ))
        }))
        .await
        .into_iter()
        .collect::<Result<Vec<_>, Error>>()?;
        let listings = listings
            .iter()
            .map(|(chain, listing)| (*chain, listing.as_str()))
            .collect::<Vec<_>>();
        let script = nft_delete_rules_matching_script(family, &listings, &matches);
        if script.is_empty() {
            return Ok(());
        }

        match nft_execute_transaction_until(program, &script, deadline).await {
            Ok(()) => return Ok(()),
            Err(error) if nft_error_is_stale(&error) => {
                tracing::warn!(
                    "nft batch delete: stale handle on attempt {attempt}/{NFT_RULE_MAX_ATTEMPTS}"
                );
                last_err = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_err.expect("loop only exits here via the stale-handle path, which sets last_err"))
}

pub(crate) async fn nft_delete_rules_with_comment_prefix_v6(
    chains: &[&str],
    prefix: &str,
) -> Result<(), Error> {
    nft_delete_rules_matching("ip6", chains, |comment| comment.starts_with(prefix)).await
}

/// Converges the tagged rule, retrying stale handles.
/// `undo` deletes all matching tags; `prepend` inserts additions at the chain head.
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
    let deadline = nft_deadline();
    nft_ensure_base_until("nft", deadline).await?;

    let mut last_err = None;
    for attempt in 1..=NFT_RULE_MAX_ATTEMPTS {
        let existing = nft_rules_with_comment(family, chain, comment, deadline).await?;

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

        match nft_execute_transaction_until("nft", &script, deadline).await {
            Ok(()) => return Ok(()),
            // Stale handle: a concurrent reconcile won the race; re-read and
            // retry. Any other error is real and surfaces immediately.
            Err(error) if nft_error_is_stale(&error) => {
                tracing::warn!(
                    "nft_rule {chain}/{comment}: stale handle on attempt {attempt}/{NFT_RULE_MAX_ATTEMPTS}"
                );
                last_err = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_err.expect("loop only exits here via the stale-handle path, which sets last_err"))
}

#[cfg(test)]
mod nft_process_tests {
    use super::*;

    fn scratch() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("start-core-nft-{}", crate::util::new_guid()));
        std::fs::create_dir(&dir).unwrap();
        dir
    }

    async fn assert_group_stopped(block_stdin: bool, cancel: bool) {
        let dir = scratch();
        let task_dir = dir.clone();
        let task: NonDetachingJoinHandle<_> = tokio::spawn(async move {
            let mut command = Command::new("sh");
            command
                .args([
                    "-c",
                    "echo $$ > \"$1/pid\"; (sleep 0.7; echo survived > \"$1/sentinel\") & wait",
                    "sh",
                ])
                .arg(task_dir);
            let input = if block_stdin {
                vec![0; 4 * 1024 * 1024]
            } else {
                Vec::new()
            };
            nft_invoke_until(
                &mut command,
                &input,
                tokio::time::Instant::now() + Duration::from_millis(300),
            )
            .await
        })
        .into();
        tokio::time::timeout(Duration::from_secs(2), async {
            while !dir.join("pid").exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        if cancel {
            assert!(task.wait_for_abort().await.unwrap_err().is_cancelled());
        } else {
            assert_eq!(task.await.unwrap().unwrap_err().kind, ErrorKind::Timeout);
            let pid = std::fs::read_to_string(dir.join("pid"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            assert_eq!(
                nix::sys::wait::waitpid(
                    nix::unistd::Pid::from_raw(pid),
                    Some(nix::sys::wait::WaitPidFlag::WNOHANG),
                ),
                Err(nix::errno::Errno::ECHILD),
            );
        }
        tokio::time::sleep(Duration::from_millis(800)).await;
        assert!(!dir.join("sentinel").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn nft_timeout_covers_blocked_stdin_and_reaps_child() {
        assert_group_stopped(true, false).await;
    }

    #[tokio::test]
    async fn nft_timeout_kills_inherited_group_and_reaps_child() {
        assert_group_stopped(false, false).await;
    }

    #[tokio::test]
    async fn nft_cancellation_kills_inherited_group() {
        assert_group_stopped(true, true).await;
    }

    #[tokio::test]
    async fn nft_captures_output_and_errors() {
        let output = nft_invoke_until(
            Command::new("sh").args(["-c", "cat"]),
            b"transaction",
            nft_deadline(),
        )
        .await
        .unwrap();
        assert_eq!(output, b"transaction");
        let error = nft_invoke_until(
            Command::new("sh").args(["-c", "echo 'No such file or directory' >&2; exit 1"]),
            &[],
            nft_deadline(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Network);
        assert!(nft_error_is_stale(&error));
        let error = nft_invoke_until(
            Command::new("sh").args(["-c", "echo 'No such file or directory' >&2; exit 1"]),
            &vec![0; 4 * 1024 * 1024],
            nft_deadline(),
        )
        .await
        .unwrap_err();
        assert!(nft_error_is_stale(&error));
    }

    fn fake_nft(dir: &std::path::Path, script: &str) -> String {
        use std::os::unix::fs::PermissionsExt;
        let file = dir.join("nft");
        std::fs::write(
            &file,
            format!("#!/bin/sh\ncd -- \"$(dirname -- \"$0\")\"\n{script}\n"),
        )
        .unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o700)).unwrap();
        file.to_str().unwrap().to_owned()
    }

    #[tokio::test]
    async fn nft_batch_retries_share_base_listing_and_transaction_deadline() {
        let dir = scratch();
        let program = fake_nft(
            &dir,
            r#"
case "$1" in
    -a) echo list >> calls; sleep 0.1; echo 'accept comment "pinhole:test" # handle 1';;
    -f) cat > transaction; echo transaction >> calls; sleep 0.1; echo 'No such file or directory' >&2; exit 1;;
    *) echo base >> calls; sleep 0.1;;
esac
"#,
        );
        let deadline = tokio::time::Instant::now() + Duration::from_millis(650);
        let error = nft_delete_rules_matching_with_program_until(
            &program,
            "ip6",
            &["prerouting", "forward"],
            |comment| comment.starts_with("pinhole:"),
            deadline,
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Timeout);
        assert!(tokio::time::Instant::now() < deadline + Duration::from_secs(1));
        let calls = std::fs::read_to_string(dir.join("calls")).unwrap();
        assert_eq!(calls.lines().filter(|line| *line == "base").count(), 2);
        assert!(calls.lines().filter(|line| *line == "list").count() >= 4);
        assert!(calls.contains("transaction"));
        let transaction = std::fs::read_to_string(dir.join("transaction")).unwrap();
        assert!(transaction.contains("delete rule ip6 startos prerouting handle 1"));
        assert!(transaction.contains("delete rule ip6 startos forward handle 1"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn nft_batch_preserves_listing_error_without_transacting() {
        let dir = scratch();
        let program = fake_nft(
            &dir,
            r#"
case "$1" in
    -a) echo 'listing denied' >&2; exit 1;;
    -f) touch transaction;;
esac
"#,
        );
        let error = nft_delete_rules_matching_with_program_until(
            &program,
            "ip6",
            &["prerouting", "forward"],
            |_| true,
            nft_deadline(),
        )
        .await
        .unwrap_err();
        assert!(error.source.to_string().contains("listing denied"));
        assert!(!dir.join("transaction").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

const DYNAMIC_FORWARD_CHAINS: [&str; 4] = ["prerouting", "output", "postrouting", "forward"];

fn is_dynamic_forward_comment(family: &str, comment: &str) -> bool {
    let hash = match family {
        "ip" => comment.strip_prefix('F'),
        "ip6" => comment.strip_prefix("F6"),
        _ => None,
    };
    hash.is_some_and(|hash| hash.len() == 15 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

async fn remove_stale_dynamic_forwarding_rules_family(family: &str) -> Result<(), Error> {
    nft_delete_rules_matching(family, &DYNAMIC_FORWARD_CHAINS, |comment| {
        is_dynamic_forward_comment(family, comment)
    })
    .await
}

async fn initialize_port_forwarding() -> Result<(), Error> {
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
    remove_stale_dynamic_forwarding_rules_family("ip").await?;
    remove_stale_dynamic_forwarding_rules_family("ip6").await?;
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
        let drain_completion = Arc::new(SyncMutex::new(None::<PortForwardDrainFuture>));
        let actor_drain = drain_completion.clone();
        let thread = NonDetachingJoinHandle::from(tokio::spawn(async move {
            loop {
                match initialize().await {
                    Ok(()) => break,
                    Err(error) => {
                        tracing::error!("forwarding initialization failed: {error:#}");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
            let reject_pending = actor_drain.mutate(|completion| completion.is_some());
            let mut state = PortForwardState::default();
            let mut gc_interval = tokio::time::interval_at(
                tokio::time::Instant::now() + PORT_FORWARD_GC_INTERVAL,
                PORT_FORWARD_GC_INTERVAL,
            );
            gc_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                let cmd = tokio::select! {
                    cmd = req_recv.recv() => cmd,
                    _ = gc_interval.tick() => {
                        state.gc().await.log_err();
                        continue;
                    }
                };
                let Some(cmd) = cmd else {
                    break;
                };
                if (reject_pending && !matches!(&cmd, PortForwardCommand::Drain { .. }))
                    || cmd.response_is_closed()
                {
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
                    PortForwardCommand::Gc { respond } => {
                        respond.send(state.gc().await).ok();
                    }
                    PortForwardCommand::Dump { respond } => {
                        respond.send(state.dump()).ok();
                    }
                    PortForwardCommand::Drain { respond } => {
                        let mut attempt = 1usize;
                        loop {
                            match state.drain().await {
                                Ok(()) => {
                                    respond.send(Ok(())).ok();
                                    break;
                                }
                                Err(error) => {
                                    tracing::error!(
                                        "port forwarding drain failed on attempt {attempt}; retrying in {FORWARD_DRAIN_RETRY_INTERVAL:?}: {error:#}"
                                    );
                                    tracing::debug!("{error:?}");
                                    attempt = attempt.saturating_add(1);
                                    tokio::time::sleep(FORWARD_DRAIN_RETRY_INTERVAL).await;
                                }
                            }
                        }
                        break;
                    }
                }
            }
        }));

        Self {
            req: req_send,
            drain_completion,
            thread: SyncMutex::new(Some(thread)),
        }
    }

    fn send(&self, command: PortForwardCommand) -> Result<(), Error> {
        self.drain_completion.mutate(|completion| {
            if completion.is_some() {
                return Err(err_has_exited(()));
            }
            self.req.send(command).map_err(err_has_exited)
        })
    }

    /// Forwards contiguous TCP and UDP ports, mapping the two bases by offset.
    pub async fn add_forward_range(
        &self,
        source: SocketAddrV4,
        target: SocketAddrV4,
        count: u16,
        target_prefix: u8,
        src_filter: Option<IpNet>,
    ) -> Result<Arc<()>, Error> {
        let (send, recv) = oneshot::channel();
        self.send(PortForwardCommand::AddForward {
            source,
            target,
            count,
            target_prefix,
            src_filter,
            respond: send,
        })?;

        recv.await.map_err(err_has_exited)?
    }

    pub async fn gc(&self) -> Result<(), Error> {
        let (send, recv) = oneshot::channel();
        self.send(PortForwardCommand::Gc { respond: send })?;

        recv.await.map_err(err_has_exited)?
    }

    pub(crate) async fn drain(&self) -> Result<(), Error> {
        self.drain_until(tokio::time::Instant::now() + FORWARD_DRAIN_TIMEOUT)
            .await
    }

    pub(crate) async fn drain_until(&self, deadline: tokio::time::Instant) -> Result<(), Error> {
        let completion = self.drain_completion.mutate(|completion| {
            completion
                .get_or_insert_with(|| {
                    let (send, recv) = oneshot::channel();
                    self.req
                        .send(PortForwardCommand::Drain { respond: send })
                        .ok();
                    let thread = self.thread.mutate(Option::take).unwrap();
                    let owner = tokio::spawn(async move {
                        join_forward_actor(thread, recv, deadline)
                            .await
                            .map_err(Arc::new)
                    });
                    async move { owner.await.map_err(|e| Arc::new(err_has_exited(e)))? }
                        .boxed()
                        .shared()
                })
                .clone()
        });
        completion.await.map_err(|error| error.clone_output())
    }

    async fn dump(&self) -> Result<BTreeMap<SocketAddrV4, ForwardMapping>, Error> {
        let (send, recv) = oneshot::channel();
        self.send(PortForwardCommand::Dump { respond: send })?;

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
    target_prefix_fallback: u8,
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

    fn update_request(
        &mut self,
        InterfaceForwardRequest {
            external,
            target,
            count,
            target_prefix_fallback,
            reqs,
            rc,
        }: InterfaceForwardRequest,
    ) -> Result<Arc<()>, Error> {
        if external != self.external {
            return Err(Error::new(
                eyre!("{}", t!("net.forward.mismatched-external-port")),
                ErrorKind::InvalidRequest,
            ));
        }
        if count != self.count {
            // The range width applies to every target sharing this external start.
            self.count = count;
            self.targets.clear();
            self.forwards.clear();
        }

        Ok(self.cache_target(reqs, target, target_prefix_fallback, rc))
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
    port_forward: Arc<PortForwardController>,
    pmap: PortMapController,
    state: IdOrdMap<InterfaceForwardEntry>,
    ipv6: BTreeMap<SocketAddrV6, Ipv6ForwardMapping>,
}

impl InterfaceForwardState {
    fn new(port_forward: impl Into<Arc<PortForwardController>>, pmap: PortMapController) -> Self {
        Self {
            port_forward: port_forward.into(),
            pmap,
            state: IdOrdMap::new(),
            ipv6: BTreeMap::new(),
        }
    }
}

impl InterfaceForwardState {
    fn handle_request(&mut self, request: InterfaceForwardRequest) -> Result<Arc<()>, Error> {
        let count = request.count;
        self.state
            .entry(request.external)
            .or_insert_with(|| InterfaceForwardEntry::new(request.external, count))
            .update_request(request)
    }

    fn add_forward6(&mut self, source: SocketAddrV6, spec: Ipv6ForwardSpec) -> Arc<()> {
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
        rc
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
        for mapping in self.ipv6.values_mut() {
            mapping.rc = Weak::new();
        }

        self.reconcile_forwards6(&OrdMap::new()).await
    }

    async fn drain_until_complete(&mut self) -> Result<(), Error> {
        let mut attempt = 1usize;
        loop {
            match self.drain().await {
                Ok(()) => return Ok(()),
                Err(error) => {
                    tracing::error!(
                        "interface forwarding drain failed on attempt {attempt}; retrying in {FORWARD_DRAIN_RETRY_INTERVAL:?}: {error:#}"
                    );
                    tracing::debug!("{error:?}");
                    attempt = attempt.saturating_add(1);
                    tokio::time::sleep(FORWARD_DRAIN_RETRY_INTERVAL).await;
                }
            }
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
        oneshot::Sender<Result<Arc<()>, Error>>,
    ),
    Forward6 {
        source: SocketAddrV6,
        spec: Ipv6ForwardSpec,
        respond: oneshot::Sender<Arc<()>>,
    },
    Sync(oneshot::Sender<Result<(), Error>>),
    DumpTable(oneshot::Sender<Result<ForwardTable, Error>>),
    Drain(oneshot::Sender<Result<(), Error>>),
}

pub struct InterfacePortForwardController {
    req: mpsc::UnboundedSender<InterfaceForwardCommand>,
    cancel: CancellationToken,
    thread: SyncMutex<Option<NonDetachingJoinHandle<()>>>,
    port_forward: Arc<PortForwardController>,
    drain_completion: SyncMutex<Option<PortForwardDrainFuture>>,
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
        let port_forward = Arc::new(port_forward);
        let nested = port_forward.clone();
        let (req_send, mut req_recv) = mpsc::unbounded_channel::<InterfaceForwardCommand>();
        let thread = NonDetachingJoinHandle::from(tokio::spawn(async move {
            let mut state = InterfaceForwardState::new(nested, pmap);
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
                                if actor_cancel.is_cancelled() {
                                    break 'active;
                                }
                                re.send(state.handle_request(req)).ok()
                            }
                            InterfaceForwardCommand::Forward6 { source, spec, respond } => {
                                if actor_cancel.is_cancelled() {
                                    break 'active;
                                }
                                respond.send(state.add_forward6(source, spec)).ok()
                            }
                            InterfaceForwardCommand::Sync(re) => {
                                let result = tokio::select! {
                                    biased;
                                    _ = actor_cancel.cancelled() => break 'active,
                                    result = state.port_forward.dump() => result.map(|_| ()),
                                };
                                let result = match result {
                                    Ok(()) => state.sync(&interfaces).await,
                                    Err(error) => Err(error),
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
                                respond.send(state.drain_until_complete().await).ok();
                                return;
                            }
                        };
                    }
                    _ = ip_info.changed() => {
                        interfaces = ip_info.read();
                        tokio::select! {
                            biased;
                            _ = actor_cancel.cancelled() => break,
                            result = state.port_forward.dump() => {
                                if result.log_err().is_none() { continue; }
                            },
                        };
                        state.sync(&interfaces).await.log_err();
                    }
                    _ = reconcile_interval.tick() => {
                        tokio::select! {
                            biased;
                            _ = actor_cancel.cancelled() => break,
                            result = state.port_forward.dump() => {
                                if result.log_err().is_none() { continue; }
                            },
                        };
                        state.reconcile(&interfaces).await.log_err();
                    }
                }
            }
            while let Some(cmd) = req_recv.recv().await {
                if let InterfaceForwardCommand::Drain(respond) = cmd {
                    respond.send(state.drain_until_complete().await).ok();
                    return;
                }
            }
        }));

        Self {
            req: req_send,
            cancel,
            thread: SyncMutex::new(Some(thread)),
            port_forward,
            drain_completion: SyncMutex::new(None),
        }
    }

    fn send(&self, command: InterfaceForwardCommand) -> Result<(), Error> {
        self.drain_completion.mutate(|completion| {
            if completion.is_some() {
                return Err(err_has_exited(()));
            }
            self.req.send(command).map_err(err_has_exited)
        })
    }

    pub(super) async fn forward6(
        &self,
        source: SocketAddrV6,
        target: SocketAddrV6,
        target_prefix: u8,
        src_filter: Option<IpNet>,
    ) -> Result<Arc<()>, Error> {
        let (respond, receive) = oneshot::channel();
        self.send(InterfaceForwardCommand::Forward6 {
            source,
            spec: Ipv6ForwardSpec {
                target,
                target_prefix,
                src_filter,
            },
            respond,
        })?;
        receive.await.map_err(err_has_exited)
    }

    pub(super) async fn add_range(
        &self,
        external: u16,
        count: u16,
        reqs: ForwardRequirements,
        target: SocketAddrV4,
        target_prefix: u8,
    ) -> Result<Arc<()>, Error> {
        let rc = Arc::new(());
        let (send, recv) = oneshot::channel();
        self.send(InterfaceForwardCommand::Forward(
            InterfaceForwardRequest {
                external,
                target,
                count,
                target_prefix_fallback: target_prefix,
                reqs,
                rc,
            },
            send,
        ))?;

        recv.await.map_err(err_has_exited)?
    }

    pub async fn gc(&self) -> Result<(), Error> {
        let (send, recv) = oneshot::channel();
        self.send(InterfaceForwardCommand::Sync(send))?;

        recv.await.map_err(err_has_exited)?
    }

    pub async fn dump_table(&self) -> Result<ForwardTable, Error> {
        let (req, res) = oneshot::channel();
        self.send(InterfaceForwardCommand::DumpTable(req))?;
        res.await.map_err(err_has_exited)?
    }

    pub async fn drain(&self) -> Result<(), Error> {
        self.drain_until(tokio::time::Instant::now() + FORWARD_DRAIN_TIMEOUT)
            .await
    }

    pub(crate) async fn drain_until(&self, deadline: tokio::time::Instant) -> Result<(), Error> {
        let completion = self.drain_completion.mutate(|completion| {
            completion
                .get_or_insert_with(|| {
                    self.cancel.cancel();
                    let (send, recv) = oneshot::channel();
                    self.req.send(InterfaceForwardCommand::Drain(send)).ok();
                    let thread = self.thread.mutate(Option::take).unwrap();
                    let nested = self.port_forward.clone();
                    let owner = tokio::spawn(async move {
                        let (outer, inner) = tokio::join!(
                            join_forward_actor(thread, recv, deadline),
                            nested.drain_until(deadline)
                        );
                        outer.and(inner).map_err(Arc::new)
                    });
                    async move { owner.await.map_err(|e| Arc::new(err_has_exited(e)))? }
                        .boxed()
                        .shared()
                })
                .clone()
        });
        completion.await.map_err(|error| error.clone_output())
    }
}

async fn forward(
    source: SocketAddrV4,
    target: SocketAddrV4,
    count: u16,
    target_prefix: u8,
    src_filter: Option<&IpNet>,
) -> Result<(), Error> {
    invoke_forward(source, target, count, target_prefix, src_filter, false).await
}

async fn unforward(
    source: SocketAddrV4,
    target: SocketAddrV4,
    count: u16,
    target_prefix: u8,
    src_filter: Option<&IpNet>,
) -> Result<(), Error> {
    invoke_forward(source, target, count, target_prefix, src_filter, true).await
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
    invoke_forward6(source, target, target_prefix, src_filter, false).await
}

/// Tear down a forward created by [`forward6`]. Passes the same identifying env
/// so the script recomputes the matching comment tag.
pub(crate) async fn unforward6(
    source: SocketAddrV6,
    target: SocketAddrV6,
    target_prefix: u8,
    src_filter: Option<&IpNet>,
) -> Result<(), Error> {
    invoke_forward6(source, target, target_prefix, src_filter, true).await
}

#[cfg(test)]
tokio::task_local! {
    static FORWARD_TEST_CALLS: std::cell::RefCell<(Vec<(bool, SocketAddrV4, u16)>, bool)>;
}

async fn invoke_forward(
    source: SocketAddrV4,
    target: SocketAddrV4,
    count: u16,
    target_prefix: u8,
    src_filter: Option<&IpNet>,
    undo: bool,
) -> Result<(), Error> {
    #[cfg(test)]
    if let Ok(result) = FORWARD_TEST_CALLS.try_with(|calls| {
        let mut calls = calls.borrow_mut();
        calls.0.push((undo, source, count));
        if undo && calls.1 {
            Err(Error::new(
                eyre!("injected removal failure"),
                ErrorKind::Network,
            ))
        } else {
            Ok(())
        }
    }) {
        return result;
    }
    let mut cmd = Command::new("/usr/lib/startos/scripts/forward-port");
    cmd.env("sip", source.ip().to_string())
        .env("dip", target.ip().to_string())
        .env("dprefix", target_prefix.to_string())
        .env("sport", source.port().to_string())
        .env("dport", target.port().to_string())
        .env("count", count.to_string());
    if undo {
        cmd.env("UNDO", "1");
    }
    if let Some(subnet) = src_filter {
        cmd.env("src_subnet", subnet.to_string());
    }
    cmd.kill_process_group_on_drop()
        .timeout(Some(FORWARD_SCRIPT_TIMEOUT))
        .invoke(ErrorKind::Network)
        .await?;
    Ok(())
}

async fn invoke_forward6(
    source: SocketAddrV6,
    target: SocketAddrV6,
    target_prefix: u8,
    src_filter: Option<&IpNet>,
    undo: bool,
) -> Result<(), Error> {
    let mut cmd = Command::new("/usr/lib/startos/scripts/forward-port6");
    cmd.env("sip", source.ip().to_string())
        .env("dip", target.ip().to_string())
        .env("dprefix", target_prefix.to_string())
        .env("sport", source.port().to_string())
        .env("dport", target.port().to_string());
    if !undo {
        cmd.env("bridge_subnet", START9_BRIDGE_V6_SUBNET);
    }
    if undo {
        cmd.env("UNDO", "1");
    }
    if let Some(subnet) = src_filter {
        cmd.env("src_subnet", subnet.to_string());
    }
    cmd.kill_process_group_on_drop()
        .timeout(Some(FORWARD_SCRIPT_TIMEOUT))
        .invoke(ErrorKind::Network)
        .await?;
    Ok(())
}

pub(crate) async fn drain_forwarding<F, P>(forward: F, port_map: P) -> Result<(), Error>
where
    F: Future<Output = Result<(), Error>>,
    P: Future<Output = Result<(), Error>>,
{
    match tokio::join!(forward, port_map) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(forward_error), Err(port_map_error)) => Err(Error::new(
            eyre!(
                "forwarding drains failed: interface forwarding: {forward_error:#}; port mapping: {port_map_error:#}"
            ),
            ErrorKind::Network,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn overlapping_ranges_retire_dead_rules_before_installing() {
        FORWARD_TEST_CALLS
            .scope(std::cell::RefCell::new((Vec::new(), false)), async {
                let mut state = PortForwardState::default();
                let source = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 40000);
                let target = SocketAddrV4::new(Ipv4Addr::new(10, 0, 3, 2), 40000);
                let lease = state
                    .add_forward(source, target, 10, 24, None)
                    .await
                    .unwrap();
                let shared = state
                    .add_forward(source, target, 10, 24, None)
                    .await
                    .unwrap();
                assert!(Arc::ptr_eq(&lease, &shared));
                let overlap = SocketAddrV4::new(*source.ip(), 40009);
                assert!(
                    state
                        .add_forward(overlap, target, 2, 24, None)
                        .await
                        .is_err()
                );
                drop((lease, shared));
                FORWARD_TEST_CALLS.with(|calls| calls.borrow_mut().1 = true);
                assert!(
                    state
                        .add_forward(overlap, target, 2, 24, None)
                        .await
                        .is_err()
                );
                assert!(state.mappings.contains_key(&source));
                assert!(!state.mappings.contains_key(&overlap));
                FORWARD_TEST_CALLS.with(|calls| calls.borrow_mut().1 = false);
                let _lease = state
                    .add_forward(overlap, target, 2, 24, None)
                    .await
                    .unwrap();
                assert!(!state.mappings.contains_key(&source));
                FORWARD_TEST_CALLS.with(|calls| {
                    assert_eq!(
                        calls.borrow().0,
                        vec![
                            (false, source, 10),
                            (true, source, 10),
                            (true, source, 10),
                            (false, overlap, 2),
                        ]
                    )
                });
            })
            .await;
    }

    #[tokio::test]
    async fn forwarding_range_boundaries_and_same_key_replacement() {
        FORWARD_TEST_CALLS
            .scope(std::cell::RefCell::new((Vec::new(), false)), async {
                let mut state = PortForwardState::default();
                let source = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 40000);
                let target = SocketAddrV4::new(Ipv4Addr::new(10, 0, 3, 2), 40000);
                let _original = state
                    .add_forward(source, target, 10, 24, None)
                    .await
                    .unwrap();
                let adjacent = SocketAddrV4::new(*source.ip(), 40010);
                let _adjacent = state
                    .add_forward(adjacent, target, 1, 24, None)
                    .await
                    .unwrap();
                let other_ip = SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 1), 40000);
                let _other = state
                    .add_forward(other_ip, target, 10, 24, None)
                    .await
                    .unwrap();
                assert!(
                    state
                        .add_forward(source, target, 11, 24, None)
                        .await
                        .is_err()
                );
                FORWARD_TEST_CALLS.with(|calls| calls.borrow_mut().1 = true);
                assert!(
                    state
                        .add_forward(source, target, 5, 24, None)
                        .await
                        .is_err()
                );
                assert_eq!(state.mappings[&source].count, 10);
                FORWARD_TEST_CALLS.with(|calls| calls.borrow_mut().1 = false);
                let _replacement = state
                    .add_forward(source, target, 5, 24, None)
                    .await
                    .unwrap();
                assert_eq!(state.mappings[&source].count, 5);
            })
            .await;
    }

    #[tokio::test]
    async fn drain_rejects_queued_preinitialization_requests() {
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
        assert!(futures::poll!(&mut request).is_pending());
        let drain = controller.drain();
        tokio::pin!(drain);
        assert!(futures::poll!(&mut drain).is_pending());
        initialize.notify_one();
        drain.await.unwrap();
        assert!(request.await.is_err());
        assert!(controller.req.is_closed());
    }

    #[tokio::test]
    async fn cancelled_drain_waiter_still_joins_initialization_on_deadline() {
        let controller =
            PortForwardController::spawn(|| std::future::pending::<Result<(), Error>>());
        let mut first = Box::pin(
            controller.drain_until(tokio::time::Instant::now() + Duration::from_millis(30)),
        );
        assert!(futures::poll!(&mut first).is_pending());
        drop(first);
        let error = tokio::time::timeout(Duration::from_secs(1), controller.drain())
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Timeout);
        assert!(controller.req.is_closed());
        assert!(controller.thread.mutate(|thread| thread.is_none()));
    }

    #[tokio::test]
    async fn interface_deadline_joins_nested_initialization() {
        let interfaces = Watch::new(OrdMap::new());
        let controller = InterfacePortForwardController::with_port_forward(
            interfaces.clone(),
            PortMapController::new(interfaces),
            PortForwardController::spawn(|| std::future::pending::<Result<(), Error>>()),
        );
        let _lease = controller
            .forward6(
                SocketAddrV6::new("2001:db8::1".parse().unwrap(), 80, 0, 0),
                ipv6_spec().target,
                64,
                None,
            )
            .await
            .unwrap();
        let gc = controller.gc();
        tokio::pin!(gc);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut gc)
                .await
                .is_err()
        );
        assert!(
            controller
                .drain_until(tokio::time::Instant::now() + Duration::from_millis(30))
                .await
                .is_err()
        );
        assert!(controller.req.is_closed());
        assert!(controller.port_forward.req.is_closed());
        assert!(gc.await.is_err());
    }

    fn ipv6_spec() -> Ipv6ForwardSpec {
        Ipv6ForwardSpec {
            target: SocketAddrV6::new("fd00:3::2".parse().unwrap(), 8080, 0, 0),
            target_prefix: 64,
            src_filter: None,
        }
    }

    #[test]
    fn dynamic_forward_comment_detection_preserves_other_owners() {
        assert!(is_dynamic_forward_comment("ip", "F0123456789abcde"));
        assert!(is_dynamic_forward_comment("ip6", "F60123456789abcde"));
        assert!(!is_dynamic_forward_comment("ip", "base-established"));
        assert!(!is_dynamic_forward_comment("ip", "F60123456789abcde"));
        assert!(!is_dynamic_forward_comment("ip6", "F0123456789abcde"));
        assert!(!is_dynamic_forward_comment("ip", "ForwardedBySomeoneElse"));
    }

    #[test]
    fn batch_delete_selects_only_dynamic_forward_rules() {
        let listing = r#"
            ip daddr 192.0.2.1 comment "F0123456789abcde" # handle 10
            ip daddr 192.0.2.2 comment "Fshort" # handle 11
            ip daddr 192.0.2.3 comment "ForwardedBySomeoneElse" # handle 12
            ip daddr 192.0.2.4 comment "F60123456789abcde" # handle 13
        "#;

        assert_eq!(
            nft_delete_rules_matching_script("ip", &[("prerouting", listing)], &|comment| {
                is_dynamic_forward_comment("ip", comment)
            }),
            "delete rule ip startos prerouting handle 10\n"
        );
    }

    #[test]
    fn batch_delete_selects_pinhole_rules_across_chains() {
        let prerouting = r#"
            ip6 daddr fd00::2 comment "pinhole:[fd00::2]:80" # handle 10
            ip6 daddr fd00::3 comment "base-established" # handle 11
            ip6 daddr fd00::4 comment "other:pinhole:[fd00::4]:80" # handle 12
        "#;
        let forward = r#"
            ip6 daddr fd00::2 comment "pinhole:[fd00::2]:80" # handle 20
            ip6 daddr fd00::2 comment "pinhole:[fd00::2]:80" # handle 21
        "#;

        assert_eq!(
            nft_delete_rules_matching_script(
                "ip6",
                &[("prerouting", prerouting), ("forward", forward)],
                &|comment| comment.starts_with("pinhole:"),
            ),
            "delete rule ip6 startos prerouting handle 10\n\
             delete rule ip6 startos forward handle 20\n\
             delete rule ip6 startos forward handle 21\n"
        );
    }

    #[test]
    fn batch_delete_is_empty_when_converged() {
        assert!(
            nft_delete_rules_matching_script(
                "ip6",
                &[("prerouting", ""), ("forward", "")],
                &|comment| comment.starts_with("pinhole:"),
            )
            .is_empty()
        );
    }

    #[test]
    fn batch_delete_many_rules_remains_one_script() {
        let listing = (0..10_000)
            .map(|handle| {
                format!(
                    "ip6 daddr fd00::{handle:x} comment \"pinhole:[fd00::{handle:x}]:80\" # handle {handle}\n"
                )
            })
            .collect::<String>();
        let script =
            nft_delete_rules_matching_script("ip6", &[("forward", &listing)], &|comment| {
                comment.starts_with("pinhole:")
            });

        assert_eq!(script.lines().count(), 10_000);
        assert!(script.starts_with("delete rule ip6 startos forward handle 0\n"));
        assert!(script.ends_with("delete rule ip6 startos forward handle 9999\n"));
    }

    #[test]
    fn stale_nft_error_detection_is_specific() {
        assert!(nft_error_is_stale(&Error::new(
            eyre!("Could not process rule: No such file or directory"),
            ErrorKind::Network,
        )));
        assert!(!nft_error_is_stale(&Error::new(
            eyre!("Operation not permitted"),
            ErrorKind::Network,
        )));
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
    fn request_seeds_caller_current_target_prefix() {
        let requirements = ForwardRequirements {
            public_gateways: BTreeSet::new(),
            private_ips: BTreeSet::new(),
            secure: true,
        };
        let target = SocketAddrV4::new(Ipv4Addr::new(10, 0, 3, 2), 8080);

        let mut entry = InterfaceForwardEntry::new(8080, 1);
        let lease = entry
            .update_request(InterfaceForwardRequest {
                external: 8080,
                target,
                count: 1,
                target_prefix_fallback: 24,
                reqs: requirements.clone(),
                rc: Arc::new(()),
            })
            .unwrap();

        assert_eq!(entry.targets[&requirements].1, 24);
        drop(lease);
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
        assert_eq!(mapping.next_operation(), Some(operation.clone()));
        mapping.operation_succeeded(operation);
        assert_eq!(mapping.applied, None);
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
    async fn drain_during_initialization_waits_for_stale_rule_cleanup() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

        let initialization_started = Arc::new(AtomicUsize::new(0));
        let cleanup_complete = Arc::new(AtomicBool::new(false));
        let initialize = Arc::new(tokio::sync::Notify::new());
        let controller = PortForwardController::spawn({
            let initialization_started = initialization_started.clone();
            let cleanup_complete = cleanup_complete.clone();
            let initialize = initialize.clone();
            move || {
                initialization_started.fetch_add(1, Ordering::SeqCst);
                let cleanup_complete = cleanup_complete.clone();
                let initialize = initialize.clone();
                async move {
                    initialize.notified().await;
                    cleanup_complete.store(true, Ordering::SeqCst);
                    Ok(())
                }
            }
        });

        while initialization_started.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }

        let first = controller.drain();
        tokio::pin!(first);
        assert!(futures::poll!(&mut first).is_pending());
        assert!(!cleanup_complete.load(Ordering::SeqCst));

        initialize.notify_waiters();
        let (first_result, concurrent) = tokio::join!(first, controller.drain());
        first_result.unwrap();
        concurrent.unwrap();
        controller.drain().await.unwrap();
        assert!(controller.dump().await.is_err());
        assert!(cleanup_complete.load(Ordering::SeqCst));
        assert_eq!(initialization_started.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn aggregate_drain_polls_both_and_reports_both_errors() {
        let (forward_started, forward_received) = oneshot::channel();
        let (port_map_started, port_map_received) = oneshot::channel();
        let (release_forward, forward_released) = oneshot::channel();
        let (release_port_map, port_map_released) = oneshot::channel();
        let drain = tokio::spawn(drain_forwarding(
            async move {
                forward_started.send(()).unwrap();
                forward_released.await.unwrap();
                Err(Error::new(eyre!("forward failed"), ErrorKind::Network))
            },
            async move {
                port_map_started.send(()).unwrap();
                port_map_released.await.unwrap();
                Err(Error::new(eyre!("port map failed"), ErrorKind::Network))
            },
        ));

        forward_received.await.unwrap();
        port_map_received.await.unwrap();
        release_forward.send(()).unwrap();
        release_port_map.send(()).unwrap();

        let error = drain.await.unwrap().unwrap_err();
        assert_eq!(error.kind, ErrorKind::Network);
        assert!(error.to_string().contains("forward failed"));
        assert!(error.to_string().contains("port map failed"));
    }

    #[tokio::test]
    async fn aggregate_drain_waits_for_workers_after_a_timeout_error() {
        let (release, joined) = oneshot::channel();
        let drain = drain_forwarding(
            async { Err(Error::new(eyre!("deadline expired"), ErrorKind::Timeout)) },
            async {
                joined.await.unwrap();
                Ok(())
            },
        );
        tokio::pin!(drain);
        assert!(futures::poll!(&mut drain).is_pending());
        release.send(()).unwrap();
        assert_eq!(drain.await.unwrap_err().kind, ErrorKind::Timeout);
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
        let initialize = Arc::new(tokio::sync::Notify::new());
        let port_forward = PortForwardController::spawn({
            let initialize = initialize.clone();
            move || {
                let initialize = initialize.clone();
                async move {
                    initialize.notified().await;
                    Ok(())
                }
            }
        });
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
                24,
            )
            .await
            .unwrap();
        let gc = controller.gc();
        tokio::pin!(gc);

        assert!(futures::poll!(&mut gc).is_pending());

        let drain = controller.drain();
        tokio::pin!(drain);
        assert!(futures::poll!(&mut drain).is_pending());
        initialize.notify_one();
        tokio::time::timeout(Duration::from_secs(1), &mut drain)
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
