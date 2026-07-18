use std::any::Any;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::future::Future;
use std::net::{IpAddr, Ipv6Addr, SocketAddr, SocketAddrV6};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::task::{Poll, ready};
use std::time::{Duration, Instant};

use async_acme::acme::ACME_TLS_ALPN_NAME;
use clap::Parser;
use color_eyre::eyre::eyre;
use futures::FutureExt;
use futures::future::BoxFuture;
use imbl::{OrdMap, OrdSet};
use imbl_value::{InOMap, InternedString};
use ipnet::IpNet;
use rpc_toolkit::{Context, HandlerArgs, HandlerExt, ParentHandler, from_fn, from_fn_async};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::crypto::CryptoProvider;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::server::ClientHello;
use tokio_rustls::rustls::{ClientConfig, ServerConfig};
use tokio_util::sync::CancellationToken;
use tracing::instrument;
use ts_rs::TS;
use visit_rs::Visit;

use crate::context::{CliContext, RpcContext};
use crate::db::model::Database;
use crate::db::model::public::{AcmeSettings, NetworkInterfaceInfo};
use crate::db::{DbAccessByKey, DbAccessMut};
use crate::net::acme::{
    AcmeCertStore, AcmeProvider, AcmeTlsAlpnCache, AcmeTlsHandler, GetAcmeProvider,
};
use crate::net::gateway::{
    GatewayInfo, NetworkInterfaceController, NetworkInterfaceListenerAcceptMetadata,
};
use crate::net::port_map::{PortMapController, candidate_gateways};
use crate::net::ssl::{CertBranding, CertStore, RootCaTlsHandler};
use crate::net::tls::{ChainedHandler, TlsHandler, TlsHandlerAction, TlsListener, TlsMetadata};
use crate::net::utils::{bind_mio_listener, ipv6_is_link_local, is_private_ip};
use crate::net::web_server::{Accept, AcceptStream, ExtractVisitor, TcpMetadata, extract};
use crate::prelude::*;
use crate::util::collections::EqSet;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::ReadWriter;
use crate::util::serde::{HandlerExtSerde, MaybeUtf8String, display_serializable};
use crate::util::sync::{SyncMutex, Watch};
use crate::{GatewayId, HostId, PackageId, ResultExt};

/// Identifies which service+host contributed a set of SNI hostname mappings, so
/// each can be reconciled independently — a service only ever adds or removes
/// *its own* hostnames on a shared external port, never another's.
type HostMapOwner = (Option<PackageId>, HostId);

#[derive(Debug, Clone, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct PassthroughInfo {
    #[ts(type = "string")]
    pub hostname: InternedString,
    pub listen_port: u16,
    #[ts(type = "string")]
    pub backend: SocketAddr,
    #[ts(type = "string[]")]
    pub public_gateways: BTreeSet<GatewayId>,
    #[ts(type = "string[]")]
    pub private_ips: BTreeSet<IpAddr>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "kebab-case")]
struct AddPassthroughParams {
    #[arg(long)]
    pub hostname: InternedString,
    #[arg(long)]
    pub listen_port: u16,
    #[arg(long)]
    pub backend: SocketAddr,
    #[arg(long)]
    pub public_gateway: Vec<GatewayId>,
    #[arg(long)]
    pub private_ip: Vec<IpAddr>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "kebab-case")]
struct RemovePassthroughParams {
    #[arg(long)]
    pub hostname: InternedString,
    #[arg(long)]
    pub listen_port: u16,
}

pub fn vhost_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "dump-table",
            from_fn(dump_table)
                .with_display_serializable()
                .with_custom_display_fn(|HandlerArgs { params, .. }, res| {
                    use prettytable::*;

                    if let Some(format) = params.format {
                        display_serializable(format, res)?;
                        return Ok::<_, Error>(());
                    }

                    let mut table = Table::new();
                    table.add_row(row![bc => "FROM", "TO", "ACTIVE"]);

                    for (external, targets) in res {
                        for (host, targets) in targets {
                            for (idx, target) in targets.into_iter().enumerate() {
                                table.add_row(row![
                                    format!(
                                        "{}:{}",
                                        host.as_ref().map(|s| &**s).unwrap_or("*"),
                                        external.0
                                    ),
                                    target,
                                    idx == 0
                                ]);
                            }
                        }
                    }

                    table.print_tty(false)?;

                    Ok(())
                })
                .with_about("about.dump-vhost-proxy-table")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "add-passthrough",
            from_fn_async(add_passthrough)
                .no_display()
                .with_about("about.add-vhost-passthrough")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove-passthrough",
            from_fn_async(remove_passthrough)
                .no_display()
                .with_about("about.remove-vhost-passthrough")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "list-passthrough",
            from_fn(list_passthrough)
                .with_display_serializable()
                .with_about("about.list-vhost-passthrough")
                .with_call_remote::<CliContext>(),
        )
}

fn dump_table(
    ctx: RpcContext,
) -> Result<BTreeMap<JsonKey<u16>, BTreeMap<JsonKey<Option<InternedString>>, EqSet<String>>>, Error>
{
    Ok(ctx.net_controller.vhost.dump_table())
}

async fn add_passthrough(
    ctx: RpcContext,
    AddPassthroughParams {
        hostname,
        listen_port,
        backend,
        public_gateway,
        private_ip,
    }: AddPassthroughParams,
) -> Result<(), Error> {
    let public_gateways: BTreeSet<GatewayId> = public_gateway.into_iter().collect();
    let private_ips: BTreeSet<IpAddr> = private_ip.into_iter().collect();
    ctx.net_controller.vhost.add_passthrough(
        hostname.clone(),
        listen_port,
        backend,
        public_gateways.clone(),
        private_ips.clone(),
    )?;
    ctx.db
        .mutate(|db| {
            let pts = db
                .as_public_mut()
                .as_server_info_mut()
                .as_network_mut()
                .as_passthroughs_mut();
            let mut vec: Vec<PassthroughInfo> = pts.de()?;
            vec.retain(|p| !(p.hostname == hostname && p.listen_port == listen_port));
            vec.push(PassthroughInfo {
                hostname,
                listen_port,
                backend,
                public_gateways,
                private_ips,
            });
            pts.ser(&vec)
        })
        .await
        .result?;
    Ok(())
}

async fn remove_passthrough(
    ctx: RpcContext,
    RemovePassthroughParams {
        hostname,
        listen_port,
    }: RemovePassthroughParams,
) -> Result<(), Error> {
    ctx.net_controller
        .vhost
        .remove_passthrough(&hostname, listen_port);
    ctx.db
        .mutate(|db| {
            let pts = db
                .as_public_mut()
                .as_server_info_mut()
                .as_network_mut()
                .as_passthroughs_mut();
            let mut vec: Vec<PassthroughInfo> = pts.de()?;
            vec.retain(|p| !(p.hostname == hostname && p.listen_port == listen_port));
            pts.ser(&vec)
        })
        .await
        .result?;
    Ok(())
}

fn list_passthrough(ctx: RpcContext) -> Result<Vec<PassthroughInfo>, Error> {
    Ok(ctx.net_controller.vhost.list_passthrough())
}

struct PassthroughHandle {
    _rc: Arc<()>,
    backend: SocketAddr,
    public: BTreeSet<GatewayId>,
    private: BTreeSet<IpAddr>,
}

pub struct VHostController {
    db: TypedPatchDb<Database>,
    interfaces: Arc<NetworkInterfaceController>,
    crypto_provider: Arc<CryptoProvider>,
    acme_cache: AcmeTlsAlpnCache,
    branding: CertBranding,
    max_proxy_conns_per_target: usize,
    servers: SyncMutex<BTreeMap<u16, VHostServer<VHostBindListener>>>,
    passthrough_handles: SyncMutex<BTreeMap<(InternedString, u16), PassthroughHandle>>,
    port_map: PortMapController,
    /// Per-owner set of `(ext_ip, ext_port, hostname)` upstream port maps this
    /// controller has asked the port-mapper to maintain for its vhosts —
    /// everything: IPv4 SNI HOSTNAME routes (`Some(hostname)`), IPv4 bare-IP (`*`
    /// vhost) pinholes and IPv6 GUA pinholes (`None`), and the v6 80->443 redirect.
    /// Keyed by owner so one service's reconcile only adds/removes its own entries
    /// on a shared port.
    port_mappings:
        SyncMutex<BTreeMap<HostMapOwner, BTreeSet<(IpAddr, u16, Option<InternedString>)>>>,
}
impl VHostController {
    pub fn new(
        db: TypedPatchDb<Database>,
        interfaces: Arc<NetworkInterfaceController>,
        crypto_provider: Arc<CryptoProvider>,
        branding: CertBranding,
        passthroughs: Vec<PassthroughInfo>,
        max_proxy_conns_per_target: usize,
        port_map: PortMapController,
    ) -> Self {
        let controller = Self {
            db,
            interfaces,
            crypto_provider,
            acme_cache: Arc::new(SyncMutex::new(BTreeMap::new())),
            branding,
            max_proxy_conns_per_target,
            servers: SyncMutex::new(BTreeMap::new()),
            passthrough_handles: SyncMutex::new(BTreeMap::new()),
            port_map,
            port_mappings: SyncMutex::new(BTreeMap::new()),
        };
        for pt in passthroughs {
            if let Err(e) = controller.add_passthrough(
                pt.hostname,
                pt.listen_port,
                pt.backend,
                pt.public_gateways,
                pt.private_ips,
            ) {
                tracing::warn!("failed to restore passthrough: {e}");
            }
        }
        controller
    }
    #[instrument(skip_all)]
    pub fn add(
        &self,
        hostname: Option<InternedString>,
        external: u16,
        target: DynVHostTarget<VHostBindListener>,
    ) -> Result<Arc<()>, Error> {
        self.servers.mutate(|writable| {
            let server = if let Some(server) = writable.remove(&external) {
                server
            } else {
                self.create_server(external)
            };
            let rc = server.add(hostname, target, self.max_proxy_conns_per_target);
            writable.insert(external, server);
            Ok(rc?)
        })
    }

