use std::any::Any;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::future::Future;
use std::net::{IpAddr, Ipv6Addr, SocketAddr, SocketAddrV6};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Weak};
use std::task::{Poll, ready};
use std::time::{Duration, Instant};

use async_acme::acme::ACME_TLS_ALPN_NAME;
use clap::Parser;
use color_eyre::eyre::eyre;
use futures::FutureExt;
use futures::future::BoxFuture;
use imbl::{OrdMap, OrdSet};
use imbl_value::{InOMap, InternedString};
use ipnet::{IpNet, Ipv4Net, Ipv6Net};
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
    ReportOrderFailure,
};
use crate::net::forward::START9_BRIDGE_V6_SUBNET;
use crate::net::gateway::{
    GatewayInfo, NetworkInterfaceController, NetworkInterfaceListenerAcceptMetadata,
};
use crate::net::port_map::{PortMapController, candidate_gateways};
use crate::net::ssl::{CertBranding, CertStore, RootCaTlsHandler};
use crate::net::tls::{TlsHandler, TlsHandlerAction, TlsListener, TlsMetadata};
use crate::net::utils::{bind_mio_listener, ipv6_is_link_local, is_private_ip};
use crate::net::web_server::{Accept, AcceptStream, ExtractVisitor, TcpMetadata, extract};
use crate::notifications::NotificationLevel;
use crate::prelude::*;
use crate::util::collections::EqSet;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::ReadWriter;
use crate::util::serde::{HandlerExtSerde, MaybeUtf8String, display_serializable};
use crate::util::sync::{SyncMutex, Watch};
use crate::{GatewayId, HOST_IP, HostId, PackageId, ResultExt};

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
    /// A hostname-less key can be wanted by several owners at once.
    port_mappings: SyncMutex<BTreeMap<HostMapOwner, BTreeSet<PortMapKey>>>,
    /// Port-443 bind requirements from ACME domains served on another port.
    challenge_binds: SyncMutex<BTreeMap<HostMapOwner, VHostBindRequirements>>,
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
            challenge_binds: SyncMutex::new(BTreeMap::new()),
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

    /// Once per name until a certificate lands: the address refuses
    /// connections meanwhile, and nothing else would say why.
    fn report_acme_failure(&self) -> ReportOrderFailure {
        let db = self.db.clone();
        Arc::new(move |sans, error| {
            let db = db.clone();
            async move {
                let domain = sans.iter().map(|s| &**s).collect::<Vec<_>>().join(", ");
                db.mutate(|db| {
                    crate::notifications::notify(
                        db,
                        None,
                        NotificationLevel::Warning,
                        t!("acme.order-failed-title", domain = domain).to_string(),
                        t!("acme.order-failed-message", domain = domain, error = error).to_string(),
                        (),
                    )
                })
                .await
                .result
                .log_err();
            }
            .boxed()
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
            self.report_acme_failure(),
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
            // A manual passthrough is not a container the box gateways.
            addr_v6: None,
            add_x_forwarded_headers: false,
            auth: None,
            connect_ssl: None,
            alpn: None,
            passthrough: true,
            // Manual SNI demux to a LAN host: the box isn't its gateway, so
            // source-preserving egress would strand the backend's replies.
            preserve_source_ip: false,
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

    /// Call on every update; empty `targets` withdraws this owner's contribution.
    /// Best-effort: a gateway that can't honor a mapping leaves a manual forward.
    pub fn reconcile_port_maps(
        &self,
        owner: HostMapOwner,
        targets: &BTreeMap<VHostKey, ProxyTarget>,
    ) {
        let ip_info = self.interfaces.watcher.ip_info();
        self.sync_port_maps(owner.clone(), desired_port_maps(targets, &ip_info));
        self.sync_challenge_binds(owner, challenge_bind_reqs(targets));
    }

    /// The union must be taken under `servers`.
    fn sync_challenge_binds(&self, owner: HostMapOwner, reqs: VHostBindRequirements) {
        self.servers.mutate(|writable| {
            let union = self.challenge_binds.mutate(|owners| {
                if reqs.is_empty() {
                    owners.remove(&owner);
                } else {
                    owners.insert(owner, reqs);
                }
                let mut union = VHostBindRequirements::default();
                for reqs in owners.values() {
                    union.extend(reqs);
                }
                union
            });
            let existing = writable.remove(&ACME_CHALLENGE_PORT);
            if existing.is_none() && union.is_empty() {
                return;
            }
            let server = existing.unwrap_or_else(|| self.create_server(ACME_CHALLENGE_PORT));
            server.set_challenge_bind_reqs(union);
            if !server.is_empty() {
                writable.insert(ACME_CHALLENGE_PORT, server);
            }
        });
    }

    fn sync_port_maps(&self, owner: HostMapOwner, desired: DesiredPortMaps) {
        let want: BTreeSet<PortMapKey> = desired
            .iter()
            .flat_map(|((ip, port), (_, hostnames))| {
                hostnames.keys().map(move |h| (*ip, *port, h.clone()))
            })
            .collect();
        // Withdrawal must stay under the lock that decided it.
        self.port_mappings.mutate(|owners| {
            for (ip, port, hostname) in take_ownership(owners, owner, want) {
                match hostname {
                    Some(h) => self.port_map.remove_hostname(ip, port, h.to_string()),
                    None => self.port_map.remove(ip, port),
                }
            }
        });
        for ((ip, port), (gateways, hostnames)) in &desired {
            for (hostname, internal) in hostnames {
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
    }
}

/// `(hostname, external port, is the public leg)`. A name reachable both ways
/// has one entry per side; only the public one carries an authority.
pub type VHostKey = (Option<InternedString>, u16, bool);

/// Where TLS-ALPN-01 is validated, whatever port the name is served on.
pub const ACME_CHALLENGE_PORT: u16 = 443;

/// `(box IP, external port, hostname)`. `Some(hostname)` is a PCP HOSTNAME
/// mapping; `None` is a bare-IP forward or a GUA pinhole.
type PortMapKey = (IpAddr, u16, Option<InternedString>);

/// Records this owner's mappings and returns the ones to withdraw. A key
/// another owner still wants is not among them.
fn take_ownership(
    owners: &mut BTreeMap<HostMapOwner, BTreeSet<PortMapKey>>,
    owner: HostMapOwner,
    want: BTreeSet<PortMapKey>,
) -> Vec<PortMapKey> {
    let had = owners.get(&owner).cloned().unwrap_or_default();
    if want.is_empty() {
        owners.remove(&owner);
    } else {
        owners.insert(owner, want.clone());
    }
    had.difference(&want)
        .filter(|key| !owners.values().any(|kept| kept.contains(key)))
        .cloned()
        .collect()
}

/// `(box IP, external port) -> (gateway candidates in preference order,
/// hostname -> box-side port)`. `Some(hostname)` is a PCP HOSTNAME mapping the
/// gateway SNI-demuxes; `None` is a bare-IP forward or a GUA pinhole.
type DesiredPortMaps = BTreeMap<
    (IpAddr, u16),
    (
        Vec<(IpAddr, Option<u32>)>,
        BTreeMap<Option<InternedString>, u16>,
    ),
>;

fn desired_port_maps(
    targets: &BTreeMap<VHostKey, ProxyTarget>,
    ip_info: &OrdMap<GatewayId, NetworkInterfaceInfo>,
) -> DesiredPortMaps {
    let mut desired = DesiredPortMaps::new();
    // GUAs serving 443, as opposed to carrying a challenge pinhole.
    let mut https_guas = BTreeSet::new();
    for ((maybe_host, external, _), target) in targets {
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
                    .or_insert_with(|| (gateways.clone(), BTreeMap::new()))
                    .1
                    .insert(maybe_host.clone(), *external);
                // By name: the unnamed forward on 443 is the OS's own.
                if let Some(hostname) = maybe_host
                    && target.acme.is_some()
                    && *external != ACME_CHALLENGE_PORT
                {
                    desired
                        .entry((IpAddr::V4(local_ip), ACME_CHALLENGE_PORT))
                        .or_insert_with(|| (gateways.clone(), BTreeMap::new()))
                        .1
                        .entry(Some(hostname.clone()))
                        .or_insert(ACME_CHALLENGE_PORT);
                }
            }
        }
        // IPv6 has no NAT and no SNI demux: pinholes, keyed hostname-less.
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
                .or_insert_with(|| (v6_gateways.clone(), BTreeMap::new()))
                .1
                .insert(None, *external);
            if *external == ACME_CHALLENGE_PORT {
                https_guas.insert(*gua);
            }
            if maybe_host.is_some() && target.acme.is_some() && *external != ACME_CHALLENGE_PORT {
                desired
                    .entry((IpAddr::V6(*gua), ACME_CHALLENGE_PORT))
                    .or_insert_with(|| (v6_gateways, BTreeMap::new()))
                    .1
                    .entry(None)
                    .or_insert(ACME_CHALLENGE_PORT);
            }
        }
    }
    // v6 HTTP->HTTPS redirect: for a GUA serving 443, ask the gateway for an
    // 80->443 redirect pinhole, unless 80 is already a real pinhole on that GUA.
    // (No IPv4 equivalent — the upstream gateway serves the port-80 redirect.)
    let redirects: Vec<(Ipv6Addr, Vec<(IpAddr, Option<u32>)>)> = desired
        .iter()
        .filter_map(|((ip, port), (gateways, _))| match ip {
            IpAddr::V6(gua) if *port == 443 && https_guas.contains(gua) => {
                Some((*gua, gateways.clone()))
            }
            _ => None,
        })
        .filter(|(gua, _)| !desired.contains_key(&(IpAddr::V6(*gua), 80)))
        .collect();
    for (gua, gateways) in redirects {
        desired
            .entry((IpAddr::V6(gua), 80))
            .or_insert((gateways, BTreeMap::from([(None, 443)])));
    }
    desired
}

/// Union of all ProxyTargets' bind requirements for a VHostServer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VHostBindRequirements {
    pub public_gateways: BTreeSet<GatewayId>,
    pub private_ips: BTreeSet<IpAddr>,
}
impl VHostBindRequirements {
    fn extend(&mut self, other: &Self) {
        self.public_gateways
            .extend(other.public_gateways.iter().cloned());
        self.private_ips.extend(other.private_ips.iter().copied());
    }

