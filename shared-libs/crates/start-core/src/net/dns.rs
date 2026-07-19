use std::borrow::Borrow;
use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6};
use std::str::FromStr;
use std::sync::{Arc, Weak};
use std::time::Duration;

use clap::Parser;
use color_eyre::eyre::eyre;
use futures::{FutureExt, StreamExt, TryStreamExt};
use hickory_server::net::NetError;
use hickory_server::net::runtime::Time;
use hickory_server::proto::op::{Header, HeaderCounts, Metadata, ResponseCode};
use hickory_server::proto::rr::{Name, Record, RecordType};
use hickory_server::resolver::config::{
    ConnectionConfig, NameServerConfig, ResolverConfig, ResolverOpts,
};
use hickory_server::server::{Request, RequestHandler, ResponseHandler, ResponseInfo, Server};
use hickory_server::store::forwarder::{ForwardConfig, ForwardZoneHandler};
use hickory_server::zone_handler::{Catalog, MessageResponseBuilder, ZoneHandler};
use imbl::OrdMap;
use imbl_value::InternedString;
use itertools::Itertools;
use rpc_toolkit::{
    Context, HandlerArgs, HandlerExt, ParentHandler, from_fn_async, from_fn_blocking,
};
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tracing::instrument;
use ts_rs::TS;

use crate::context::{CliContext, RpcContext};
use crate::db::model::Database;
use crate::db::model::public::NetworkInterfaceInfo;
use crate::net::gateway::NetworkInterfaceWatcher;
use crate::net::utils::{bind_tokio_listener_reuse_port, ipv6_is_link_local};
use crate::prelude::*;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::file_string_stream;
use crate::util::serde::{HandlerExtSerde, display_serializable};
use crate::util::sync::{SyncRwLock, Watch};
use crate::{GatewayId, HOST_IP, OptionExt, PackageId};

pub fn dns_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "query",
            from_fn_blocking(query_dns::<C>)
                .with_display_serializable()
                .with_custom_display_fn(|HandlerArgs { params, .. }, res| {
                    if let Some(format) = params.format {
                        return display_serializable(format, res);
                    }

                    if let Some(ip) = res.ipv4 {
                        println!("{ip}")
                    }
                    if let Some(ip) = res.ipv6 {
                        println!("{ip}")
                    }

                    Ok(())
                })
                .with_about("about.test-dns-configuration-for-domain"),
        )
        .subcommand(
            "set-static",
            from_fn_async(set_static_dns)
                .no_display()
                .with_about("about.set-static-dns-servers")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "dump-table",
            from_fn_async(dump_table)
                .with_display_serializable()
                .with_custom_display_fn(|HandlerArgs { params, .. }, res| {
                    use prettytable::*;

                    if let Some(format) = params.format {
                        return display_serializable(format, res);
                    }

                    let mut table = Table::new();
                    table.add_row(row![bc => "FQDN", "DESTINATION"]);
                    for (hostname, destination) in res {
                        if let Some(ip) = destination {
                            table.add_row(row![hostname, ip]);
                        } else {
                            table.add_row(row![hostname, "SELF"]);
                        }
                    }

                    table.print_tty(false)?;

                    Ok(())
                })
                .with_about("about.dump-address-resolution-table")
                .with_call_remote::<CliContext>(),
        )
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
pub struct QueryDnsParams {
    #[arg(help = "help.arg.fqdn")]
    pub fqdn: InternedString,
}

/// What a public domain currently resolves to, per address family. A public
/// domain is DualStack: the operator points an `A` record at the gateway's WAN
/// IPv4 and an `AAAA` at the box's IPv6 GUA. Either may be absent.
#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct QueryDnsRes {
    #[ts(type = "string | null")]
    pub ipv4: Option<Ipv4Addr>,
    #[ts(type = "string | null")]
    pub ipv6: Option<Ipv6Addr>,
}