    fn create_server(&self, port: u16) -> VHostServer<VHostBindListener> {
        let bind_reqs = Watch::new(VHostBindRequirements::default());
        let listener = VHostBindListener {
            ip_info: self.interfaces.watcher.subscribe(),
            port,
            bind_reqs: bind_reqs.clone_unseen(),
            listeners: BTreeMap::new(),
            retry: None,
        };
        VHostServer::new(
            listener,
            bind_reqs,
            self.db.clone(),
            self.crypto_provider.clone(),
            self.branding.clone(),
            self.acme_cache.clone(),
        )
    }

    pub fn add_passthrough(
        &self,
        hostname: InternedString,
        port: u16,
        backend: SocketAddr,
        public: BTreeSet<GatewayId>,
        private: BTreeSet<IpAddr>,
    ) -> Result<(), Error> {
        let target = ProxyTarget {
            // A TLS passthrough is domain (SNI) based, i.e. dual-stack public: it is
            // public on its gateways' bare IPv4 and on each of their GUAs.
            public_v4: public.clone(),
            public_v6: crate::net::utils::gua_ips(&self.interfaces.watcher.ip_info(), &public),
            private: private.clone(),
            acme: None,
            addr: backend,
            add_x_forwarded_headers: false,
            auth: None,
            connect_ssl: Err(AlpnInfo::Reflect),
            passthrough: true,
        };
        let rc = self.add(Some(hostname.clone()), port, DynVHostTarget::new(target))?;
        self.passthrough_handles.mutate(|h| {
            h.insert(
                (hostname, port),
                PassthroughHandle {
                    _rc: rc,
                    backend,
                    public,
                    private,
                },
            );
        });
        Ok(())
    }

    pub fn remove_passthrough(&self, hostname: &InternedString, port: u16) {
        self.passthrough_handles
            .mutate(|h| h.remove(&(hostname.clone(), port)));
        self.gc(Some(hostname.clone()), port);
    }

    pub fn list_passthrough(&self) -> Vec<PassthroughInfo> {
        self.passthrough_handles.peek(|h| {
            h.iter()
                .map(|((hostname, port), handle)| PassthroughInfo {
                    hostname: hostname.clone(),
                    listen_port: *port,
                    backend: handle.backend,
                    public_gateways: handle.public.clone(),
                    private_ips: handle.private.clone(),
                })
                .collect()
        })
    }

    pub fn dump_table(
        &self,
    ) -> BTreeMap<JsonKey<u16>, BTreeMap<JsonKey<Option<InternedString>>, EqSet<String>>> {
        self.servers.peek(|s| {
            s.iter()
                .map(|(k, v)| {
                    (
                        JsonKey::new(*k),
                        v.mapping.peek(|m| {
                            m.iter()
                                .map(|(k, v)| {
                                    (
                                        JsonKey::new(k.clone()),
                                        v.iter()
                                            .filter(|(_, e)| e.alive())
                                            .map(|(k, _)| format!("{k:#?}"))
                                            .collect(),
                                    )
                                })
                                .collect()
                        }),
                    )
                })
                .collect()
        })
    }

    #[instrument(skip_all)]
    pub fn gc(&self, hostname: Option<InternedString>, external: u16) {
        self.servers.mutate(|writable| {
            if let Some(server) = writable.remove(&external) {
                server.gc(hostname);
                if !server.is_empty() {
                    writable.insert(external, server);
                }
            }
        })
    }

    /// Reconcile best-effort upstream port mappings for `owner`'s public vhosts.
    /// `desired` maps `(box IP to map from, external port)` to `(internal port,
    /// candidate gateways, hostnames)`. A `Some(hostname)` is mapped via PCP
    /// HOSTNAME — the gateway treats the hostname as part of the mapping's identity
    /// and demultiplexes the shared external port by TLS SNI, so one service adding
    /// or removing a hostname never disturbs another's on the same port. A `None`
    /// is a bare-IP (`*` vhost) forward: no SNI, so a plain pinhole to the box's own
    /// IP where the OS reverse proxy listens. Like the rest of the port-map layer
    /// this is best-effort: a gateway that can't honor it leaves a manual forward.
    /// Reconcile every upstream IPv4 port map `owner`'s vhosts need, derived from
    /// the vhost `targets` themselves — the controller owns the whole port-map
    /// lifecycle, so callers hand over the same `ProxyTarget` set they use to bind
    /// the listeners and compute nothing. For each target public on IPv4
    /// (`public_v4`), each of that gateway's box IPv4s gets a mapping keyed by the
    /// vhost's hostname: `Some(host)` is a PCP HOSTNAME mapping (SNI demux), `None`
    /// is a bare-IP (`*` vhost) pinhole. IPv6 GUAs carry no NAT, so they get no
    /// mapping here. Always call this every update — an owner whose target set went
    /// empty withdraws its prior mappings via the per-owner diff.
    pub fn reconcile_port_maps(
        &self,
        owner: HostMapOwner,
        targets: &BTreeMap<(Option<InternedString>, u16), ProxyTarget>,
    ) {
        let ip_info = self.interfaces.watcher.ip_info();
        // `(box IP, ext port) -> (internal port, ordered PCP gateway candidates,
        // hostnames)`. Gateways stay an ordered Vec (the port-mapper tries them in
        // preference order and takes the first that binds); hostnames are a set —
        // `None` is the bare-IP/GUA pinhole, `Some(h)` an SNI route, and neither
        // repeats nor cares about order.
        let mut desired: BTreeMap<
            (IpAddr, u16),
            (
                u16,
                Vec<(IpAddr, Option<u32>)>,
                BTreeSet<Option<InternedString>>,
            ),
        > = BTreeMap::new();
        for ((maybe_host, external), target) in targets {
            // IPv4: forward the port to the box's LAN IPv4 — a PCP HOSTNAME mapping
            // (SNI demux) for a domain, a plain pinhole for a bare `*` vhost.
            for gw_id in &target.public_v4 {
                let Some(info) = ip_info.get(gw_id) else {
                    continue;
                };
                let Some(gw_ip_info) = &info.ip_info else {
                    continue;
                };
                let gateways = candidate_gateways(info);
                if gateways.is_empty() {
                    continue;
                }
                for subnet in &gw_ip_info.subnets {
                    let IpAddr::V4(local_ip) = subnet.addr() else {
                        continue;
                    };
                    desired
                        .entry((IpAddr::V4(local_ip), *external))
                        .or_insert_with(|| (*external, gateways.clone(), BTreeSet::new()))
                        .2
                        .insert(maybe_host.clone());
                }
            }
            // IPv6: the box's own GUA is the listener (no NAT), so open a firewall
            // pinhole for GUA:port. No SNI demux over v6 (every host has its own
            // GUA, nothing to share), so these are always hostname-less.
            for gua in &target.public_v6 {
                let Some(info) = ip_info.iter().map(|(_, i)| i).find(|info| {
                    info.ip_info.as_ref().map_or(false, |i| {
                        i.subnets.iter().any(|s| s.addr() == IpAddr::V6(*gua))
                    })
                }) else {
                    continue;
                };
                let v6_gateways: Vec<(IpAddr, Option<u32>)> = candidate_gateways(info)
                    .into_iter()
                    .filter(|(g, _)| g.is_ipv6())
                    .collect();
                if v6_gateways.is_empty() {
                    continue;
                }
                desired
                    .entry((IpAddr::V6(*gua), *external))
                    .or_insert_with(|| (*external, v6_gateways, BTreeSet::new()))
                    .2
                    .insert(None);
            }
        }
        // v6 HTTP->HTTPS redirect: for a GUA exposing 443, ask the gateway for an
        // 80->443 redirect pinhole, unless 80 is already a real pinhole on that GUA.
        // (No IPv4 equivalent — the upstream gateway serves the port-80 redirect.)
        let redirects: Vec<(Ipv6Addr, Vec<(IpAddr, Option<u32>)>)> = desired
            .iter()
            .filter_map(|((ip, port), (_, gateways, _))| match ip {
                IpAddr::V6(gua) if *port == 443 => Some((*gua, gateways.clone())),
                _ => None,
            })
            .filter(|(gua, _)| !desired.contains_key(&(IpAddr::V6(*gua), 80)))
            .collect();
        for (gua, gateways) in redirects {
            desired
                .entry((IpAddr::V6(gua), 80))
                .or_insert((443, gateways, BTreeSet::from([None])));
        }
        self.sync_port_maps(owner, desired);
    }