    fn is_empty(&self) -> bool {
        self.public_gateways.is_empty() && self.private_ips.is_empty()
    }
}

/// Port-443 binds for the challenges of domains served on another port.
fn challenge_bind_reqs(targets: &BTreeMap<VHostKey, ProxyTarget>) -> VHostBindRequirements {
    let mut reqs = VHostBindRequirements::default();
    for ((maybe_host, external, _), target) in targets {
        if maybe_host.is_none() || target.acme.is_none() || *external == ACME_CHALLENGE_PORT {
            continue;
        }
        reqs.public_gateways
            .extend(target.public_v4.iter().cloned());
        reqs.private_ips
            .extend(target.public_v6.iter().map(|v6| IpAddr::V6(*v6)));
    }
    reqs
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
    /// Whether this target answers `metadata` as one of the box's own private
    /// addresses. Preferred over a plain [`filter`](Self::filter) match, which
    /// on IPv4 cannot tell a local dial from a forwarded one.
    #[allow(unused_variables)]
    fn filter_private(&self, metadata: &<A as Accept>::Metadata) -> bool {
        false
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
    fn filter_private(&self, metadata: &<A as Accept>::Metadata) -> bool;
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
    fn filter_private(&self, metadata: &<A as Accept>::Metadata) -> bool {
        VHostTarget::filter_private(self, metadata)
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
    /// The container's IPv6 address on the bridge, when it has one. Only the
    /// source-preserving leg uses it — the plain connect stays on `addr` — since
    /// a v6 client's source can only be bound on a socket dialing v6.
    pub addr_v6: Option<SocketAddrV6>,
    pub add_x_forwarded_headers: bool,
    /// Optional `Authorization` header value to inject on upstream
    /// requests. Implies HTTP-aware proxying (same path as forwarded headers).
    pub auth: Option<crate::net::host::binding::ProxyAuth>,
    /// The config StartOS dials the container with when the container serves
    /// its own TLS. `None` dials it in plaintext.
    pub connect_ssl: Option<Arc<ClientConfig>>,
    /// The protocols this binding answers a client with, from those it asked
    /// for. `None` answers with whatever it asked for.
    pub alpn: Option<AlpnInfo>,
    pub passthrough: bool,
    /// Open the internal leg with the client's source IP (`IP_TRANSPARENT`).
    /// Only for targets the box gateways — service containers — whose replies
    /// transit the box; a manual LAN passthrough's replies don't, so it connects
    /// plainly and the backend sees the box IP.
    pub preserve_source_ip: bool,
}
impl PartialEq for ProxyTarget {
    fn eq(&self, other: &Self) -> bool {
        self.public_v4 == other.public_v4
            && self.public_v6 == other.public_v6
            && self.private == other.private
            && self.acme == other.acme
            && self.addr == other.addr
            && self.addr_v6 == other.addr_v6
            && self.add_x_forwarded_headers == other.add_x_forwarded_headers
            && self.auth == other.auth
            && self.passthrough == other.passthrough
            && self.preserve_source_ip == other.preserve_source_ip
            && self.alpn == other.alpn
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
            .field("addr_v6", &self.addr_v6)
            .field("add_x_forwarded_headers", &self.add_x_forwarded_headers)
            .field("auth", &self.auth.as_ref().map(|_| "<redacted>"))
            .field("connect_ssl", &self.connect_ssl.as_ref().map(|_| ()))
            .field("alpn", &self.alpn)
            .field("passthrough", &self.passthrough)
            .field("preserve_source_ip", &self.preserve_source_ip)
            .finish()
    }
}

/// Whether `ip` sits on lxcbr0, where the containers are.
fn on_container_bridge(ip: IpAddr) -> bool {
    static V6: LazyLock<Ipv6Net> =
        LazyLock::new(|| START9_BRIDGE_V6_SUBNET.parse().expect("const subnet"));
    static V4: LazyLock<Ipv4Net> = LazyLock::new(|| {
        Ipv4Net::new(HOST_IP.into(), 24)
            .expect("const prefix")
            .trunc()
    });
    match ip {
        IpAddr::V4(v4) => V4.contains(&v4),
        IpAddr::V6(v6) => V6.contains(&v6),
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
        wan || self.accepts_as_private(subnets, src, dst)
    }

    /// A dial of one of our own private addresses from that address's network.
    /// On IPv4 the source is all that separates it from a forwarded one.
    fn accepts_as_private(&self, subnets: &OrdSet<IpNet>, src: IpAddr, dst: IpAddr) -> bool {
        self.private.contains(&dst)
            && (subnets.iter().any(|s| s.contains(&src)) || is_private_ip(src))
    }

    /// Whether the box gateways `client`, which source preservation requires:
    /// the backend's reply is addressed to the client, and only reaches our
    /// transparent socket if it transits this host to be diverted. It doesn't
    /// for a client on the box (also its own peer — its `(ip, port)` is already
    /// bound to its own socket) nor for one on the container bridge, which the
    /// backend answers directly over that link.
    fn gateways_client(&self, client: IpAddr) -> bool {
        !client.is_loopback() && !self.private.contains(&client) && !on_container_bridge(client)
    }

    /// The `(client, container)` pair to open the internal leg with when the
    /// client's source address is to be preserved; `None` to connect plainly.
    /// Both ends must be one family, so a v6 client needs the container's v6.
    fn transparent_leg(&self, peer: Option<SocketAddr>) -> Option<(SocketAddr, SocketAddr)> {
        if !self.preserve_source_ip {
            return None;
        }
        let client = peer?;
        if !self.gateways_client(client.ip()) {
            return None;
        }
        let target = match client {
            SocketAddr::V4(_) => matches!(self.addr, SocketAddr::V4(_)).then_some(self.addr)?,
            SocketAddr::V6(_) => SocketAddr::V6(self.addr_v6?),
        };
        Some((client, target))
    }
}

struct Arrival {
    gateway: GatewayId,
    subnets: OrdSet<IpNet>,
    src: IpAddr,
    dst: IpAddr,
}

fn arrival<A>(metadata: &<A as Accept>::Metadata) -> Option<Arrival>
where
    A: Accept + 'static,
    <A as Accept>::Metadata: Visit<ExtractVisitor<GatewayInfo>>
        + Visit<ExtractVisitor<TcpMetadata>>
        + Clone
        + Send
        + Sync
        + 'static,
{
    let gw = extract::<GatewayInfo, _>(metadata)?;
    let tcp = extract::<TcpMetadata, _>(metadata)?;
    let ip_info = gw.info.ip_info.as_ref()?;
    Some(Arrival {
        gateway: gw.id.clone(),
        subnets: ip_info.subnets.clone(),
        src: tcp.peer_addr.ip(),
        dst: tcp.local_addr.ip(),
    })
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
        let Some(at) = arrival::<A>(metadata) else {
            return false;
        };
        self.accepts(&at.gateway, &at.subnets, at.src, at.dst)
    }
    fn filter_private(&self, metadata: &<A as Accept>::Metadata) -> bool {
        let Some(at) = arrival::<A>(metadata) else {
            return false;
        };
        self.accepts_as_private(&at.subnets, at.src, at.dst)
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
        let plain_connect = || async {
            TcpStream::connect(self.addr)
                .await
                .with_ctx(|_| (ErrorKind::Network, self.addr))
                .log_err()
        };
        // Source-preserving passthrough (container): open the internal leg from
        // the client's own address so the backend sees the real peer (RFC §4.6).
        // The box gateways the container, so replies transit it and the divert
        // routes them back. Manual LAN passthroughs and terminating targets
        // connect plainly — the box isn't their gateway.
        let tcp_stream = match self.transparent_leg(peer) {
            Some((client, target)) => {
                crate::net::transparent::ensure_divert_infra_once()
                    .await
                    .log_err();
                match crate::net::transparent::transparent_connect(client, target).await {
                    Ok(stream) => stream,
                    // Degraded, not fatal: the backend sees this host rather than
                    // the client. Better than dropping a working connection.
                    Err(e) => {
                        // A service bound to `0.0.0.0` refuses the v6 leg, which is
                        // the common case and not worth warning about per
                        // connection; anything else is a real misconfiguration.
                        if e.kind() == std::io::ErrorKind::ConnectionRefused {
                            tracing::debug!(
                                "{target} has no listener for {client}'s family; connecting plainly"
                            );
                        } else {
                            tracing::warn!(
                                "transparent egress to {target} for {client} failed ({e}); connecting plainly, so the backend will see this host as the peer"
                            );
                        }
                        plain_connect().await?
                    }
                }
            }
            None => plain_connect().await?,
        };
        if let Err(e) = socket2::SockRef::from(&tcp_stream)
            .set_tcp_keepalive(&crate::net::utils::default_keepalive())
        {
            tracing::error!("Failed to set tcp keepalive: {e}");
            tracing::debug!("{e:?}");
        }
        let client_alpn: Vec<Vec<u8>> = hello
            .alpn()
            .into_iter()
            .flatten()
            .map(|proto| proto.to_vec())
            .collect();
        let offered: Vec<Vec<u8>> = match &self.alpn {
            Some(AlpnInfo(protos)) => protos.iter().map(|proto| proto.0.clone()).collect(),
            None => client_alpn.clone(),
        };
        // The container is offered only protocols the client also named, since
        // its choice is what the client is handed back.
        let dialled: Vec<Vec<u8>> = offered
            .iter()
            .filter(|proto| client_alpn.contains(proto))
            .cloned()
            .collect();
        // rustls turns a client away only when both its list and ours name
        // something.
        if self.connect_ssl.is_some()
            && !offered.is_empty()
            && !client_alpn.is_empty()
            && dialled.is_empty()
        {
            prev.alpn_protocols = offered;
            return Some((prev, Box::pin(tcp_stream)));
        }
        let (stream, negotiated): (AcceptStream, _) = match &self.connect_ssl {
            Some(client_cfg) => {
                // Called even for an empty list: without it the connector falls
                // back to `client_cfg`'s own protocols.
                let target_stream = TlsConnector::from(client_cfg.clone())
                    .with_alpn(dialled)
                    .connect(ServerName::IpAddress(self.addr.ip().into()), tcp_stream)
                    .await
                    .with_ctx(|_| (ErrorKind::Network, self.addr))
                    .log_err()?;
                let negotiated = target_stream
                    .get_ref()
                    .1
                    .alpn_protocol()
                    .map(|proto| proto.to_vec());
                (Box::pin(target_stream), negotiated)
            }
            None => (Box::pin(tcp_stream), None),
        };
        // rustls picks by this list's order.
        prev.alpn_protocols = match &self.connect_ssl {
            // One protocol frames both legs, so the client is offered the
            // container's choice.
            Some(_) => negotiated.into_iter().collect(),
            None => offered,
        };
        Some((prev, stream))
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

/// The protocols a binding answers with, carried on the wire as the list itself.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, TS)]
#[serde(transparent)]
#[ts(export)]
pub struct AlpnInfo(pub Vec<MaybeUtf8String>);

#[cfg(test)]
mod alpn_wire_format {
    use super::*;

    /// A manifest writes the list itself, and no list at all is `null`.
    #[test]
    fn alpn_is_carried_as_the_list_itself() {
        let pinned = Some(AlpnInfo(vec![
            MaybeUtf8String(b"h2".to_vec()),
            MaybeUtf8String(b"http/1.1".to_vec()),
        ]));
        let json = serde_json::json!(["h2", "http/1.1"]);
        assert_eq!(serde_json::to_value(&pinned).unwrap(), json);
        assert_eq!(
            serde_json::from_value::<Option<AlpnInfo>>(json).unwrap(),
            pinned,
        );
        assert_eq!(
            serde_json::from_value::<Option<AlpnInfo>>(serde_json::Value::Null).unwrap(),
            None,
        );
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

/// The [`Mapping`] key for a connection's SNI.
///
/// `None` is a connection that named no host — no SNI, or an SNI carrying an IP
/// literal, which RFC 6066 forbids but clients send anyway — and is served by
/// the bare-IP entry. A name is served only if it has an entry of its own: the
/// lookup has no fallback, so a host answers to the names it was given and
/// nothing else.
fn host_key(server_name: Option<&str>) -> Option<InternedString> {
    server_name
        .filter(|name| name.parse::<IpAddr>().is_err())
        .map(InternedString::from)
}

/// A challenge names the host it validates. One that names nothing is an
/// ordinary connection, whatever it advertises.
fn is_acme_challenge<'a>(
    server_name: Option<&str>,
    mut alpn: impl Iterator<Item = &'a [u8]>,
) -> bool {
    server_name.is_some() && alpn.any(|a| a == ACME_TLS_ALPN_NAME)
}

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
                    let acme = m
                        .get(&Some(x.clone()))?
                        .iter()
                        .filter(|(_, e)| e.alive())
                        .find_map(|(t, _)| t.0.acme())?;
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
pub struct VHostTlsHandler<Acme, RootCa, A: Accept + 'static> {
    acme: Acme,
    root_ca: RootCa,
    crypto_provider: Arc<CryptoProvider>,
    mapping: Watch<Mapping<A>>,
    preprocessed: Option<Preprocessed<A>>,
}
impl<Acme: Clone, RootCa: Clone, A: Accept + 'static> Clone for VHostTlsHandler<Acme, RootCa, A> {
    fn clone(&self) -> Self {
        Self {
            acme: self.acme.clone(),
            root_ca: self.root_ca.clone(),
            crypto_provider: self.crypto_provider.clone(),
            mapping: self.mapping.clone(),
            // Per-connection state — never carried across clones; each
            // accepted connection clones the handler before populating it.
            preprocessed: None,
        }
    }
}

impl<'a, A, Acme, RootCa> TlsHandler<'a, A> for VHostTlsHandler<Acme, RootCa, A>
where
    A: Accept + 'a,
    <A as Accept>::Metadata:
        Visit<ExtractVisitor<GatewayInfo>> + Visit<ExtractVisitor<TcpMetadata>> + Send + Sync,
    Acme: TlsHandler<'a, A> + Send,
    RootCa: TlsHandler<'a, A> + Send,
{
    async fn get_config(
        &'a mut self,
        hello: &'a ClientHello<'a>,
        metadata: &'a <A as Accept>::Metadata,
    ) -> Option<TlsHandlerAction> {
        let sni = host_key(hello.server_name());

        let routed = self.mapping.peek(|m| {
            let alive = || m.get(&sni).into_iter().flatten().filter(|(_, e)| e.alive());
            alive()
                .find(|(t, _)| t.0.filter_private(metadata))
                .or_else(|| alive().find(|(t, _)| t.0.filter(metadata)))
                .map(|(t, e)| (t.clone(), e.ctx.clone()))
        });

        let acme_challenge =
            is_acme_challenge(hello.server_name(), hello.alpn().into_iter().flatten());

        let Some((target, ctx)) = routed else {
            // Validation is at :443 whatever port the name is served on, so a
            // challenge lands here. Answered only from our orders in flight.
            if acme_challenge {
                return self.acme.get_config(hello, metadata).await;
            }
            return None;
        };

        // Passthroughs should not intermediate ACME challenges — the
        // backend is the ACME client and holds the challenge cert.
        if target.0.is_passthrough() {
            let stub = passthrough_stub_config(&self.crypto_provider).log_err()?;
            let (_, store) = target.into_preprocessed(ctx, stub, hello, metadata).await?;
            self.preprocessed = Some(store);
            return Some(TlsHandlerAction::Passthrough);
        }

        // ACME challenge for a terminating target: answer it ourselves.
        if acme_challenge {
            return self.acme.get_config(hello, metadata).await;
        }

        let action = if target.0.acme().is_some() {
            // The authority's certificate or nothing: the root CA would answer
            // for the name with a chain shared by every address on this server.
            self.acme.get_config(hello, metadata).await?
        } else {
            // The authority's certificate where one exists, so a name served
            // both ways presents the same one either way; the root CA else.
            match self.acme.get_config(hello, metadata).await {
                Some(action) => action,
                None => self.root_ca.get_config(hello, metadata).await?,
            }
        };
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
        VHostTlsHandler<Arc<AcmeTlsHandler<M, GetVHostAcmeProvider<A>>>, RootCaTlsHandler<M>, A>,
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
    /// Bind requirements from outside this server's own mapping.
    challenge_bind_reqs: SyncMutex<VHostBindRequirements>,
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
        report_acme_failure: ReportOrderFailure,
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
            challenge_bind_reqs: SyncMutex::new(VHostBindRequirements::default()),
            _thread: tokio::spawn(async move {
                let mut listener = VHostListener(TlsListener::new(
                    listener,
                    VHostTlsHandler {
                        acme: Arc::new(AcmeTlsHandler {
                            db: db.clone(),
                            acme_cache,
                            crypto_provider: crypto_provider.clone(),
                            get_provider: GetVHostAcmeProvider(mapping.clone()),
                            in_progress: Watch::new(BTreeMap::new()),
                            report_failure: Some(report_acme_failure),
                        }),
                        root_ca: RootCaTlsHandler {
                            db,
                            crypto_provider: crypto_provider.clone(),
                            branding,
                        },
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
        let mut new_reqs = compute_bind_reqs(mapping);
        self.challenge_bind_reqs
            .peek(|extra| new_reqs.extend(extra));
        self.bind_reqs.send_if_modified(|reqs| {
            if *reqs != new_reqs {
                *reqs = new_reqs;
                true
            } else {
                false
            }
        });
    }
    fn set_challenge_bind_reqs(&self, reqs: VHostBindRequirements) {
        self.challenge_bind_reqs.replace(reqs);
        self.mapping.peek(|mapping| self.update_bind_reqs(mapping));
    }
    /// A server held open by challenge requirements alone is in use.
    fn is_empty(&self) -> bool {
        self.mapping.peek(|m| m.is_empty()) && self.challenge_bind_reqs.peek(|r| r.is_empty())
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
mod host_key_tests {
    use super::*;

    /// A dial that names no host is the bare-IP case the `None` entry serves.
    #[test]
    fn an_ip_literal_sni_is_the_same_as_no_sni() {
        assert_eq!(host_key(None), None);
        assert_eq!(host_key(Some("192.168.1.5")), None);
        assert_eq!(host_key(Some("::1")), None);
        assert_eq!(host_key(Some("fd00:3::1")), None);
    }

    /// Otherwise it reaches a handler with no name to check, which declines,
    /// and the root CA answers.
    #[test]
    fn a_challenge_without_a_name_is_not_a_challenge() {
        let acme = || [ACME_TLS_ALPN_NAME].into_iter();
        assert!(is_acme_challenge(Some("example.com"), acme()));
        assert!(!is_acme_challenge(None, acme()));
        assert!(!is_acme_challenge(
            Some("example.com"),
            [b"h2".as_slice()].into_iter()
        ));
        assert!(!is_acme_challenge(
            Some("example.com"),
            std::iter::empty::<&[u8]>()
        ));
    }

    #[test]
    fn a_name_keys_to_itself() {
        assert_eq!(
            host_key(Some("server-name.local")),
            Some(InternedString::intern("server-name.local"))
        );
        assert_eq!(
            host_key(Some("example.com")),
            Some(InternedString::intern("example.com"))
        );
        // Not an IP, so it keys like any other name — and nothing registers it,
        // so the lookup misses and the connection is refused.
        assert_eq!(
            host_key(Some("server-name")),
            Some(InternedString::intern("server-name"))
        );
    }
}

#[cfg(test)]
mod port_map_tests {
    use std::str::FromStr;

    use super::*;
    use crate::db::model::public::{GatewayType, IpInfo, NetworkInterfaceType};

    const GATEWAY: &str = "wg0";
    const BOX_IP: &str = "10.13.13.5";
    const GUA: &str = "2001:db8::5";

    /// On-link like StartTunnel: no next hop, so the relay comes from the subnet.
    fn ip_info() -> OrdMap<GatewayId, NetworkInterfaceInfo> {
        let mut info = OrdMap::new();
        info.insert(
            GatewayId::from(InternedString::intern(GATEWAY)),
            NetworkInterfaceInfo {
                name: None,
                secure: None,
                ip_info: Some(Arc::new(IpInfo {
                    name: InternedString::intern(GATEWAY),
                    scope_id: 0,
                    device_type: Some(NetworkInterfaceType::Wireguard),
                    subnets: ["10.13.13.5/24", "2001:db8::5/64"]
                        .into_iter()
                        .map(|s| s.parse::<IpNet>().unwrap())
                        .collect(),
                    lan_ip: Default::default(),
                    wan_ip: None,
                    ntp_servers: Default::default(),
                    dns_servers: Default::default(),
                })),
                gateway_type: GatewayType::InboundOutbound,
                port_map: Default::default(),
                dns_update: Default::default(),
            },
        );
        info
    }

    fn target(acme: bool, v6: bool) -> ProxyTarget {
        ProxyTarget {
            public_v4: [GatewayId::from(InternedString::intern(GATEWAY))]
                .into_iter()
                .collect(),
            public_v6: if v6 {
                [GUA.parse().unwrap()].into_iter().collect()
            } else {
                BTreeSet::new()
            },
            private: BTreeSet::new(),
            acme: acme.then(|| AcmeProvider::from_str("letsencrypt").unwrap()),
            addr: "10.0.3.2:443".parse().unwrap(),
            addr_v6: None,
            add_x_forwarded_headers: false,
            auth: None,
            connect_ssl: None,
            alpn: None,
            passthrough: false,
            preserve_source_ip: false,
        }
    }

    /// All public legs; a private one has no public gateway to map.
    fn targets<'a>(
        entries: impl IntoIterator<Item = (Option<&'a str>, u16, ProxyTarget)>,
    ) -> BTreeMap<VHostKey, ProxyTarget> {
        entries
            .into_iter()
            .map(|(host, port, target)| ((host.map(InternedString::intern), port, true), target))
            .collect()
    }

    fn routes(desired: &DesiredPortMaps, ip: &str, external: u16) -> Vec<(String, u16)> {
        let key: (IpAddr, u16) = (ip.parse().unwrap(), external);
        desired
            .get(&key)
            .map(|(_, hosts)| {
                hosts
                    .iter()
                    .map(|(h, port)| (h.as_ref().map_or("*".into(), |h| h.to_string()), *port))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn an_acme_domain_off_443_also_routes_the_challenge_port() {
        let desired = desired_port_maps(
            &targets([(Some("electrum.example.com"), 50002, target(true, false))]),
            &ip_info(),
        );

        assert_eq!(
            routes(&desired, BOX_IP, 50002),
            [("electrum.example.com".to_string(), 50002)],
        );
        assert_eq!(
            routes(&desired, BOX_IP, 443),
            [("electrum.example.com".to_string(), 443)],
        );
    }

    #[test]
    fn every_acme_domain_takes_the_same_challenge_route() {
        let desired = desired_port_maps(
            &targets([
                (Some("electrum.example.com"), 50002, target(true, false)),
                (Some("turn.example.com"), 5349, target(true, false)),
            ]),
            &ip_info(),
        );

        assert_eq!(
            routes(&desired, BOX_IP, 443),
            [
                ("electrum.example.com".to_string(), 443),
                ("turn.example.com".to_string(), 443),
            ],
        );
    }

    #[test]
    fn nothing_else_claims_the_challenge_port() {
        let desired = desired_port_maps(
            &targets([
                (Some("plain.example.com"), 50002, target(false, false)),
                (None, 8443, target(true, false)),
            ]),
            &ip_info(),
        );

        assert!(routes(&desired, BOX_IP, 443).is_empty());
    }

    #[test]
    fn a_real_443_binding_owns_the_port_it_serves() {
        let desired = desired_port_maps(
            &targets([
                (Some("example.com"), 443, target(true, false)),
                (Some("example.com"), 50002, target(true, false)),
            ]),
            &ip_info(),
        );

        assert_eq!(
            routes(&desired, BOX_IP, 443),
            [("example.com".to_string(), 443)],
        );
    }

    #[test]
    fn ipv6_pinholes_are_unchanged() {
        let desired = desired_port_maps(
            &targets([(Some("example.com"), 443, target(true, true))]),
            &ip_info(),
        );

        assert_eq!(routes(&desired, GUA, 443), [("*".to_string(), 443)]);
        assert_eq!(routes(&desired, GUA, 80), [("*".to_string(), 443)]);
    }

    #[test]
    fn an_acme_domain_off_443_pinholes_the_challenge_port_over_ipv6() {
        let desired = desired_port_maps(
            &targets([(Some("electrum.example.com"), 50002, target(true, true))]),
            &ip_info(),
        );

        assert_eq!(routes(&desired, GUA, 50002), [("*".to_string(), 50002)]);
        assert_eq!(routes(&desired, GUA, 443), [("*".to_string(), 443)]);
        assert!(routes(&desired, GUA, 80).is_empty());
    }

    #[test]
    fn a_served_443_still_earns_the_ipv6_redirect_beside_a_challenge_pinhole() {
        let desired = desired_port_maps(
            &targets([
                (Some("example.com"), 443, target(true, true)),
                (Some("electrum.example.com"), 50002, target(true, true)),
            ]),
            &ip_info(),
        );

        assert_eq!(routes(&desired, GUA, 80), [("*".to_string(), 443)]);
    }

    fn challenge_reqs(
        entries: impl IntoIterator<Item = (Option<&'static str>, u16, ProxyTarget)>,
    ) -> VHostBindRequirements {
        challenge_bind_reqs(&targets(entries))
    }

    #[test]
    fn an_acme_domain_off_443_binds_the_challenge_port_where_it_is_public() {
        let reqs = challenge_reqs([(Some("electrum.example.com"), 50002, target(true, true))]);

        assert_eq!(
            reqs.public_gateways,
            [GatewayId::from(InternedString::intern(GATEWAY))]
                .into_iter()
                .collect(),
        );
        assert_eq!(
            reqs.private_ips,
            [GUA.parse::<IpAddr>().unwrap()].into_iter().collect(),
        );
    }

    fn owner(name: &str) -> HostMapOwner {
        (Some(name.parse().unwrap()), "main".parse().unwrap())
    }

    fn key(port: u16) -> PortMapKey {
        (GUA.parse().unwrap(), port, None)
    }

    #[test]
    fn a_pinhole_another_owner_still_wants_is_not_withdrawn() {
        let mut owners = BTreeMap::new();
        take_ownership(&mut owners, owner("electrs"), [key(50002), key(443)].into());
        take_ownership(&mut owners, owner("cln"), [key(3010), key(443)].into());

        assert_eq!(
            take_ownership(&mut owners, owner("electrs"), BTreeSet::new()),
            [key(50002)],
        );
        assert_eq!(
            take_ownership(&mut owners, owner("cln"), BTreeSet::new()),
            [key(443), key(3010)],
        );
    }

    #[test]
    fn only_an_acme_domain_off_443_binds_the_challenge_port() {
        assert!(
            challenge_reqs([(Some("plain.example.com"), 50002, target(false, true))]).is_empty()
        );
        assert!(challenge_reqs([(Some("example.com"), 443, target(true, true))]).is_empty());
        assert!(challenge_reqs([(None, 8443, target(true, true))]).is_empty());
    }
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
            addr_v6: None,
            add_x_forwarded_headers: false,
            auth: None,
            connect_ssl: None,
            alpn: None,
            passthrough: false,
            preserve_source_ip: false,
        }
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    /// Source preservation needs the backend's reply to transit this host. It
    /// doesn't for a client that reaches the backend without us.
    #[test]
    fn only_a_gatewayed_client_gets_source_preservation() {
        let t = target(&["enp1s0"], &[], &["192.168.1.5", "fd00:3::1"]);

        // remote, both families: we gateway it
        assert!(t.gateways_client(ip("192.168.1.99")));
        assert!(t.gateways_client(ip("2001:db8::99")));

        // this box: its (ip, port) is already bound to its own socket
        assert!(!t.gateways_client(ip("127.0.0.1")));
        assert!(!t.gateways_client(ip("::1")));
        assert!(!t.gateways_client(ip("192.168.1.5")));
        assert!(!t.gateways_client(ip("fd00:3::1")));

        // on lxcbr0 with the backend, which answers it directly over that link
        assert!(!t.gateways_client(ip("10.0.3.42")));
        assert!(!t.gateways_client(ip("fd00:3::c9f:81ff:fe37:c0a9")));
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

    /// IPv4 hands a forwarded dial and a local one the same gateway, so the
    /// public leg claims both; the private leg's claim is the stronger one.
    #[test]
    fn a_lan_dial_of_a_dual_exposure_domain_belongs_to_the_private_leg() {
        let g = gw("eth0");
        let no_subnets = OrdSet::new();
        let lan_dst = ip("192.168.1.2");
        let public_leg = target(&["eth0"], &[], &[]);
        let private_leg = target(&[], &[], &["192.168.1.2"]);

        let lan_src = ip("192.168.1.50");
        assert!(private_leg.accepts_as_private(&no_subnets, lan_src, lan_dst));
        assert!(!public_leg.accepts_as_private(&no_subnets, lan_src, lan_dst));
        assert!(
            public_leg.accepts(&g, &no_subnets, lan_src, lan_dst),
            "the public leg cannot tell a local dial from a forwarded one, \
             which is why the private leg is preferred"
        );

        let wan_src = ip("203.0.113.9");
        assert!(!private_leg.accepts_as_private(&no_subnets, wan_src, lan_dst));
        assert!(!private_leg.accepts(&g, &no_subnets, wan_src, lan_dst));
        assert!(public_leg.accepts(&g, &no_subnets, wan_src, lan_dst));
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

/// Which protocols `preprocess` offers the container and the client.
#[cfg(test)]
mod upstream_alpn_tests {
    use std::net::Ipv4Addr;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_rustls::TlsAcceptor;

    use super::*;
    use crate::net::tls::client_config_no_verify;
    use crate::net::tls::test::{provider, self_signed_for_loopback, server_config};

    fn config_advertising(alpn: &[&str]) -> ServerConfig {
        let (key, cert) = self_signed_for_loopback();
        let mut cfg = server_config(&key, &cert);
        cfg.alpn_protocols = alpn.iter().map(|a| a.as_bytes().to_vec()).collect();
        cfg
    }

    /// As [`spawn_backend_reporting`], for a caller with nothing to assert
    /// about what the container negotiated.
    async fn spawn_backend(alpn: &[&str]) -> SocketAddr {
        spawn_backend_reporting(alpn).await.0
    }

    /// A TLS backend that reports the first `len` bytes it reads through its
    /// own session.
    async fn spawn_backend_reading(
        len: usize,
    ) -> (SocketAddr, tokio::sync::oneshot::Receiver<Vec<u8>>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(config_advertising(&[])));
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            if let Ok(mut tls) = acceptor.accept(tcp).await {
                let mut read = vec![0; len];
                if tls.read_exact(&mut read).await.is_ok() {
                    let _ = tx.send(read);
                }
            }
        });
        (addr, rx)
    }

    /// A TLS backend advertising `alpn`, standing in for a container serving
    /// its own TLS behind an `https` binding. Reports what it negotiated on its
    /// first connection.
    async fn spawn_backend_reporting(
        alpn: &[&str],
    ) -> (SocketAddr, tokio::sync::oneshot::Receiver<Option<Vec<u8>>>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(config_advertising(alpn)));
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let mut tx = Some(tx);
            while let Ok((tcp, _)) = listener.accept().await {
                let acceptor = acceptor.clone();
                // Moved into the task, so a failed handshake drops it and the
                // receiver closes.
                let report = tx.take();
                tokio::spawn(async move {
                    if let Ok(tls) = acceptor.accept(tcp).await {
                        if let Some(report) = report {
                            let _ =
                                report.send(tls.get_ref().1.alpn_protocol().map(|p| p.to_vec()));
                        }
                        // The backend holds its side open under the assertion.
                        std::future::pending::<()>().await;
                    }
                });
            }
        });
        (addr, rx)
    }

    /// Serves the client-facing config that the real [`ProxyTarget::preprocess`]
    /// produces for an accepted `ClientHello`.
    #[derive(Clone)]
    struct Preprocessing {
        target: ProxyTarget,
        base_alpn: Vec<String>,
        /// Written to the stream `preprocess` hands back.
        probe: &'static [u8],
    }
    impl<'a> TlsHandler<'a, TcpListener> for Preprocessing {
        async fn get_config(
            &'a mut self,
            hello: &'a ClientHello<'a>,
            metadata: &'a TcpMetadata,
        ) -> Option<TlsHandlerAction> {
            let base_alpn: Vec<&str> = self.base_alpn.iter().map(|a| a.as_str()).collect();
            let base = config_advertising(&base_alpn);
            let (cfg, upstream) =
                VHostTarget::<TcpListener>::preprocess(&self.target, base, hello, metadata).await?;
            if self.probe.is_empty() {
                // The client handshakes off `cfg` alone.
                drop(upstream);
            } else {
                let probe = self.probe;
                tokio::spawn(async move {
                    let mut upstream = upstream;
                    let _ = upstream.write_all(probe).await;
                    let _ = upstream.flush().await;
                    // The stream stays open until the backend has the probe.
                    std::future::pending::<()>().await;
                });
            }
            Some(TlsHandlerAction::Tls(cfg))
        }
    }

    /// Dial the backend over TLS, as a container serving its own TLS is dialled.
    fn rewrap() -> Option<Arc<ClientConfig>> {
        rewrap_configured_with(&[])
    }

    /// A rewrap whose own config carries `alpn`, which the dial always
    /// overrides.
    fn rewrap_configured_with(alpn: &[&str]) -> Option<Arc<ClientConfig>> {
        let mut cfg = client_config_no_verify(provider()).unwrap();
        cfg.alpn_protocols = alpn.iter().map(|a| a.as_bytes().to_vec()).collect();
        Some(Arc::new(cfg))
    }

    fn target(
        backend: SocketAddr,
        connect_ssl: Option<Arc<ClientConfig>>,
        alpn: Option<AlpnInfo>,
    ) -> ProxyTarget {
        ProxyTarget {
            public_v4: BTreeSet::new(),
            public_v6: BTreeSet::new(),
            private: BTreeSet::new(),
            acme: None,
            addr: backend,
            addr_v6: None,
            add_x_forwarded_headers: true,
            auth: None,
            connect_ssl,
            alpn,
            passthrough: false,
            preserve_source_ip: false,
        }
    }

    /// The protocol the client and the vhost listener settle on for one
    /// connection offering `client_alpn`, in front of a rewrapped backend
    /// advertising `backend_alpn`.
    async fn negotiate(backend_alpn: &[&str], client_alpn: &[&str]) -> Option<String> {
        try_negotiate(
            rewrap(),
            None,
            &[],
            spawn_backend(backend_alpn).await,
            client_alpn,
        )
        .await
        .expect("the client completes its handshake with the listener")
    }

    /// As [`negotiate`], but against a given `backend`, for any dial strategy
    /// and any `alpn`, and with `base_alpn` already on the config handed to
    /// `preprocess`.
    async fn try_negotiate(
        connect_ssl: Option<Arc<ClientConfig>>,
        alpn: Option<AlpnInfo>,
        base_alpn: &[&str],
        backend: SocketAddr,
        client_alpn: &[&str],
    ) -> Result<Option<String>, std::io::Error> {
        try_negotiate_probing(connect_ssl, alpn, base_alpn, backend, client_alpn, b"").await
    }

    async fn try_negotiate_probing(
        connect_ssl: Option<Arc<ClientConfig>>,
        alpn: Option<AlpnInfo>,
        base_alpn: &[&str],
        backend: SocketAddr,
        client_alpn: &[&str],
        probe: &'static [u8],
    ) -> Result<Option<String>, std::io::Error> {
        let handler = Preprocessing {
            target: target(backend, connect_ssl, alpn),
            base_alpn: base_alpn.iter().map(|a| a.to_string()).collect(),
            probe,
        };
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut vhost = TlsListener::new(listener, handler);

        let accepted = tokio::spawn(async move {
            futures::future::poll_fn(|cx| vhost.poll_accept(cx))
                .await
                .map(|(metadata, _stream)| metadata.tls_info.alpn)
        });

        let mut client = client_config_no_verify(provider()).unwrap();
        client.alpn_protocols = client_alpn.iter().map(|a| a.as_bytes().to_vec()).collect();
        let tcp = TcpStream::connect(addr).await.unwrap();
        // Neither the client's connect nor `get_config` is bounded.
        let handshake = tokio::time::timeout(
            Duration::from_secs(10),
            TlsConnector::from(Arc::new(client))
                .connect(ServerName::IpAddress(Ipv4Addr::LOCALHOST.into()), tcp),
        )
        .await
        .expect("the client handshake settles within 10s");

        if let Err(e) = handshake {
            // The listener answered with an alert, so it has no metadata to
            // report.
            accepted.abort();
            return Err(e);
        }

        let alpn = tokio::time::timeout(Duration::from_secs(10), accepted)
            .await
            .expect("the listener finishes its handshake within 10s")
            .expect("the listener's accept task runs to completion")
            .expect("the listener accepts the connection");
        Ok(alpn.map(|a| String::from_utf8(a.0).unwrap()))
    }

    #[tokio::test]
    async fn the_client_lands_on_the_protocol_the_backend_chose() {
        assert_eq!(
            negotiate(&["h2", "http/1.1"], &["h2", "http/1.1"]).await,
            Some("h2".to_owned()),
            "the backend picks h2 out of the client's own list, so proxying \
             the pair as HTTP/1 writes h1 framing onto an h2 connection",
        );
    }

    #[tokio::test]
    async fn a_backend_that_declines_alpn_leaves_the_client_without_it() {
        assert_eq!(negotiate(&[], &["h2", "http/1.1"]).await, None);
    }

    #[tokio::test]
    async fn a_backend_limited_to_http1_holds_the_client_there() {
        assert_eq!(
            negotiate(&["http/1.1"], &["h2", "http/1.1"]).await,
            Some("http/1.1".to_owned()),
        );
    }

    /// rustls selects by the server list's order.
    #[tokio::test]
    async fn a_protocol_on_the_base_config_does_not_outrank_the_backend() {
        assert_eq!(
            try_negotiate(
                rewrap(),
                None,
                &["http/1.1"],
                spawn_backend(&["h2", "http/1.1"]).await,
                &["h2", "http/1.1"],
            )
            .await
            .expect("the client completes its handshake with the listener"),
            Some("h2".to_owned()),
        );
    }

    /// A failed upstream handshake makes `preprocess` decline, and the client's
    /// handshake fails with it.
    #[tokio::test]
    async fn a_backend_refusing_the_clients_alpn_declines_the_connection() {
        try_negotiate(
            rewrap(),
            None,
            &[],
            spawn_backend(&["h2"]).await,
            &["http/1.1"],
        )
        .await
        .expect_err("the listener cannot serve a client the backend refused");
    }

    /// The vhost reuses a live target whose config compares equal, so `alpn`
    /// has to be part of a target's identity.
    #[test]
    fn a_target_is_not_equal_to_one_that_pins_a_different_protocol() {
        let addr: SocketAddr = "10.0.3.2:443".parse().unwrap();
        let pinned = |proto: &[u8]| Some(AlpnInfo(vec![MaybeUtf8String(proto.to_vec())]));
        assert_ne!(
            target(addr, None, pinned(b"h2")),
            target(addr, None, pinned(b"http/1.1")),
        );
        assert_eq!(
            target(addr, None, pinned(b"h2")),
            target(addr, None, pinned(b"h2")),
        );
    }

    /// A pin narrows what the client asked for rather than replacing it.
    #[tokio::test]
    async fn a_client_speaking_one_of_the_pinned_protocols_is_served() {
        let both = AlpnInfo(vec![
            MaybeUtf8String(b"h2".to_vec()),
            MaybeUtf8String(b"http/1.1".to_vec()),
        ]);
        assert_eq!(
            try_negotiate(
                rewrap(),
                Some(both),
                &[],
                spawn_backend(&["h2", "http/1.1"]).await,
                &["http/1.1"],
            )
            .await
            .expect("a client speaking one of the pinned protocols is served"),
            Some("http/1.1".to_owned()),
        );
    }

    /// rustls alerts only when the list it was given is non-empty, so an empty
    /// pin turns nobody away and the container is still reached over TLS.
    #[tokio::test]
    async fn an_empty_pin_still_reaches_the_container_over_tls() {
        let (backend, negotiated) = spawn_backend_reporting(&["h2", "http/1.1"]).await;
        assert_eq!(
            try_negotiate(
                rewrap(),
                Some(AlpnInfo(Vec::new())),
                &[],
                backend,
                &["h2", "http/1.1"],
            )
            .await
            .expect("an empty pin turns nobody away"),
            None,
        );
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(10), negotiated)
                .await
                .expect("the container is dialled and its handshake settles within 10s")
                .expect("the container completes its handshake"),
            None,
        );
    }

    /// A client that shares no protocol with the pin is refused.
    #[tokio::test]
    async fn a_client_sharing_no_protocol_with_the_pin_is_refused() {
        let h2 = AlpnInfo(vec![MaybeUtf8String(b"h2".to_vec())]);
        try_negotiate(
            rewrap(),
            Some(h2),
            &[],
            spawn_backend(&["h2", "http/1.1"]).await,
            &["http/1.1"],
        )
        .await
        .expect_err("a client that cannot meet the pin is not served");
    }

    /// The stream `preprocess` hands back is the container's TLS session, and
    /// the client's bytes are spliced onto it.
    #[tokio::test]
    async fn the_client_is_spliced_onto_the_containers_tls_session() {
        let probe = b"start9";
        let (backend, read) = spawn_backend_reading(probe.len()).await;
        try_negotiate_probing(rewrap(), None, &[], backend, &["h2"], probe)
            .await
            .expect("a rewrap completes the client handshake");
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(10), read)
                .await
                .expect("the container reads the probe within 10s")
                .expect("the container reads the probe through its own session"),
            probe,
        );
    }

    /// The pin is the list a binding with no TLS leg offers the client, so
    /// rustls is what turns away a client that shares none of it.
    #[tokio::test]
    async fn a_plaintext_binding_refuses_a_client_that_cannot_meet_the_pin() {
        let h2 = AlpnInfo(vec![MaybeUtf8String(b"h2".to_vec())]);
        try_negotiate(None, Some(h2), &[], spawn_backend(&[]).await, &["http/1.1"])
            .await
            .expect_err("a client that cannot meet the pin is not served");
    }

    /// A client that names no protocol leaves the container named none either.
    #[tokio::test]
    async fn a_pin_does_not_reach_a_container_when_the_client_names_nothing() {
        let (backend, negotiated) = spawn_backend_reporting(&["h2", "http/1.1"]).await;
        let h2 = AlpnInfo(vec![MaybeUtf8String(b"h2".to_vec())]);
        assert_eq!(
            try_negotiate(rewrap(), Some(h2), &[], backend, &[])
                .await
                .expect("a client that names no protocol is still served"),
            None,
        );
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(10), negotiated)
                .await
                .expect("the container is dialled and its handshake settles within 10s")
                .expect("the container completes its handshake"),
            None,
            "the container was framed a protocol the client never named",
        );
    }

    /// A protocol the pin leaves out is off the table even where both ends
    /// speak it.
    #[tokio::test]
    async fn a_pin_keeps_the_container_off_the_protocols_it_excludes() {
        let (backend, negotiated) = spawn_backend_reporting(&["h2", "http/1.1"]).await;
        let http1 = AlpnInfo(vec![MaybeUtf8String(b"http/1.1".to_vec())]);
        assert_eq!(
            try_negotiate(rewrap(), Some(http1), &[], backend, &["h2", "http/1.1"])
                .await
                .expect("a pinned rewrap completes the client handshake"),
            Some("http/1.1".to_owned()),
        );
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(10), negotiated)
                .await
                .expect("the container is dialled and its handshake settles within 10s")
                .expect("the container completes its handshake"),
            Some(b"http/1.1".to_vec()),
            "the container picked from the client's list instead of the pinned one",
        );
    }

    /// An unset filter leaves the client its own list, in its own order.
    #[tokio::test]
    async fn an_unset_filter_gives_the_client_its_own_list() {
        assert_eq!(
            try_negotiate(
                None,
                None,
                &["http/1.1"],
                spawn_backend(&[]).await,
                &["h2", "http/1.1"],
            )
            .await
            .expect("an unfiltered binding completes the client handshake"),
            Some("h2".to_owned()),
        );
    }

    /// A client that offers no ALPN leaves the container offered none. The
    /// connector falls back to the dialling config's own list unless it is
    /// given one.
    #[tokio::test]
    async fn a_client_offering_no_alpn_does_not_fall_back_to_the_dialling_config() {
        try_negotiate(
            rewrap_configured_with(&["h2"]),
            None,
            &[],
            spawn_backend(&["http/1.1"]).await,
            &[],
        )
        .await
        .expect("the backend is offered nothing, so it has nothing to refuse");
    }

    /// A pin is the list the client is offered.
    #[tokio::test]
    async fn specified_offers_the_bindings_own_list() {
        let http1 = AlpnInfo(vec![MaybeUtf8String(b"http/1.1".to_vec())]);
        assert_eq!(
            try_negotiate(
                None,
                Some(http1),
                &["h2"],
                spawn_backend(&[]).await,
                &["h2", "http/1.1"],
            )
            .await
            .expect("a specified binding completes the client handshake"),
            Some("http/1.1".to_owned()),
        );
    }
}
