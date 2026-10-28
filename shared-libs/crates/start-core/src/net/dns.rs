use std::borrow::Borrow;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6};
use std::str::FromStr;
use std::sync::{Arc, Weak};
use std::time::Duration;

use clap::Parser;
use color_eyre::eyre::eyre;
use futures::FutureExt;
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
use rpc_toolkit::{
    Context, HandlerArgs, HandlerExt, ParentHandler, from_fn, from_fn_async, from_fn_blocking,
};
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tracing::instrument;

use crate::context::{CliContext, DiagnosticContext, InitContext, RpcContext, SetupContext};
use crate::db::model::Database;
use crate::db::model::public::{NetworkInterfaceInfo, Public};
use crate::net::gateway::{GatewayInfo, NetworkInterfaceController};
use crate::net::utils::{bind_tokio_listener_reuse_port, ipv6_is_link_local};
use crate::prelude::*;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::serde::{HandlerExtSerde, display_serializable};
use crate::util::sync::{SyncRwLock, Watch};
use crate::{GatewayId, HOST_IP, OptionExt, PackageId};

pub(crate) mod upstream;

use upstream::{
    GatewayRuntime, auto_gateway, is_host_address, is_host_source, source_zone_matches_interface,
    underlay_gateway, valid_upstream,
};

pub fn dns_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "status",
            from_fn(|ctx: InitContext| Ok::<_, Error>(ctx.dns.status())).no_cli(),
        )
        .subcommand(
            "status",
            from_fn(|ctx: SetupContext| Ok::<_, Error>(ctx.dns.status())).no_cli(),
        )
        .subcommand(
            "status",
            from_fn(|ctx: DiagnosticContext| Ok::<_, Error>(ctx.dns.status())).no_cli(),
        )
        .subcommand(
            "status",
            from_fn(|ctx: RpcContext| Ok::<_, Error>(ctx.net_controller.dns.status())).no_cli(),
        )
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

                    print_table(&table)?;

                    Ok(())
                })
                .with_about("about.dump-address-resolution-table")
                .with_call_remote::<CliContext>(),
        )
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
pub struct QueryDnsParams {
    #[arg(help = "help.arg.fqdn")]
    pub fqdn: InternedString,
}

rpc_toolkit::reflect_ts!(QueryDnsParams);
rpc_toolkit::ts_export!(QueryDnsParams, namespaces = [""]);

/// What a public domain currently resolves to, per address family. A public
/// domain is DualStack: the operator points an `A` record at the gateway's WAN
/// IPv4 and an `AAAA` at the box's IPv6 GUA. Either may be absent.
#[derive(Debug, Clone, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct QueryDnsRes {
    pub ipv4: Option<Ipv4Addr>,
    pub ipv6: Option<Ipv6Addr>,
}

rpc_toolkit::reflect_ts!(QueryDnsRes);
rpc_toolkit::ts_export!(QueryDnsRes, namespaces = [""]);

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

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
pub struct SetStaticDnsParams {
    #[arg(help = "help.arg.gateway-id")]
    pub gateway: GatewayId,
    #[arg(help = "help.arg.dns-servers")]
    pub servers: Option<Vec<String>>,
}

rpc_toolkit::reflect_ts!(SetStaticDnsParams);
rpc_toolkit::ts_export!(SetStaticDnsParams, namespaces = [""]);

pub async fn set_static_dns(
    ctx: RpcContext,
    SetStaticDnsParams { gateway, servers }: SetStaticDnsParams,
) -> Result<(), Error> {
    let interfaces = ctx.net_controller.net_iface.watcher.ip_info();
    let servers = validate_static_servers(servers, &interfaces)?;
    ctx.net_controller
        .net_iface
        .set_dns(&gateway, servers)
        .await
}

/// An absent update preserves DNS intent; null servers select gateway DNS.
#[derive(Debug, Clone, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct OutboundDnsUpdate {
    pub servers: Option<Vec<String>>,
}

rpc_toolkit::reflect_ts!(OutboundDnsUpdate);
rpc_toolkit::ts_export!(OutboundDnsUpdate, namespaces = [""]);

pub(crate) fn validate_dns_update(
    update: Option<OutboundDnsUpdate>,
    interfaces: &OrdMap<GatewayId, NetworkInterfaceInfo>,
) -> Result<Option<Option<std::collections::VecDeque<SocketAddr>>>, Error> {
    update
        .map(|update| validate_static_servers(update.servers, interfaces))
        .transpose()
}

pub(crate) fn validate_static_servers(
    servers: Option<Vec<String>>,
    interfaces: &OrdMap<GatewayId, NetworkInterfaceInfo>,
) -> Result<Option<std::collections::VecDeque<SocketAddr>>, Error> {
    let servers = parse_static_servers(servers)?;
    for server in servers.iter().flatten() {
        ensure_code!(
            !is_host_address(
                server.ip(),
                interfaces
                    .values()
                    .filter_map(|info| info.ip_info.as_deref())
            ),
            ErrorKind::InvalidRequest,
            "{}",
            t!("net.dns.static-server-is-self", server = server)
        );
    }
    Ok(servers)
}

fn parse_static_servers(
    servers: Option<Vec<String>>,
) -> Result<Option<std::collections::VecDeque<SocketAddr>>, Error> {
    servers
        .map(|servers| {
            ensure_code!(
                !servers.is_empty(),
                ErrorKind::InvalidRequest,
                "{}",
                t!("net.dns.empty-servers")
            );
            servers
                .into_iter()
                .map(|s| {
                    let addr = s
                        .parse::<SocketAddr>()
                        .or_else(|_| s.parse::<IpAddr>().map(|a| SocketAddr::from((a, 53))))?;
                    ensure_code!(
                        valid_upstream(addr),
                        ErrorKind::InvalidRequest,
                        "{}",
                        t!("net.dns.invalid-server", server = s)
                    );
                    Ok(addr)
                })
                .collect()
        })
        .transpose()
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
                        .find(|(_, registration)| registration.lifetime.strong_count() > 0)
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
    services: BTreeMap<Option<PackageId>, BTreeMap<Ipv4Addr, ServiceRegistration>>,
}

#[derive(Default)]
struct ServiceRegistration {
    ipv6: Option<Ipv6Addr>,
    lifetime: Weak<()>,
}

impl ResolveMap {
    fn requester(&self, source: IpAddr) -> Result<Option<PackageId>, Error> {
        let source = source.to_canonical();
        let mut retired = false;
        for (id, addresses) in &self.services {
            for (ipv4, registration) in addresses {
                if source == IpAddr::V4(*ipv4) || registration.ipv6.map(IpAddr::V6) == Some(source)
                {
                    if registration.lifetime.strong_count() > 0 {
                        return Ok(id.clone());
                    }
                    retired = true;
                }
            }
        }
        // NetService can retire before its LXC container exits.
        ensure_code!(!retired, ErrorKind::Network, "DNS requester has retired");
        Ok(None)
    }
}

/// Active DNS authority and its bootstrap gateway.
#[derive(Clone, Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct DnsStatus {
    /// True until stored network policy takes authority.
    pub bootstrap: bool,
    /// Present only before stored network policy takes authority.
    pub gateway: Option<GatewayInfo>,
}

rpc_toolkit::reflect_ts!(DnsStatus);
rpc_toolkit::ts_export!(DnsStatus, namespaces = [""]);

#[derive(Clone)]
enum DnsAuthority {
    Bootstrap(Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>),
    Runtime(Arc<NetworkInterfaceController>),
}

impl DnsAuthority {
    fn interfaces(&self) -> Watch<OrdMap<GatewayId, NetworkInterfaceInfo>> {
        match self {
            Self::Bootstrap(interfaces) => interfaces.clone(),
            Self::Runtime(network) => network.watcher.subscribe(),
        }
    }
}

