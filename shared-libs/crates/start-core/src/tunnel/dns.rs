use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;

use hickory_server::proto::rr::Name;
use hickory_server::resolver as hickory_resolver;
use hickory_server::resolver::config::{NameServerConfig, ResolverConfig, ResolverOpts};
use hickory_server::server::Server;
use hickory_server::store::forwarder::{ForwardConfig, ForwardZoneHandler};
use hickory_server::zone_handler::{Catalog, ZoneHandler};
use ipnet::Ipv4Net;
use tokio::net::{TcpListener, UdpSocket};
use tokio_util::sync::CancellationToken;

use crate::net::dns::{
    DNS_RESPONSE_BUFFER_SIZE, forward_name_server, name_server_socket_addr, parse_resolv_conf,
};
use crate::net::dns_update::rfc2136::{DnsInjector, InjectingHandler};
use crate::prelude::*;
use crate::tunnel::wg::{DnsConfig, WgServer};
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::read_file_to_string;
use crate::util::sync::SyncMutex;

const DNS_PORT: u16 = 53;
const FORWARD_TIMEOUT: Duration = Duration::from_secs(30);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// A running per-subnet proxy: the hickory server's cancellation token plus the
/// task driving it. Shutdown must go through the token — aborting the task only
/// schedules the abort of hickory's internal socket tasks, so the sockets can
/// still be open when the caller rebinds the same address (EADDRINUSE).
struct ProxyHandle {
    shutdown: CancellationToken,
    task: NonDetachingJoinHandle<()>,
}

/// Manages the per-subnet forward-only DNS proxies. Each subnet gets one
/// `ServerFuture` bound to that subnet's in-tunnel server address (`.1:53`,
/// UDP + TCP) that forwards every query to the upstream(s) selected for the
/// subnet. Binding to the specific in-tunnel address — never `0.0.0.0` — keeps
/// the proxy reachable only over the tunnel, not from the WAN.
#[derive(Default)]
pub struct DnsProxyController {
    listeners: SyncMutex<BTreeMap<Ipv4Net, ProxyHandle>>,
}
impl DnsProxyController {
    pub fn new() -> Self {
        Self::default()
    }

    /// Tear down every listener and rebind one proxy per current subnet. Full
    /// rebuild keeps this race-free against the `wg-quick` down/up cycle that
    /// re-creates the interface addresses; callers MUST invoke this *after*
    /// `WgServer::sync()` so the `.1` addresses exist to bind to.
    pub async fn sync(&self, server: &WgServer, injector: Arc<DnsInjector>) -> Result<(), Error> {
        // Shut the old listeners down gracefully and wait, so the sockets are
        // actually closed before we rebind the same addresses.
        let old = self.listeners.mutate(std::mem::take);
        for (subnet, proxy) in old {
            proxy.shutdown.cancel();
            if tokio::time::timeout(SHUTDOWN_TIMEOUT, proxy.task)
                .await
                .is_err()
            {
                tracing::warn!("DNS proxy for {subnet} did not shut down in time; aborting it");
            }
        }

        let mut listeners = BTreeMap::new();
        for (subnet, config) in &server.subnets.0 {
            let upstreams = match resolve_upstreams(&config.dns).await {
                Ok(u) => u,
                Err(e) => {
                    tracing::error!("failed to resolve DNS upstreams for {subnet}: {e}");
                    tracing::debug!("{e:?}");
                    continue;
                }
            };
            if upstreams.is_empty() {
                tracing::warn!("no DNS upstreams for subnet {subnet}; proxy not started");
                continue;
            }
            match bind_proxy(subnet.addr(), upstreams, injector.clone()).await {
                Ok(handle) => {
                    listeners.insert(*subnet, handle);
                }
                Err(e) => {
                    tracing::error!("failed to start DNS proxy for subnet {subnet}: {e}");
                    tracing::debug!("{e:?}");
                }
            }
        }
        self.listeners.mutate(|l| *l = listeners);
        Ok(())
    }
}

/// The upstream resolvers a subnet's proxy should forward to, per its mode.
async fn resolve_upstreams(config: &DnsConfig) -> Result<Vec<SocketAddr>, Error> {
    match config {
        DnsConfig::Default => default_upstreams().await,
        DnsConfig::Device { ip } => Ok(vec![SocketAddr::new(IpAddr::V4(*ip), DNS_PORT)]),
        DnsConfig::Custom { servers } => Ok(servers.clone()),
    }
}

