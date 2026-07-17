use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use clap::Parser;
use cookie::{Cookie, Expiration, SameSite};
use http::HeaderMap;
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
use url::Url;

use crate::GatewayId;
use crate::auth::Sessions;
use crate::context::config::ContextConfig;
use crate::context::{CliContext, RpcContext};
use crate::db::model::public::{NetworkInterfaceInfo, NetworkInterfaceType};
use crate::middleware::auth::Auth;
use crate::middleware::auth::local::LocalAuthContext;
use crate::middleware::cors::Cors;
use crate::net::dns_update::rfc2136::{DnsInjector, InjectedRecord};
use crate::net::forward::{PortForwardController, nft_comments_with_prefix, nft_rule, nft_rule_v6};
use crate::net::static_server::{EMPTY_DIR, UiContext};
use crate::prelude::*;
use crate::rpc_continuations::{OpenAuthedContinuations, RpcContinuations};
use crate::tunnel::TUNNEL_DEFAULT_LISTEN;
use crate::tunnel::api::tunnel_api;
use crate::tunnel::db::{DnsRecordEntry, DnsRecords, PortForward, PortForwards, TunnelDatabase};
use crate::tunnel::dns::DnsProxyController;
use crate::tunnel::migrations::run_migrations;
use crate::tunnel::wg::{WIREGUARD_INTERFACE_NAME, WgServer, current_ifindex};
use crate::util::Invoke;
use crate::util::collections::OrdMapIterMut;
use crate::util::io::read_file_to_string;
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
                out.insert(
                    IpAddr::V4(*ip),
                    crate::net::dns_update::derive_tsig_key(&client.psk.0),
                );
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
    pub ephemeral_sessions: SyncMutex<Sessions>,
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
    pub active_forwards: SyncMutex<BTreeMap<SocketAddrV4, Arc<()>>>,
    /// In-memory leases for auto (PCP-created) forwards/pinholes/SNI routes,
    /// reaped by [`crate::tunnel::forward::lease`] when a client stops renewing.
    pub leases: SyncMutex<crate::tunnel::forward::lease::Leases>,
    /// Wakes the lease reaper when a stamp may have moved the soonest expiry
    /// earlier, so it can pull its next wake-up forward.
    pub lease_wake: tokio::sync::Notify,
    /// The wg interface's current ifindex, republished after every `wg-quick`
    /// bounce. The PCP/IGD listeners are `SO_BINDTODEVICE`-bound, which pins the
    /// ifindex at bind time, so a recreated interface leaves them deaf until
    /// they rebind. Level-triggered on the value (not an edge notify) so a
    /// listener re-subscribing after a bounce still observes the new index, and
    /// an unchanged index skips a needless rebind.
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