pub fn query_dns<C: Context>(
    _: C,
    QueryDnsParams { fqdn }: QueryDnsParams,
) -> Result<QueryDnsRes, Error> {
    let hints = dns_lookup::AddrInfoHints {
        flags: 0,
        address: libc::AF_UNSPEC,
        socktype: 0,
        protocol: 0,
    };
    let addrs = dns_lookup::getaddrinfo(Some(&*fqdn), None, Some(hints))
        .map(Some)
        .or_else(|e| {
            if matches!(
                e.kind(),
                dns_lookup::LookupErrorKind::NoName | dns_lookup::LookupErrorKind::NoData
            ) {
                Ok(None)
            } else {
                Err(std::io::Error::from(e))
            }
        })
        .with_kind(ErrorKind::Network)?
        .into_iter()
        .flatten();
    let mut res = QueryDnsRes {
        ipv4: None,
        ipv6: None,
    };
    for a in addrs {
        match a.map_err(Error::from)?.sockaddr.ip() {
            IpAddr::V4(v4) => {
                res.ipv4.get_or_insert(v4);
            }
            IpAddr::V6(v6) => {
                res.ipv6.get_or_insert(v6);
            }
        }
    }
    Ok(res)
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
pub struct SetStaticDnsParams {
    #[arg(help = "help.arg.dns-servers")]
    pub servers: Option<Vec<String>>,
}

pub async fn set_static_dns(
    ctx: RpcContext,
    SetStaticDnsParams { servers }: SetStaticDnsParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_network_mut()
                .as_dns_mut()
                .as_static_servers_mut()
                .ser(
                    &servers
                        .map(|s| {
                            s.into_iter()
                                .map(|s| {
                                    s.parse::<SocketAddr>()
                                        .or_else(|_| s.parse::<IpAddr>().map(|a| (a, 53).into()))
                                })
                                .collect()
                        })
                        .transpose()?,
                )
        })
        .await
        .result
}

pub async fn dump_table(
    ctx: RpcContext,
) -> Result<BTreeMap<InternedString, Option<IpAddr>>, Error> {
    Ok(ctx
        .net_controller
        .dns
        .resolve
        .upgrade()
        .or_not_found("DnsController")?
        .peek(|map| {
            map.private_domains
                .iter()
                .map(|(d, _)| (d.clone(), None))
                .chain(map.services.iter().filter_map(|(svc, ip)| {
                    ip.iter()
                        .find(|(_, rc)| rc.strong_count() > 0)
                        .map(|(ip, _)| {
                            (
                                svc.as_ref().map_or(
                                    InternedString::from_static("startos"),
                                    |svc| {
                                        InternedString::from_display(&lazy_format!("{svc}.startos"))
                                    },
                                ),
                                Some(IpAddr::V4(*ip)),
                            )
                        })
                }))
                .collect()
        }))
}

#[derive(Default)]
struct ResolveMap {
    private_domains: BTreeMap<InternedString, (BTreeSet<GatewayId>, Weak<()>)>,
    services: BTreeMap<Option<PackageId>, BTreeMap<Ipv4Addr, Weak<()>>>,
}

pub struct DnsController {
    resolve: Weak<SyncRwLock<ResolveMap>>,
    #[allow(dead_code)]
    dns_server: NonDetachingJoinHandle<()>,
}

lazy_static::lazy_static! {
    static ref LOCALHOST: Name = Name::from_ascii("localhost").unwrap();
    static ref STARTOS: Name = Name::from_ascii("startos").unwrap();
    static ref EMBASSY: Name = Name::from_ascii("embassy").unwrap();
}

/// Per-connection outgoing-response buffer size for TCP DNS listeners.
pub(crate) const DNS_RESPONSE_BUFFER_SIZE: usize = 32;

/// A forwarding upstream for `addr`, reachable over both UDP and TCP on its port.
pub(crate) fn forward_name_server(addr: SocketAddr) -> NameServerConfig {
    let mut udp = ConnectionConfig::udp();
    udp.port = addr.port();
    let mut tcp = ConnectionConfig::tcp();
    tcp.port = addr.port();
    NameServerConfig::new(addr.ip(), true, vec![udp, tcp])
}

/// The socket address a parsed upstream `NameServerConfig` points at.
pub(crate) fn name_server_socket_addr(ns: &NameServerConfig) -> SocketAddr {
    SocketAddr::new(ns.ip, ns.connections.first().map_or(53, |c| c.port))
}