    fn sync_port_maps(
        &self,
        owner: HostMapOwner,
        desired: BTreeMap<
            (IpAddr, u16),
            (
                u16,
                Vec<(IpAddr, Option<u32>)>,
                BTreeSet<Option<InternedString>>,
            ),
        >,
    ) {
        let want: BTreeSet<(IpAddr, u16, Option<InternedString>)> = desired
            .iter()
            .flat_map(|((ip, port), (_, _, hostnames))| {
                hostnames.iter().map(move |h| (*ip, *port, h.clone()))
            })
            .collect();
        let had = self
            .port_mappings
            .peek(|owners| owners.get(&owner).cloned().unwrap_or_default());
        for (ip, port, hostname) in had.difference(&want) {
            match hostname {
                Some(h) => self.port_map.remove_hostname(*ip, *port, h.to_string()),
                None => self.port_map.remove(*ip, *port),
            }
        }
        for ((ip, port), (internal, gateways, hostnames)) in &desired {
            for hostname in hostnames {
                match hostname {
                    Some(h) => self.port_map.ensure_hostname(
                        *ip,
                        *port,
                        *internal,
                        gateways.clone(),
                        h.to_string(),
                    ),
                    None => self
                        .port_map
                        .ensure(*ip, *port, *internal, gateways.clone()),
                }
            }
        }
        self.port_mappings.mutate(|owners| {
            if want.is_empty() {
                owners.remove(&owner);
            } else {
                owners.insert(owner, want);
            }
        });
    }
}

/// Union of all ProxyTargets' bind requirements for a VHostServer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VHostBindRequirements {
    pub public_gateways: BTreeSet<GatewayId>,
    pub private_ips: BTreeSet<IpAddr>,
}

fn compute_bind_reqs<A: Accept + 'static>(mapping: &Mapping<A>) -> VHostBindRequirements {
    let mut reqs = VHostBindRequirements::default();
    for (_, targets) in mapping {
        for (target, entry) in targets {
            if entry.alive() {
                let (pub_gw, priv_ip) = target.0.bind_requirements();
                reqs.public_gateways.extend(pub_gw);
                reqs.private_ips.extend(priv_ip);
            }
        }
    }
    reqs
}

/// Back off this long before re-attempting a bind that failed. `IP_FREEBIND`
/// (set in `build_listen_socket`) already lets a not-yet-assignable address
/// bind, so this backstops the rarer transient failures — e.g. a port briefly
/// still held by a torn-down listener — instead of latching the hole until the
/// next network change.
const BIND_RETRY_BACKOFF: Duration = Duration::from_secs(2);

/// Listener that manages its own TCP listeners with IP-level precision.
/// Binds ALL IPs of public gateways and ONLY matching private IPs.
pub struct VHostBindListener {
    ip_info: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    port: u16,
    bind_reqs: Watch<VHostBindRequirements>,
    listeners: BTreeMap<SocketAddr, (TcpListener, GatewayInfo)>,
    /// Backoff timer armed after a failed bind; fires a retry reconcile.
    retry: Option<Pin<Box<tokio::time::Sleep>>>,
}

/// The listeners `reqs` calls for: every IP of a required public gateway, plus
/// each explicitly required private IP, bound on `port`.
fn desired_listeners(
    port: u16,
    ip_info: &OrdMap<GatewayId, NetworkInterfaceInfo>,
    reqs: &VHostBindRequirements,
) -> BTreeMap<SocketAddr, GatewayInfo> {
    ip_info
        .iter()
        .filter_map(|(id, iface)| Some((id, iface, iface.ip_info.as_ref()?)))
        .flat_map(|(id, iface, ip_info)| {
            ip_info.subnets.iter().filter_map(move |subnet| {
                let ip = subnet.addr();
                let wanted = reqs.public_gateways.contains(id) || reqs.private_ips.contains(&ip);
                wanted.then(|| {
                    let addr = match ip {
                        IpAddr::V6(v6) if ipv6_is_link_local(v6) => {
                            SocketAddrV6::new(v6, port, 0, ip_info.scope_id).into()
                        }
                        ip => SocketAddr::new(ip, port),
                    };
                    let gateway = GatewayInfo {
                        id: id.clone(),
                        info: iface.clone(),
                    };
                    (addr, gateway)
                })
            })
        })
        .collect()
}

/// Reconcile the bound listeners to match `desired_listeners`. Best-effort: a
/// bind that fails is logged and left unbound, and reported back (`true`) so the
/// caller can retry — rather than aborting the pass and latching the hole until
/// the next network change.
fn update_vhost_listeners(
    listeners: &mut BTreeMap<SocketAddr, (TcpListener, GatewayInfo)>,
    port: u16,
    ip_info: &OrdMap<GatewayId, NetworkInterfaceInfo>,
    reqs: &VHostBindRequirements,
) -> bool {
    let desired = desired_listeners(port, ip_info, reqs);
    listeners.retain(|addr, _| desired.contains_key(addr));

    let mut failed = false;
    for (addr, gateway) in desired {
        match listeners.get_mut(&addr) {
            Some((_, current)) => *current = gateway,
            None => match bind_mio_listener(addr).and_then(|l| TcpListener::from_std(l.into())) {
                Ok(listener) => {
                    listeners.insert(addr, (listener, gateway));
                }
                Err(e) => {
                    tracing::warn!("failed to bind vhost listener on {addr}: {e}");
                    failed = true;
                }
            },
        }
    }
    failed
}

impl Accept for VHostBindListener {
    type Metadata = NetworkInterfaceListenerAcceptMetadata;
    fn poll_accept(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(Self::Metadata, AcceptStream), Error>> {
        // Rebind when the interface set or bind requirements change, or when a
        // pending retry (from an earlier failed bind) elapses.
        let mut reconcile = false;
        while self.ip_info.poll_changed(cx).is_ready() || self.bind_reqs.poll_changed(cx).is_ready()
        {
            reconcile = true;
        }
        if let Some(retry) = self.retry.as_mut() {
            reconcile |= retry.as_mut().poll(cx).is_ready();
        }
        if reconcile {
            let reqs = self.bind_reqs.read_and_mark_seen();
            let listeners = &mut self.listeners;
            let port = self.port;
            let failed = self.ip_info.peek_and_mark_seen(|ip_info| {
                update_vhost_listeners(listeners, port, ip_info, &reqs)
            });
            // A failed bind arms a backoff to retry; a clean pass clears it.
            // Reset unconditionally — a config change may have re-triggered us
            // before the timer elapsed.
            self.retry = failed.then(|| {
                let mut backoff = Box::pin(tokio::time::sleep(BIND_RETRY_BACKOFF));
                // Poll once to arm the timer against the current waker, so it
                // wakes us to retry even if nothing else re-polls the listener.
                let _ = backoff.as_mut().poll(cx);
                backoff
            });
        }

        // Poll each listener for incoming connections
        for (&addr, (listener, gw_info)) in &self.listeners {
            match listener.poll_accept(cx) {
                Poll::Ready(Ok((stream, peer_addr))) => {
                    if let Err(e) = socket2::SockRef::from(&stream)
                        .set_tcp_keepalive(&crate::net::utils::default_keepalive())
                    {
                        tracing::error!("Failed to set tcp keepalive: {e}");
                        tracing::debug!("{e:?}");
                    }
                    return Poll::Ready(Ok((
                        NetworkInterfaceListenerAcceptMetadata {
                            inner: TcpMetadata {
                                local_addr: addr,
                                peer_addr,
                            },
                            info: gw_info.clone(),
                        },
                        Box::pin(stream),
                    )));
                }
                Poll::Ready(Err(e)) => {
                    tracing::trace!("VHostBindListener accept error on {addr}: {e}");
                }
                Poll::Pending => {}
            }
        }
        Poll::Pending
    }
}

pub trait VHostTarget<A: Accept>: std::fmt::Debug + Eq {
    type PreprocessRes: Send + 'static;
    #[allow(unused_variables)]
    fn filter(&self, metadata: &<A as Accept>::Metadata) -> bool {
        true
    }
    fn acme(&self) -> Option<&AcmeProvider> {
        None
    }
    /// Returns (public_gateways, private_ips) this target needs the listener to bind on.
    fn bind_requirements(&self) -> (BTreeSet<GatewayId>, BTreeSet<IpAddr>) {
        (BTreeSet::new(), BTreeSet::new())
    }
    fn is_passthrough(&self) -> bool {
        false
    }
    fn preprocess<'a>(
        &'a self,
        prev: ServerConfig,
        hello: &'a ClientHello<'a>,
        metadata: &'a <A as Accept>::Metadata,
    ) -> impl Future<Output = Option<(ServerConfig, Self::PreprocessRes)>> + Send + 'a;
    fn handle_stream(
        &self,
        stream: AcceptStream,
        metadata: TlsMetadata<<A as Accept>::Metadata>,
        prev: Self::PreprocessRes,
        ctx: ProxyContext,
    );
}

