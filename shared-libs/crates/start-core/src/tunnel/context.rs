use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use clap::Parser;
use http::HeaderMap;
use http::header::AUTHORIZATION;
use imbl::OrdMap;
use imbl_value::InternedString;
use include_dir::Dir;
use ipnet::{IpNet, Ipv6Net};
use patch_db::PatchDb;
use patch_db::json_ptr::ROOT;
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{CallRemote, Context, Empty, ParentHandler};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::sync::broadcast::Sender;
use tracing::instrument;

use crate::GatewayId;
use crate::context::CliContext;
use crate::context::config::ContextConfig;
use crate::db::model::public::{NetworkInterfaceInfo, NetworkInterfaceType};
use crate::middleware::auth::Auth;
use crate::middleware::auth::local::{LocalAuthContext, dial_addr, local_auth_header};
use crate::middleware::auth::signature::{NonceCache, url_host_str};
use crate::middleware::db::SyncDb;
use crate::net::dns_update::rfc2136::{DnsInjector, InjectedRecord, UpdateAuth};
use crate::net::forward::{PortForwardController, nft_comments_with_prefix, nft_rule, nft_rule_v6};
use crate::net::static_server::{EMPTY_DIR, UiContext};
use crate::net::transport::Transport;
use crate::prelude::*;
use crate::rpc_continuations::{OpenAuthedContinuations, RpcContinuations};
use crate::tunnel::TUNNEL_DEFAULT_LISTEN;

/// Legacy HTTP RPC version a tunnel serves.
const TUNNEL_HTTP_RPC_VERSION: u8 = 0;
use crate::tunnel::api::tunnel_api;
use crate::tunnel::db::{
    DnsRecordEntry, DnsRecords, PortForward, PortForwards, SniRoute, TunnelDatabase,
};
use crate::tunnel::dns::DnsProxyController;
use crate::tunnel::forward::address::ForwardAddress;
use crate::tunnel::forward::lease::{LeaseKey, Leases};
use crate::tunnel::migrations::run_migrations;
use crate::tunnel::web::WebserverInfo;
use crate::tunnel::wg::{WIREGUARD_INTERFACE_NAME, WgServer, current_ifindex};
use crate::util::Invoke;
use crate::util::collections::OrdMapIterMut;
use crate::util::sync::{SyncMutex, Watch};

#[derive(Debug, Clone, Default, Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "kebab-case")]
#[command(rename_all = "kebab-case")]
pub struct TunnelConfig {
    #[arg(short = 'c', long = "config", help = "help.arg.config-file-path")]
    pub config: Option<PathBuf>,
    #[arg(short = 'l', long = "listen", help = "help.arg.tunnel-listen-address")]
    pub tunnel_listen: Option<SocketAddr>,
    #[arg(short = 'd', long = "datadir", help = "help.arg.data-directory")]
    pub datadir: Option<PathBuf>,
}
impl ContextConfig for TunnelConfig {
    fn next(&mut self) -> Option<PathBuf> {
        self.config.take()
    }
    fn merge_with(&mut self, other: Self) {
        self.tunnel_listen = self.tunnel_listen.take().or(other.tunnel_listen);
        self.datadir = self.datadir.take().or(other.datadir);
    }
}

impl TunnelConfig {
    pub fn load(mut self) -> Result<Self, Error> {
        let path = self.next();
        self.load_path_rec(path)?;
        self.load_path_rec(Some("/etc/start-tunneld"))?;
        Ok(self)
    }
}

/// WireGuard client IPs whose `allow_dns_injection` toggle is on.
fn allowed_injectors(server: &WgServer) -> BTreeSet<IpAddr> {
    let mut out = BTreeSet::new();
    for (_, subnet) in &server.subnets.0 {
        for (ip, client) in &subnet.clients.0 {
            if client.allow_dns_injection {
                out.insert(IpAddr::V4(*ip));
                // The client's v6 under the subnet's delegated prefix is the
                // same device — a v6-sourced UPDATE is equally authorized.
                if let Some(prefix) = subnet.ipv6 {
                    out.insert(IpAddr::V6(crate::tunnel::wg6::host_v6(prefix, *ip)));
                }
            }
        }
    }
    out
}

/// Allowed-injector client IPs -> their per-device TSIG key (derived from the
/// WireGuard PSK), so the injector can cryptographically verify each UPDATE.
fn injector_keys(server: &WgServer) -> BTreeMap<IpAddr, [u8; 32]> {
    let mut out = BTreeMap::new();
    for (_, subnet) in &server.subnets.0 {
        for (ip, client) in &subnet.clients.0 {
            if client.allow_dns_injection {
                let key = crate::net::dns_update::derive_tsig_key(&client.psk.0);
                out.insert(IpAddr::V4(*ip), key);
                if let Some(prefix) = subnet.ipv6 {
                    out.insert(IpAddr::V6(crate::tunnel::wg6::host_v6(prefix, *ip)), key);
                }
            }
        }
    }
    out
}

/// Seed the injector from persisted records, dropping any that no longer parse.
fn seed_records(records: &DnsRecords) -> Vec<InjectedRecord> {
    records
        .0
        .iter()
        .filter_map(|e| {
            let src = e
                .source
                .as_deref()
                .and_then(|s| s.parse().ok())
                .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
            InjectedRecord::from_parts(&e.name, &e.rtype, &e.value, e.ttl, src).ok()
        })
        .collect()
}

fn dns_entry(r: &InjectedRecord) -> DnsRecordEntry {
    let (name, rtype, value, ttl, source) = r.to_parts();
    DnsRecordEntry {
        name,
        rtype,
        value,
        ttl,
        source: source.map(|s| s.to_string()),
    }
}

/// Install a client's /128 host route over the wg interface. Best-effort like
/// [`neigh_proxy`]. `replace`-ing every client is a full reconcile — the
/// `wg-quick` bounce preceding every resync cleared the interface's routes.
async fn client_route(addr: Ipv6Addr) {
    Command::new("ip")
        .arg("-6")
        .arg("route")
        .arg("replace")
        .arg(format!("{addr}/128"))
        .arg("dev")
        .arg(WIREGUARD_INTERFACE_NAME)
        .invoke(ErrorKind::Network)
        .await
        .log_err();
}

/// Add or remove a proxy-NDP entry so the tunnel host answers Neighbor
/// Discovery for a client's on-link IPv6 on the WAN interface. Best-effort:
/// failures are logged, not fatal (reconciled on the next resync).
async fn neigh_proxy(add: bool, addr: Ipv6Addr, iface: &str) {
    Command::new("ip")
        .arg("-6")
        .arg("neigh")
        .arg(if add { "replace" } else { "del" })
        .arg("proxy")
        .arg(addr.to_string())
        .arg("dev")
        .arg(iface)
        .invoke(ErrorKind::Network)
        .await
        .log_err();
}

pub struct TunnelContextSeed {
    pub listen: SocketAddr,
    pub db: TypedPatchDb<TunnelDatabase>,
    pub datadir: PathBuf,
    pub rpc_continuations: RpcContinuations,
    pub open_authed_continuations: OpenAuthedContinuations<Option<InternedString>>,
    pub auth_sig_nonce_cache: SyncMutex<NonceCache>,
    pub server_identity: crate::sign::Signer,
    pub net_iface: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    pub forward: PortForwardController,
    pub dns_proxy: DnsProxyController,
    pub sni: Arc<crate::tunnel::forward::sni::SniDemux>,
    pub dns_injector: Arc<DnsInjector>,
    /// Injector authorizer reads this live, so a toggle change takes effect
    /// without rebuilding the injector.
    pub dns_allowed: Arc<SyncMutex<BTreeSet<IpAddr>>>,
    /// Per-injector TSIG keys, read live so the injector can verify UPDATEs.
    pub dns_keys: Arc<SyncMutex<BTreeMap<IpAddr, [u8; 32]>>>,
    pub active_forwards: SyncMutex<BTreeMap<ForwardAddress, Arc<()>>>,
    /// Label edits must serialize with staged-forward rollback.
    pub forward_write_lock: tokio::sync::Mutex<()>,
    /// In-memory leases for auto (PCP-created) forwards/pinholes/SNI routes,
    /// reaped by [`crate::tunnel::forward::lease`] when a client stops renewing.
    pub leases: SyncMutex<crate::tunnel::forward::lease::Leases>,
    /// Wakes the lease reaper when a stamp may have moved the soonest expiry
    /// earlier, so it can pull its next wake-up forward.
    pub lease_wake: tokio::sync::Notify,
    /// Current WireGuard ifindex; listeners rebind after interface recreation.
    pub forward_ifindex: tokio::sync::watch::Sender<u32>,
    /// Serializes `resync_egress`; its read-DB → install → prune isn't atomic,
    /// so a concurrent reconcile could prune a rule another call just installed.
    pub egress_lock: tokio::sync::Mutex<()>,
    /// Proxy-NDP entries we've installed (client GUA -> WAN interface), so a
    /// resync can withdraw the ones that no longer apply. Only populated when a
    /// client's /128 sits inside an on-link /64 (the shared-prefix case).
    pub v6_proxy: SyncMutex<BTreeMap<Ipv6Addr, GatewayId>>,
    /// Serializes `resync_v6` — same non-atomic read → diff → apply → overwrite
    /// pattern as `egress_lock` guards for `resync_egress`, so two concurrent
    /// config changes can't leave the tracked proxy map out of sync with the
    /// kernel's neighbor table.
    pub v6_lock: tokio::sync::Mutex<()>,
    pub shutdown: Sender<Option<bool>>,
}

/// Also tightens the files an earlier release left world-readable.
async fn restrict_datadir(datadir: &Path) -> Result<(), Error> {
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;

    tokio::fs::create_dir_all(datadir).await?;
    tokio::fs::set_permissions(datadir, Permissions::from_mode(0o700)).await?;
    for name in ["tunnel.db", "tunnel.bak", "tunnel.bak.tmp", "tunnel.failed"] {
        match tokio::fs::set_permissions(datadir.join(name), Permissions::from_mode(0o600)).await {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => (),
        }
    }
    Ok(())
}

#[derive(Clone)]
pub struct TunnelContext(
    Arc<TunnelContextSeed>,
    Option<Arc<crate::middleware::auth::session::SecureSession>>,
);