/// Parse systemd-resolved's resolv.conf. hickory 0.26 only exposes
/// `system_conf::parse_resolv_conf` on non-apple unix; this path only runs on
/// the (Linux) server, so non-Linux targets just need a stub to compile.
#[cfg(target_os = "linux")]
pub(crate) fn parse_resolv_conf(
    data: impl AsRef<[u8]>,
) -> Result<(ResolverConfig, ResolverOpts), Error> {
    hickory_server::resolver::system_conf::parse_resolv_conf(data)
        .with_kind(ErrorKind::ParseSysInfo)
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn parse_resolv_conf(
    _data: impl AsRef<[u8]>,
) -> Result<(ResolverConfig, ResolverOpts), Error> {
    Err(Error::new(
        eyre!("resolv.conf parsing is only supported on Linux"),
        ErrorKind::ParseSysInfo,
    ))
}

/// What private domains a resolver (one per bound socket) may answer.
#[derive(Clone)]
enum PrivateScope {
    /// Wildcard catch-all — never answers private domains.
    None,
    /// A gateway interface — answers a private domain only when its gateway set
    /// contains this gateway, returning that gateway's own address.
    Gateway(GatewayId),
    /// Loopback / container bridge — answers every private domain, returning
    /// these fixed local addresses so the host and service containers can always
    /// reach a private domain locally.
    Local(Vec<IpAddr>),
}

struct Resolver {
    catalog: Arc<SyncRwLock<Arc<Catalog>>>,
    resolve: Arc<SyncRwLock<ResolveMap>>,
    net_iface: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    scope: PrivateScope,
}

/// Keep the forwarder authority in `catalog` in sync with resolv.conf and the
/// user's static upstream servers.
fn spawn_forwarder(
    db: TypedPatchDb<Database>,
    catalog: Arc<SyncRwLock<Arc<Catalog>>>,
) -> NonDetachingJoinHandle<()> {
    tokio::spawn(async move {
        let mut prev = crate::util::serde::hash_serializable::<sha2::Sha256, _>(&(
            ResolverConfig::from_parts(None, Vec::new(), Vec::new()),
            ResolverOpts::default(),
            Option::<std::collections::VecDeque<SocketAddr>>::None,
        ))
        .unwrap_or_default();
        loop {
            let res: Result<(), Error> = async {
                let mut file_stream = file_string_stream("/run/systemd/resolve/resolv.conf")
                    .filter_map(|a| futures::future::ready(a.transpose()))
                    .boxed();
                let mut static_sub = db
                    .subscribe(
                        "/public/serverInfo/network/dns/staticServers"
                            .parse()
                            .unwrap(),
                    )
                    .await;
                let mut last_config: Option<(ResolverConfig, ResolverOpts)> = None;
                loop {
                    let got_file = tokio::select! {
                        res = file_stream.try_next() => {
                            let conf = res?
                                .ok_or_else(|| Error::new(
                                    eyre!("resolv.conf stream ended"),
                                    ErrorKind::Network,
                                ))?;
                            let (config, mut opts) = parse_resolv_conf(conf)?;
                            // Per-attempt upstream timeout. Container stub
                            // resolvers give up around 5s, so a longer forward
                            // serves nobody; keep the default retry count.
                            opts.timeout = Duration::from_secs(5);
                            last_config = Some((config, opts));
                            true
                        }
                        _ = static_sub.recv() => false,
                    };
                    let Some((ref config, ref opts)) = last_config else {
                        continue;
                    };
                    let static_servers: Option<std::collections::VecDeque<SocketAddr>> = db
                        .peek()
                        .await
                        .as_public()
                        .as_server_info()
                        .as_network()
                        .as_dns()
                        .as_static_servers()
                        .de()?;
                    let hash = crate::util::serde::hash_serializable::<sha2::Sha256, _>(&(
                        config,
                        opts,
                        &static_servers,
                    ))?;
                    if hash == prev {
                        prev = hash;
                        continue;
                    }
                    if got_file {
                        db.mutate(|db| {
                            db.as_public_mut()
                                .as_server_info_mut()
                                .as_network_mut()
                                .as_dns_mut()
                                .as_dhcp_servers_mut()
                                .ser(
                                    &config
                                        .name_servers()
                                        .iter()
                                        .map(name_server_socket_addr)
                                        .dedup()
                                        .skip(2)
                                        .collect(),
                                )
                        })
                        .await
                        .result?;
                    }
                    let forward_servers: Vec<NameServerConfig> =
                        if let Some(servers) = &static_servers {
                            servers
                                .iter()
                                .map(|addr| forward_name_server(*addr))
                                .collect()
                        } else {
                            config.name_servers().iter().skip(2).cloned().collect()
                        };
                    let auth: Vec<Arc<dyn ZoneHandler>> = vec![Arc::new(
                        ForwardZoneHandler::builder_tokio(ForwardConfig {
                            name_servers: forward_servers,
                            options: Some(opts.clone()),
                        })
                        .build()
                        .map_err(|e| Error::new(eyre!("{e}"), ErrorKind::Network))?,
                    )];
                    let mut next = Catalog::new();
                    next.upsert(Name::root().into(), auth);
                    catalog.mutate(|c| *c = Arc::new(next));
                    prev = hash;
                }
            }
            .await;
            if let Err(e) = res {
                tracing::error!("{e}");
                tracing::debug!("{e:?}");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    })
    .into()
}
/// How a query should be answered.
enum Resolution {
    /// Answer these addresses locally (an empty vec for the queried family is a
    /// NODATA answer, as before).
    Answer(Vec<IpAddr>),
    /// The name is in a private StartOS zone but no service claims it — answer
    /// authoritatively with NXDOMAIN instead of forwarding it upstream (which
    /// would both leak internal hostnames and, during an upstream outage, pile
    /// restarting-service lookups onto the slow forwarder).
    NxDomain,
    /// Not ours — forward upstream.
    Forward,
}

impl Resolver {
    fn resolve(&self, name: &Name, mut src: IpAddr) -> Resolution {
        if name.zone_of(&*LOCALHOST) {
            return Resolution::Answer(vec![
                Ipv4Addr::LOCALHOST.into(),
                Ipv6Addr::LOCALHOST.into(),
            ]);
        }
        src = match src {
            IpAddr::V6(v6) => {
                if let Some(v4) = v6.to_ipv4_mapped() {
                    IpAddr::V4(v4)
                } else {
                    IpAddr::V6(v6)
                }
            }
            a => a,
        };
        let domain = name.to_lowercase().to_utf8();
        let domain = domain.trim_end_matches('.');
        self.resolve.peek(|r| {
            match &self.scope {
                PrivateScope::None => {}
                PrivateScope::Gateway(my_gateway) => {
                    let serves_here = r
                        .private_domains
                        .get(domain)
                        .filter(|(_, rc)| rc.strong_count() > 0)
                        .map_or(false, |(gateways, _)| gateways.contains(my_gateway));
                    if serves_here {
                        if let Some(res) = self.net_iface.peek(|i| {
                            i.get(my_gateway)
                                .and_then(|info| info.ip_info.as_ref())
                                .map(|ip_info| {
                                    let mut res = ip_info.subnets.iter().collect::<Vec<_>>();
                                    res.sort_by_cached_key(|a| !a.contains(&src));
                                    res.into_iter().map(|s| s.addr()).collect()
                                })
                        }) {
                            return Resolution::Answer(res);
                        }
                    }
                }
                PrivateScope::Local(addrs) => {
                    if r.private_domains
                        .get(domain)
                        .map_or(false, |(_, rc)| rc.strong_count() > 0)
                    {
                        return Resolution::Answer(addrs.clone());
                    }
                }
            }
            if STARTOS.zone_of(name) || EMBASSY.zone_of(name) {
                let Ok(pkg) = name
                    .iter()
                    .rev()
                    .skip(1)
                    .next()
                    .map(std::str::from_utf8)
                    .transpose()
                    .map_err(|_| ())
                    .and_then(|s| s.map(PackageId::from_str).transpose().map_err(|_| ()))
                else {
                    return Resolution::NxDomain;
                };
                // the server's own services are registered under `None`;
                // `start-os.startos` is the server, same as bare `startos`
                let pkg = pkg.filter(|p| !p.is_start_os());
                if let Some(ip) = r.services.get(&pkg) {
                    Resolution::Answer(
                        ip.iter()
                            .filter(|(_, rc)| rc.strong_count() > 0)
                            .map(|(ip, _)| (*ip).into())
                            .collect(),
                    )
                } else {
                    Resolution::NxDomain
                }
            } else {
                Resolution::Forward
            }
        })
    }
}

#[async_trait::async_trait]
impl RequestHandler for Resolver {
    async fn handle_request<R: ResponseHandler, T: Time>(
        &self,
        request: &Request,
        mut response_handle: R,
    ) -> ResponseInfo {
        match async {
            let req = request
                .request_info()
                .map_err(|e| NetError::from(e.to_string()))?;
            let query = req.query;
            let name = query.name();

            match self.resolve(name, req.src.ip()) {
                Resolution::Forward => Ok(None),
                Resolution::NxDomain => {
                    let mut header = Metadata::response_from_request(&request.metadata);
                    header.recursion_available = true;
                    header.authoritative = true;
                    header.response_code = ResponseCode::NXDomain;
                    response_handle
                        .send_response(
                            MessageResponseBuilder::from_message_request(&*request).build(
                                header,
                                [],
                                [],
                                [],
                                [],
                            ),
                        )
                        .await
                        .map(Some)
                }
                Resolution::Answer(ip) => match query.query_type() {
                    RecordType::A => {
                        let mut header = Metadata::response_from_request(&request.metadata);
                        header.recursion_available = true;
                        response_handle
                            .send_response(
                                MessageResponseBuilder::from_message_request(&*request).build(
                                    header,
                                    &ip.into_iter()
                                        .filter_map(|a| {
                                            if let IpAddr::V4(a) = a { Some(a) } else { None }
                                        })
                                        .map(|ip| {
                                            Record::from_rdata(
                                                query.name().to_owned().into(),
                                                0,
                                                hickory_server::proto::rr::RData::A(ip.into()),
                                            )
                                        })
                                        .collect::<Vec<_>>(),
                                    [],
                                    [],
                                    [],
                                ),
                            )
                            .await
                            .map(Some)
                    }
                    RecordType::AAAA => {
                        let mut header = Metadata::response_from_request(&request.metadata);
                        header.recursion_available = true;
                        response_handle
                            .send_response(
                                MessageResponseBuilder::from_message_request(&*request).build(
                                    header,
                                    &ip.into_iter()
                                        .filter_map(|a| {
                                            if let IpAddr::V6(a) = a { Some(a) } else { None }
                                        })
                                        .map(|ip| {
                                            Record::from_rdata(
                                                query.name().to_owned().into(),
                                                0,
                                                hickory_server::proto::rr::RData::AAAA(ip.into()),
                                            )
                                        })
                                        .collect::<Vec<_>>(),
                                    [],
                                    [],
                                    [],
                                ),
                            )
                            .await
                            .map(Some)
                    }
                    _ => {
                        let mut header = Metadata::response_from_request(&request.metadata);
                        header.recursion_available = true;
                        response_handle
                            .send_response(
                                MessageResponseBuilder::from_message_request(&*request).build(
                                    header,
                                    [],
                                    [],
                                    [],
                                    [],
                                ),
                            )
                            .await
                            .map(Some)
                    }
                },
            }
        }
        .await
        {
            Ok(Some(a)) => return a,
            Ok(None) => (),
            Err(e) => {
                tracing::error!(
                    "{}",
                    t!("net.dns.error-resolving-internal", error = e.to_string())
                );
                tracing::debug!("{e:?}");
                let mut header = Metadata::response_from_request(&request.metadata);
                header.recursion_available = true;
                header.response_code = ResponseCode::ServFail;
                return response_handle
                    .send_response(
                        MessageResponseBuilder::from_message_request(&*request).build(
                            header,
                            [],
                            [],
                            [],
                            [],
                        ),
                    )
                    .await
                    .unwrap_or_else(|_| {
                        Header {
                            metadata: header,
                            counts: HeaderCounts::default(),
                        }
                        .into()
                    });
            }
        }
        let catalog = self.catalog.peek(|c| c.clone());
        catalog
            .handle_request::<R, T>(request, response_handle)
            .await
    }
}

fn bind_reuse_udp(addr: SocketAddr, dual_stack: bool) -> Result<UdpSocket, Error> {
    let domain = match addr {
        SocketAddr::V4(_) => socket2::Domain::IPV4,
        SocketAddr::V6(_) => socket2::Domain::IPV6,
    };
    let socket = socket2::Socket::new(domain, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))
        .with_kind(ErrorKind::Network)?;
    socket
        .set_reuse_address(true)
        .with_kind(ErrorKind::Network)?;
    socket.set_reuse_port(true).with_kind(ErrorKind::Network)?;
    if matches!(addr, SocketAddr::V6(_)) {
        socket
            .set_only_v6(!dual_stack)
            .with_kind(ErrorKind::Network)?;
    }
    socket.set_nonblocking(true).with_kind(ErrorKind::Network)?;
    socket.bind(&addr.into()).with_kind(ErrorKind::Network)?;
    UdpSocket::from_std(socket.into()).with_kind(ErrorKind::Network)
}

fn dns_server_on(resolver: Resolver, addr: SocketAddr) -> Result<Server<Resolver>, Error> {
    let dual_stack = matches!(addr, SocketAddr::V6(v6) if v6.ip().is_unspecified());
    let mut server = Server::new(resolver);
    server.register_listener(
        bind_tokio_listener_reuse_port(addr).with_kind(ErrorKind::Network)?,
        Duration::from_secs(30),
        DNS_RESPONSE_BUFFER_SIZE,
    );
    server.register_socket(bind_reuse_udp(addr, dual_stack)?);
    Ok(server)
}

/// Serve DNS on a wildcard catch-all socket plus a socket bound to each gateway
/// interface address, loopback, and the container bridge. Because each specific
/// socket only receives datagrams sent to its address, the kernel routes a query
/// to the socket for the interface it ingressed on. A gateway socket answers a
/// private domain only when the domain is configured for that gateway (so it
/// never leaks across interfaces), while loopback and the container bridge answer
/// every private domain locally; the wildcard answers none.
async fn run_dns_servers(
    db: TypedPatchDb<Database>,
    catalog: Arc<SyncRwLock<Arc<Catalog>>>,
    resolve: Arc<SyncRwLock<ResolveMap>>,
    mut net_iface: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
) -> Result<(), Error> {
    let _forwarder = spawn_forwarder(db, catalog.clone());
    let resolver_iface = net_iface.clone();
    let resolver = |scope: PrivateScope| Resolver {
        catalog: catalog.clone(),
        resolve: resolve.clone(),
        net_iface: resolver_iface.clone(),
        scope,
    };

    let loopback = vec![
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
    ];
    let host_ip = IpAddr::V4(Ipv4Addr::from(HOST_IP));

    let mut servers: BTreeMap<SocketAddr, Server<Resolver>> = BTreeMap::new();
    loop {
        let mut desired: BTreeMap<SocketAddr, PrivateScope> = BTreeMap::new();
        net_iface.peek(|ifaces| {
            for (gw, info) in ifaces {
                let Some(ip_info) = info.ip_info.as_ref() else {
                    continue;
                };
                for subnet in &ip_info.subnets {
                    let addr = match subnet.addr() {
                        IpAddr::V6(v6) if ipv6_is_link_local(v6) => {
                            SocketAddr::V6(SocketAddrV6::new(v6, 53, 0, ip_info.scope_id))
                        }
                        ip => SocketAddr::new(ip, 53),
                    };
                    desired.insert(addr, PrivateScope::Gateway(gw.clone()));
                }
            }
        });
        // wildcard catch-all + the always-on loopback / container-bridge scopes,
        // inserted last so they win over any gateway that reports the same address.
        desired.insert(
            SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 53),
            PrivateScope::None,
        );
        desired.insert(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 53),
            PrivateScope::Local(loopback.clone()),
        );
        desired.insert(
            SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 53),
            PrivateScope::Local(loopback.clone()),
        );
        desired.insert(
            SocketAddr::new(host_ip, 53),
            PrivateScope::Local(vec![host_ip]),
        );

        servers.retain(|addr, _| desired.contains_key(addr));
        for (addr, scope) in desired {
            if !servers.contains_key(&addr) {
                match dns_server_on(resolver(scope), addr) {
                    Ok(server) => {
                        servers.insert(addr, server);
                    }
                    Err(e) => {
                        tracing::error!("failed to bind DNS on {addr}: {e}");
                        tracing::debug!("{e:?}");
                    }
                }
            }
        }

        net_iface.changed().await;
    }
}