pub trait DynVHostTargetT<A: Accept>: std::fmt::Debug + Any {
    fn filter(&self, metadata: &<A as Accept>::Metadata) -> bool;
    fn acme(&self) -> Option<&AcmeProvider>;
    fn bind_requirements(&self) -> (BTreeSet<GatewayId>, BTreeSet<IpAddr>);
    fn is_passthrough(&self) -> bool;
    fn preprocess<'a>(
        &'a self,
        prev: ServerConfig,
        hello: &'a ClientHello<'a>,
        metadata: &'a <A as Accept>::Metadata,
    ) -> BoxFuture<'a, Option<(ServerConfig, Box<dyn Any + Send>)>>
    where
        <A as Accept>::Metadata: Visit<ExtractVisitor<TcpMetadata>>;
    fn handle_stream(
        &self,
        stream: AcceptStream,
        metadata: TlsMetadata<<A as Accept>::Metadata>,
        prev: Box<dyn Any + Send>,
        ctx: ProxyContext,
    );
    fn eq(&self, other: &dyn DynVHostTargetT<A>) -> bool;
}
impl<A: Accept, T: VHostTarget<A> + 'static> DynVHostTargetT<A> for T {
    fn filter(&self, metadata: &<A as Accept>::Metadata) -> bool {
        VHostTarget::filter(self, metadata)
    }
    fn acme(&self) -> Option<&AcmeProvider> {
        VHostTarget::acme(self)
    }
    fn is_passthrough(&self) -> bool {
        VHostTarget::is_passthrough(self)
    }
    fn bind_requirements(&self) -> (BTreeSet<GatewayId>, BTreeSet<IpAddr>) {
        VHostTarget::bind_requirements(self)
    }
    fn preprocess<'a>(
        &'a self,
        prev: ServerConfig,
        hello: &'a ClientHello<'a>,
        metadata: &'a <A as Accept>::Metadata,
    ) -> BoxFuture<'a, Option<(ServerConfig, Box<dyn Any + Send>)>> {
        VHostTarget::preprocess(self, prev, hello, metadata)
            .map(|o| o.map(|(cfg, res)| (cfg, Box::new(res) as Box<dyn Any + Send>)))
            .boxed()
    }
    fn handle_stream(
        &self,
        stream: AcceptStream,
        metadata: TlsMetadata<<A as Accept>::Metadata>,
        prev: Box<dyn Any + Send>,
        ctx: ProxyContext,
    ) {
        if let Ok(prev) = prev.downcast() {
            VHostTarget::handle_stream(self, stream, metadata, *prev, ctx);
        }
    }
    fn eq(&self, other: &dyn DynVHostTargetT<A>) -> bool {
        Some(self) == (other as &dyn Any).downcast_ref()
    }
}

pub struct DynVHostTarget<A: Accept>(Arc<dyn DynVHostTargetT<A> + Send + Sync>);
impl<A: Accept> DynVHostTarget<A> {
    pub fn new<T: VHostTarget<A> + Send + Sync + 'static>(target: T) -> Self {
        Self(Arc::new(target))
    }
}
impl<A: Accept> Clone for DynVHostTarget<A> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<A: Accept> std::fmt::Debug for DynVHostTarget<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl<A: Accept + 'static> PartialEq for DynVHostTarget<A> {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&*other.0)
    }
}
impl<A: Accept + 'static> Eq for DynVHostTarget<A> {}
struct Preprocessed<A: Accept>(DynVHostTarget<A>, ProxyContext, Box<dyn Any + Send>);
impl<A: Accept> fmt::Debug for Preprocessed<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.0).0.fmt(f)
    }
}
impl<A: Accept + 'static> DynVHostTarget<A> {
    async fn into_preprocessed(
        self,
        ctx: ProxyContext,
        prev: ServerConfig,
        hello: &ClientHello<'_>,
        metadata: &<A as Accept>::Metadata,
    ) -> Option<(ServerConfig, Preprocessed<A>)>
    where
        <A as Accept>::Metadata: Visit<ExtractVisitor<TcpMetadata>>,
    {
        let (cfg, res) = self.0.preprocess(prev, hello, metadata).await?;
        Some((cfg, Preprocessed(self, ctx, res)))
    }
}
impl<A: Accept + 'static> Preprocessed<A> {
    fn finish(self, stream: AcceptStream, metadata: TlsMetadata<<A as Accept>::Metadata>) {
        (self.0).0.handle_stream(stream, metadata, self.2, self.1);
    }
}

#[derive(Clone)]
pub struct ProxyTarget {
    /// Gateways on which this address is WAN-public over bare IPv4 (drives the
    /// IPv4 accept filter and the derived IPv4 upstream forward). Split from
    /// `public_v6` because a service can expose its GUA to the WAN while keeping
    /// its bare IPv4 LAN-only (and vice versa), so accept must gate per family.
    pub public_v4: BTreeSet<GatewayId>,
    /// The box's own GUAs that are WAN-public (drives the IPv6 accept filter; the
    /// GUA has no NAT, so no IPv4-style forward). Per-IP, not per-gateway, because
    /// one gateway can carry several GUAs (SLAAC / multiple prefixes) that are
    /// independently Local vs Public.
    pub public_v6: BTreeSet<Ipv6Addr>,
    pub private: BTreeSet<IpAddr>,
    pub acme: Option<AcmeProvider>,
    pub addr: SocketAddr,
    pub add_x_forwarded_headers: bool,
    /// Optional `Authorization` header value to inject on upstream
    /// requests. Implies HTTP-aware proxying (same path as forwarded headers).
    pub auth: Option<crate::net::host::binding::ProxyAuth>,
    pub connect_ssl: Result<Arc<ClientConfig>, AlpnInfo>, // Ok: yes, connect using ssl, pass through alpn; Err: connect tcp, use provided strategy for alpn
    pub passthrough: bool,
}
impl PartialEq for ProxyTarget {
    fn eq(&self, other: &Self) -> bool {
        self.public_v4 == other.public_v4
            && self.public_v6 == other.public_v6
            && self.private == other.private
            && self.acme == other.acme
            && self.addr == other.addr
            && self.add_x_forwarded_headers == other.add_x_forwarded_headers
            && self.auth == other.auth
            && self.passthrough == other.passthrough
            && self.connect_ssl.as_ref().map(Arc::as_ptr)
                == other.connect_ssl.as_ref().map(Arc::as_ptr)
    }
}
impl Eq for ProxyTarget {}
impl fmt::Debug for ProxyTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProxyTarget")
            .field("public_v4", &self.public_v4)
            .field("public_v6", &self.public_v6)
            .field("private", &self.private)
            .field("acme", &self.acme)
            .field("addr", &self.addr)
            .field("add_x_forwarded_headers", &self.add_x_forwarded_headers)
            .field("auth", &self.auth.as_ref().map(|_| "<redacted>"))
            .field("connect_ssl", &self.connect_ssl.as_ref().map(|_| ()))
            .field("passthrough", &self.passthrough)
            .finish()
    }
}