impl SessionContext for TunnelContext {
    fn secure_session(&self) -> Option<&crate::middleware::auth::session::SecureSession> {
        self.1.as_deref()
    }
}
impl TunnelContext {
    pub(crate) fn replay_check(&self) -> crate::middleware::auth::signature::RequestReplayCheck {
        crate::middleware::auth::signature::RequestReplayCheck::new(&self.0, |seed| {
            Some(Self(seed, None))
        })
    }
    pub(crate) fn server_identity(&self) -> Result<crate::sign::Signer, Error> {
        Ok(self.server_identity.clone())
    }
    pub(crate) fn with_secure_session(
        mut self,
        session: crate::middleware::auth::session::SecureSession,
    ) -> Self {
        self.1 = Some(Arc::new(session));
        self
    }
    #[cfg(test)]
    pub(in crate::tunnel) async fn for_test(initial: TunnelDatabase) -> (tempfile::TempDir, Self) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("tunnel.db");
        let db =
            TypedPatchDb::<TunnelDatabase>::load_unchecked(PatchDb::open(&db_path).await.unwrap());
        db.put(&ROOT, &initial).await.unwrap();
        let (shutdown, _) = tokio::sync::broadcast::channel(1);
        let ctx = Self(
            Arc::new(TunnelContextSeed {
                listen: SocketAddr::from(([127, 0, 0, 1], 0)),
                db,
                datadir: dir.path().to_path_buf(),
                rpc_continuations: RpcContinuations::new(),
                open_authed_continuations: OpenAuthedContinuations::new(),
                auth_sig_nonce_cache: SyncMutex::new(Default::default()),
                server_identity: crate::sign::AnySigningKey::generate().into(),
                net_iface: Watch::new(OrdMap::new()),
                forward: PortForwardController::failing_for_test(),
                dns_proxy: DnsProxyController::new(),
                sni: crate::tunnel::forward::sni::SniDemux::new(),
                dns_injector: DnsInjector::new(
                    Vec::new(),
                    |_| false,
                    |_| None,
                    |_| {},
                    |_, _, _| async { hickory_server::proto::op::ResponseCode::Refused },
                ),
                dns_allowed: Arc::new(SyncMutex::new(BTreeSet::new())),
                dns_keys: Arc::new(SyncMutex::new(BTreeMap::new())),
                active_forwards: SyncMutex::new(BTreeMap::new()),
                forward_write_lock: tokio::sync::Mutex::new(()),
                leases: SyncMutex::new(BTreeMap::new()),
                lease_wake: tokio::sync::Notify::new(),
                forward_ifindex: tokio::sync::watch::channel(0).0,
                egress_lock: tokio::sync::Mutex::new(()),
                v6_proxy: SyncMutex::new(BTreeMap::new()),
                v6_lock: tokio::sync::Mutex::new(()),
                shutdown,
            }),
            None,
        );
        (dir, ctx)
    }
}
impl TunnelContext {
    #[instrument(skip_all)]
    pub async fn init(config: &TunnelConfig) -> Result<Self, Error> {
        Self::init_auth_cookie().await?;
        let (shutdown, _) = tokio::sync::broadcast::channel(1);
        let datadir = config
            .datadir
            .as_deref()
            .unwrap_or_else(|| Path::new("/var/lib/start-tunnel"))
            .to_owned();
        restrict_datadir(&datadir).await?;
        let db_path = datadir.join("tunnel.db");
        let db = TypedPatchDb::<TunnelDatabase>::load_unchecked(PatchDb::open(&db_path).await?);
        if db.dump(&ROOT).await.value.is_null() {
            db.put(&ROOT, &TunnelDatabase::init()).await?;
        }
        db.mutate(|db| run_migrations(db)).await.result?;
        let server_identity = crate::sign::instance::load(&db).await?;
        let listen = config.tunnel_listen.unwrap_or(TUNNEL_DEFAULT_LISTEN);
        let ip_info = crate::net::utils::load_ip_info().await?;
        let net_iface = db
            .mutate(|db| {
                db.as_gateways_mut().mutate(|g| {
                    for (_, v) in OrdMapIterMut::from(&mut *g) {
                        v.ip_info = None;
                    }
                    for (id, info) in ip_info {
                        if id.as_str() != WIREGUARD_INTERFACE_NAME {
                            g.entry(id).or_default().ip_info = Some(Arc::new(info));
                        }
                    }
                    Ok(g.clone())
                })
            })
            .await
            .result?;
        let net_iface = Watch::new(net_iface);
        let forward = PortForwardController::new();
        nft_rule(
            "forward",
            "wg-forward",
            false,
            false,
            &format!("iifname \"{WIREGUARD_INTERFACE_NAME}\" ct state new accept"),
        )
        .await?;
        // The ct rule loads conntrack before its sysctls are accessed.
        crate::tunnel::conntrack::configure().await.log_err();
        // Inbound IPv6 to a client requires a firewall pinhole.
        nft_rule_v6(
            "forward",
            "wg-forward",
            false,
            false,
            &format!("iifname \"{WIREGUARD_INTERFACE_NAME}\" ct state new accept"),
        )
        .await?;
        // Large TLS ClientHellos must fit the WireGuard path MTU.
        nft_rule(
            "mangle_forward",
            "wg-mss-clamp",
            false,
            false,
            "tcp flags syn / syn,rst tcp option maxseg size set rt mtu",
        )
        .await?;

        let dns_proxy = DnsProxyController::new();
        let peek = db.peek().await;
        let wg = peek.as_wg().de()?;
        let dns_allowed = Arc::new(SyncMutex::new(allowed_injectors(&wg)));
        let dns_keys = Arc::new(SyncMutex::new(injector_keys(&wg)));
        let dns_injector = {
            let seed = seed_records(&peek.as_dns_records().de()?);
            let allowed = dns_allowed.clone();
            let keys = dns_keys.clone();
            let persist_db = db.clone();
            DnsInjector::new(
                seed,
                move |src| allowed.peek(|s| s.contains(&src)),
                move |src| keys.peek(|m| m.get(&src).copied()),
                move |records| {
                    let db = persist_db.clone();
                    let entries: Vec<_> = records.iter().map(dns_entry).collect();
                    tokio::spawn(async move {
                        db.mutate(|d| d.as_dns_records_mut().ser(&DnsRecords(entries)))
                            .await
                            .result
                            .log_err();
                    });
                },
                // Every tunnel client holds a PSK; an unsigned UPDATE is refused.
                |_, _, auth: UpdateAuth| async move {
                    if auth.tsig {
                        hickory_server::proto::op::ResponseCode::NoError
                    } else {
                        hickory_server::proto::op::ResponseCode::Refused
                    }
                },
            )
        };
        wg.sync().await?;
        dns_proxy.sync(&wg, dns_injector.clone()).await?;

        let sni = crate::tunnel::forward::sni::SniDemux::new();
        let mut active_forwards = BTreeMap::new();
        for (from, entry) in peek.as_port_forwards().de()?.0 {
            match entry {
                PortForward::Dnat {
                    target,
                    enabled,
                    count,
                    ..
                } => {
                    if !enabled {
                        continue;
                    }
                    let to = target;
                    let prefix = net_iface
                        .peek(|i| {
                            i.iter()
                                .find_map(|(_, i)| {
                                    i.ip_info.as_ref().and_then(|i| {
                                        i.subnets
                                            .iter()
                                            .find(|s| s.contains(&IpAddr::from(*to.ip())))
                                    })
                                })
                                .cloned()
                        })
                        .map(|s| s.prefix_len())
                        .unwrap_or(32);
                    active_forwards.insert(
                        from.clone(),
                        forward
                            .add_forward_range(
                                from.address,
                                to,
                                count,
                                prefix,
                                None,
                                from.transport,
                            )
                            .await?,
                    );
                }
                PortForward::Sni { routes, fallback } => {
                    for (hostname, route) in routes {
                        if !route.enabled {
                            continue;
                        }
                        if let Err(code) = sni.register(
                            *from.ip(),
                            from.port(),
                            &[hostname.clone()],
                            route.target,
                            None,
                        ) {
                            tracing::warn!(
                                "failed to restore SNI route {hostname} on {from}: code {code}"
                            );
                        }
                    }
                    if let Some(f) = fallback.filter(|f| f.enabled) {
                        if let Err(code) = sni.register_fallback(*from.ip(), from.port(), f.target)
                        {
                            tracing::warn!("failed to restore SNI fallback on {from}: code {code}");
                        }
                    }
                }
            }
        }

        let ctx = Self(
            Arc::new(TunnelContextSeed {
                listen,
                db,
                datadir,
                rpc_continuations: RpcContinuations::new(),
                open_authed_continuations: OpenAuthedContinuations::new(),
                auth_sig_nonce_cache: SyncMutex::new(Default::default()),
                server_identity,
                net_iface,
                forward,
                dns_proxy,
                sni,
                dns_injector,
                dns_allowed,
                dns_keys,
                active_forwards: SyncMutex::new(active_forwards),
                forward_write_lock: tokio::sync::Mutex::new(()),
                leases: SyncMutex::new(BTreeMap::new()),
                lease_wake: tokio::sync::Notify::new(),
                forward_ifindex: tokio::sync::watch::channel(current_ifindex()).0,
                egress_lock: tokio::sync::Mutex::new(()),
                v6_proxy: SyncMutex::new(BTreeMap::new()),
                v6_lock: tokio::sync::Mutex::new(()),
                shutdown,
            }),
            None,
        );

        ctx.resync_egress().await?;
        ctx.resync_v6().await?;
        crate::tunnel::forward::pinhole::seed_pinholes(&ctx).await?;
        // Grant every restored auto entry a fresh lease so a client that never
        // reconnects is reaped rather than lingering forever.
        crate::tunnel::forward::lease::seed_from_db(&ctx).await?;

        // PCP (preferred) + UPnP IGD (fallback) let connected clients open their
        // public ports automatically; the reaper expires auto mappings whose
        // client stops renewing.
        tokio::spawn(crate::tunnel::forward::pcp::run(ctx.clone()));
        tokio::spawn(crate::tunnel::forward::igd::run(ctx.clone()));
        tokio::spawn(crate::tunnel::forward::lease::run(ctx.clone()));

        Ok(ctx)
    }