/// The VPS's own system resolvers. Prefer systemd-resolved's upstream list
/// (`/run/systemd/resolve/resolv.conf`, which holds the real upstreams rather
/// than the `127.0.0.53` stub), falling back to `/etc/resolv.conf` on systems
/// that don't run systemd-resolved. Loopback entries are dropped so the proxy
/// never forwards to itself / the stub (which would loop).
async fn default_upstreams() -> Result<Vec<SocketAddr>, Error> {
    let from_systemd = systemd_resolved_upstreams().await;
    if !from_systemd.is_empty() {
        return Ok(from_systemd);
    }
    let (config, _) =
        hickory_resolver::system_conf::read_system_conf().with_kind(ErrorKind::ParseSysInfo)?;
    Ok(resolv_conf_upstreams(&config))
}

/// systemd-resolved's upstream resolvers, or empty if the file is absent,
/// unreadable, or unparseable (e.g. the system doesn't run systemd-resolved).
async fn systemd_resolved_upstreams() -> Vec<SocketAddr> {
    let Ok(contents) = read_file_to_string("/run/systemd/resolve/resolv.conf").await else {
        return Vec::new();
    };
    match parse_resolv_conf(contents) {
        Ok((config, _)) => resolv_conf_upstreams(&config),
        Err(_) => Vec::new(),
    }
}

/// Non-loopback nameservers from a parsed resolv.conf, de-duplicated.
fn resolv_conf_upstreams(config: &ResolverConfig) -> Vec<SocketAddr> {
    let mut seen = BTreeSet::new();
    config
        .name_servers()
        .iter()
        .map(name_server_socket_addr)
        .filter(|addr| !addr.ip().is_loopback())
        .filter(|addr| seen.insert(*addr))
        .collect()
}

/// Bind UDP + TCP on `addr:53` and spawn a `ServerFuture` forwarding to `upstreams`.
/// Binds before spawning so bind failures surface to the caller.
async fn bind_proxy(
    addr: Ipv4Addr,
    upstreams: Vec<SocketAddr>,
    injector: Arc<DnsInjector>,
) -> Result<ProxyHandle, Error> {
    let listen = SocketAddrV4::new(addr, DNS_PORT);
    let udp = UdpSocket::bind(listen)
        .await
        .with_kind(ErrorKind::Network)?;
    let tcp = TcpListener::bind(listen)
        .await
        .with_kind(ErrorKind::Network)?;

    let mut server = Server::new(InjectingHandler::new(
        injector,
        forwarding_catalog(upstreams)?,
    ));
    server.register_socket(udp);
    server.register_listener(tcp, FORWARD_TIMEOUT, DNS_RESPONSE_BUFFER_SIZE);

    let shutdown = server.shutdown_token().clone();
    let task = tokio::spawn(async move {
        server
            .block_until_done()
            .await
            .map_err(|e| Error::new(eyre!("{e}"), ErrorKind::Network))
            .log_err();
    })
    .into();
    Ok(ProxyHandle { shutdown, task })
}

/// A `Catalog` whose root zone is a single `ForwardAuthority` pointed at
/// `upstreams` (UDP + TCP per server). `Catalog` itself implements
/// `RequestHandler`, so no custom handler is needed for a pure forwarder.
fn forwarding_catalog(upstreams: Vec<SocketAddr>) -> Result<Catalog, Error> {
    let name_servers: Vec<NameServerConfig> =
        upstreams.into_iter().map(forward_name_server).collect();
    let mut opts = ResolverOpts::default();
    opts.timeout = FORWARD_TIMEOUT;
    let authority = ForwardZoneHandler::builder_tokio(ForwardConfig {
        name_servers,
        options: Some(opts),
    })
    .build()
    .map_err(|e| Error::new(eyre!("{e}"), ErrorKind::Network))?;

    let mut catalog = Catalog::new();
    let auth: Vec<Arc<dyn ZoneHandler>> = vec![Arc::new(authority)];
    catalog.upsert(Name::root().into(), auth);
    Ok(catalog)
}