pub struct DnsController {
    resolve: Weak<SyncRwLock<ResolveMap>>,
    authority: Watch<DnsAuthority>,
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

/// Parses a Linux resolver configuration.
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
#[derive(Clone, PartialEq, Eq)]
enum PrivateScope {
    /// Wildcard catch-all — never answers private domains.
    None,
    Gateway(GatewayId),
    Local(Vec<IpAddr>),
}

struct Resolver {
    authority: Watch<DnsAuthority>,
    forwarders: Arc<SyncRwLock<ForwarderCache>>,
    resolve: Arc<SyncRwLock<ResolveMap>>,
    scope: PrivateScope,
}

type ForwarderCache = BTreeMap<(GatewayId, VecDeque<SocketAddr>), GatewayForwarder>;

struct GatewayForwarder {
    index: u32,
    catalog: Arc<Catalog>,
}

fn configured_dns_profiles(
    public: &Model<Public>,
) -> Result<BTreeSet<VecDeque<SocketAddr>>, Error> {
    let mut profiles = BTreeSet::new();
    profiles.extend(
        public
            .as_server_info()
            .as_network()
            .as_outbound_dns()
            .de()?,
    );
    for (_, package) in public.as_package_data().as_entries()? {
        profiles.extend(package.as_outbound_dns().de()?);
    }
    Ok(profiles)
}

fn gateway_catalog(
    gateway: GatewayId,
    index: u32,
    servers: &VecDeque<SocketAddr>,
) -> Result<Arc<Catalog>, Error> {
    let mut options = ResolverOpts::default();
    options.timeout = Duration::from_secs(5);
    let forwarder = ForwardZoneHandler::builder_with_config(
        ForwardConfig {
            name_servers: servers
                .iter()
                .map(|addr| forward_name_server(*addr))
                .collect(),
            options: Some(options),
        },
        GatewayRuntime::new(gateway, index),
    )
    .build()
    .map_err(|e| Error::new(eyre!("{e}"), ErrorKind::Network))?;
    let mut catalog = Catalog::new();
    catalog.upsert(
        Name::root().into(),
        vec![Arc::new(forwarder) as Arc<dyn ZoneHandler>],
    );
    Ok(Arc::new(catalog))
}
/// How a query should be answered.
enum Resolution {
    /// Answers locally; an absent address family returns NODATA.
    Answer(Vec<IpAddr>),
    /// Rejects unknown StartOS names without forwarding them.
    NxDomain,
    Forward,
}

impl Resolver {
    async fn request_catalog(
        &self,
        source: SocketAddr,
        name: &Name,
    ) -> Result<Arc<Catalog>, Error> {
        let authority = self.authority.read();
        let interfaces = authority.interfaces().read();
        match authority {
            DnsAuthority::Bootstrap(_) => self.bootstrap_catalog(source, &interfaces),
            DnsAuthority::Runtime(network) => {
                let domain = name.to_lowercase().to_utf8();
                let domain = domain.trim_end_matches('.');
                let from_host = is_host_source(
                    source,
                    interfaces.values().filter_map(|i| i.ip_info.as_deref()),
                );
                if from_host
                    && interfaces
                        .values()
                        .any(|i| i.wireguard_endpoint_domains.contains(domain))
                {
                    let gateway = underlay_gateway(&interfaces)
                        .or_not_found("WireGuard DNS bootstrap underlay")?;
                    let db = network.db.peek().await;
                    self.catalog_for_profile(
                        gateway,
                        None,
                        &interfaces,
                        &configured_dns_profiles(db.as_public())?,
                    )
                } else {
                    self.routed_catalog(source, &network.db, &interfaces).await
                }
            }
        }
    }

    fn bootstrap_catalog(
        &self,
        source: SocketAddr,
        interfaces: &OrdMap<GatewayId, NetworkInterfaceInfo>,
    ) -> Result<Arc<Catalog>, Error> {
        ensure_code!(
            is_host_source(
                source,
                interfaces
                    .values()
                    .filter_map(|info| info.ip_info.as_deref())
            ),
            ErrorKind::Network,
            "bootstrap DNS only serves the host"
        );
        let gateway = underlay_gateway(interfaces).or_not_found("bootstrap DNS underlay")?;
        self.catalog_for_profile(gateway, None, interfaces, &BTreeSet::new())
    }

    async fn routed_catalog(
        &self,
        source: SocketAddr,
        db: &TypedPatchDb<Database>,
        interfaces: &OrdMap<GatewayId, NetworkInterfaceInfo>,
    ) -> Result<Arc<Catalog>, Error> {
        let source_ip = source.ip().to_canonical();
        let package = self.resolve.peek(|r| r.requester(source_ip))?;
        if package.is_none()
            && !is_host_source(
                source,
                interfaces
                    .values()
                    .filter_map(|info| info.ip_info.as_deref()),
            )
        {
            let bridge_listener = matches!(&self.scope, PrivateScope::Local(addresses) if addresses.contains(&IpAddr::V4(HOST_IP.into())));
            let bridge_source = interfaces
                .values()
                .filter_map(|info| info.ip_info.as_deref())
                .filter(|info| {
                    info.device_type == Some(crate::db::model::public::NetworkInterfaceType::Bridge)
                        && source_zone_matches_interface(source, info)
                })
                .any(|info| {
                    info.subnets
                        .iter()
                        .any(|subnet| subnet.contains(&source_ip))
                });
            ensure_code!(
                !bridge_listener && !bridge_source,
                ErrorKind::Network,
                "unregistered container DNS requester"
            );
        }
        let db = db.peek().await;
        let public = db.as_public();
        let network = public.as_server_info().as_network();
        let (service_gateway, dns) = match package {
            Some(id) => {
                let package = public
                    .as_package_data()
                    .as_idx(&id)
                    .or_not_found("DNS requester package")?;
                (
                    package.as_outbound_gateway().de()?,
                    package.as_outbound_dns().de()?,
                )
            }
            None => (None, network.as_outbound_dns().de()?),
        };
        let selected = service_gateway.or(network.as_default_outbound().de()?);
        let gateway = selected
            .or_else(|| auto_gateway(interfaces))
            .or_not_found("outbound DNS gateway")?;
        self.catalog_for_profile(gateway, dns, interfaces, &configured_dns_profiles(public)?)
    }