    pub async fn gc_forwards(
        &self,
        keep: &BTreeSet<ForwardAddress>,
        dropped_sni: &[(SocketAddrV4, String, SocketAddrV4)],
        dropped_fallbacks: &[(SocketAddrV4, SocketAddrV4)],
    ) -> Result<(), Error> {
        for (source, hostname, target) in dropped_sni {
            self.sni.unregister(
                *source.ip(),
                source.port(),
                std::slice::from_ref(hostname),
                *target,
            );
        }
        for (source, target) in dropped_fallbacks {
            self.sni
                .unregister_fallback(*source.ip(), source.port(), *target);
        }
        self.active_forwards
            .mutate(|pf| pf.retain(|k, _| keep.contains(k)));
        // Drop leases for forwards whose target is no longer a known client.
        use crate::tunnel::forward::lease::LeaseKey;
        self.leases.mutate(|l| {
            l.retain(|k, _| match k {
                LeaseKey::Dnat(s) => keep.contains(s),
                LeaseKey::Sni { source: s, .. } | LeaseKey::SniFallback(s) => {
                    keep.contains(&(*s).into())
                }
                LeaseKey::Pinhole(_) => true,
            })
        });
        self.forward.gc().await
    }

    /// Reload the wg interface, then rebind the per-subnet DNS proxies. The
    /// proxies bind to the addresses `wg-quick up` (re)creates, so the rebind
    /// must follow `server.sync()`.
    pub async fn sync_network(&self, server: &WgServer) -> Result<(), Error> {
        server.sync().await?;
        let ifindex = current_ifindex();
        self.forward_ifindex.send_if_modified(|cur| {
            let changed = *cur != ifindex;
            *cur = ifindex;
            changed
        });
        self.dns_allowed.mutate(|s| *s = allowed_injectors(server));
        self.dns_keys.mutate(|m| *m = injector_keys(server));
        self.dns_proxy
            .sync(server, self.dns_injector.clone())
            .await?;
        self.resync_egress().await?;
        self.resync_v6().await
    }

    /// Reconcile IPv6 delivery for client addresses. Every client /128 gets an
    /// explicit route over the wg interface: wg-quick installs none (its
    /// Address-derived route covers them), and that interface route ties with —
    /// and loses to — a WAN that holds the delegated prefix on-link at the same
    /// length (DigitalOcean configures its /124 this way), sending return
    /// traffic out the WAN. Clients inside an on-link prefix additionally get
    /// proxy-NDP on the WAN (Hetzner, Vultr, DO) so the VPS gateway's Neighbor
    /// Discovery resolves them here; a routed prefix needs no proxy entry.
    pub async fn resync_v6(&self) -> Result<(), Error> {
        let _guard = self.v6_lock.lock().await;
        let server = self.db.peek().await.as_wg().de()?;
        let any_v6 = server.subnets.0.values().any(|c| c.ipv6.is_some());
        let mut desired: BTreeMap<Ipv6Addr, GatewayId> = BTreeMap::new();
        if any_v6 {
            Command::new("sysctl")
                .arg("-w")
                .arg("net.ipv6.conf.all.proxy_ndp=1")
                .invoke(ErrorKind::Network)
                .await
                .log_err();
            let onlink: Vec<(GatewayId, Ipv6Net)> = self.net_iface.peek(|ifaces| {
                let mut v = Vec::new();
                for (id, info) in ifaces.iter() {
                    if id.as_str() == WIREGUARD_INTERFACE_NAME {
                        continue;
                    }
                    let Some(ip) = info.ip_info.as_ref() else {
                        continue;
                    };
                    if ip.device_type == Some(NetworkInterfaceType::Loopback) {
                        continue;
                    }
                    for net in ip.subnets.iter() {
                        if let IpNet::V6(n) = net {
                            v.push((id.clone(), n.trunc()));
                        }
                    }
                }
                v
            });
            // For each subnet with a prefix: route every client's /128 over wg,
            // and proxy-NDP those inside a WAN interface's on-link prefix (the
            // delivery path for an on-link prefix; a routed one needs no ND).
            for (_, cfg) in &server.subnets.0 {
                let Some(prefix) = cfg.ipv6 else {
                    continue;
                };
                for (client_v4, _) in &cfg.clients.0 {
                    let addr = crate::tunnel::wg6::host_v6(prefix, *client_v4);
                    client_route(addr).await;
                    if let Some((iface, _)) = onlink.iter().find(|(_, n)| n.contains(&addr)) {
                        desired.insert(addr, iface.clone());
                    }
                }
            }
        }
        let current = self.v6_proxy.peek(|m| m.clone());
        for (addr, iface) in &current {
            if desired.get(addr) != Some(iface) {
                neigh_proxy(false, *addr, iface.as_str()).await;
            }
        }
        for (addr, iface) in &desired {
            if current.get(addr) != Some(iface) {
                neigh_proxy(true, *addr, iface.as_str()).await;
            }
        }
        self.v6_proxy.mutate(|m| *m = desired);
        Ok(())
    }

    /// Reconcile per-subnet and per-device egress NAT rules in `postrouting`:
    /// `wan_ip` SNATs, else masquerade; a device `/32` rule outranks its subnet.
    /// Comment tags are stable per (subnet/device, iface) so each re-run replaces
    /// in place rather than leaving stale duplicates.
    pub async fn resync_egress(&self) -> Result<(), Error> {
        let _guard = self.egress_lock.lock().await;
        let ifaces: Vec<GatewayId> = self.net_iface.peek(|i| {
            i.iter()
                .filter(|(_, info)| {
                    info.ip_info.as_ref().map_or(false, |i| {
                        i.device_type != Some(NetworkInterfaceType::Loopback)
                    })
                })
                .map(|(name, _)| name)
                .filter(|id| id.as_str() != WIREGUARD_INTERFACE_NAME)
                .cloned()
                .collect()
        });
        let subnets = self.db.peek().await.as_wg().as_subnets().de()?;
        let mut want_dev: BTreeSet<String> = BTreeSet::new();
        for iface in &ifaces {
            for (subnet, cfg) in &subnets.0 {
                let net = subnet.trunc();
                let subnet_rule = match cfg.wan_ip {
                    Some(wan) => format!("ip saddr {net} oifname \"{iface}\" snat to {wan}"),
                    None => format!("ip saddr {net} oifname \"{iface}\" masquerade"),
                };
                nft_rule(
                    "postrouting",
                    &format!("tunnel-masq-{net}-{iface}"),
                    false,
                    false,
                    &subnet_rule,
                )
                .await?;

                for (client_ip, client) in &cfg.clients.0 {
                    if let Some(wan) = client.wan_ip {
                        let comment = format!("tunnel-snat-dev-{client_ip}-{iface}");
                        nft_rule(
                            "postrouting",
                            &comment,
                            false,
                            true,
                            &format!("ip saddr {client_ip}/32 oifname \"{iface}\" snat to {wan}"),
                        )
                        .await?;
                        want_dev.insert(comment);
                    }
                }
            }
        }
        // Prune device-override rules whose device was removed or cleared its `wan_ip`.
        for comment in nft_comments_with_prefix("postrouting", "tunnel-snat-dev-").await {
            if !want_dev.contains(&comment) {
                nft_rule("postrouting", &comment, true, false, "").await?;
            }
        }
        Ok(())
    }

    /// Moves forwards to each target's active WAN address.
    pub async fn resync_forward_keys(&self) -> Result<(), Error> {
        let _guard = self.forward_write_lock.lock().await;
        let peek = self.db.peek().await;
        let old = peek.as_port_forwards().de()?.0;
        let webserver = peek.as_webserver().de()?;

        let mut target_ips = BTreeSet::new();
        for entry in old.values() {
            match entry {
                PortForward::Dnat { target, .. } => {
                    target_ips.insert(*target.ip());
                }
                PortForward::Sni { routes, fallback } => {
                    for route in routes.values() {
                        target_ips.insert(*route.target.ip());
                    }
                    if let Some(f) = fallback {
                        target_ips.insert(*f.target.ip());
                    }
                }
            }
        }
        let mut resolved_ips = BTreeMap::new();
        for target_ip in target_ips {
            if let Some(ip) = crate::tunnel::forward::igd::external_ipv4(self, target_ip).await {
                resolved_ips.insert(target_ip, ip);
            }
        }
        let (want, lease_moves) =
            rekey_forwards(&old, &webserver, |ip| resolved_ips.get(&ip).copied());

        let dnat_specs = |map: &BTreeMap<ForwardAddress, PortForward>| {
            map.iter()
                .filter_map(|(src, e)| match e {
                    PortForward::Dnat {
                        target,
                        enabled,
                        count,
                        ..
                    } => Some((src.clone(), (*target, *count, *enabled))),
                    _ => None,
                })
                .collect::<BTreeMap<_, _>>()
        };
        let old_dnat = dnat_specs(&old);
        let new_dnat = dnat_specs(&want);
        let mut installed = Vec::new();
        for (src, spec @ (target, count, enabled)) in &new_dnat {
            if !enabled {
                continue;
            }
            if old_dnat.get(src) == Some(spec) && self.active_forwards.peek(|m| m.contains_key(src))
            {
                continue;
            }
            let prefix = crate::tunnel::forward::igd::prefix_for(self, target.ip()).await;
            match self
                .forward
                .add_forward_range(
                    src.address,
                    *target,
                    *count,
                    prefix,
                    None,
                    src.transport.clone(),
                )
                .await
            {
                Ok(rc) => installed.push((src.clone(), rc)),
                Err(e) => {
                    invalidate_active_forwards(
                        &self.active_forwards,
                        installed.iter().map(|(s, _)| s).chain(std::iter::once(src)),
                    );
                    drop(installed);
                    self.forward.gc().await.log_err();
                    return Err(e);
                }
            }
        }

        let want = PortForwards(want);
        let db_res = self
            .db
            .mutate(|db| db.as_port_forwards_mut().ser(&want))
            .await
            .result;
        let committed = db_res.is_ok()
            || self
                .db
                .peek()
                .await
                .as_port_forwards()
                .de()
                .ok()
                .is_some_and(|pf| pf.0 == want.0);
        if committed {
            self.active_forwards.mutate(|m| {
                for (src, spec) in &old_dnat {
                    if new_dnat.get(src) != Some(spec) {
                        m.remove(src);
                    }
                }
                for (src, rc) in installed {
                    m.insert(src, rc);
                }
            });
            self.forward.gc().await.log_err();
            reconcile_sni_demux(&self.sni, &old, &want.0);
            reconcile_leases(&self.leases, &lease_moves);
        } else {
            invalidate_active_forwards(&self.active_forwards, installed.iter().map(|(s, _)| s));
            drop(installed);
            self.forward.gc().await.log_err();
        }
        db_res?;
        Ok(())
    }
}

fn invalidate_active_forwards<'a>(
    active: &SyncMutex<BTreeMap<ForwardAddress, Arc<()>>>,
    sources: impl IntoIterator<Item = &'a ForwardAddress>,
) {
    active.mutate(|m| {
        for done_src in sources {
            m.retain(|k, _| {
                !(k.address == done_src.address && k.transport.overlaps(&done_src.transport))
            });
        }
    });
}