impl DnsController {
    #[instrument(skip_all)]
    pub async fn init(
        db: TypedPatchDb<Database>,
        watcher: &NetworkInterfaceWatcher,
    ) -> Result<Self, Error> {
        let resolve = Arc::new(SyncRwLock::new(ResolveMap::default()));
        let catalog = Arc::new(SyncRwLock::new(Arc::new(Catalog::new())));
        let weak = Arc::downgrade(&resolve);
        let net_iface = watcher.subscribe();

        let dns_server = tokio::spawn(run_dns_servers(db, catalog, resolve, net_iface).map(|r| {
            r.log_err();
        }))
        .into();

        Ok(Self {
            resolve: weak,
            dns_server,
        })
    }

    pub fn add_service(&self, pkg_id: Option<PackageId>, ip: Ipv4Addr) -> Result<Arc<()>, Error> {
        if let Some(resolve) = Weak::upgrade(&self.resolve) {
            resolve.mutate(|writable| {
                let ips = writable.services.entry(pkg_id).or_default();
                let weak = ips.entry(ip).or_default();
                let rc = if let Some(rc) = Weak::upgrade(&*weak) {
                    rc
                } else {
                    let new = Arc::new(());
                    *weak = Arc::downgrade(&new);
                    new
                };
                Ok(rc)
            })
        } else {
            Err(Error::new(
                eyre!("{}", t!("net.dns.server-thread-exited")),
                crate::ErrorKind::Network,
            ))
        }
    }