    fn catalog_for_profile(
        &self,
        gateway: GatewayId,
        servers: Option<VecDeque<SocketAddr>>,
        interfaces: &OrdMap<GatewayId, NetworkInterfaceInfo>,
        profiles: &BTreeSet<VecDeque<SocketAddr>>,
    ) -> Result<Arc<Catalog>, Error> {
        self.forwarders.mutate(|cache| {
            cache.retain(|(id, servers), forwarder| {
                interfaces.get(id).is_some_and(|info| {
                    info.ip_info
                        .as_ref()
                        .is_some_and(|ip| ip.scope_id == forwarder.index)
                        && (servers == &info.dns.servers || profiles.contains(servers))
                })
            });
            let info = interfaces.get(&gateway).or_not_found("DNS gateway")?;
            let ip = info
                .ip_info
                .as_ref()
                .or_not_found("connected DNS gateway")?;
            let servers = servers.as_ref().unwrap_or(&info.dns.servers);
            let key = (gateway.clone(), servers.clone());
            if let Some(forwarder) = cache.get(&key) {
                return Ok(forwarder.catalog.clone());
            }
            let catalog = gateway_catalog(gateway, ip.scope_id, servers)?;
            cache.insert(
                key,
                GatewayForwarder {
                    index: ip.scope_id,
                    catalog: catalog.clone(),
                },
            );
            Ok(catalog)
        })
    }

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
        let interfaces = self
            .authority
            .peek(|authority| authority.interfaces().read());
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
                        if let Some(res) = interfaces
                            .get(my_gateway)
                            .and_then(|info| info.ip_info.as_ref())
                            .map(|ip_info| {
                                let mut res = ip_info.subnets.iter().collect::<Vec<_>>();
                                res.sort_by_cached_key(|a| !a.contains(&src));
                                res.into_iter().map(|s| s.addr()).collect()
                            })
                        {
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
                // start-os.startos aliases the server's startos record.
                let pkg = pkg.filter(|p| !p.is_start_os());
                if let Some(ip) = r.services.get(&pkg) {
                    Resolution::Answer(
                        ip.iter()
                            .filter(|(_, registration)| registration.lifetime.strong_count() > 0)
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

fn response_header(request: &Request) -> Metadata {
    let mut header = Metadata::response_from_request(&request.metadata);
    header.recursion_available = true;
    header
}

fn address_records(name: &Name, record_type: RecordType, addresses: Vec<IpAddr>) -> Vec<Record> {
    use hickory_server::proto::rr::RData;

    addresses
        .into_iter()
        .filter_map(|ip| match (record_type, ip) {
            (RecordType::A, IpAddr::V4(ip)) => Some(RData::A(ip.into())),
            (RecordType::AAAA, IpAddr::V6(ip)) => Some(RData::AAAA(ip.into())),
            _ => None,
        })
        .map(|data| Record::from_rdata(name.clone(), 0, data))
        .collect()
}

async fn servfail<R: ResponseHandler>(request: &Request, mut response_handle: R) -> ResponseInfo {
    let mut header = response_header(request);
    header.response_code = ResponseCode::ServFail;
    response_handle
        .send_response(
            MessageResponseBuilder::from_message_request(&*request).build(header, [], [], [], []),
        )
        .await
        .unwrap_or_else(|_| {
            Header {
                metadata: header,
                counts: HeaderCounts::default(),
            }
            .into()
        })
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
            let mut header = response_header(request);

            match self.resolve(name, req.src.ip()) {
                Resolution::Forward => {
                    let catalog = match self.request_catalog(req.src, name).await {
                        Ok(catalog) => catalog,
                        Err(e) if matches!(e.kind, ErrorKind::NotFound | ErrorKind::Network) => {
                            tracing::debug!(error = ?e, "DNS forwarding unavailable");
                            return Ok(servfail(request, response_handle.clone()).await);
                        }
                        Err(e) => return Err(NetError::from(e.to_string())),
                    };
                    Ok(catalog
                        .handle_request::<R, T>(request, response_handle.clone())
                        .await)
                }
                Resolution::NxDomain => {
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
                }
                Resolution::Answer(addresses) => {
                    let records = address_records(name, query.query_type(), addresses);
                    response_handle
                        .send_response(
                            MessageResponseBuilder::from_message_request(&*request).build(
                                header,
                                &records,
                                [],
                                [],
                                [],
                            ),
                        )
                        .await
                }
            }
        }
        .await
        {
            Ok(response) => response,
            Err(e) => {
                tracing::error!(
                    "{}",
                    t!("net.dns.error-resolving-internal", error = e.to_string())
                );
                tracing::debug!("{e:?}");
                servfail(request, response_handle).await
            }
        }
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
    // Same as `build_listen_socket`: a GUA still tentative from DAD is not yet bindable.
    #[cfg(target_os = "linux")]
    match addr {
        SocketAddr::V4(_) => socket.set_freebind_v4(true),
        SocketAddr::V6(_) => socket.set_freebind_v6(true),
    }
    .with_kind(ErrorKind::Network)?;
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

fn retain_current_dns_listeners(
    servers: &mut BTreeMap<SocketAddr, (PrivateScope, Server<Resolver>)>,
    desired: &BTreeMap<SocketAddr, PrivateScope>,
) {
    servers.retain(|addr, (scope, _)| desired.get(addr) == Some(scope));
}

/// Serves gateway-address scopes and the host/container stubs.
async fn run_dns_servers(
    mut authority: Watch<DnsAuthority>,
    resolve: Arc<SyncRwLock<ResolveMap>>,
) -> Result<(), Error> {
    let forwarders = Arc::new(SyncRwLock::new(BTreeMap::new()));
    let resolver_authority = authority.clone();
    let mut net_iface = authority.peek(DnsAuthority::interfaces);
    let resolver = |scope: PrivateScope| Resolver {
        authority: resolver_authority.clone(),
        forwarders: forwarders.clone(),
        resolve: resolve.clone(),
        scope,
    };

    let loopback = vec![
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
    ];
    let host_ip = IpAddr::V4(Ipv4Addr::from(HOST_IP));

    let mut servers: BTreeMap<SocketAddr, (PrivateScope, Server<Resolver>)> = BTreeMap::new();
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

        retain_current_dns_listeners(&mut servers, &desired);
        for (addr, scope) in desired {
            if !servers.contains_key(&addr) {
                match dns_server_on(resolver(scope.clone()), addr) {
                    Ok(server) => {
                        servers.insert(addr, (scope, server));
                    }
                    Err(e) => {
                        tracing::error!("failed to bind DNS on {addr}: {e}");
                        tracing::debug!("{e:?}");
                    }
                }
            }
        }

        tokio::select! {
            _ = net_iface.changed() => {},
            _ = authority.changed() => { net_iface = authority.peek(DnsAuthority::interfaces); },
        }
    }
}

impl DnsController {
    #[instrument(skip_all)]
    pub fn new(interfaces: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>) -> Self {
        let resolve = Arc::new(SyncRwLock::new(ResolveMap::default()));
        let weak = Arc::downgrade(&resolve);
        let authority = Watch::new(DnsAuthority::Bootstrap(interfaces));
        let dns_server = tokio::spawn(run_dns_servers(authority.clone(), resolve).map(|r| {
            r.log_err();
        }))
        .into();
        Self {
            resolve: weak,
            authority,
            dns_server,
        }
    }

    pub(crate) fn activate(&self, network: Arc<NetworkInterfaceController>) {
        self.authority.send_replace(DnsAuthority::Runtime(network));
    }

    pub fn status(&self) -> DnsStatus {
        match self.authority.read() {
            DnsAuthority::Runtime(_) => DnsStatus {
                bootstrap: false,
                gateway: None,
            },
            DnsAuthority::Bootstrap(interfaces) => {
                let interfaces = interfaces.read();
                let gateway = underlay_gateway(&interfaces).map(|id| GatewayInfo {
                    info: interfaces.get(&id).unwrap().clone(),
                    id,
                });
                DnsStatus {
                    bootstrap: true,
                    gateway,
                }
            }
        }
    }

    pub fn add_service(
        &self,
        pkg_id: Option<PackageId>,
        ip: Ipv4Addr,
        ipv6: Option<Ipv6Addr>,
    ) -> Result<Arc<()>, Error> {
        if let Some(resolve) = Weak::upgrade(&self.resolve) {
            resolve.mutate(|writable| {
                let ips = writable.services.entry(pkg_id).or_default();
                let registration = ips.entry(ip).or_default();
                registration.ipv6 = ipv6;
                let rc = if let Some(rc) = registration.lifetime.upgrade() {
                    rc
                } else {
                    let new = Arc::new(());
                    registration.lifetime = Arc::downgrade(&new);
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
                if let Some(registration) = ips.remove(&ip) {
                    if registration.lifetime.strong_count() > 0 {
                        ips.insert(ip, registration);
                    }
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

#[cfg(test)]
mod tests {
    use futures::StreamExt;
    use hickory_server::proto::op::{Message, Query};
    use hickory_server::proto::rr::RData;

    use super::*;

    struct ResolverFixture {
        resolver: Resolver,
        db: TypedPatchDb<Database>,
        net_iface: Watch<OrdMap<GatewayId, NetworkInterfaceInfo>>,
    }

    impl std::ops::Deref for ResolverFixture {
        type Target = Resolver;
        fn deref(&self) -> &Resolver {
            &self.resolver
        }
    }

    impl std::ops::DerefMut for ResolverFixture {
        fn deref_mut(&mut self) -> &mut Resolver {
            &mut self.resolver
        }
    }

    impl ResolverFixture {
        fn controller(&self) -> DnsController {
            DnsController {
                resolve: Arc::downgrade(&self.resolve),
                authority: self.authority.clone(),
                dns_server: tokio::spawn(async {}).into(),
            }
        }

        async fn request_catalog(
            &self,
            source: IpAddr,
            name: &Name,
        ) -> Result<Arc<Catalog>, Error> {
            self.resolver
                .request_catalog(SocketAddr::new(source, 12345), name)
                .await
        }

        async fn forward_catalog(&self, source: IpAddr) -> Result<Arc<Catalog>, Error> {
            self.request_catalog(source, &Name::from_ascii("example.com.").unwrap())
                .await
        }
    }

    async fn exchange(resolver: &Resolver, message: Message, source: SocketAddr) -> Message {
        use hickory_server::net::BufDnsStreamHandle;
        use hickory_server::net::runtime::TokioTime;
        use hickory_server::net::xfer::Protocol;
        use hickory_server::server::ResponseHandle;

        let request =
            Request::from_bytes(message.to_vec().unwrap(), source, Protocol::Udp).unwrap();
        let (stream, mut responses) = BufDnsStreamHandle::new(source);
        resolver
            .handle_request::<_, TokioTime>(
                &request,
                ResponseHandle::new(source, stream, Protocol::Udp),
            )
            .await;
        Message::from_vec(responses.next().await.unwrap().bytes()).unwrap()
    }

    #[tokio::test]
    async fn private_dns_responses_preserve_addresses_nodata_and_nxdomain() {
        let (_temp, resolver) = resolver_fixture().await;
        let name = Name::from_ascii("localhost.").unwrap();
        for (record_type, expected) in [
            (
                RecordType::A,
                vec![Record::from_rdata(
                    name.clone(),
                    0,
                    RData::A(Ipv4Addr::LOCALHOST.into()),
                )],
            ),
            (
                RecordType::AAAA,
                vec![Record::from_rdata(
                    name.clone(),
                    0,
                    RData::AAAA(Ipv6Addr::LOCALHOST.into()),
                )],
            ),
            (RecordType::TXT, vec![]),
        ] {
            let mut message = Message::query();
            message.add_query(Query::query(name.clone(), record_type));
            let reply = exchange(&resolver, message, "127.0.0.1:12345".parse().unwrap()).await;
            assert_eq!(reply.metadata.response_code, ResponseCode::NoError);
            assert!(reply.metadata.recursion_available);
            assert_eq!(reply.answers, expected);
        }
        let mut message = Message::query();
        message.add_query(Query::query(
            Name::from_ascii("missing.startos.").unwrap(),
            RecordType::A,
        ));
        let reply = exchange(&resolver, message, "127.0.0.1:12345".parse().unwrap()).await;
        assert_eq!(reply.metadata.response_code, ResponseCode::NXDomain);
        assert!(reply.metadata.authoritative);
        assert!(reply.answers.is_empty());
    }

    #[tokio::test]
    async fn unavailable_gateway_servfails_without_error_log_but_malformed_requests_log() {
        use tracing::instrument::WithSubscriber;

        struct Logs(Arc<std::sync::Mutex<Vec<u8>>>);
        impl std::io::Write for Logs {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let (_temp, resolver) = resolver_fixture().await;
        let controller = resolver.controller();
        let _pinned = controller
            .add_service(
                Some("pinned".parse().unwrap()),
                "10.0.3.2".parse().unwrap(),
                None,
            )
            .unwrap();
        resolver.net_iface.send_if_modified(|interfaces| {
            interfaces
                .get_mut(&"wg0".parse::<GatewayId>().unwrap())
                .unwrap()
                .ip_info = None;
            true
        });
        let logs = Arc::new(std::sync::Mutex::new(Vec::new()));
        let writer = logs.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::ERROR)
            .without_time()
            .with_ansi(false)
            .with_writer(move || Logs(writer.clone()))
            .finish();
        let dispatch = tracing::Dispatch::new(subscriber);
        let mut message = Message::query();
        message.add_query(Query::query(
            Name::from_ascii("example.com.").unwrap(),
            RecordType::A,
        ));
        let source = "10.0.3.2:12345".parse().unwrap();
        let reply = exchange(&resolver, message, source)
            .with_subscriber(dispatch.clone())
            .await;
        assert_eq!(reply.metadata.response_code, ResponseCode::ServFail);
        assert!(logs.lock().unwrap().is_empty());
        let reply = exchange(&resolver, Message::query(), source)
            .with_subscriber(dispatch)
            .await;
        assert_eq!(reply.metadata.response_code, ResponseCode::ServFail);
        assert!(!logs.lock().unwrap().is_empty());
    }

    async fn resolver_fixture() -> (tempfile::TempDir, ResolverFixture) {
        let temp = tempfile::tempdir().unwrap();
        let db = TypedPatchDb::<Database>::load_unchecked(
            patch_db::PatchDb::open(temp.path().join("db"))
                .await
                .unwrap(),
        );
        db.put(&patch_db::json_ptr::ROOT, &imbl_value::json!({
            "public": {
                "serverInfo": { "network": { "defaultOutbound": "eth0" } },
                "packageData": { "pinned": { "outboundGateway": "wg0" }, "inherited": { "outboundGateway": null } }
            }
        })).await.unwrap();
        let ifaces = [("eth0", 2), ("wg0", 3)]
            .into_iter()
            .map(|(id, index)| {
                (
                    id.parse::<GatewayId>().unwrap(),
                    NetworkInterfaceInfo {
                        ip_info: Some(Arc::new(crate::db::model::public::IpInfo {
                            scope_id: index,
                            default_route_v4: (id == "eth0").then_some(100),
                            ..Default::default()
                        })),
                        ..Default::default()
                    },
                )
            })
            .collect();
        let net_iface = Watch::new(ifaces);
        let network = Arc::new(NetworkInterfaceController::with_test_interfaces(
            db.clone(),
            net_iface.clone(),
        ));
        let resolver = Resolver {
            authority: Watch::new(DnsAuthority::Runtime(network)),
            forwarders: Arc::new(SyncRwLock::new(BTreeMap::new())),
            resolve: Arc::new(SyncRwLock::new(ResolveMap::default())),
            scope: PrivateScope::Local(vec![]),
        };
        (
            temp,
            ResolverFixture {
                resolver,
                db,
                net_iface,
            },
        )
    }

    #[tokio::test]
    async fn listeners_retire_when_an_address_changes_private_gateway_scope() {
        let (_temp, fixture) = resolver_fixture().await;
        let local =
            PrivateScope::Local(vec![Ipv4Addr::LOCALHOST.into(), Ipv6Addr::LOCALHOST.into()]);
        let ethernet = PrivateScope::Gateway("eth0".parse().unwrap());
        let mut servers = [
            ("127.0.0.1:53", local.clone()),
            ("192.0.2.2:53", ethernet.clone()),
            ("192.0.2.3:53", ethernet),
        ]
        .into_iter()
        .map(|(address, scope)| {
            let resolver = Resolver {
                authority: fixture.authority.clone(),
                forwarders: fixture.forwarders.clone(),
                resolve: fixture.resolve.clone(),
                scope: scope.clone(),
            };
            (
                address.parse::<SocketAddr>().unwrap(),
                (scope, Server::new(resolver)),
            )
        })
        .collect();
        let desired = [
            ("127.0.0.1:53".parse().unwrap(), local),
            (
                "192.0.2.2:53".parse().unwrap(),
                PrivateScope::Gateway("eth1".parse().unwrap()),
            ),
        ]
        .into_iter()
        .collect();
        retain_current_dns_listeners(&mut servers, &desired);
        assert_eq!(servers.len(), 1);
        assert!(servers.contains_key(&"127.0.0.1:53".parse().unwrap()));
    }

    #[tokio::test]
    async fn process_dns_starts_in_bootstrap_and_survives_context_handles() {
        let dns = Arc::new(DnsController::new(Watch::new(OrdMap::new())));
        let owner = Arc::downgrade(&dns);
        let setup = dns.clone();
        let diagnostic = dns.clone();
        assert!(dns.status().bootstrap);
        assert!(dns.status().gateway.is_none());
        drop(dns);
        drop(setup);
        assert!(diagnostic.status().bootstrap);
        assert!(owner.upgrade().is_some());
        drop(diagnostic);
        assert!(owner.upgrade().is_none());
        tokio::task::yield_now().await;
    }

    #[tokio::test]
    async fn bootstrap_dns_is_host_only_and_reports_the_actual_underlay_source() {
        let (_temp, mut fixture) = resolver_fixture().await;
        fixture.resolver.authority = Watch::new(DnsAuthority::Bootstrap(fixture.net_iface.clone()));
        fixture.net_iface.send_if_modified(|interfaces| {
            let tunnel = interfaces
                .get_mut(&"wg0".parse::<GatewayId>().unwrap())
                .unwrap();
            let ip = Arc::make_mut(tunnel.ip_info.as_mut().unwrap());
            ip.device_type = Some(crate::db::model::public::NetworkInterfaceType::Wireguard);
            ip.default_route_v4 = Some(1);
            let ethernet = interfaces
                .get_mut(&"eth0".parse::<GatewayId>().unwrap())
                .unwrap();
            Arc::make_mut(ethernet.ip_info.as_mut().unwrap()).dns_servers =
                ["192.0.2.53".parse::<IpAddr>().unwrap()]
                    .into_iter()
                    .collect();
            ethernet.dns.refresh(ethernet.ip_info.as_deref());
            true
        });
        let controller = fixture.controller();
        let status = controller.status();
        assert!(status.bootstrap);
        let gateway = status.gateway.unwrap();
        assert_eq!(gateway.id.as_str(), "eth0");
        assert_eq!(
            gateway.info.dns.source,
            crate::db::model::public::DnsSource::Gateway
        );
        assert_eq!(gateway.info.dns.servers, ["192.0.2.53:53".parse().unwrap()]);
        let provided = fixture
            .forward_catalog("127.0.0.1".parse().unwrap())
            .await
            .unwrap();
        assert!(Arc::ptr_eq(
            &provided,
            &fixture
                .forward_catalog("::1".parse().unwrap())
                .await
                .unwrap()
        ));
        for source in ["10.0.3.2", "fd00:3::2", "192.0.2.100"] {
            assert!(
                fixture
                    .forward_catalog(source.parse().unwrap())
                    .await
                    .is_err()
            );
        }
        fixture.net_iface.send_if_modified(|interfaces| {
            let ethernet = interfaces
                .get_mut(&"eth0".parse::<GatewayId>().unwrap())
                .unwrap();
            Arc::make_mut(ethernet.ip_info.as_mut().unwrap())
                .dns_servers
                .clear();
            ethernet.dns.refresh(ethernet.ip_info.as_deref());
            true
        });
        let public = fixture
            .forward_catalog("127.0.0.1".parse().unwrap())
            .await
            .unwrap();
        assert!(!Arc::ptr_eq(&provided, &public));
        let dns = controller.status().gateway.unwrap().info.dns;
        assert_eq!(
            dns.source,
            crate::db::model::public::DnsSource::PublicFallback
        );
        assert_eq!(
            dns.servers,
            [
                "1.1.1.1:53".parse().unwrap(),
                "[2606:4700:4700::1111]:53".parse().unwrap()
            ]
        );
        fixture.net_iface.send_if_modified(|interfaces| {
            interfaces
                .remove(&"eth0".parse::<GatewayId>().unwrap())
                .is_some()
        });
        assert!(
            fixture
                .forward_catalog("127.0.0.1".parse().unwrap())
                .await
                .is_err()
        );
        assert!(controller.status().gateway.is_none());
    }

    #[tokio::test]
    async fn link_local_clients_cannot_impersonate_host_sources_on_another_interface() {
        let (_temp, mut fixture) = resolver_fixture().await;
        fixture
            .db
            .put(
                &"/public/serverInfo/network/defaultOutbound"
                    .parse::<patch_db::json_ptr::JsonPointer>()
                    .unwrap(),
                &"wg0",
            )
            .await
            .unwrap();
        fixture.net_iface.send_if_modified(|interfaces| {
            let ethernet = interfaces
                .get_mut(&"eth0".parse::<GatewayId>().unwrap())
                .unwrap();
            Arc::make_mut(ethernet.ip_info.as_mut().unwrap()).subnets =
                ["fe80::1/64".parse::<ipnet::IpNet>().unwrap()]
                    .into_iter()
                    .collect();
            let tunnel = interfaces
                .get_mut(&"wg0".parse::<GatewayId>().unwrap())
                .unwrap();
            tunnel
                .wireguard_endpoint_domains
                .insert("vpn.example".into());
            tunnel.ip_info = None;
            interfaces.insert(
                "lxcbr0".parse().unwrap(),
                NetworkInterfaceInfo {
                    ip_info: Some(Arc::new(crate::db::model::public::IpInfo {
                        scope_id: 5,
                        device_type: Some(crate::db::model::public::NetworkInterfaceType::Bridge),
                        subnets: ["fe80::10/64".parse::<ipnet::IpNet>().unwrap()]
                            .into_iter()
                            .collect(),
                        ..Default::default()
                    })),
                    ..Default::default()
                },
            );
            true
        });
        let DnsAuthority::Runtime(network) = fixture.authority.read() else {
            panic!("runtime fixture")
        };
        fixture.resolver.authority = Watch::new(DnsAuthority::Bootstrap(fixture.net_iface.clone()));
        let controller = fixture.controller();
        let host = "[fe80::1%2]:12345".parse().unwrap();
        let other_link = "[fe80::1%4]:12345".parse().unwrap();
        let endpoint = Name::from_ascii("vpn.example.").unwrap();
        assert!(
            fixture
                .resolver
                .request_catalog(host, &endpoint)
                .await
                .is_ok()
        );
        assert!(
            fixture
                .resolver
                .request_catalog(other_link, &endpoint)
                .await
                .is_err()
        );
        controller.activate(network);
        let host_catalog = fixture
            .resolver
            .request_catalog(host, &endpoint)
            .await
            .unwrap();
        assert!(
            fixture
                .resolver
                .request_catalog(other_link, &endpoint)
                .await
                .is_err()
        );
        fixture.net_iface.send_if_modified(|interfaces| {
            interfaces
                .get_mut(&"wg0".parse::<GatewayId>().unwrap())
                .unwrap()
                .ip_info = Some(Arc::new(crate::db::model::public::IpInfo {
                scope_id: 3,
                ..Default::default()
            }));
            true
        });
        let client_catalog = fixture
            .resolver
            .request_catalog(other_link, &endpoint)
            .await
            .unwrap();
        assert!(!Arc::ptr_eq(&host_catalog, &client_catalog));
        assert!(Arc::ptr_eq(
            &client_catalog,
            &fixture
                .resolver
                .request_catalog(other_link, &Name::from_ascii("example.com.").unwrap())
                .await
                .unwrap()
        ));
    }

    #[tokio::test]
    async fn policy_adoption_replaces_bootstrap_observations_and_survives_init_failure() {
        let (_temp, mut fixture) = resolver_fixture().await;
        fixture.resolver.authority = Watch::new(DnsAuthority::Bootstrap(fixture.net_iface.clone()));
        let controller = fixture.controller();
        let bootstrap = fixture
            .forward_catalog("127.0.0.1".parse().unwrap())
            .await
            .unwrap();
        fixture
            .db
            .put(
                &"/public/serverInfo/network/defaultOutbound"
                    .parse::<patch_db::json_ptr::JsonPointer>()
                    .unwrap(),
                &"wg0",
            )
            .await
            .unwrap();
        let runtime_interfaces = Watch::new(fixture.net_iface.read());
        runtime_interfaces.send_if_modified(|interfaces| {
            interfaces
                .get_mut(&"wg0".parse::<GatewayId>().unwrap())
                .unwrap()
                .ip_info = None;
            true
        });
        let network = Arc::new(NetworkInterfaceController::with_test_interfaces(
            fixture.db.clone(),
            runtime_interfaces.clone(),
        ));
        let network_owner = Arc::downgrade(&network);
        controller.activate(network.clone());
        drop(network);
        assert!(network_owner.upgrade().is_some());
        assert!(!controller.status().bootstrap);
        assert!(controller.status().gateway.is_none());
        assert!(
            fixture
                .forward_catalog("127.0.0.1".parse().unwrap())
                .await
                .is_err()
        );
        fixture.net_iface.send_if_modified(|interfaces| {
            interfaces.clear();
            true
        });
        runtime_interfaces.send_if_modified(|interfaces| {
            let info = interfaces
                .get_mut(&"wg0".parse::<GatewayId>().unwrap())
                .unwrap();
            info.ip_info = Some(Arc::new(crate::db::model::public::IpInfo {
                scope_id: 9,
                ..Default::default()
            }));
            info.dns.static_servers = Some(["192.0.2.54:53".parse().unwrap()].into());
            info.dns.refresh(info.ip_info.as_deref());
            true
        });
        let runtime = fixture
            .forward_catalog("127.0.0.1".parse().unwrap())
            .await
            .unwrap();
        assert!(!Arc::ptr_eq(&bootstrap, &runtime));
        assert!(
            runtime_interfaces
                .read()
                .contains_key(&"wg0".parse::<GatewayId>().unwrap())
        );
        drop(controller);
        assert!(network_owner.upgrade().is_some());
        drop(fixture);
        assert!(network_owner.upgrade().is_none());
    }

    #[tokio::test]
    async fn dns_uses_requester_gateway_and_keeps_caches_separate() {
        let (_temp, resolver) = resolver_fixture().await;
        let controller = resolver.controller();
        let pinned = controller
            .add_service(
                Some("pinned".parse().unwrap()),
                "10.0.3.2".parse().unwrap(),
                Some("fd00:3::2".parse().unwrap()),
            )
            .unwrap();
        let _inherited = controller
            .add_service(
                Some("inherited".parse().unwrap()),
                "10.0.3.3".parse().unwrap(),
                None,
            )
            .unwrap();
        let system = resolver
            .forward_catalog("127.0.0.1".parse().unwrap())
            .await
            .unwrap();
        let service = resolver
            .forward_catalog("10.0.3.2".parse().unwrap())
            .await
            .unwrap();
        assert!(!Arc::ptr_eq(&system, &service));
        assert!(Arc::ptr_eq(
            &service,
            &resolver
                .forward_catalog("fd00:3::2".parse().unwrap())
                .await
                .unwrap()
        ));
        assert!(Arc::ptr_eq(
            &service,
            &resolver
                .forward_catalog("::ffff:10.0.3.2".parse().unwrap())
                .await
                .unwrap()
        ));
        assert!(Arc::ptr_eq(
            &system,
            &resolver
                .forward_catalog("10.0.3.3".parse().unwrap())
                .await
                .unwrap()
        ));
        drop(pinned);
        for source in ["10.0.3.2", "fd00:3::2"] {
            assert!(
                resolver
                    .forward_catalog(source.parse().unwrap())
                    .await
                    .is_err()
            );
        }
        let _reused = controller
            .add_service(
                Some("inherited".parse().unwrap()),
                "10.0.3.2".parse().unwrap(),
                Some("fd00:3::2".parse().unwrap()),
            )
            .unwrap();
        assert!(Arc::ptr_eq(
            &system,
            &resolver
                .forward_catalog("10.0.3.2".parse().unwrap())
                .await
                .unwrap()
        ));
        resolver
            .db
            .put(
                &"/public/packageData"
                    .parse::<patch_db::json_ptr::JsonPointer>()
                    .unwrap(),
                &imbl_value::json!({}),
            )
            .await
            .unwrap();
        assert!(
            resolver
                .forward_catalog("10.0.3.2".parse().unwrap())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn unregistered_container_dns_never_uses_system_default() {
        let (_temp, mut resolver) = resolver_fixture().await;
        resolver.scope = PrivateScope::Local(vec![IpAddr::V4(HOST_IP.into())]);
        assert!(
            resolver
                .forward_catalog("10.0.3.99".parse().unwrap())
                .await
                .is_err()
        );
        assert!(
            resolver
                .forward_catalog(IpAddr::V4(HOST_IP.into()))
                .await
                .is_ok()
        );
        resolver.scope = PrivateScope::Gateway("eth0".parse().unwrap());
        resolver.net_iface.send_if_modified(|interfaces| {
            interfaces.insert(
                "lxcbr0".parse::<GatewayId>().unwrap(),
                NetworkInterfaceInfo {
                    ip_info: Some(Arc::new(crate::db::model::public::IpInfo {
                        device_type: Some(crate::db::model::public::NetworkInterfaceType::Bridge),
                        subnets: ["10.0.3.1/24", "fd00:3::1/64"]
                            .into_iter()
                            .map(|s| s.parse::<ipnet::IpNet>().unwrap())
                            .collect(),
                        ..Default::default()
                    })),
                    ..Default::default()
                },
            );
            true
        });
        for source in ["10.0.3.99", "fd00:3::99", "::ffff:10.0.3.99"] {
            assert!(
                resolver
                    .forward_catalog(source.parse().unwrap())
                    .await
                    .is_err()
            );
        }
        assert!(
            resolver
                .forward_catalog("192.0.2.100".parse().unwrap())
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn endpoint_bootstrap_is_exact_and_host_only() {
        let (_temp, resolver) = resolver_fixture().await;
        let controller = resolver.controller();
        let _pinned = controller
            .add_service(
                Some("pinned".parse().unwrap()),
                "10.0.3.2".parse().unwrap(),
                Some("fd00:3::2".parse().unwrap()),
            )
            .unwrap();
        let underlay = resolver
            .forward_catalog("127.0.0.1".parse().unwrap())
            .await
            .unwrap();
        let tunnel = resolver
            .forward_catalog("10.0.3.2".parse().unwrap())
            .await
            .unwrap();
        resolver.net_iface.send_if_modified(|interfaces| {
            let info = interfaces
                .get_mut(&"wg0".parse::<GatewayId>().unwrap())
                .unwrap();
            info.wireguard_endpoint_domains.insert("vpn.example".into());
            let mut ip = info.ip_info.as_deref().unwrap().clone();
            ip.device_type = Some(crate::db::model::public::NetworkInterfaceType::Wireguard);
            info.ip_info = Some(Arc::new(ip));
            true
        });
        resolver
            .db
            .put(
                &"/public/serverInfo/network/defaultOutbound"
                    .parse::<patch_db::json_ptr::JsonPointer>()
                    .unwrap(),
                &"wg0",
            )
            .await
            .unwrap();
        let endpoint = Name::from_ascii("VPN.Example.").unwrap();
        for source in ["127.0.0.1", "::1"] {
            assert!(Arc::ptr_eq(
                &underlay,
                &resolver
                    .request_catalog(source.parse().unwrap(), &endpoint)
                    .await
                    .unwrap()
            ));
        }
        for source in ["10.0.3.2", "fd00:3::2", "192.0.2.100"] {
            assert!(Arc::ptr_eq(
                &tunnel,
                &resolver
                    .request_catalog(source.parse().unwrap(), &endpoint)
                    .await
                    .unwrap()
            ));
        }
        for domain in ["other.example.", "other.vpn.example."] {
            assert!(Arc::ptr_eq(
                &tunnel,
                &resolver
                    .request_catalog(
                        "127.0.0.1".parse().unwrap(),
                        &Name::from_ascii(domain).unwrap()
                    )
                    .await
                    .unwrap()
            ));
        }
        resolver.net_iface.send_if_modified(|interfaces| {
            interfaces
                .remove(&"eth0".parse::<GatewayId>().unwrap())
                .is_some()
        });
        assert!(
            resolver
                .request_catalog("127.0.0.1".parse().unwrap(), &endpoint)
                .await
                .is_err()
        );
        assert!(
            resolver
                .request_catalog("10.0.3.2".parse().unwrap(), &endpoint)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn dns_reconciles_gateway_changes_without_fallback() {
        let (_temp, resolver) = resolver_fixture().await;
        let controller = resolver.controller();
        let _pinned = controller
            .add_service(
                Some("pinned".parse().unwrap()),
                "10.0.3.2".parse().unwrap(),
                None,
            )
            .unwrap();
        let before = resolver
            .forward_catalog("10.0.3.2".parse().unwrap())
            .await
            .unwrap();
        resolver.net_iface.send_if_modified(|interfaces| {
            let info = interfaces
                .get_mut(&"wg0".parse::<GatewayId>().unwrap())
                .unwrap();
            info.dns.static_servers = Some(["192.0.2.53:5353".parse().unwrap()].into());
            info.dns.refresh(info.ip_info.as_deref());
            true
        });
        let after = resolver
            .forward_catalog("10.0.3.2".parse().unwrap())
            .await
            .unwrap();
        assert!(!Arc::ptr_eq(&before, &after));
        resolver.net_iface.send_if_modified(|interfaces| {
            interfaces
                .get_mut(&"wg0".parse::<GatewayId>().unwrap())
                .unwrap()
                .ip_info = None;
            true
        });
        assert!(
            resolver
                .forward_catalog("10.0.3.2".parse().unwrap())
                .await
                .is_err()
        );
        assert!(
            resolver
                .forward_catalog("127.0.0.1".parse().unwrap())
                .await
                .is_ok()
        );
        resolver
            .db
            .put(
                &"/public/serverInfo/network/defaultOutbound"
                    .parse::<patch_db::json_ptr::JsonPointer>()
                    .unwrap(),
                &"wg0",
            )
            .await
            .unwrap();
        assert!(
            resolver
                .forward_catalog("127.0.0.1".parse().unwrap())
                .await
                .is_err()
        );
        resolver
            .db
            .put(
                &"/public/serverInfo/network/defaultOutbound"
                    .parse::<patch_db::json_ptr::JsonPointer>()
                    .unwrap(),
                &Option::<GatewayId>::None,
            )
            .await
            .unwrap();
        assert!(
            resolver
                .forward_catalog("127.0.0.1".parse().unwrap())
                .await
                .is_ok()
        );
        assert!(matches!(
            resolver.resolve(
                &Name::from_ascii("missing.startos.").unwrap(),
                "10.0.3.2".parse().unwrap()
            ),
            Resolution::NxDomain
        ));
    }

    #[tokio::test]
    async fn requester_dns_profiles_are_independent_and_cached_side_by_side() {
        let (_temp, resolver) = resolver_fixture().await;
        let controller = resolver.controller();
        let _pinned = controller
            .add_service(
                Some("pinned".parse().unwrap()),
                "10.0.3.2".parse().unwrap(),
                Some("fd00:3::2".parse().unwrap()),
            )
            .unwrap();
        let _inherited = controller
            .add_service(
                Some("inherited".parse().unwrap()),
                "10.0.3.3".parse().unwrap(),
                None,
            )
            .unwrap();
        resolver
            .db
            .put(
                &"/public/serverInfo/network/outboundDns"
                    .parse::<patch_db::json_ptr::JsonPointer>()
                    .unwrap(),
                &imbl_value::json!(["192.0.2.54:5353", "[2001:db8::54]:53"]),
            )
            .await
            .unwrap();
        let host = resolver
            .forward_catalog("127.0.0.1".parse().unwrap())
            .await
            .unwrap();
        let eth0: GatewayId = "eth0".parse().unwrap();
        let host_servers: std::collections::VecDeque<SocketAddr> = [
            "192.0.2.54:5353".parse().unwrap(),
            "[2001:db8::54]:53".parse().unwrap(),
        ]
        .into();
        let gateway_servers = resolver.net_iface.read()[&eth0].dns.servers.clone();
        let cached = |servers: &[&std::collections::VecDeque<SocketAddr>]| {
            let expected: BTreeSet<_> = servers
                .iter()
                .map(|servers| (eth0.clone(), (*servers).clone()))
                .collect();
            assert_eq!(
                resolver
                    .forwarders
                    .peek(|c| c.keys().cloned().collect::<BTreeSet<_>>()),
                expected
            );
        };
        cached(&[&host_servers]);
        let inherited = resolver
            .forward_catalog("10.0.3.3".parse().unwrap())
            .await
            .unwrap();
        assert!(!Arc::ptr_eq(&host, &inherited));
        cached(&[&host_servers, &gateway_servers]);
        assert!(Arc::ptr_eq(
            &host,
            &resolver
                .forward_catalog("127.0.0.1".parse().unwrap())
                .await
                .unwrap()
        ));
        assert!(Arc::ptr_eq(
            &inherited,
            &resolver
                .forward_catalog("10.0.3.3".parse().unwrap())
                .await
                .unwrap()
        ));
        resolver
            .db
            .put(
                &"/public/packageData/pinned"
                    .parse::<patch_db::json_ptr::JsonPointer>()
                    .unwrap(),
                &imbl_value::json!({"outboundGateway": null, "outboundDns": ["192.0.2.55:53"]}),
            )
            .await
            .unwrap();
        let service = resolver
            .forward_catalog("10.0.3.2".parse().unwrap())
            .await
            .unwrap();
        assert!(!Arc::ptr_eq(&inherited, &service));
        let service_servers: std::collections::VecDeque<SocketAddr> =
            ["192.0.2.55:53".parse().unwrap()].into();
        cached(&[&host_servers, &gateway_servers, &service_servers]);
        assert!(Arc::ptr_eq(
            &service,
            &resolver
                .forward_catalog("fd00:3::2".parse().unwrap())
                .await
                .unwrap()
        ));
        let inherited_again = resolver
            .forward_catalog("10.0.3.3".parse().unwrap())
            .await
            .unwrap();
        assert!(Arc::ptr_eq(&inherited, &inherited_again));
        resolver
            .db
            .put(
                &"/public/packageData/pinned/outboundDns"
                    .parse::<patch_db::json_ptr::JsonPointer>()
                    .unwrap(),
                &host_servers,
            )
            .await
            .unwrap();
        assert!(Arc::ptr_eq(
            &host,
            &resolver
                .forward_catalog("10.0.3.2".parse().unwrap())
                .await
                .unwrap()
        ));
        cached(&[&host_servers, &gateway_servers]);
        resolver
            .db
            .put(
                &"/public/packageData/pinned/outboundDns"
                    .parse::<patch_db::json_ptr::JsonPointer>()
                    .unwrap(),
                &Value::Null,
            )
            .await
            .unwrap();
        assert!(Arc::ptr_eq(
            &inherited_again,
            &resolver
                .forward_catalog("10.0.3.2".parse().unwrap())
                .await
                .unwrap()
        ));
        resolver
            .db
            .put(
                &"/public/packageData/inherited/outboundDns"
                    .parse::<patch_db::json_ptr::JsonPointer>()
                    .unwrap(),
                &imbl_value::json!(["192.0.2.56:53"]),
            )
            .await
            .unwrap();
        assert!(!Arc::ptr_eq(
            &inherited_again,
            &resolver
                .forward_catalog("10.0.3.3".parse().unwrap())
                .await
                .unwrap()
        ));
        let inherited_servers: std::collections::VecDeque<SocketAddr> =
            ["192.0.2.56:53".parse().unwrap()].into();
        cached(&[&host_servers, &gateway_servers, &inherited_servers]);
        resolver.net_iface.send_if_modified(|interfaces| {
            interfaces.get_mut(&eth0).unwrap().ip_info = None;
            true
        });
        for source in ["127.0.0.1", "10.0.3.2", "10.0.3.3"] {
            assert!(
                resolver
                    .forward_catalog(source.parse().unwrap())
                    .await
                    .is_err()
            );
        }
        assert_eq!(resolver.forwarders.peek(|c| c.len()), 0);
        assert!(matches!(
            resolver.resolve(
                &Name::from_ascii("localhost.").unwrap(),
                "10.0.3.3".parse().unwrap()
            ),
            Resolution::Answer(_)
        ));
    }

    #[tokio::test]
    async fn obsolete_dns_profiles_are_pruned_on_policy_and_interface_changes() {
        let (_temp, resolver) = resolver_fixture().await;
        let controller = resolver.controller();
        let _inherited = controller
            .add_service(
                Some("inherited".parse().unwrap()),
                "10.0.3.3".parse().unwrap(),
                None,
            )
            .unwrap();
        let mut gateway = resolver
            .forward_catalog("10.0.3.3".parse().unwrap())
            .await
            .unwrap();
        for server in ["192.0.2.54:53", "192.0.2.55:53", "192.0.2.56:53"] {
            resolver
                .db
                .put(
                    &"/public/serverInfo/network/outboundDns"
                        .parse::<patch_db::json_ptr::JsonPointer>()
                        .unwrap(),
                    &imbl_value::json!([server]),
                )
                .await
                .unwrap();
            let host = resolver
                .forward_catalog("127.0.0.1".parse().unwrap())
                .await
                .unwrap();
            assert!(!Arc::ptr_eq(&gateway, &host));
            assert!(Arc::ptr_eq(
                &gateway,
                &resolver
                    .forward_catalog("10.0.3.3".parse().unwrap())
                    .await
                    .unwrap()
            ));
            assert_eq!(resolver.forwarders.peek(|c| c.len()), 2);
        }
        resolver
            .db
            .put(
                &"/public/packageData/inherited/outboundDns"
                    .parse::<patch_db::json_ptr::JsonPointer>()
                    .unwrap(),
                &imbl_value::json!(["192.0.2.57:53"]),
            )
            .await
            .unwrap();
        resolver
            .forward_catalog("10.0.3.3".parse().unwrap())
            .await
            .unwrap();
        assert_eq!(resolver.forwarders.peek(|c| c.len()), 3);
        resolver
            .db
            .put(
                &"/public/packageData"
                    .parse::<patch_db::json_ptr::JsonPointer>()
                    .unwrap(),
                &imbl_value::json!({}),
            )
            .await
            .unwrap();
        resolver
            .forward_catalog("127.0.0.1".parse().unwrap())
            .await
            .unwrap();
        assert_eq!(resolver.forwarders.peek(|c| c.len()), 2);
        resolver
            .db
            .put(
                &"/public/serverInfo/network/outboundDns"
                    .parse::<patch_db::json_ptr::JsonPointer>()
                    .unwrap(),
                &Value::Null,
            )
            .await
            .unwrap();
        assert!(Arc::ptr_eq(
            &gateway,
            &resolver
                .forward_catalog("127.0.0.1".parse().unwrap())
                .await
                .unwrap()
        ));
        assert_eq!(resolver.forwarders.peek(|c| c.len()), 1);
        let eth0: GatewayId = "eth0".parse().unwrap();
        for server in ["192.0.2.60:53", "192.0.2.61:53"] {
            resolver.net_iface.send_if_modified(|interfaces| {
                let info = interfaces.get_mut(&eth0).unwrap();
                info.dns.static_servers = Some([server.parse().unwrap()].into());
                info.dns.refresh(info.ip_info.as_deref());
                true
            });
            let updated = resolver
                .forward_catalog("127.0.0.1".parse().unwrap())
                .await
                .unwrap();
            assert!(!Arc::ptr_eq(&gateway, &updated));
            assert_eq!(resolver.forwarders.peek(|c| c.len()), 1);
            gateway = updated;
        }
        resolver.net_iface.send_if_modified(|interfaces| {
            Arc::make_mut(interfaces.get_mut(&eth0).unwrap().ip_info.as_mut().unwrap()).scope_id +=
                1;
            true
        });
        let recreated = resolver
            .forward_catalog("127.0.0.1".parse().unwrap())
            .await
            .unwrap();
        assert!(!Arc::ptr_eq(&gateway, &recreated));
        assert_eq!(resolver.forwarders.peek(|c| c.len()), 1);
        resolver
            .db
            .put(
                &"/public/serverInfo/network/defaultOutbound"
                    .parse::<patch_db::json_ptr::JsonPointer>()
                    .unwrap(),
                &eth0,
            )
            .await
            .unwrap();
        resolver
            .net_iface
            .send_if_modified(|interfaces| interfaces.remove(&eth0).is_some());
        assert!(
            resolver
                .forward_catalog("127.0.0.1".parse().unwrap())
                .await
                .is_err()
        );
        assert_eq!(resolver.forwarders.peek(|c| c.len()), 0);
    }

    #[tokio::test]
    async fn host_static_dns_never_changes_endpoint_or_initial_bootstrap_upstreams() {
        let (_temp, mut resolver) = resolver_fixture().await;
        let underlay = resolver
            .forward_catalog("127.0.0.1".parse().unwrap())
            .await
            .unwrap();
        resolver
            .db
            .put(
                &"/public/serverInfo/network"
                    .parse::<patch_db::json_ptr::JsonPointer>()
                    .unwrap(),
                &imbl_value::json!({"defaultOutbound": "wg0", "outboundDns": ["192.0.2.54:53"]}),
            )
            .await
            .unwrap();
        resolver.net_iface.send_if_modified(|interfaces| {
            interfaces
                .get_mut(&"wg0".parse::<GatewayId>().unwrap())
                .unwrap()
                .wireguard_endpoint_domains
                .insert("vpn.example".into());
            true
        });
        let host_static = resolver
            .forward_catalog("127.0.0.1".parse().unwrap())
            .await
            .unwrap();
        let endpoint = resolver
            .request_catalog(
                "127.0.0.1".parse().unwrap(),
                &Name::from_ascii("vpn.example.").unwrap(),
            )
            .await
            .unwrap();
        assert!(Arc::ptr_eq(&underlay, &endpoint));
        assert!(!Arc::ptr_eq(&host_static, &endpoint));
        resolver.resolver.authority =
            Watch::new(DnsAuthority::Bootstrap(resolver.net_iface.clone()));
        assert!(Arc::ptr_eq(
            &endpoint,
            &resolver
                .forward_catalog("127.0.0.1".parse().unwrap())
                .await
                .unwrap()
        ));
    }

    #[test]
    fn static_dns_cli_requires_gateway_and_accepts_multiple_servers() {
        let params = SetStaticDnsParams::try_parse_from([
            "set-static",
            "eth0",
            "192.0.2.53",
            "2001:db8::53",
        ])
        .unwrap();
        assert_eq!(params.gateway.as_str(), "eth0");
        assert_eq!(params.servers.unwrap(), ["192.0.2.53", "2001:db8::53"]);
        assert!(SetStaticDnsParams::try_parse_from(["set-static"]).is_err());
        assert_eq!(
            SetStaticDnsParams::try_parse_from(["set-static", "eth0"])
                .unwrap()
                .servers,
            None
        );
    }

    #[test]
    fn static_dns_accepts_addresses_and_ports() {
        let servers = parse_static_servers(Some(vec![
            "192.0.2.53".into(),
            "[2001:db8::53]:5353".into(),
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(
            servers.into_iter().collect::<Vec<_>>(),
            vec![
                "192.0.2.53:53".parse().unwrap(),
                "[2001:db8::53]:5353".parse().unwrap()
            ]
        );
        assert_eq!(parse_static_servers(None).unwrap(), None);
    }

    #[test]
    fn static_dns_rejects_empty_self_and_zero_port() {
        assert!(parse_static_servers(Some(vec![])).is_err());
        for server in [
            "127.0.0.1",
            "::1",
            "::ffff:127.0.0.1",
            "0.0.0.0",
            "::",
            "192.0.2.53:0",
        ] {
            assert!(
                parse_static_servers(Some(vec![server.into()])).is_err(),
                "{server}"
            );
        }
    }
}