fn insert_sni_slot(
    want: &mut PortForwards,
    socket: SocketAddrV4,
) -> Result<(&mut BTreeMap<String, SniRoute>, &mut Option<SniRoute>), ForwardAddress> {
    let key = ForwardAddress::from(socket);
    if !matches!(want.0.get(&key), Some(PortForward::Sni { .. })) {
        if let Some(conflict) = want.conflicting(ForwardAddress::new(socket, Transport::tcp()), 1) {
            return Err(conflict);
        }
        want.0.insert(
            key.clone(),
            PortForward::Sni {
                routes: BTreeMap::new(),
                fallback: None,
            },
        );
    }
    match want.0.get_mut(&key) {
        Some(PortForward::Sni { routes, fallback }) => Ok((routes, fallback)),
        _ => unreachable!(),
    }
}

fn rekey_forwards(
    old: &BTreeMap<ForwardAddress, PortForward>,
    webserver: &WebserverInfo,
    resolve: impl Fn(Ipv4Addr) -> Option<Ipv4Addr>,
) -> (
    BTreeMap<ForwardAddress, PortForward>,
    Vec<(LeaseKey, LeaseKey)>,
) {
    let mut want = PortForwards::default();
    let mut lease_moves = Vec::new();
    for stationary in [true, false] {
        let resolve_for_pass = |src_ip: Ipv4Addr, target_ip: Ipv4Addr| {
            let ip = resolve(target_ip).unwrap_or(src_ip);
            ((ip == src_ip) == stationary).then_some(ip)
        };
        for (src, entry) in old {
            match entry {
                PortForward::Dnat {
                    target,
                    enabled,
                    count,
                    auto,
                    ..
                } => {
                    let Some(ip) = resolve_for_pass(*src.ip(), *target.ip()) else {
                        continue;
                    };
                    let key = ForwardAddress::new(
                        SocketAddrV4::new(ip, src.port()),
                        src.transport.clone(),
                    );
                    if *enabled
                        && src.transport != Transport::udp()
                        && webserver.reserves_plain_forward(key.address, *count)
                    {
                        tracing::warn!(
                            "dropping DNAT forward {src} -> {target} on {key}: collides with admin UI listener"
                        );
                    } else if let Some(conflict) = want.conflicting(key.clone(), *count) {
                        tracing::warn!(
                            "dropping DNAT forward {src} -> {target} on {key}: collides with {conflict}"
                        );
                    } else {
                        want.0.insert(key.clone(), entry.clone());
                        if *auto {
                            lease_moves.push((LeaseKey::Dnat(src.clone()), LeaseKey::Dnat(key)));
                        }
                    }
                }
                PortForward::Sni { routes, fallback } => {
                    for (host, route) in routes {
                        let Some(ip) = resolve_for_pass(*src.ip(), *route.target.ip()) else {
                            continue;
                        };
                        let socket = SocketAddrV4::new(ip, src.port());
                        match insert_sni_slot(&mut want, socket) {
                            Ok((routes, _)) => {
                                if routes
                                    .keys()
                                    .any(|existing| existing.eq_ignore_ascii_case(host))
                                {
                                    tracing::warn!(
                                        "dropping SNI route {host} ({src} -> {}) on {socket}: collides with an existing SNI route",
                                        route.target,
                                    );
                                } else {
                                    routes.insert(host.clone(), route.clone());
                                    if route.auto {
                                        lease_moves.push((
                                            LeaseKey::Sni {
                                                source: src.address,
                                                hostname: host.clone(),
                                            },
                                            LeaseKey::Sni {
                                                source: socket,
                                                hostname: host.clone(),
                                            },
                                        ));
                                    }
                                }
                            }
                            Err(conflict) => {
                                tracing::warn!(
                                    "dropping SNI route {host} ({src} -> {}) on {socket}: collides with {conflict}",
                                    route.target,
                                );
                            }
                        }
                    }
                    if let Some(f) = fallback {
                        if let Some(ip) = resolve_for_pass(*src.ip(), *f.target.ip()) {
                            let socket = SocketAddrV4::new(ip, src.port());
                            if f.enabled && webserver.reserves_plain_forward(socket, 1) {
                                tracing::warn!(
                                    "dropping SNI fallback ({src} -> {}) on {socket}: collides with admin UI listener",
                                    f.target,
                                );
                            } else {
                                match insert_sni_slot(&mut want, socket) {
                                    Ok((_, fb)) => {
                                        if fb.is_some() {
                                            tracing::warn!(
                                                "dropping SNI fallback ({src} -> {}) on {socket}: collides with an existing SNI fallback",
                                                f.target,
                                            );
                                        } else {
                                            *fb = Some(f.clone());
                                            if f.auto {
                                                lease_moves.push((
                                                    LeaseKey::SniFallback(src.address),
                                                    LeaseKey::SniFallback(socket),
                                                ));
                                            }
                                        }
                                    }
                                    Err(conflict) => {
                                        tracing::warn!(
                                            "dropping SNI fallback ({src} -> {}) on {socket}: collides with {conflict}",
                                            f.target,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    (want.0, lease_moves)
}

fn reconcile_leases(leases: &SyncMutex<Leases>, moved: &[(LeaseKey, LeaseKey)]) {
    leases.mutate(|l| {
        let mut next = BTreeMap::new();
        for (key, exp) in l.iter() {
            if matches!(key, LeaseKey::Pinhole(_)) {
                next.insert(key.clone(), *exp);
            }
        }
        for (old_key, new_key) in moved {
            if let Some(exp) = l.get(old_key) {
                next.insert(new_key.clone(), *exp);
            }
        }
        *l = next;
    });
}

fn reconcile_sni_demux(
    sni: &Arc<crate::tunnel::forward::sni::SniDemux>,
    old: &BTreeMap<ForwardAddress, PortForward>,
    want: &BTreeMap<ForwardAddress, PortForward>,
) {
    let routes = |map: &BTreeMap<ForwardAddress, PortForward>, enabled_only: bool| {
        let mut out: BTreeMap<(SocketAddrV4, String), SocketAddrV4> = BTreeMap::new();
        for (source, entry) in map {
            if let PortForward::Sni { routes, .. } = entry {
                for (hostname, route) in routes {
                    if !enabled_only || route.enabled {
                        out.insert((source.address, hostname.clone()), route.target);
                    }
                }
            }
        }
        out
    };
    let old_routes = routes(old, false);
    let desired_routes = routes(want, true);
    for ((source, hostname), target) in &old_routes {
        if desired_routes.get(&(*source, hostname.clone())) != Some(target) {
            sni.unregister(
                *source.ip(),
                source.port(),
                std::slice::from_ref(hostname),
                *target,
            );
        }
    }
    for ((source, hostname), target) in &desired_routes {
        if let Err(code) = sni.register(
            *source.ip(),
            source.port(),
            std::slice::from_ref(hostname),
            *target,
            None,
        ) {
            tracing::warn!("failed to register SNI route {hostname} on {source}: code {code}");
        }
    }

    let fallbacks = |map: &BTreeMap<ForwardAddress, PortForward>, enabled_only: bool| {
        let mut out = BTreeMap::new();
        for (source, entry) in map {
            if let PortForward::Sni {
                fallback: Some(fallback),
                ..
            } = entry
            {
                if !enabled_only || fallback.enabled {
                    out.insert(source.clone(), fallback.target);
                }
            }
        }
        out
    };
    let old_fallbacks = fallbacks(old, false);
    let desired_fallbacks = fallbacks(want, true);
    for (source, target) in &old_fallbacks {
        if desired_fallbacks.get(source) != Some(target) {
            sni.unregister_fallback(*source.ip(), source.port(), *target);
        }
    }
    for (source, target) in &desired_fallbacks {
        if let Err(code) = sni.register_fallback(*source.ip(), source.port(), *target) {
            tracing::warn!("failed to register SNI fallback on {source}: code {code}");
        }
    }
}

impl AsRef<RpcContinuations> for TunnelContext {
    fn as_ref(&self) -> &RpcContinuations {
        &self.rpc_continuations
    }
}

impl AsRef<OpenAuthedContinuations<Option<InternedString>>> for TunnelContext {
    fn as_ref(&self) -> &OpenAuthedContinuations<Option<InternedString>> {
        &self.open_authed_continuations
    }
}

impl Context for TunnelContext {}
impl Deref for TunnelContext {
    type Target = TunnelContextSeed;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

#[derive(Debug, Deserialize, Serialize, Parser)]
#[group(skip)]
pub struct TunnelAddrParams {
    #[arg(help = "help.arg.tunnel-ip-address")]
    pub tunnel: IpAddr,
}

impl CallRemote<TunnelContext> for CliContext {
    async fn call_remote(
        &self,
        mut method: &str,

        _: OrdMap<&'static str, Value>,
        params: Value,
        _: Empty,
    ) -> Result<Value, RpcError> {
        let (tunnel_addr, addr_from_config) = if let Some(addr) = self.tunnel_addr {
            (addr, true)
        } else if let Some(addr) = self.tunnel_listen {
            (dial_addr(addr), true)
        } else {
            (TUNNEL_DEFAULT_LISTEN, false)
        };

        let local_auth = if tunnel_addr.ip().is_loopback() {
            local_auth_header::<TunnelContext>().await
        } else {
            None
        };

        let mut headers = HeaderMap::new();
        let (scheme, sig_ctx) = if let Some(auth) = local_auth {
            headers.insert(AUTHORIZATION, auth);
            ("http", None)
        } else if addr_from_config {
            ("https", Some(url_host_str(tunnel_addr.ip())))
        } else {
            return Err(Error::new(eyre!("`--tunnel` required"), ErrorKind::InvalidRequest).into());
        };

        let base: url::Url = format!("{scheme}://{tunnel_addr}").parse()?;
        method = method.strip_prefix("tunnel.").unwrap_or(method);

        self.call_rpc(
            &base,
            TUNNEL_HTTP_RPC_VERSION,
            headers,
            sig_ctx.as_deref().unwrap_or_default(),
            method,
            params,
        )
        .await
    }
}

pub static TUNNEL_UI_CELL: OnceLock<Dir<'static>> = OnceLock::new();

impl UiContext for TunnelContext {
    fn ui_dir() -> &'static Dir<'static> {
        TUNNEL_UI_CELL.get().unwrap_or(&EMPTY_DIR)
    }
    fn api() -> ParentHandler<Self> {
        tracing::info!("loading tunnel api...");
        tunnel_api()
    }
    fn middleware(server: rpc_toolkit::Server<Self>) -> rpc_toolkit::HttpServer<Self> {
        server
            .middleware(Auth::new().with_local_auth().with_signature_auth())
            .middleware(SyncDb::new())
    }
    fn rpc_server_routes(self, server: rpc_toolkit::HttpServer<Self>) -> axum::Router {
        crate::net::static_server::session_rpc_router(
            self,
            server.clone(),
            axum::routing::any(server).layer(crate::middleware::cors::layer()),
        )
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn restrict_datadir_tightens_an_existing_install() {
        use std::os::unix::fs::PermissionsExt;

        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        let temp = tempfile::tempdir().unwrap();
        let datadir = temp.path().join("nested/start-tunnel");
        std::fs::create_dir_all(&datadir).unwrap();
        let db = datadir.join("tunnel.db");
        let failed = datadir.join("tunnel.failed");
        let unrelated = datadir.join("unrelated");
        for file in [&db, &failed, &unrelated] {
            std::fs::write(file, b"").unwrap();
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        std::fs::set_permissions(&datadir, std::fs::Permissions::from_mode(0o755)).unwrap();

        super::restrict_datadir(&datadir).await.unwrap();

        assert_eq!(mode(&datadir), 0o700);
        assert_eq!(mode(&db), 0o600);
        assert_eq!(mode(&failed), 0o600);
        assert_eq!(mode(&unrelated), 0o644);
    }

    #[tokio::test]
    async fn restrict_datadir_creates_a_missing_directory() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let datadir = temp.path().join("nested/start-tunnel");

        super::restrict_datadir(&datadir).await.unwrap();

        let mode = std::fs::metadata(&datadir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    use super::*;
    use crate::tunnel::wg::{WgClientKind, WgConfig, WgSubnetClients, WgSubnetConfig, WgSubnetMap};

    fn server_with_client(ipv6: Option<Ipv6Net>) -> WgServer {
        let mut server = WgServer::default();
        server.subnets = WgSubnetMap::default();
        server.subnets.0.insert(
            "10.59.0.1/24".parse().unwrap(),
            WgSubnetConfig {
                name: "net".into(),
                clients: {
                    let mut c = WgSubnetClients::default();
                    // A Server-kind client gets gateway-autoconfig (incl. DNS
                    // injection) on by default.
                    c.0.insert(
                        "10.59.0.2".parse().unwrap(),
                        WgConfig::generate("box".into(), WgClientKind::Server),
                    );
                    c
                },
                ipv6,
                ..Default::default()
            },
        );
        server
    }

    #[test]
    fn v6_injector_shares_the_clients_authorization_and_key() {
        let server = server_with_client(Some("2001:db8:abcd::/64".parse().unwrap()));
        let v4 = IpAddr::V4(Ipv4Addr::new(10, 59, 0, 2));
        let v6 = IpAddr::V6("2001:db8:abcd::a3b:2".parse().unwrap());

        let allowed = allowed_injectors(&server);
        assert!(allowed.contains(&v4));
        assert!(allowed.contains(&v6));

        let keys = injector_keys(&server);
        assert!(keys.contains_key(&v4));
        assert_eq!(keys.get(&v4), keys.get(&v6), "same device, same TSIG key");
    }

    #[test]
    fn no_delegated_prefix_means_no_v6_injector() {
        let server = server_with_client(None);
        assert_eq!(allowed_injectors(&server).len(), 1);
        assert_eq!(injector_keys(&server).len(), 1);
    }

    #[tokio::test]
    async fn unchanged_sni_route_retries_after_bind_failure() {
        let blocked = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let source = match blocked.local_addr().unwrap() {
            SocketAddr::V4(source) => source,
            SocketAddr::V6(_) => unreachable!(),
        };
        let target = "10.59.0.2:443".parse().unwrap();
        let hostname = "retry.example.com".to_string();
        let mut routes = BTreeMap::new();
        routes.insert(
            hostname.clone(),
            crate::tunnel::db::SniRoute {
                target,
                label: None,
                enabled: true,
                auto: false,
            },
        );
        let forwards = BTreeMap::from([(
            source.into(),
            PortForward::Sni {
                routes,
                fallback: None,
            },
        )]);
        let sni = crate::tunnel::forward::sni::SniDemux::new();

        reconcile_sni_demux(&sni, &forwards, &forwards);
        assert!(sni.snapshot().is_empty());

        drop(blocked);
        reconcile_sni_demux(&sni, &forwards, &forwards);
        assert_eq!(sni.snapshot().len(), 1);
        assert_eq!(sni.snapshot()[0].hostname, hostname);
    }

    fn test_dnat(target: &str, count: u16, enabled: bool) -> PortForward {
        PortForward::Dnat {
            target: target.parse().unwrap(),
            label: None,
            enabled,
            count,
            auto: false,
        }
    }

    fn test_sni_route(target: &str, enabled: bool) -> SniRoute {
        SniRoute {
            target: target.parse().unwrap(),
            label: None,
            enabled,
            auto: false,
        }
    }

    fn assert_valid_port_forwards(forwards: &BTreeMap<ForwardAddress, PortForward>) {
        let entries: Vec<_> = forwards.iter().collect();
        for (i, (k1, e1)) in entries.iter().enumerate() {
            if let PortForward::Sni { routes, fallback } = e1 {
                assert_eq!(k1.transport, Transport::default());
                assert!(!routes.is_empty() || fallback.is_some());
                let lower: BTreeSet<String> =
                    routes.keys().map(|h| h.to_ascii_lowercase()).collect();
                assert_eq!(
                    lower.len(),
                    routes.len(),
                    "case-colliding SNI routes on {k1}"
                );
            }
            for (k2, e2) in &entries[i + 1..] {
                if k1.ip() != k2.ip() {
                    continue;
                }
                if !e1
                    .transport((*k1).clone())
                    .overlaps(&e2.transport((*k2).clone()))
                {
                    continue;
                }
                let hi1 = k1.port().saturating_add(e1.port_span().saturating_sub(1));
                let hi2 = k2.port().saturating_add(e2.port_span().saturating_sub(1));
                assert!(
                    hi1 < k2.port() || hi2 < k1.port(),
                    "overlapping forwards {k1} ({e1:?}) and {k2} ({e2:?})",
                );
            }
        }
    }

    fn capture_warnings<T>(f: impl FnOnce() -> T) -> (T, Vec<String>) {
        use std::sync::Mutex;

        use tracing::field::{Field, Visit};
        use tracing_subscriber::layer::{Context as LayerContext, SubscriberExt};

        #[derive(Default)]
        struct Msg(String);
        impl Visit for Msg {
            fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0 = format!("{value:?}");
                }
            }
        }
        struct WarnCollector(Arc<Mutex<Vec<String>>>);
        impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for WarnCollector {
            fn on_event(&self, event: &tracing::Event<'_>, _ctx: LayerContext<'_, S>) {
                if *event.metadata().level() == tracing::Level::WARN {
                    let mut msg = Msg::default();
                    event.record(&mut msg);
                    self.0.lock().unwrap().push(msg.0);
                }
            }
        }

        let warnings = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::registry().with(WarnCollector(warnings.clone()));
        let out = tracing::subscriber::with_default(subscriber, f);
        let collected = warnings.lock().unwrap().clone();
        (out, collected)
    }

    fn rekey_with(
        old: &BTreeMap<ForwardAddress, PortForward>,
        webserver: &WebserverInfo,
        pairs: &[(&str, &str)],
    ) -> (BTreeMap<ForwardAddress, PortForward>, Vec<String>) {
        let map: BTreeMap<Ipv4Addr, Ipv4Addr> = pairs
            .iter()
            .map(|(k, v)| (k.parse().unwrap(), v.parse().unwrap()))
            .collect();
        let ((want, _), warnings) =
            capture_warnings(|| rekey_forwards(old, webserver, |ip| map.get(&ip).copied()));
        assert_valid_port_forwards(&want);
        (want, warnings)
    }

    fn rekey_default(
        old: &BTreeMap<ForwardAddress, PortForward>,
        pairs: &[(&str, &str)],
    ) -> (BTreeMap<ForwardAddress, PortForward>, Vec<String>) {
        rekey_with(old, &WebserverInfo::default(), pairs)
    }

    #[test]
    fn rekey_forwards_exact_dnat_collision_preserves_stationary_in_both_ip_orders() {
        for (stationary_ip, moving_ip) in [("1.2.3.4", "5.6.7.8"), ("5.6.7.8", "1.2.3.4")] {
            let stationary_key: ForwardAddress = format!("{stationary_ip}:443").parse().unwrap();
            let old = BTreeMap::from([
                (stationary_key.clone(), test_dnat("10.59.0.2:443", 1, true)),
                (
                    format!("{moving_ip}:443").parse().unwrap(),
                    test_dnat("10.59.0.3:443", 1, true),
                ),
            ]);
            let (want, warnings) = rekey_default(
                &old,
                &[("10.59.0.2", stationary_ip), ("10.59.0.3", stationary_ip)],
            );
            assert_eq!(want.len(), 1);
            assert_eq!(
                want.get(&stationary_key),
                Some(&test_dnat("10.59.0.2:443", 1, true)),
            );
            assert_eq!(warnings.len(), 1);
            assert!(warnings[0].contains("dropping DNAT forward"));
        }

        let old = BTreeMap::from([
            (
                "1.2.3.4:443".parse().unwrap(),
                test_dnat("10.59.0.2:443", 1, true),
            ),
            (
                "5.6.7.8:443".parse().unwrap(),
                test_dnat("10.59.0.3:443", 1, true),
            ),
        ]);
        let (want, warnings) =
            rekey_default(&old, &[("10.59.0.2", "9.9.9.9"), ("10.59.0.3", "9.9.9.9")]);
        assert_eq!(want.len(), 1);
        assert_eq!(
            want.get(&"9.9.9.9:443".parse().unwrap()),
            Some(&test_dnat("10.59.0.2:443", 1, true)),
        );
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn rekey_forwards_overlapping_dnat_ranges_preserve_non_overlap() {
        for (stationary_ip, moving_ip) in [("1.2.3.4", "5.6.7.8"), ("5.6.7.8", "1.2.3.4")] {
            let old_range_incumbent = BTreeMap::from([
                (
                    format!("{stationary_ip}:8000").parse().unwrap(),
                    test_dnat("10.59.0.2:8000", 10, true),
                ),
                (
                    format!("{moving_ip}:8005").parse().unwrap(),
                    test_dnat("10.59.0.3:8005", 1, true),
                ),
            ]);
            let (want, warnings) = rekey_default(
                &old_range_incumbent,
                &[("10.59.0.2", stationary_ip), ("10.59.0.3", stationary_ip)],
            );
            assert_eq!(want.len(), 1);
            assert!(want.contains_key(&format!("{stationary_ip}:8000").parse().unwrap()));
            assert_eq!(warnings.len(), 1);

            let old_single_incumbent = BTreeMap::from([
                (
                    format!("{stationary_ip}:8005").parse().unwrap(),
                    test_dnat("10.59.0.2:8005", 1, true),
                ),
                (
                    format!("{moving_ip}:8000").parse().unwrap(),
                    test_dnat("10.59.0.3:8000", 10, true),
                ),
            ]);
            let (want, warnings) = rekey_default(
                &old_single_incumbent,
                &[("10.59.0.2", stationary_ip), ("10.59.0.3", stationary_ip)],
            );
            assert_eq!(want.len(), 1);
            assert!(want.contains_key(&format!("{stationary_ip}:8005").parse().unwrap()));
            assert_eq!(warnings.len(), 1);
        }

        let disjoint = BTreeMap::from([
            (
                "1.2.3.4:8000".parse().unwrap(),
                test_dnat("10.59.0.2:8000", 10, true),
            ),
            (
                "5.6.7.8:8010".parse().unwrap(),
                test_dnat("10.59.0.3:8010", 10, true),
            ),
            (
                "tcp@1.2.3.4:9000".parse().unwrap(),
                test_dnat("10.59.0.2:9000", 5, true),
            ),
            (
                "udp@5.6.7.8:9002".parse().unwrap(),
                test_dnat("10.59.0.3:9002", 1, true),
            ),
        ]);
        let (want, warnings) = rekey_default(
            &disjoint,
            &[("10.59.0.2", "1.2.3.4"), ("10.59.0.3", "1.2.3.4")],
        );
        assert_eq!(want.len(), 4);
        assert!(warnings.is_empty());

        let transport_overlap = BTreeMap::from([
            (
                "5.6.7.8:8000".parse().unwrap(),
                test_dnat("10.59.0.2:8000", 10, true),
            ),
            (
                "tcp@1.2.3.4:8005".parse().unwrap(),
                test_dnat("10.59.0.3:8005", 1, true),
            ),
        ]);
        let (want, warnings) = rekey_default(
            &transport_overlap,
            &[("10.59.0.2", "5.6.7.8"), ("10.59.0.3", "5.6.7.8")],
        );
        assert_eq!(want.len(), 1);
        assert!(want.contains_key(&"5.6.7.8:8000".parse().unwrap()));
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn rekey_forwards_dnat_and_sni_collisions_leave_no_empty_sni() {
        let sni_entry = |target: &str| PortForward::Sni {
            routes: BTreeMap::from([("app.example.com".to_owned(), test_sni_route(target, true))]),
            fallback: Some(test_sni_route(target, true)),
        };

        for (dnat_key, dnat_count) in [
            ("5.6.7.8:443", 1),
            ("tcp@5.6.7.8:443", 1),
            ("5.6.7.8:440", 10),
            ("tcp@5.6.7.8:440", 10),
        ] {
            let dnat_stationary = BTreeMap::from([
                (
                    dnat_key.parse().unwrap(),
                    test_dnat("10.59.0.2:440", dnat_count, true),
                ),
                ("1.2.3.4:443".parse().unwrap(), sni_entry("10.59.0.3:443")),
            ]);
            let (want, warnings) = rekey_default(
                &dnat_stationary,
                &[("10.59.0.2", "5.6.7.8"), ("10.59.0.3", "5.6.7.8")],
            );
            assert_eq!(want.len(), 1);
            assert!(want.contains_key(&dnat_key.parse().unwrap()));
            assert_eq!(warnings.len(), 2);

            let moving_dnat_key = dnat_key.replace("5.6.7.8", "1.2.3.4");
            let sni_stationary = BTreeMap::from([
                ("5.6.7.8:443".parse().unwrap(), sni_entry("10.59.0.2:443")),
                (
                    moving_dnat_key.parse().unwrap(),
                    test_dnat("10.59.0.3:440", dnat_count, true),
                ),
            ]);
            let (want, warnings) = rekey_default(
                &sni_stationary,
                &[("10.59.0.2", "5.6.7.8"), ("10.59.0.3", "5.6.7.8")],
            );
            assert_eq!(want.len(), 1);
            assert_eq!(
                want.get(&"5.6.7.8:443".parse().unwrap()),
                Some(&sni_entry("10.59.0.2:443")),
            );
            assert_eq!(warnings.len(), 1);
        }

        for udp_key in ["udp@1.2.3.4:443", "udp@1.2.3.4:440"] {
            let count = if udp_key.ends_with(":440") { 10 } else { 1 };
            let old = BTreeMap::from([
                ("5.6.7.8:443".parse().unwrap(), sni_entry("10.59.0.2:443")),
                (
                    udp_key.parse().unwrap(),
                    test_dnat("10.59.0.3:440", count, true),
                ),
            ]);
            let (want, warnings) =
                rekey_default(&old, &[("10.59.0.2", "5.6.7.8"), ("10.59.0.3", "5.6.7.8")]);
            assert_eq!(want.len(), 2);
            assert!(warnings.is_empty());

            let reverse_old = BTreeMap::from([
                (
                    udp_key.replace("1.2.3.4", "5.6.7.8").parse().unwrap(),
                    test_dnat("10.59.0.2:440", count, true),
                ),
                ("1.2.3.4:443".parse().unwrap(), sni_entry("10.59.0.3:443")),
            ]);
            let (want, warnings) = rekey_default(
                &reverse_old,
                &[("10.59.0.2", "5.6.7.8"), ("10.59.0.3", "5.6.7.8")],
            );
            assert_eq!(want.len(), 2);
            assert!(warnings.is_empty());
        }
    }

    #[test]
    fn rekey_forwards_sni_convergence_mixed_case_and_mixed_wan_preserve_incumbents() {
        for (incumbent_host, moving_host) in [
            ("shared.example.com", "shared.example.com"),
            ("App.example.com", "app.example.com"),
            ("app.example.com", "App.example.com"),
        ] {
            let old = BTreeMap::from([
                (
                    "1.2.3.4:443".parse().unwrap(),
                    PortForward::Sni {
                        routes: BTreeMap::from([
                            (
                                moving_host.to_owned(),
                                test_sni_route("10.59.0.3:443", true),
                            ),
                            (
                                "moved.example.com".to_owned(),
                                test_sni_route("10.59.0.3:8443", true),
                            ),
                        ]),
                        fallback: Some(test_sni_route("10.59.0.3:444", true)),
                    },
                ),
                (
                    "5.6.7.8:443".parse().unwrap(),
                    PortForward::Sni {
                        routes: BTreeMap::from([(
                            incumbent_host.to_owned(),
                            test_sni_route("10.59.0.2:443", true),
                        )]),
                        fallback: Some(test_sni_route("10.59.0.2:444", true)),
                    },
                ),
            ]);
            let (want, warnings) =
                rekey_default(&old, &[("10.59.0.2", "5.6.7.8"), ("10.59.0.3", "5.6.7.8")]);
            assert_eq!(want.len(), 1);
            assert_eq!(
                want.get(&"5.6.7.8:443".parse().unwrap()),
                Some(&PortForward::Sni {
                    routes: BTreeMap::from([
                        (
                            incumbent_host.to_owned(),
                            test_sni_route("10.59.0.2:443", true),
                        ),
                        (
                            "moved.example.com".to_owned(),
                            test_sni_route("10.59.0.3:8443", true),
                        ),
                    ]),
                    fallback: Some(test_sni_route("10.59.0.2:444", true)),
                }),
            );
            assert_eq!(warnings.len(), 2);
        }

        let mixed = BTreeMap::from([
            (
                "1.2.3.4:443".parse().unwrap(),
                test_dnat("10.59.0.4:443", 1, true),
            ),
            (
                "5.6.7.8:443".parse().unwrap(),
                PortForward::Sni {
                    routes: BTreeMap::from([
                        (
                            "stay.example.com".to_owned(),
                            test_sni_route("10.59.0.2:443", true),
                        ),
                        (
                            "leave.example.com".to_owned(),
                            test_sni_route("10.59.0.3:443", true),
                        ),
                    ]),
                    fallback: Some(test_sni_route("10.59.0.2:444", true)),
                },
            ),
        ]);
        let (want, warnings) = rekey_default(
            &mixed,
            &[
                ("10.59.0.2", "5.6.7.8"),
                ("10.59.0.3", "9.9.9.9"),
                ("10.59.0.4", "5.6.7.8"),
            ],
        );
        assert_eq!(
            want.get(&"5.6.7.8:443".parse().unwrap()),
            Some(&PortForward::Sni {
                routes: BTreeMap::from([(
                    "stay.example.com".to_owned(),
                    test_sni_route("10.59.0.2:443", true),
                )]),
                fallback: Some(test_sni_route("10.59.0.2:444", true)),
            }),
        );
        assert_eq!(
            want.get(&"9.9.9.9:443".parse().unwrap()),
            Some(&PortForward::Sni {
                routes: BTreeMap::from([(
                    "leave.example.com".to_owned(),
                    test_sni_route("10.59.0.3:443", true),
                )]),
                fallback: None,
            }),
        );
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn rekey_forwards_respects_webserver_listener_on_destination_wan() {
        let web = WebserverInfo {
            enabled: true,
            listen: Some("5.6.7.8:8443".parse().unwrap()),
            certificate: None,
        };
        let old = BTreeMap::from([
            (
                "1.2.3.4:8440".parse().unwrap(),
                test_dnat("10.59.0.2:8440", 10, true),
            ),
            (
                "udp@1.2.3.4:8443".parse().unwrap(),
                test_dnat("10.59.0.2:8443", 1, true),
            ),
            (
                "1.2.3.4:8443".parse().unwrap(),
                PortForward::Sni {
                    routes: BTreeMap::from([(
                        "ui-share.example.com".to_owned(),
                        test_sni_route("10.59.0.2:8443", true),
                    )]),
                    fallback: Some(test_sni_route("10.59.0.2:8444", true)),
                },
            ),
        ]);
        let (want, warnings) = rekey_with(&old, &web, &[("10.59.0.2", "5.6.7.8")]);
        assert!(!want.contains_key(&"5.6.7.8:8440".parse().unwrap()));
        assert!(want.contains_key(&"udp@5.6.7.8:8443".parse().unwrap()));
        assert_eq!(
            want.get(&"5.6.7.8:8443".parse().unwrap()),
            Some(&PortForward::Sni {
                routes: BTreeMap::from([(
                    "ui-share.example.com".to_owned(),
                    test_sni_route("10.59.0.2:8443", true),
                )]),
                fallback: None,
            }),
        );
        assert_eq!(warnings.len(), 2);

        let disabled_dnat = BTreeMap::from([(
            "1.2.3.4:8443".parse().unwrap(),
            test_dnat("10.59.0.2:8443", 1, false),
        )]);
        let (want, warnings) = rekey_with(&disabled_dnat, &web, &[("10.59.0.2", "5.6.7.8")]);
        assert!(want.contains_key(&"5.6.7.8:8443".parse().unwrap()));
        assert!(warnings.is_empty());
    }

    #[test]
    fn rekey_forwards_non_colliding_rekey_succeeds_without_warnings() {
        let old = BTreeMap::from([
            (
                "1.2.3.4:8000".parse().unwrap(),
                test_dnat("10.59.0.2:8000", 5, true),
            ),
            (
                "tcp@1.2.3.4:9000".parse().unwrap(),
                test_dnat("10.59.0.2:9000", 1, false),
            ),
            (
                "1.2.3.4:443".parse().unwrap(),
                PortForward::Sni {
                    routes: BTreeMap::from([(
                        "ok.example.com".to_owned(),
                        test_sni_route("10.59.0.2:443", true),
                    )]),
                    fallback: Some(test_sni_route("10.59.0.2:444", true)),
                },
            ),
        ]);
        let (want, warnings) = rekey_default(&old, &[("10.59.0.2", "9.9.9.9")]);
        assert!(warnings.is_empty());
        assert_eq!(want.len(), 3);
        assert_eq!(
            want.get(&"9.9.9.9:8000".parse().unwrap()),
            Some(&test_dnat("10.59.0.2:8000", 5, true)),
        );
        assert_eq!(
            want.get(&"tcp@9.9.9.9:9000".parse().unwrap()),
            Some(&test_dnat("10.59.0.2:9000", 1, false)),
        );
        assert!(want.contains_key(&"9.9.9.9:443".parse().unwrap()));
    }

    fn db_with_pinned_clients(clients: &[(&str, &str)]) -> TunnelDatabase {
        let mut db = TunnelDatabase::default();
        let mut subnet = WgSubnetConfig::default();
        for (client_ip, wan_ip) in clients {
            let mut cfg = WgConfig::generate("dev".into(), WgClientKind::Server);
            cfg.wan_ip = Some(wan_ip.parse().unwrap());
            subnet.clients.0.insert(client_ip.parse().unwrap(), cfg);
        }
        db.wg
            .subnets
            .0
            .insert("10.59.0.1/24".parse().unwrap(), subnet);
        db
    }

    #[tokio::test]
    async fn resync_forward_keys_preserves_stationary_dnat_and_enforces_non_overlap() {
        let mut db = db_with_pinned_clients(&[("10.59.0.2", "1.2.3.4"), ("10.59.0.3", "1.2.3.4")]);
        let incumbent_key: ForwardAddress = "1.2.3.4:443".parse().unwrap();
        let mover_key: ForwardAddress = "5.6.7.8:443".parse().unwrap();
        db.port_forwards
            .0
            .insert(incumbent_key.clone(), test_dnat("10.59.0.2:443", 1, true));
        db.port_forwards
            .0
            .insert(mover_key.clone(), test_dnat("10.59.0.3:443", 1, true));
        db.port_forwards.0.insert(
            "1.2.3.4:8000".parse().unwrap(),
            test_dnat("10.59.0.2:8000", 10, false),
        );
        db.port_forwards.0.insert(
            "5.6.7.8:8005".parse().unwrap(),
            test_dnat("10.59.0.3:8005", 1, false),
        );
        db.port_forwards.0.insert(
            "5.6.7.8:9000".parse().unwrap(),
            test_dnat("10.59.0.3:9000", 1, false),
        );

        let (_dir, ctx) = TunnelContext::for_test(db).await;
        let incumbent_rc = Arc::new(());
        let mover_rc = Arc::new(());
        ctx.active_forwards.mutate(|m| {
            m.insert(incumbent_key.clone(), incumbent_rc.clone());
            m.insert(mover_key.clone(), mover_rc.clone());
        });

        ctx.resync_forward_keys().await.unwrap();

        let after = ctx.db.peek().await.as_port_forwards().de().unwrap();
        assert_eq!(
            after.0.get(&incumbent_key),
            Some(&test_dnat("10.59.0.2:443", 1, true)),
        );
        assert!(!after.0.contains_key(&mover_key));
        assert!(after.0.contains_key(&"1.2.3.4:8000".parse().unwrap()));
        assert!(!after.0.contains_key(&"1.2.3.4:8005".parse().unwrap()));
        assert_eq!(
            after.0.get(&"1.2.3.4:9000".parse().unwrap()),
            Some(&test_dnat("10.59.0.3:9000", 1, false)),
        );
        assert_valid_port_forwards(&after.0);
        ctx.active_forwards.peek(|m| {
            assert!(Arc::ptr_eq(m.get(&incumbent_key).unwrap(), &incumbent_rc));
            assert!(!m.contains_key(&mover_key));
        });
    }

    #[tokio::test]
    async fn resync_forward_keys_prevents_sni_overlap_and_preserves_incumbent_sni() {
        let mut db = db_with_pinned_clients(&[("10.59.0.2", "1.2.3.4"), ("10.59.0.3", "1.2.3.4")]);
        db.port_forwards.0.insert(
            "1.2.3.4:440".parse().unwrap(),
            test_dnat("10.59.0.2:440", 10, false),
        );
        db.port_forwards.0.insert(
            "5.6.7.8:443".parse().unwrap(),
            PortForward::Sni {
                routes: BTreeMap::from([(
                    "range-hit.example.com".to_owned(),
                    test_sni_route("10.59.0.3:443", false),
                )]),
                fallback: Some(test_sni_route("10.59.0.3:443", false)),
            },
        );
        db.port_forwards.0.insert(
            "tcp@1.2.3.4:8443".parse().unwrap(),
            test_dnat("10.59.0.2:8443", 1, false),
        );
        db.port_forwards.0.insert(
            "5.6.7.8:8443".parse().unwrap(),
            PortForward::Sni {
                routes: BTreeMap::from([(
                    "tcp-hit.example.com".to_owned(),
                    test_sni_route("10.59.0.3:8443", false),
                )]),
                fallback: None,
            },
        );
        db.port_forwards.0.insert(
            "1.2.3.4:9443".parse().unwrap(),
            PortForward::Sni {
                routes: BTreeMap::from([(
                    "shared.example.com".to_owned(),
                    test_sni_route("10.59.0.2:9443", false),
                )]),
                fallback: Some(test_sni_route("10.59.0.2:9444", false)),
            },
        );
        db.port_forwards.0.insert(
            "5.6.7.8:9443".parse().unwrap(),
            PortForward::Sni {
                routes: BTreeMap::from([
                    (
                        "shared.example.com".to_owned(),
                        test_sni_route("10.59.0.3:9443", false),
                    ),
                    (
                        "unique.example.com".to_owned(),
                        test_sni_route("10.59.0.3:9445", false),
                    ),
                ]),
                fallback: Some(test_sni_route("10.59.0.3:9444", false)),
            },
        );

        let (_dir, ctx) = TunnelContext::for_test(db).await;
        ctx.resync_forward_keys().await.unwrap();

        let after = ctx.db.peek().await.as_port_forwards().de().unwrap();
        assert!(!after.0.contains_key(&"1.2.3.4:443".parse().unwrap()));
        assert!(!after.0.contains_key(&"1.2.3.4:8443".parse().unwrap()));
        assert_eq!(
            after.0.get(&"1.2.3.4:9443".parse().unwrap()),
            Some(&PortForward::Sni {
                routes: BTreeMap::from([
                    (
                        "shared.example.com".to_owned(),
                        test_sni_route("10.59.0.2:9443", false),
                    ),
                    (
                        "unique.example.com".to_owned(),
                        test_sni_route("10.59.0.3:9445", false),
                    ),
                ]),
                fallback: Some(test_sni_route("10.59.0.2:9444", false)),
            }),
        );
        assert_valid_port_forwards(&after.0);
    }

    #[tokio::test]
    async fn resync_forward_keys_retires_replaced_active_handle_and_reconciles_leases() {
        use std::time::{Duration, Instant};

        use crate::tunnel::forward::address::PinholeAddress;

        let mut db = db_with_pinned_clients(&[
            ("10.59.0.2", "9.9.9.9"),
            ("10.59.0.3", "1.2.3.4"),
            ("10.59.0.4", "9.9.9.9"),
            ("10.59.0.5", "5.6.7.8"),
        ]);
        let replaced_key: ForwardAddress = "1.2.3.4:443".parse().unwrap();
        let incumbent_9: ForwardAddress = "9.9.9.9:443".parse().unwrap();
        db.port_forwards.0.insert(
            replaced_key.clone(),
            PortForward::Dnat {
                target: "10.59.0.2:443".parse().unwrap(),
                label: None,
                enabled: true,
                count: 1,
                auto: true,
            },
        );
        db.port_forwards
            .0
            .insert(incumbent_9.clone(), test_dnat("10.59.0.4:443", 1, true));
        db.port_forwards.0.insert(
            "5.6.7.8:443".parse().unwrap(),
            test_dnat("10.59.0.3:443", 1, false),
        );
        let swap_a: ForwardAddress = "1.2.3.4:7000".parse().unwrap();
        let swap_b: ForwardAddress = "5.6.7.8:7000".parse().unwrap();
        db.port_forwards.0.insert(
            swap_a.clone(),
            PortForward::Dnat {
                target: "10.59.0.5:7000".parse().unwrap(),
                label: None,
                enabled: false,
                count: 1,
                auto: true,
            },
        );
        db.port_forwards.0.insert(
            swap_b.clone(),
            PortForward::Dnat {
                target: "10.59.0.3:7000".parse().unwrap(),
                label: None,
                enabled: false,
                count: 1,
                auto: true,
            },
        );
        let sni_old: SocketAddrV4 = "5.6.7.8:8443".parse().unwrap();
        let sni_new: SocketAddrV4 = "1.2.3.4:8443".parse().unwrap();
        db.port_forwards.0.insert(
            sni_old.into(),
            PortForward::Sni {
                routes: BTreeMap::from([(
                    "auto.example.com".to_owned(),
                    SniRoute {
                        target: "10.59.0.3:8443".parse().unwrap(),
                        label: None,
                        enabled: false,
                        auto: true,
                    },
                )]),
                fallback: Some(SniRoute {
                    target: "10.59.0.3:8444".parse().unwrap(),
                    label: None,
                    enabled: false,
                    auto: true,
                }),
            },
        );

        let (_dir, ctx) = TunnelContext::for_test(db).await;
        let replaced_rc = Arc::new(());
        let incumbent_rc = Arc::new(());
        ctx.active_forwards.mutate(|m| {
            m.insert(replaced_key.clone(), replaced_rc.clone());
            m.insert(incumbent_9.clone(), incumbent_rc.clone());
        });

        let now = Instant::now();
        let exp_dropped = now + Duration::from_secs(60);
        let exp_swap_a = now + Duration::from_secs(120);
        let exp_swap_b = now + Duration::from_secs(240);
        let exp_sni = now + Duration::from_secs(360);
        let exp_fb = now + Duration::from_secs(480);
        let exp_pinhole = now + Duration::from_secs(600);
        let pinhole_key = LeaseKey::Pinhole("[2001:db8::2]:443".parse::<PinholeAddress>().unwrap());
        ctx.leases.mutate(|l| {
            l.insert(LeaseKey::Dnat(replaced_key.clone()), exp_dropped);
            l.insert(LeaseKey::Dnat(swap_a.clone()), exp_swap_a);
            l.insert(LeaseKey::Dnat(swap_b.clone()), exp_swap_b);
            l.insert(
                LeaseKey::Sni {
                    source: sni_old,
                    hostname: "auto.example.com".to_owned(),
                },
                exp_sni,
            );
            l.insert(LeaseKey::SniFallback(sni_old), exp_fb);
            l.insert(pinhole_key.clone(), exp_pinhole);
        });

        ctx.resync_forward_keys().await.unwrap();

        let after = ctx.db.peek().await.as_port_forwards().de().unwrap();
        assert_valid_port_forwards(&after.0);
        assert_eq!(
            after.0.get(&replaced_key),
            Some(&test_dnat("10.59.0.3:443", 1, false)),
        );
        ctx.active_forwards.peek(|m| {
            assert!(!m.contains_key(&replaced_key));
            assert!(Arc::ptr_eq(m.get(&incumbent_9).unwrap(), &incumbent_rc));
        });
        ctx.leases.peek(|l| {
            assert!(!l.contains_key(&LeaseKey::Dnat(replaced_key)));
            assert_eq!(l.get(&LeaseKey::Dnat(swap_b)), Some(&exp_swap_a));
            assert_eq!(l.get(&LeaseKey::Dnat(swap_a)), Some(&exp_swap_b));
            assert_eq!(
                l.get(&LeaseKey::Sni {
                    source: sni_new,
                    hostname: "auto.example.com".to_owned(),
                }),
                Some(&exp_sni),
            );
            assert_eq!(l.get(&LeaseKey::SniFallback(sni_new)), Some(&exp_fb));
            assert_eq!(l.get(&pinhole_key), Some(&exp_pinhole));
            assert_eq!(l.len(), 5);
        });
    }

    #[tokio::test]
    async fn resync_forward_keys_failed_install_leaves_state_intact_and_recovers_on_reversal() {
        use std::time::{Duration, Instant};

        let mut db = db_with_pinned_clients(&[("10.59.0.2", "5.6.7.8"), ("10.59.0.3", "5.6.7.8")]);
        let old_key: ForwardAddress = "1.2.3.4:443".parse().unwrap();
        let dnat_entry = PortForward::Dnat {
            target: "10.59.0.2:443".parse().unwrap(),
            label: None,
            enabled: true,
            count: 1,
            auto: true,
        };
        db.port_forwards
            .0
            .insert(old_key.clone(), dnat_entry.clone());
        let (_dir, ctx) = TunnelContext::for_test(db).await;

        let sni_port = (0..5)
            .find_map(|_| {
                let probe = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).ok()?;
                let port = probe.local_addr().ok()?.port();
                drop(probe);
                ctx.sni
                    .register(
                        Ipv4Addr::LOCALHOST,
                        port,
                        &["keep.example.com".to_owned()],
                        "10.59.0.3:8443".parse().unwrap(),
                        None,
                    )
                    .ok()
                    .map(|()| port)
            })
            .expect("bind ephemeral loopback port for SNI test");
        ctx.db
            .mutate(|db| {
                db.as_port_forwards_mut().mutate(|pf| {
                    pf.0.insert(
                        ForwardAddress::from(SocketAddrV4::new(Ipv4Addr::LOCALHOST, sni_port)),
                        PortForward::Sni {
                            routes: BTreeMap::from([(
                                "keep.example.com".to_owned(),
                                test_sni_route("10.59.0.3:8443", true),
                            )]),
                            fallback: None,
                        },
                    );
                    Ok(())
                })
            })
            .await
            .result
            .unwrap();

        let handle = Arc::new(());
        ctx.active_forwards.mutate(|m| {
            m.insert(old_key.clone(), handle.clone());
        });
        let exp = Instant::now() + Duration::from_secs(300);
        ctx.leases.mutate(|l| {
            l.insert(LeaseKey::Dnat(old_key.clone()), exp);
        });

        assert!(ctx.resync_forward_keys().await.is_err());

        let after_err = ctx.db.peek().await.as_port_forwards().de().unwrap();
        assert_eq!(after_err.0.get(&old_key), Some(&dnat_entry));
        ctx.active_forwards.peek(|m| {
            assert!(Arc::ptr_eq(m.get(&old_key).unwrap(), &handle));
        });
        ctx.leases.peek(|l| {
            assert_eq!(l.get(&LeaseKey::Dnat(old_key.clone())), Some(&exp));
        });
        assert_eq!(ctx.sni.snapshot().len(), 1);

        ctx.db
            .mutate(|db| {
                let clients = db
                    .as_wg_mut()
                    .as_subnets_mut()
                    .as_idx_mut(&"10.59.0.1/24".parse().unwrap())
                    .unwrap()
                    .as_clients_mut();
                clients
                    .as_idx_mut(&"10.59.0.2".parse().unwrap())
                    .unwrap()
                    .as_wan_ip_mut()
                    .ser(&Some("1.2.3.4".parse().unwrap()))?;
                clients
                    .as_idx_mut(&"10.59.0.3".parse().unwrap())
                    .unwrap()
                    .as_wan_ip_mut()
                    .ser(&Some(Ipv4Addr::LOCALHOST))
            })
            .await
            .result
            .unwrap();
        ctx.resync_forward_keys().await.unwrap();
        ctx.active_forwards.peek(|m| {
            assert!(Arc::ptr_eq(m.get(&old_key).unwrap(), &handle));
        });
        assert_eq!(ctx.sni.snapshot().len(), 1);

        let mut replace_db = db_with_pinned_clients(&[
            ("10.59.0.2", "9.9.9.9"),
            ("10.59.0.3", "1.2.3.4"),
            ("10.59.0.4", "9.9.9.9"),
        ]);
        replace_db
            .port_forwards
            .0
            .insert(old_key.clone(), test_dnat("10.59.0.2:443", 1, true));
        replace_db.port_forwards.0.insert(
            "9.9.9.9:443".parse().unwrap(),
            test_dnat("10.59.0.4:443", 1, false),
        );
        replace_db.port_forwards.0.insert(
            "5.6.7.8:443".parse().unwrap(),
            test_dnat("10.59.0.3:443", 1, true),
        );
        let (_dir2, ctx2) = TunnelContext::for_test(replace_db).await;
        ctx2.active_forwards.mutate(|m| {
            m.insert(old_key.clone(), Arc::new(()));
        });
        assert!(ctx2.resync_forward_keys().await.is_err());
        ctx2.active_forwards.peek(|m| {
            assert!(!m.contains_key(&old_key));
        });
    }

    #[tokio::test]
    async fn manual_dnat_add_and_remove_serialize_and_revalidate_under_forward_write_lock() {
        let mut db = db_with_pinned_clients(&[("10.59.0.2", "1.2.3.4")]);
        let remove_key: ForwardAddress = "1.2.3.4:9443".parse().unwrap();
        db.port_forwards
            .0
            .insert(remove_key.clone(), test_dnat("10.59.0.2:9443", 1, false));
        let (_dir, ctx) = TunnelContext::for_test(db).await;
        let guard = ctx.forward_write_lock.lock().await;
        let ctx_add = ctx.clone();
        let mut add = tokio::spawn(async move {
            crate::tunnel::api::add_forward(
                ctx_add,
                serde_json::from_value(serde_json::json!({
                    "externalPort": 8443,
                    "target": "10.59.0.2:8443",
                }))
                .unwrap(),
            )
            .await
        });
        let ctx_rm = ctx.clone();
        let mut rm = tokio::spawn(async move {
            crate::tunnel::api::remove_forward(
                ctx_rm,
                serde_json::from_value(serde_json::json!({ "source": "1.2.3.4:9443" })).unwrap(),
            )
            .await
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), &mut add)
                .await
                .is_err()
        );
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), &mut rm)
                .await
                .is_err()
        );
        let replaced_sni = PortForward::Sni {
            routes: BTreeMap::from([(
                "keep.example.com".to_owned(),
                test_sni_route("10.59.0.2:9443", false),
            )]),
            fallback: None,
        };
        ctx.db
            .mutate(|db| {
                db.as_wg_mut()
                    .as_subnets_mut()
                    .as_idx_mut(&"10.59.0.1/24".parse().unwrap())
                    .unwrap()
                    .as_clients_mut()
                    .as_idx_mut(&"10.59.0.2".parse().unwrap())
                    .unwrap()
                    .as_wan_ip_mut()
                    .ser(&Some("5.6.7.8".parse().unwrap()))?;
                db.as_port_forwards_mut().mutate(|pf| {
                    pf.0.insert(
                        "5.6.7.8:8443".parse().unwrap(),
                        test_dnat("10.59.0.9:8443", 1, false),
                    );
                    pf.0.insert(remove_key.clone(), replaced_sni.clone());
                    Ok(())
                })
            })
            .await
            .result
            .unwrap();
        drop(guard);
        let add_err = add.await.unwrap().unwrap_err();
        assert!(add_err.to_string().contains("5.6.7.8:8443"), "{add_err}");
        rm.await.unwrap().unwrap();
        let after = ctx.db.peek().await.as_port_forwards().de().unwrap();
        assert_eq!(after.0.get(&remove_key), Some(&replaced_sni));
    }
}