impl ProxyTarget {
    /// Whether to accept a connection that arrived on local address `dst` from
    /// `src`, via gateway `gw_id` whose on-link subnets are `subnets`. WAN accept
    /// is gated per family by `dst`'s family — a gateway public only on its GUA
    /// accepts IPv6 WAN but rejects IPv4 WAN, and vice versa. LAN accept (via
    /// `private`, matched by the exact local `dst`) is inherently family-scoped.
    fn accepts(
        &self,
        gw_id: &GatewayId,
        subnets: &OrdSet<IpNet>,
        src: IpAddr,
        dst: IpAddr,
    ) -> bool {
        // IPv4 WAN is keyed by the arrival gateway (NAT hides the WAN IP; the box
        // sees its LAN IPv4); IPv6 WAN is keyed by the exact GUA the connection
        // arrived on, since one gateway can carry several independently-public GUAs.
        let wan = match dst {
            IpAddr::V4(_) => self.public_v4.contains(gw_id),
            IpAddr::V6(v6) => self.public_v6.contains(&v6),
        };
        wan || (self.private.contains(&dst)
            && (subnets.iter().any(|s| s.contains(&src)) || is_private_ip(src)))
    }
}

impl<A> VHostTarget<A> for ProxyTarget
where
    A: Accept + 'static,
    <A as Accept>::Metadata: Visit<ExtractVisitor<GatewayInfo>>
        + Visit<ExtractVisitor<TcpMetadata>>
        + Clone
        + Send
        + Sync,
{
    type PreprocessRes = AcceptStream;
    fn filter(&self, metadata: &<A as Accept>::Metadata) -> bool {
        let gw = extract::<GatewayInfo, _>(metadata);
        let tcp = extract::<TcpMetadata, _>(metadata);
        let (Some(gw), Some(tcp)) = (gw, tcp) else {
            return false;
        };
        let Some(ip_info) = &gw.info.ip_info else {
            return false;
        };

        let src = tcp.peer_addr.ip();
        let dst = tcp.local_addr.ip();

        self.accepts(&gw.id, &ip_info.subnets, src, dst)
    }
    fn acme(&self) -> Option<&AcmeProvider> {
        self.acme.as_ref()
    }
    fn bind_requirements(&self) -> (BTreeSet<GatewayId>, BTreeSet<IpAddr>) {
        // Bind every IP of an IPv4-public gateway (the box's LAN IPv4 is the DNAT
        // target; `filter` gates WAN acceptance per gateway) plus each public GUA
        // and private IP explicitly. `filter` still gates acceptance per address,
        // so binding a not-actually-public address is harmless.
        let mut bind_ips = self.private.clone();
        bind_ips.extend(self.public_v6.iter().map(|v6| IpAddr::V6(*v6)));
        (self.public_v4.clone(), bind_ips)
    }
    fn is_passthrough(&self) -> bool {
        self.passthrough
    }
    async fn preprocess<'a>(
        &'a self,
        mut prev: ServerConfig,
        hello: &'a ClientHello<'a>,
        metadata: &'a <A as Accept>::Metadata,
    ) -> Option<(ServerConfig, Self::PreprocessRes)> {
        let peer = extract::<TcpMetadata, _>(metadata).map(|m| m.peer_addr);
        let tcp_stream = match (self.passthrough, peer, self.addr) {
            // Passthrough (service handles its own TLS): open the internal leg
            // from the client's own source address so the backend sees the real
            // peer (RFC §4.6). Non-passthrough/terminating targets keep the plain
            // connect — they don't preserve source IP.
            (true, Some(SocketAddr::V4(client)), SocketAddr::V4(target)) => {
                crate::net::transparent::ensure_divert_infra_once()
                    .await
                    .log_err();
                crate::net::transparent::transparent_connect(client, target)
                    .await
                    .with_ctx(|_| (ErrorKind::Network, self.addr))
                    .log_err()?
            }
            _ => TcpStream::connect(self.addr)
                .await
                .with_ctx(|_| (ErrorKind::Network, self.addr))
                .log_err()?,
        };
        if let Err(e) = socket2::SockRef::from(&tcp_stream)
            .set_tcp_keepalive(&crate::net::utils::default_keepalive())
        {
            tracing::error!("Failed to set tcp keepalive: {e}");
            tracing::debug!("{e:?}");
        }
        match &self.connect_ssl {
            Ok(client_cfg) => {
                let mut client_cfg = (&**client_cfg).clone();
                client_cfg.alpn_protocols = hello
                    .alpn()
                    .into_iter()
                    .flatten()
                    .map(|x| x.to_vec())
                    .collect();
                let target_stream = TlsConnector::from(Arc::new(client_cfg))
                    .connect_with(
                        ServerName::IpAddress(self.addr.ip().into()),
                        tcp_stream,
                        |conn| {
                            prev.alpn_protocols
                                .extend(conn.alpn_protocol().into_iter().map(|p| p.to_vec()))
                        },
                    )
                    .await
                    .log_err()?;
                return Some((prev, Box::pin(target_stream)));
            }
            Err(AlpnInfo::Reflect) => {
                for alpn in hello.alpn().into_iter().flatten() {
                    prev.alpn_protocols.push(alpn.to_vec());
                }
            }
            Err(AlpnInfo::Specified(a)) => {
                for alpn in a {
                    prev.alpn_protocols.push(alpn.0.clone());
                }
            }
        }
        Some((prev, Box::pin(tcp_stream)))
    }
    fn handle_stream(
        &self,
        stream: AcceptStream,
        metadata: TlsMetadata<<A as Accept>::Metadata>,
        mut prev: Self::PreprocessRes,
        ctx: ProxyContext,
    ) {
        let add_x_forwarded_headers = self.add_x_forwarded_headers;
        // Pre-compile the auth gate once per stream — base64-encode all
        // accepted credentials, build the lookup map and the
        // WWW-Authenticate challenge — so each request on this connection
        // is just a HashMap probe. Compile errors (e.g. credentials that
        // can't fit in a `HeaderValue`, or any other authoring bug) fail
        // closed: we log and drop the connection rather than risk
        // silently exposing an upstream that the operator intended to
        // gate.
        let auth_gate = match self
            .auth
            .as_ref()
            .map(crate::net::http::AuthGate::from_auth)
        {
            Some(Ok(g)) => Some(g),
            Some(Err(e)) => {
                tracing::error!("Failed to compile proxy auth gate; refusing connection: {e}");
                tracing::debug!("{e:?}");
                drop(stream);
                drop(prev);
                return;
            }
            None => None,
        };
        let http_aware = add_x_forwarded_headers || auth_gate.is_some();
        let (mut stream, registration, conn_cancel) = ctx.track_with(stream);
        let target_cancel = ctx.cancel.clone();
        tokio::spawn(async move {
            // Force capture by the outer `async move`; without a reference
            // here the registry entry would deregister immediately.
            let _registration = registration;
            let work = async move {
                if http_aware {
                    crate::net::http::run_http_proxy(
                        stream,
                        prev,
                        metadata.tls_info.alpn,
                        extract::<TcpMetadata, _>(&metadata.inner).map(|m| m.peer_addr.ip()),
                        add_x_forwarded_headers,
                        auth_gate,
                    )
                    .await
                    .ok();
                } else {
                    tokio::io::copy_bidirectional(&mut stream, &mut prev)
                        .await
                        .ok();
                }
            };
            tokio::select! {
                _ = target_cancel.cancelled() => {}
                _ = conn_cancel.cancelled() => {}
                _ = work => {}
            }
        });
    }
}

pub const MAX_PROXY_CONNS_PER_TARGET: usize = 4096;

fn monotonic_millis() -> u64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Ticks `last_active` on byte progress; never closes the stream.
pub struct ActivityStream<S> {
    inner: S,
    last_active: Arc<AtomicU64>,
}
impl<S> ActivityStream<S> {
    pub fn new(inner: S, last_active: Arc<AtomicU64>) -> Self {
        last_active.store(monotonic_millis(), Ordering::Relaxed);
        Self { inner, last_active }
    }
}
impl<S: AsyncRead + Unpin> AsyncRead for ActivityStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let res = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &res {
            if buf.filled().len() > before {
                self.last_active
                    .store(monotonic_millis(), Ordering::Relaxed);
            }
        }
        res
    }
}
impl<S: AsyncWrite + Unpin> AsyncWrite for ActivityStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let res = Pin::new(&mut self.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(n)) = &res {
            if *n > 0 {
                self.last_active
                    .store(monotonic_millis(), Ordering::Relaxed);
            }
        }
        res
    }
    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> Poll<std::io::Result<usize>> {
        let res = Pin::new(&mut self.inner).poll_write_vectored(cx, bufs);
        if let Poll::Ready(Ok(n)) = &res {
            if *n > 0 {
                self.last_active
                    .store(monotonic_millis(), Ordering::Relaxed);
            }
        }
        res
    }
    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
}

#[derive(Debug)]
struct ConnEntry {
    last_active: Arc<AtomicU64>,
    cancel: CancellationToken,
}