    pub fn gc_service(&self, pkg_id: Option<PackageId>, ip: Ipv4Addr) -> Result<(), Error> {
        if let Some(resolve) = Weak::upgrade(&self.resolve) {
            resolve.mutate(|writable| {
                let mut ips = writable.services.remove(&pkg_id).unwrap_or_default();
                if let Some(rc) = Weak::upgrade(&ips.remove(&ip).unwrap_or_default()) {
                    ips.insert(ip, Arc::downgrade(&rc));
                }
                if !ips.is_empty() {
                    writable.services.insert(pkg_id, ips);
                }
                Ok(())
            })
        } else {
            Err(Error::new(
                eyre!("{}", t!("net.dns.server-thread-exited")),
                crate::ErrorKind::Network,
            ))
        }
    }

    pub fn add_private_domain(
        &self,
        fqdn: InternedString,
        gateways: BTreeSet<GatewayId>,
    ) -> Result<Arc<()>, Error> {
        if let Some(resolve) = Weak::upgrade(&self.resolve) {
            resolve.mutate(|writable| {
                let entry = writable.private_domains.entry(fqdn).or_default();
                entry.0 = gateways;
                let rc = if let Some(rc) = Weak::upgrade(&entry.1) {
                    rc
                } else {
                    let new = Arc::new(());
                    entry.1 = Arc::downgrade(&new);
                    new
                };
                Ok(rc)
            })
        } else {
            Err(Error::new(
                eyre!("{}", t!("net.dns.server-thread-exited")),
                crate::ErrorKind::Network,
            ))
        }
    }

    pub fn gc_private_domains<'a, BK: Ord + 'a>(
        &self,
        domains: impl IntoIterator<Item = &'a BK> + 'a,
    ) -> Result<(), Error>
    where
        InternedString: Borrow<BK>,
    {
        if let Some(resolve) = Weak::upgrade(&self.resolve) {
            resolve.mutate(|writable| {
                for domain in domains {
                    if let Some((k, v)) = writable.private_domains.remove_entry(domain) {
                        if v.1.strong_count() > 0 {
                            writable.private_domains.insert(k, v);
                        }
                    }
                }
                Ok(())
            })
        } else {
            Err(Error::new(
                eyre!("{}", t!("net.dns.server-thread-exited")),
                crate::ErrorKind::Network,
            ))
        }
    }
}