#[derive(Clone)]
pub struct TunnelContext(Arc<TunnelContextSeed>);
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
        if tokio::fs::metadata(&datadir).await.is_err() {
            tokio::fs::create_dir_all(&datadir).await?;
        }
        let db_path = datadir.join("tunnel.db");
        let db = TypedPatchDb::<TunnelDatabase>::load_unchecked(PatchDb::open(&db_path).await?);
        if db.dump(&ROOT).await.value.is_null() {
            db.put(&ROOT, &TunnelDatabase::init()).await?;
        }
        db.mutate(|db| run_migrations(db)).await.result?;
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
        // Let clients originate IPv6 out through the tunnel (return traffic is
        // covered by the v6 base-established rule). Inbound IPv6 to a client is a
        // firewall pinhole, opened per-port via PCP / the manual pinhole API.
        nft_rule_v6(
            "forward",
            "wg-forward",
            false,
            false,
            &format!("iifname \"{WIREGUARD_INTERFACE_NAME}\" ct state new accept"),
        )
        .await?;
        // Clamp forwarded-SYN MSS to the WireGuard path MTU, else large TLS
        // ClientHellos (post-quantum key shares) get dropped after encapsulation.
        // See start-os#3261.
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
                        from,
                        forward
                            .add_forward_range(from, to, count, prefix, None)
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

        let ctx = Self(Arc::new(TunnelContextSeed {
            listen,
            db,
            datadir,
            rpc_continuations: RpcContinuations::new(),
            open_authed_continuations: OpenAuthedContinuations::new(),
            ephemeral_sessions: SyncMutex::new(Sessions::new()),
            net_iface,
            forward,
            dns_proxy,
            sni,
            dns_injector,
            dns_allowed,
            dns_keys,
            active_forwards: SyncMutex::new(active_forwards),
            leases: SyncMutex::new(BTreeMap::new()),
            lease_wake: tokio::sync::Notify::new(),
            forward_ifindex: tokio::sync::watch::channel(current_ifindex()).0,
            egress_lock: tokio::sync::Mutex::new(()),
            v6_proxy: SyncMutex::new(BTreeMap::new()),
            v6_lock: tokio::sync::Mutex::new(()),
            shutdown,
        }));

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
        keep: &BTreeSet<SocketAddrV4>,
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
                LeaseKey::Dnat(s) | LeaseKey::Sni { source: s, .. } | LeaseKey::SniFallback(s) => {
                    keep.contains(s)
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

    /// Reconcile proxy-NDP for client IPv6 addresses. When a client's /128 sits
    /// inside a /64 that a WAN interface holds *on-link* (Hetzner, Vultr), the
    /// VPS gateway resolves that address via Neighbor Discovery on the WAN link,
    /// so the tunnel host must answer for it (`proxy_ndp`) and then forward to
    /// the client over WireGuard. A routed prefix is delivered to the host
    /// without ND, so it needs no proxy entry.
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
            // For each subnet with a prefix, proxy-NDP every client's /128 that
            // falls inside a WAN interface's on-link /64 (the delivery path for
            // an on-link prefix; a routed prefix reaches the host without ND).
            for (_, cfg) in &server.subnets.0 {
                let Some(prefix) = cfg.ipv6 else {
                    continue;
                };
                for (client_v4, _) in &cfg.clients.0 {
                    let addr = crate::tunnel::wg6::host_v6(prefix, *client_v4);
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

    /// Re-key forwards to follow their target's WAN. A forward's external IP
    /// equals its target's WAN, so a WAN change must re-key it old IP → new IP.
    /// Idempotent: unchanged keys match in the per-key diffs and are left alone.
    pub async fn resync_forward_keys(&self) -> Result<(), Error> {
        let old = self.db.peek().await.as_port_forwards().de()?.0;

        let mut want: BTreeMap<SocketAddrV4, PortForward> = BTreeMap::new();
        for (src, entry) in &old {
            match entry {
                PortForward::Dnat {
                    target,
                    label,
                    enabled,
                    count,
                    auto,
                } => {
                    let ip = crate::tunnel::forward::igd::external_ipv4(self, *target.ip())
                        .await
                        .unwrap_or(*src.ip());
                    want.insert(
                        SocketAddrV4::new(ip, src.port()),
                        PortForward::Dnat {
                            target: *target,
                            label: label.clone(),
                            enabled: *enabled,
                            count: *count,
                            auto: *auto,
                        },
                    );
                }
                PortForward::Sni { routes, fallback } => {
                    for (host, route) in routes {
                        let ip =
                            crate::tunnel::forward::igd::external_ipv4(self, *route.target.ip())
                                .await
                                .unwrap_or(*src.ip());
                        let key = SocketAddrV4::new(ip, src.port());
                        match want.entry(key).or_insert_with(|| PortForward::Sni {
                            routes: BTreeMap::new(),
                            fallback: None,
                        }) {
                            PortForward::Sni { routes, .. } => {
                                routes.insert(host.clone(), route.clone());
                            }
                            PortForward::Dnat { .. } => {
                                tracing::warn!(
                                    "dropping SNI route {host} on {key}: external IP now collides with a DNAT forward"
                                );
                            }
                        }
                    }
                    if let Some(f) = fallback {
                        let ip = crate::tunnel::forward::igd::external_ipv4(self, *f.target.ip())
                            .await
                            .unwrap_or(*src.ip());
                        let key = SocketAddrV4::new(ip, src.port());
                        match want.entry(key).or_insert_with(|| PortForward::Sni {
                            routes: BTreeMap::new(),
                            fallback: None,
                        }) {
                            PortForward::Sni { fallback: fb, .. } => {
                                *fb = Some(f.clone());
                            }
                            PortForward::Dnat { .. } => {
                                tracing::warn!(
                                    "dropping SNI fallback on {key}: external IP now collides with a DNAT forward"
                                );
                            }
                        }
                    }
                }
            }
        }

        let sni_routes = |map: &BTreeMap<SocketAddrV4, PortForward>| {
            let mut out: BTreeMap<(SocketAddrV4, String), SocketAddrV4> = BTreeMap::new();
            for (src, entry) in map {
                if let PortForward::Sni { routes, .. } = entry {
                    for (host, route) in routes {
                        out.insert((*src, host.clone()), route.target);
                    }
                }
            }
            out
        };
        let old_sni = sni_routes(&old);
        let new_sni = sni_routes(&want);
        for ((src, host), target) in &old_sni {
            if new_sni.get(&(*src, host.clone())) != Some(target) {
                self.sni
                    .unregister(*src.ip(), src.port(), std::slice::from_ref(host), *target);
            }
        }
        for ((src, host), target) in &new_sni {
            if old_sni.get(&(*src, host.clone())) != Some(target) {
                if let Err(code) = self.sni.register(
                    *src.ip(),
                    src.port(),
                    std::slice::from_ref(host),
                    *target,
                    None,
                ) {
                    tracing::warn!("failed to register SNI route {host} on {src}: code {code}");
                }
            }
        }

        let sni_fallbacks = |map: &BTreeMap<SocketAddrV4, PortForward>| {
            let mut out: BTreeMap<SocketAddrV4, SocketAddrV4> = BTreeMap::new();
            for (src, entry) in map {
                if let PortForward::Sni {
                    fallback: Some(f), ..
                } = entry
                {
                    out.insert(*src, f.target);
                }
            }
            out
        };
        let old_fb = sni_fallbacks(&old);
        let new_fb = sni_fallbacks(&want);
        for (src, target) in &old_fb {
            if new_fb.get(src) != Some(target) {
                self.sni.unregister_fallback(*src.ip(), src.port(), *target);
            }
        }
        for (src, target) in &new_fb {
            if old_fb.get(src) != Some(target) {
                if let Err(code) = self.sni.register_fallback(*src.ip(), src.port(), *target) {
                    tracing::warn!("failed to register SNI fallback on {src}: code {code}");
                }
            }
        }

        let old_dnat: BTreeSet<SocketAddrV4> = old
            .iter()
            .filter_map(|(src, e)| matches!(e, PortForward::Dnat { .. }).then_some(*src))
            .collect();
        let new_dnat: BTreeMap<SocketAddrV4, (SocketAddrV4, u16, bool)> = want
            .iter()
            .filter_map(|(src, e)| match e {
                PortForward::Dnat {
                    target,
                    enabled,
                    count,
                    ..
                } => Some((*src, (*target, *count, *enabled))),
                _ => None,
            })
            .collect();
        for src in &old_dnat {
            if !new_dnat.contains_key(src) {
                if let Some(rc) = self.active_forwards.mutate(|m| m.remove(src)) {
                    drop(rc);
                }
            }
        }
        for (src, (target, count, enabled)) in &new_dnat {
            if !enabled {
                continue;
            }
            if self.active_forwards.peek(|m| m.contains_key(src)) {
                continue;
            }
            let prefix = crate::tunnel::forward::igd::prefix_for(self, target.ip()).await;
            let rc = self
                .forward
                .add_forward_range(*src, *target, *count, prefix, None)
                .await?;
            self.active_forwards.mutate(|m| {
                m.insert(*src, rc);
            });
        }
        self.forward.gc().await.log_err();

        self.db
            .mutate(|db| db.as_port_forwards_mut().ser(&PortForwards(want)))
            .await
            .result?;
        Ok(())
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
            (addr, true)
        } else {
            (TUNNEL_DEFAULT_LISTEN, false)
        };

        let local =
            if let Ok(local) = read_file_to_string(TunnelContext::LOCAL_AUTH_COOKIE_PATH).await {
                self.cookie_store
                    .lock()
                    .unwrap()
                    .insert_raw(
                        &Cookie::build(("local", local))
                            .domain(&tunnel_addr.ip().to_string())
                            .expires(Expiration::Session)
                            .same_site(SameSite::Strict)
                            .build(),
                        &format!("http://{tunnel_addr}").parse()?,
                    )
                    .with_kind(crate::ErrorKind::Network)?;
                true
            } else {
                false
            };

        let (url, sig_ctx) = if local && tunnel_addr.ip().is_loopback() {
            (format!("http://{tunnel_addr}/rpc/v0").parse()?, None)
        } else if addr_from_config {
            (
                format!("https://{tunnel_addr}/rpc/v0").parse()?,
                Some(InternedString::from_display(&tunnel_addr.ip())),
            )
        } else {
            return Err(Error::new(eyre!("`--tunnel` required"), ErrorKind::InvalidRequest).into());
        };

        method = method.strip_prefix("tunnel.").unwrap_or(method);

        crate::middleware::auth::signature::call_remote(
            self,
            url,
            HeaderMap::new(),
            sig_ctx.as_deref(),
            method,
            params,
        )
        .await
    }
}

#[derive(Debug, Deserialize, Serialize, Parser)]
#[group(skip)]
pub struct TunnelUrlParams {
    #[arg(help = "help.arg.tunnel-url")]
    pub tunnel: Url,
}

impl CallRemote<TunnelContext, TunnelUrlParams> for RpcContext {
    async fn call_remote(
        &self,
        mut method: &str,
        _: OrdMap<&'static str, Value>,
        params: Value,
        TunnelUrlParams { tunnel }: TunnelUrlParams,
    ) -> Result<Value, RpcError> {
        let url = tunnel.join("rpc/v0")?;
        method = method.strip_prefix("tunnel.").unwrap_or(method);

        let sig_ctx = url.host_str().map(InternedString::from_display);

        crate::middleware::auth::signature::call_remote(
            self,
            url,
            HeaderMap::new(),
            sig_ctx.as_deref(),
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
        server.middleware(Cors::new()).middleware(
            Auth::new()
                .with_local_auth()
                .with_signature_auth()
                .with_session_auth(),
        )
    }
}