/// Per-target proxy-task registry with LRU eviction at the cap.
#[derive(Debug)]
pub struct ConnRegistry {
    cap: usize,
    next_id: AtomicU64,
    entries: SyncMutex<HashMap<u64, ConnEntry>>,
}
impl ConnRegistry {
    pub fn new(cap: usize) -> Arc<Self> {
        Arc::new(Self {
            cap,
            next_id: AtomicU64::new(0),
            entries: SyncMutex::new(HashMap::new()),
        })
    }
    fn register(
        self: &Arc<Self>,
        last_active: Arc<AtomicU64>,
        cancel: CancellationToken,
    ) -> ConnRegHandle {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.entries.mutate(|m| {
            if m.len() >= self.cap {
                if let Some((victim_id, victim)) = m
                    .iter()
                    .min_by_key(|(_, e)| e.last_active.load(Ordering::Relaxed))
                    .map(|(k, e)| (*k, e.cancel.clone()))
                {
                    victim.cancel();
                    m.remove(&victim_id);
                }
            }
            m.insert(
                id,
                ConnEntry {
                    last_active,
                    cancel,
                },
            );
        });
        ConnRegHandle {
            id,
            registry: Arc::downgrade(self),
        }
    }
    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.peek(|m| m.len())
    }
}

pub struct ConnRegHandle {
    id: u64,
    registry: Weak<ConnRegistry>,
}
impl Drop for ConnRegHandle {
    fn drop(&mut self) {
        if let Some(r) = self.registry.upgrade() {
            r.entries.mutate(|m| {
                m.remove(&self.id);
            });
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProxyContext {
    pub cancel: CancellationToken,
    pub registry: Arc<ConnRegistry>,
}
impl ProxyContext {
    fn new(max_conns: usize) -> Self {
        Self {
            cancel: CancellationToken::new(),
            registry: ConnRegistry::new(max_conns),
        }
    }
    pub fn track<S>(&self, stream: S) -> (ActivityStream<S>, ConnRegHandle) {
        let last_active = Arc::new(AtomicU64::new(monotonic_millis()));
        let handle = self
            .registry
            .register(last_active.clone(), self.cancel.child_token());
        (ActivityStream::new(stream, last_active), handle)
    }
    pub fn track_with(
        &self,
        stream: AcceptStream,
    ) -> (AcceptStream, ConnRegHandle, CancellationToken) {
        let last_active = Arc::new(AtomicU64::new(monotonic_millis()));
        let conn_cancel = self.cancel.child_token();
        let handle = self
            .registry
            .register(last_active.clone(), conn_cancel.clone());
        let wrapped: Pin<Box<dyn ReadWriter + Send + 'static>> =
            Box::pin(ActivityStream::new(stream, last_active));
        (wrapped, handle, conn_cancel)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AlpnInfo {
    Reflect,
    Specified(Vec<MaybeUtf8String>),
}
impl Default for AlpnInfo {
    fn default() -> Self {
        Self::Reflect
    }
}

#[derive(Debug, Clone)]
pub struct TargetEntry {
    rc: Weak<()>,
    ctx: ProxyContext,
}
impl TargetEntry {
    fn new(rc: Weak<()>, max_conns: usize) -> Self {
        Self {
            rc,
            ctx: ProxyContext::new(max_conns),
        }
    }
    fn alive(&self) -> bool {
        self.rc.strong_count() > 0
    }
}

fn cancel_dead<A: Accept + 'static>(targets: &mut InOMap<DynVHostTarget<A>, TargetEntry>) {
    targets.retain(|_, e| {
        let alive = e.alive();
        if !alive {
            e.ctx.cancel.cancel();
        }
        alive
    });
}

type Mapping<A> = BTreeMap<Option<InternedString>, InOMap<DynVHostTarget<A>, TargetEntry>>;

pub struct GetVHostAcmeProvider<A: Accept + 'static>(pub Watch<Mapping<A>>);
impl<A: Accept + 'static> Clone for GetVHostAcmeProvider<A> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<A: Accept + 'static> GetAcmeProvider for GetVHostAcmeProvider<A> {
    async fn get_provider<'a, 'b: 'a>(
        &'b self,
        san_info: &'a BTreeSet<InternedString>,
    ) -> Option<impl AsRef<AcmeProvider> + Send + 'b> {
        self.0.peek(|m| -> Option<AcmeProvider> {
            san_info
                .iter()
                .fold(Some::<Option<&AcmeProvider>>(None), |acc, x| {
                    let acc = acc?;
                    if x.parse::<IpAddr>().is_ok() {
                        return Some(acc);
                    }
                    let (t, _) = m.get(&Some(x.clone()))?.iter().find(|(_, e)| e.alive())?;
                    let acme = t.0.acme()?;
                    Some(if let Some(acc) = acc {
                        if acme == acc {
                            // all must match
                            Some(acme)
                        } else {
                            None
                        }
                    } else {
                        Some(acme)
                    })
                })
                .flatten()
                .cloned()
        })
    }
}

/// No-op cert resolver used to build a cheap [`ServerConfig`] for the
/// passthrough path. The config is fed to [`ProxyTarget::preprocess`] so it
/// can satisfy the [`VHostTarget`] trait signature, but the caller discards
/// it before any handshake runs against it — the resolver is never asked
/// to produce a certificate.
#[derive(Debug)]
struct NoCertResolver;
impl tokio_rustls::rustls::server::ResolvesServerCert for NoCertResolver {
    fn resolve(&self, _: ClientHello) -> Option<Arc<tokio_rustls::rustls::sign::CertifiedKey>> {
        None
    }
}

fn passthrough_stub_config(crypto_provider: &Arc<CryptoProvider>) -> Result<ServerConfig, Error> {
    Ok(ServerConfig::builder_with_provider(crypto_provider.clone())
        .with_safe_default_protocol_versions()
        .with_kind(ErrorKind::OpenSsl)?
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(NoCertResolver)))
}

/// Routes incoming TLS connections by SNI. For passthrough targets the
/// expensive cert-resolution chain (`AcmeTlsHandler` + `RootCaTlsHandler`,
/// the latter of which goes through a write-locked patch-db transaction
/// and can lazily generate a keypair plus sign two leaf certs) is skipped
/// entirely — a passthrough connection only needs to TCP-connect to the
/// backend so the client's TLS handshake can complete against it, and any
/// `ServerConfig` we built locally would be discarded.
///
/// The handler holds the cert-resolution chain as `inner` and only invokes
/// it when the matched target terminates TLS, or when the connection is the
/// ACME `acme-tls/1` ALPN challenge (which has to be answered locally).
pub struct VHostTlsHandler<I, A: Accept + 'static> {
    inner: I,
    crypto_provider: Arc<CryptoProvider>,
    mapping: Watch<Mapping<A>>,
    preprocessed: Option<Preprocessed<A>>,
}
impl<I: Clone, A: Accept + 'static> Clone for VHostTlsHandler<I, A> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            crypto_provider: self.crypto_provider.clone(),
            mapping: self.mapping.clone(),
            // Per-connection state — never carried across clones; each
            // accepted connection clones the handler before populating it.
            preprocessed: None,
        }
    }
}

impl<'a, A, I> TlsHandler<'a, A> for VHostTlsHandler<I, A>
where
    A: Accept + 'a,
    <A as Accept>::Metadata:
        Visit<ExtractVisitor<GatewayInfo>> + Visit<ExtractVisitor<TcpMetadata>> + Send + Sync,
    I: TlsHandler<'a, A> + Send,
{
    async fn get_config(
        &'a mut self,
        hello: &'a ClientHello<'a>,
        metadata: &'a <A as Accept>::Metadata,
    ) -> Option<TlsHandlerAction> {
        let routed = self.mapping.peek(|m| {
            m.get(&hello.server_name().map(InternedString::from))
                .or_else(|| m.get(&None))
                .into_iter()
                .flatten()
                .filter(|(_, e)| e.alive())
                .find(|(t, _)| t.0.filter(metadata))
                .map(|(t, e)| (t.clone(), e.ctx.clone()))
        });

        let acme_alpn = hello
            .alpn()
            .into_iter()
            .flatten()
            .any(|a| a == ACME_TLS_ALPN_NAME);

        // Passthroughs should not intermediate ACME challenges — the
        // backend is the ACME client and holds the challenge cert.
        if let Some((target, ctx)) = routed.as_ref().filter(|(t, _)| t.0.is_passthrough()) {
            let stub = passthrough_stub_config(&self.crypto_provider).log_err()?;
            let (_, store) = target
                .clone()
                .into_preprocessed(ctx.clone(), stub, hello, metadata)
                .await?;
            self.preprocessed = Some(store);
            return Some(TlsHandlerAction::Passthrough);
        }

        // ACME challenge for a terminating target (or for one with no
        // explicit vhost mapping): answer locally from the inner chain.
        if acme_alpn {
            return self.inner.get_config(hello, metadata).await;
        }

        let Some((target, ctx)) = routed else {
            return None;
        };

        let action = self.inner.get_config(hello, metadata).await?;
        let cfg = match action {
            TlsHandlerAction::Tls(cfg) => cfg,
            other => return Some(other),
        };
        let (prev, store) = target.into_preprocessed(ctx, cfg, hello, metadata).await?;
        self.preprocessed = Some(store);
        Some(TlsHandlerAction::Tls(prev))
    }
}

struct VHostListener<M, A>(
    TlsListener<
        A,
        VHostTlsHandler<
            ChainedHandler<Arc<AcmeTlsHandler<M, GetVHostAcmeProvider<A>>>, RootCaTlsHandler<M>>,
            A,
        >,
    >,
)
where
    for<'a> M: HasModel<Model = Model<M>>
        + DbAccessMut<CertStore>
        + DbAccessMut<AcmeCertStore>
        + DbAccessByKey<AcmeSettings, Key<'a> = &'a AcmeProvider>
        + Send
        + Sync
        + 'static,
    A: Accept + 'static,
    <A as Accept>::Metadata: Visit<ExtractVisitor<TcpMetadata>>
        + Visit<ExtractVisitor<GatewayInfo>>
        + Clone
        + Send
        + Sync
        + 'static;
struct VHostListenerMetadata<A: Accept> {
    inner: TlsMetadata<A::Metadata>,
    preprocessed: Preprocessed<A>,
}
impl<A: Accept> fmt::Debug for VHostListenerMetadata<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VHostListenerMetadata")
            .field("inner", &self.inner)
            .field("preprocessed", &self.preprocessed)
            .finish()
    }
}
impl<M, A> Accept for VHostListener<M, A>
where
    for<'a> M: HasModel<Model = Model<M>>
        + DbAccessMut<CertStore>
        + DbAccessMut<AcmeCertStore>
        + DbAccessByKey<AcmeSettings, Key<'a> = &'a AcmeProvider>
        + Send
        + Sync
        + 'static,
    A: Accept + 'static,
    <A as Accept>::Metadata: Visit<ExtractVisitor<TcpMetadata>>
        + Visit<ExtractVisitor<GatewayInfo>>
        + Clone
        + Send
        + Sync
        + 'static,
{
    type Metadata = VHostListenerMetadata<A>;
    fn poll_accept(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(Self::Metadata, AcceptStream), Error>> {
        let (metadata, stream) = ready!(self.0.poll_accept(cx)?);
        let preprocessed = self.0.tls_handler.preprocessed.take();
        Poll::Ready(Ok((
            VHostListenerMetadata {
                inner: metadata,
                preprocessed: preprocessed.ok_or_else(|| {
                    Error::new(
                        eyre!("tlslistener yielded but preprocessed isn't set"),
                        ErrorKind::Incoherent,
                    )
                })?,
            },
            stream,
        )))
    }
}
impl<M, A> VHostListener<M, A>
where
    for<'a> M: HasModel<Model = Model<M>>
        + DbAccessMut<CertStore>
        + DbAccessMut<AcmeCertStore>
        + DbAccessByKey<AcmeSettings, Key<'a> = &'a AcmeProvider>
        + Send
        + Sync
        + 'static,
    A: Accept + 'static,
    <A as Accept>::Metadata: Visit<ExtractVisitor<TcpMetadata>>
        + Visit<ExtractVisitor<GatewayInfo>>
        + Clone
        + Send
        + Sync
        + 'static,
{
    async fn handle_next(&mut self) -> Result<(), Error> {
        let (metadata, stream) = futures::future::poll_fn(|cx| self.poll_accept(cx)).await?;

        metadata.preprocessed.finish(stream, metadata.inner);

        Ok(())
    }
}

struct VHostServer<A: Accept + 'static> {
    mapping: Watch<Mapping<A>>,
    bind_reqs: Watch<VHostBindRequirements>,
    _thread: NonDetachingJoinHandle<()>,
}

impl<A: Accept> VHostServer<A> {
    #[instrument(skip_all)]
    fn new<M: HasModel>(
        listener: A,
        bind_reqs: Watch<VHostBindRequirements>,
        db: TypedPatchDb<M>,
        crypto_provider: Arc<CryptoProvider>,
        branding: CertBranding,
        acme_cache: AcmeTlsAlpnCache,
    ) -> Self
    where
        for<'a> M: HasModel<Model = Model<M>>
            + DbAccessMut<CertStore>
            + DbAccessMut<AcmeCertStore>
            + DbAccessByKey<AcmeSettings, Key<'a> = &'a AcmeProvider>
            + Send
            + Sync
            + 'static,
        A: Accept + Send + 'static,
        <A as Accept>::Metadata: Visit<ExtractVisitor<TcpMetadata>>
            + Visit<ExtractVisitor<GatewayInfo>>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let mapping = Watch::new(BTreeMap::new());
        Self {
            mapping: mapping.clone(),
            bind_reqs,
            _thread: tokio::spawn(async move {
                let mut listener = VHostListener(TlsListener::new(
                    listener,
                    VHostTlsHandler {
                        inner: ChainedHandler(
                            Arc::new(AcmeTlsHandler {
                                db: db.clone(),
                                acme_cache,
                                crypto_provider: crypto_provider.clone(),
                                get_provider: GetVHostAcmeProvider(mapping.clone()),
                                in_progress: Watch::new(BTreeMap::new()),
                            }),
                            RootCaTlsHandler {
                                db,
                                crypto_provider: crypto_provider.clone(),
                                branding,
                            },
                        ),
                        crypto_provider,
                        mapping,
                        preprocessed: None,
                    },
                ));
                loop {
                    if let Err(e) = listener.handle_next().await {
                        tracing::trace!("VHostServer: failed to accept connection: {e}");
                        tracing::trace!("{e:?}");
                    }
                }
            })
            .into(),
        }
    }
    fn add(
        &self,
        hostname: Option<InternedString>,
        target: DynVHostTarget<A>,
        max_proxy_conns_per_target: usize,
    ) -> Result<Arc<()>, Error> {
        let target = target.into();
        let mut res = Ok(Arc::new(()));
        self.mapping.send_if_modified(|writable| {
            let mut changed = false;
            let mut targets = writable.remove(&hostname).unwrap_or_default();
            // Reuse the existing ctx on re-add so in-flight tasks keep
            // their lifecycle; cancel a stale ctx before replacing it.
            let existing = targets.remove(&target);
            let (rc, entry) = match existing {
                Some(e) => match e.rc.upgrade() {
                    Some(rc) => (
                        rc.clone(),
                        TargetEntry {
                            rc: Arc::downgrade(&rc),
                            ctx: e.ctx,
                        },
                    ),
                    None => {
                        e.ctx.cancel.cancel();
                        changed = true;
                        let rc = Arc::new(());
                        (
                            rc.clone(),
                            TargetEntry::new(Arc::downgrade(&rc), max_proxy_conns_per_target),
                        )
                    }
                },
                None => {
                    changed = true;
                    let rc = Arc::new(());
                    (
                        rc.clone(),
                        TargetEntry::new(Arc::downgrade(&rc), max_proxy_conns_per_target),
                    )
                }
            };
            cancel_dead(&mut targets);
            targets.insert(target, entry);
            writable.insert(hostname, targets);
            res = Ok(rc);
            if changed {
                self.update_bind_reqs(writable);
            }
            changed
        });
        if self.mapping.watcher_count() > 1 {
            res
        } else {
            Err(Error::new(
                eyre!("VHost Service Thread has exited"),
                crate::ErrorKind::Network,
            ))
        }
    }
    fn gc(&self, hostname: Option<InternedString>) {
        self.mapping.send_if_modified(|writable| {
            let mut targets = writable.remove(&hostname).unwrap_or_default();
            let pre = targets.len();
            cancel_dead(&mut targets);
            let post = targets.len();
            if !targets.is_empty() {
                writable.insert(hostname, targets);
            }
            if pre != post {
                self.update_bind_reqs(writable);
            }
            pre == post
        });
    }
    fn update_bind_reqs(&self, mapping: &Mapping<A>) {
        let new_reqs = compute_bind_reqs(mapping);
        self.bind_reqs.send_if_modified(|reqs| {
            if *reqs != new_reqs {
                *reqs = new_reqs;
                true
            } else {
                false
            }
        });
    }
    fn is_empty(&self) -> bool {
        self.mapping.peek(|m| m.is_empty())
    }
}

#[tokio::test]
async fn copy_bidirectional_hangs_without_keepalive_when_peer_idle() {
    use std::time::Duration;
    // Documents why we tune TCP keepalive on every accepted/connected
    // socket (see `crate::net::utils::default_keepalive`).
    // `tokio::io::copy_bidirectional` drains each
    // direction to EOF, half-closes the destination, and only returns
    // once both directions have settled. If one peer closes but the other
    // stays open (idle HTTP keep-alive, stuck WebSocket, misbehaving
    // service), it waits indefinitely.
    //
    // In production the underlying TCP sockets have tuned SO_KEEPALIVE so
    // a silent peer is surfaced as an I/O error on reads within ~2 min,
    // which errors the direction and lets copy_bidirectional return
    // without losing in-flight bytes. `tokio::io::duplex` has no keepalive
    // concept, so here the hang is unbounded and we time it out
    // explicitly to verify the shape of the behavior.
    let (mut client_facing, client_side) = tokio::io::duplex(1024);
    let (mut backend_facing, _backend_side) = tokio::io::duplex(1024);

    let mut proxy = tokio::spawn(async move {
        tokio::io::copy_bidirectional(&mut client_facing, &mut backend_facing)
            .await
            .ok();
    });

    drop(client_side);

    let res = tokio::time::timeout(Duration::from_millis(200), &mut proxy).await;
    assert!(
        res.is_err(),
        "copy_bidirectional returned without keepalive to break the idle \
         peer out — keepalive tuning is what makes this bounded in prod."
    );

    proxy.abort();
}

#[cfg(test)]
mod accept_filter_tests {
    use super::*;

    fn gw(name: &str) -> GatewayId {
        GatewayId::from(InternedString::intern(name))
    }

    fn target(public_v4: &[&str], public_v6: &[&str], private: &[&str]) -> ProxyTarget {
        ProxyTarget {
            public_v4: public_v4.iter().map(|s| gw(s)).collect(),
            public_v6: public_v6.iter().map(|s| s.parse().unwrap()).collect(),
            private: private.iter().map(|s| s.parse().unwrap()).collect(),
            acme: None,
            addr: "10.0.0.1:443".parse().unwrap(),
            add_x_forwarded_headers: false,
            auth: None,
            connect_ssl: Err(AlpnInfo::Reflect),
            passthrough: false,
        }
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    // WAN accept is gated by the family of the address the connection arrived on:
    // IPv4 by the gateway, IPv6 by the exact GUA — a gateway public only on its GUA
    // must accept IPv6 WAN but reject IPv4 WAN, and a non-public GUA on an
    // otherwise-public gateway is rejected.
    #[test]
    fn wan_accept_is_family_scoped() {
        let g = gw("wg0");
        let no_subnets = OrdSet::new();
        let src = ip("198.51.100.9");
        let v4 = ip("203.0.113.5");
        let gua = ip("2001:db8::1");
        let other_gua = ip("2001:db8::2");

        // GUA-only public (only 2001:db8::1).
        let t = target(&[], &["2001:db8::1"], &[]);
        assert!(
            t.accepts(&g, &no_subnets, src, gua),
            "IPv6 WAN to the public GUA"
        );
        assert!(
            !t.accepts(&g, &no_subnets, src, other_gua),
            "a different GUA on the same gateway is not public"
        );
        assert!(!t.accepts(&g, &no_subnets, src, v4), "IPv4 WAN rejected");

        // Bare-IPv4-only public.
        let t = target(&["wg0"], &[], &[]);
        assert!(t.accepts(&g, &no_subnets, src, v4), "IPv4 WAN accepted");
        assert!(!t.accepts(&g, &no_subnets, src, gua), "IPv6 WAN rejected");

        // Dual-stack public accepts both.
        let t = target(&["wg0"], &["2001:db8::1"], &[]);
        assert!(t.accepts(&g, &no_subnets, src, v4));
        assert!(t.accepts(&g, &no_subnets, src, gua));

        // A different gateway is never WAN-accepted on IPv4.
        assert!(!t.accepts(&gw("other"), &no_subnets, src, v4));
    }

    // LAN accept via `private` is matched by the exact local `dst` (so it is
    // already family-scoped) and unchanged by the fix: a private/on-link source to
    // a `private` local address is accepted, a WAN source is not.
    #[test]
    fn lan_accept_via_private_unchanged() {
        let g = gw("eth0");
        let no_subnets = OrdSet::new();
        let lan_dst = "192.168.1.2";
        let t = target(&[], &[], &[lan_dst]);

        // RFC1918 source to the private local address -> accept (no subnets needed).
        assert!(t.accepts(&g, &no_subnets, ip("192.168.1.50"), ip(lan_dst)));
        // A WAN (public, off-link) source to the same local address -> reject.
        assert!(!t.accepts(&g, &no_subnets, ip("203.0.113.9"), ip(lan_dst)));
        // A local address not in `private` -> reject.
        assert!(!t.accepts(&g, &no_subnets, ip("192.168.1.50"), ip("192.168.1.9")));

        // On-link (but not RFC1918) source is accepted when it falls in a subnet.
        let subnets: OrdSet<IpNet> =
            std::iter::once("203.0.113.0/24".parse::<IpNet>().unwrap()).collect();
        let onlink = target(&[], &[], &["203.0.113.2"]);
        assert!(onlink.accepts(&g, &subnets, ip("203.0.113.50"), ip("203.0.113.2")));
        assert!(!onlink.accepts(&g, &no_subnets, ip("203.0.113.50"), ip("203.0.113.2")));
    }
}

#[cfg(test)]
mod conn_cap_tests {
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    #[tokio::test]
    async fn lru_eviction_cancels_only_the_oldest() {
        let registry = ConnRegistry::new(2);

        let cancel1 = CancellationToken::new();
        let _h1 = registry.register(Arc::new(AtomicU64::new(100)), cancel1.clone());
        let cancel2 = CancellationToken::new();
        let _h2 = registry.register(Arc::new(AtomicU64::new(200)), cancel2.clone());
        let cancel3 = CancellationToken::new();
        let _h3 = registry.register(Arc::new(AtomicU64::new(300)), cancel3.clone());

        assert!(cancel1.is_cancelled());
        assert!(!cancel2.is_cancelled());
        assert!(!cancel3.is_cancelled());
    }

    #[tokio::test]
    async fn activity_stream_keeps_busy_conn_out_of_lru_seat() {
        let registry = ConnRegistry::new(2);

        let stale_cancel = CancellationToken::new();
        let _stale = registry.register(
            Arc::new(AtomicU64::new(monotonic_millis())),
            stale_cancel.clone(),
        );

        let busy_active = Arc::new(AtomicU64::new(monotonic_millis()));
        let busy_cancel = CancellationToken::new();
        let _busy = registry.register(busy_active.clone(), busy_cancel.clone());

        let (a, mut b) = tokio::io::duplex(64);
        let mut wrapped = ActivityStream::new(a, busy_active.clone());
        tokio::time::sleep(Duration::from_millis(20)).await;
        b.write_all(b"hello").await.unwrap();
        let mut buf = [0u8; 5];
        wrapped.read_exact(&mut buf).await.unwrap();

        let new_cancel = CancellationToken::new();
        let _new = registry.register(
            Arc::new(AtomicU64::new(monotonic_millis())),
            new_cancel.clone(),
        );

        assert!(stale_cancel.is_cancelled());
        assert!(!busy_cancel.is_cancelled());
    }

    #[tokio::test]
    async fn drop_handle_deregisters() {
        let registry = ConnRegistry::new(8);
        {
            let _h = registry.register(
                Arc::new(AtomicU64::new(monotonic_millis())),
                CancellationToken::new(),
            );
            assert_eq!(registry.len(), 1);
        }
        assert_eq!(registry.len(), 0);
    }

    #[tokio::test]
    async fn target_cancel_wakes_idle_proxy_task() {
        let ctx = ProxyContext::new(MAX_PROXY_CONNS_PER_TARGET);
        let (mut a, _a_peer) = tokio::io::duplex(64);
        let (mut b, _b_peer) = tokio::io::duplex(64);
        let target_cancel = ctx.cancel.clone();
        let conn_cancel = ctx.cancel.child_token();

        let mut proxy = tokio::spawn(async move {
            tokio::select! {
                _ = target_cancel.cancelled() => "target",
                _ = conn_cancel.cancelled() => "conn",
                _ = tokio::io::copy_bidirectional(&mut a, &mut b) => "io",
            }
        });

        let early = tokio::time::timeout(Duration::from_millis(50), &mut proxy).await;
        assert!(early.is_err());

        ctx.cancel.cancel();

        // conn_cancel is a child of target_cancel, so either branch winning
        // is correct — what we're asserting is that "io" doesn't win.
        let outcome = tokio::time::timeout(Duration::from_millis(200), &mut proxy)
            .await
            .unwrap()
            .unwrap();
        assert!(outcome == "target" || outcome == "conn", "got {outcome}");
    }
}
