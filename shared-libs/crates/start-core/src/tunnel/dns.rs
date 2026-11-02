use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::sync::Arc;
use std::time::Duration;

use hickory_server::net::runtime::Time;
use hickory_server::net::xfer::Protocol;
use hickory_server::proto::op::Message;
use hickory_server::resolver as hickory_resolver;
use hickory_server::resolver::config::ResolverConfig;
use hickory_server::server::{Request, RequestHandler, ResponseHandler, ResponseInfo, Server};
use hickory_server::zone_handler::{Catalog, MessageResponseBuilder};
use ipnet::Ipv4Net;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};
use tokio_util::sync::CancellationToken;

use crate::net::dns::{DNS_RESPONSE_BUFFER_SIZE, name_server_socket_addr, parse_resolv_conf};
use crate::net::dns_update::forwarding_catalog;
use crate::net::dns_update::rfc2136::{DnsInjector, InjectingHandler, fallback_info};
use crate::net::transparent::{ensure_divert_infra, transparent_connect, transparent_udp};
use crate::prelude::*;
use crate::tunnel::wg::{DnsConfig, WgServer};
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::read_file_to_string;
use crate::util::sync::SyncMutex;

const DNS_PORT: u16 = 53;
const FORWARD_TIMEOUT: Duration = Duration::from_secs(30);
const RELAY_TIMEOUT: Duration = Duration::from_secs(5);
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

        let mut divert_ready = None;
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
            let mut devices = origin_devices(*subnet, &upstreams);
            if !devices.is_empty() {
                if divert_ready.is_none() {
                    divert_ready = Some(install_divert().await);
                }
                if divert_ready == Some(false) {
                    devices.clear();
                }
            }
            let forwarder = match forwarding_catalog(upstreams, FORWARD_TIMEOUT) {
                Ok(fallback) => OriginForwarder {
                    subnet: *subnet,
                    devices,
                    fallback,
                },
                Err(e) => {
                    tracing::error!("failed to configure DNS forwarding for {subnet}: {e}");
                    tracing::debug!("{e:?}");
                    continue;
                }
            };
            // The server's v6 on the subnet's delegated prefix, so v6-sourced
            // queries and RFC 2136 UPDATEs are served too. Best-effort: a v6
            // bind failure degrades to v4-only rather than killing the proxy.
            let server_v6 = config
                .ipv6
                .map(|prefix| crate::tunnel::wg6::host_v6(prefix, subnet.addr()));
            match bind_proxy(subnet.addr(), server_v6, forwarder, injector.clone()).await {
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

/// Installs reply diversion for relayed queries. False keeps forwarding from the
/// proxy's own address.
async fn install_divert() -> bool {
    match ensure_divert_infra().await {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!(
                "DNS relay reply-path diversion failed; forwarding from the proxy's address: {e}"
            );
            tracing::debug!("{e:?}");
            false
        }
    }
}

/// An address on the subnet other than the proxy's own.
fn is_peer(subnet: Ipv4Net, ip: Ipv4Addr) -> bool {
    subnet.contains(&ip) && ip != subnet.addr()
}

/// The upstreams that are devices on the subnet, or none unless every upstream is one.
fn origin_devices(subnet: Ipv4Net, upstreams: &[SocketAddr]) -> Vec<SocketAddrV4> {
    let devices: Vec<SocketAddrV4> = upstreams
        .iter()
        .filter_map(|upstream| match upstream {
            SocketAddr::V4(upstream) if is_peer(subnet, *upstream.ip()) => Some(*upstream),
            _ => None,
        })
        .collect();
    if devices.len() == upstreams.len() {
        devices
    } else {
        Vec::new()
    }
}

/// Forwards a subnet's queries. A peer's query is relayed from the peer's own
/// address to the first of `devices` that answers; other queries, and queries
/// no device answers, go to `fallback`.
struct OriginForwarder {
    subnet: Ipv4Net,
    devices: Vec<SocketAddrV4>,
    fallback: Catalog,
}

impl OriginForwarder {
    /// (sender, device) pairs to relay a query through, in order.
    fn relay_targets(
        &self,
        src: SocketAddr,
    ) -> impl Iterator<Item = (Ipv4Addr, SocketAddrV4)> + Send + '_ {
        let origin = match src {
            SocketAddr::V4(src) if is_peer(self.subnet, *src.ip()) => Some(*src.ip()),
            _ => None,
        };
        // A device drops packets that carry its own address as their source.
        origin.into_iter().flat_map(move |origin| {
            self.devices
                .iter()
                .filter(move |device| *device.ip() != origin)
                .map(move |device| (origin, *device))
        })
    }
}

#[async_trait::async_trait]
impl RequestHandler for OriginForwarder {
    async fn handle_request<R: ResponseHandler, T: Time>(
        &self,
        request: &Request,
        response_handle: R,
    ) -> ResponseInfo {
        for (origin, device) in self.relay_targets(request.src()) {
            match relay(origin, device, request).await {
                Ok(reply) => return send_reply(request, response_handle, reply).await,
                Err(e) => tracing::debug!("DNS relay from {origin} to {device} failed: {e}"),
            }
        }
        self.fallback
            .handle_request::<R, T>(request, response_handle)
            .await
    }
}

/// Sends the request's raw query to `device` from `origin`, over the request's protocol.
async fn relay(origin: Ipv4Addr, device: SocketAddrV4, request: &Request) -> io::Result<Message> {
    let client = SocketAddr::from((origin, 0));
    let device = SocketAddr::V4(device);
    let query = request.as_slice();
    let id = request.metadata.id.to_be_bytes();
    let exchange = async {
        if request.protocol() == Protocol::Udp {
            let socket = transparent_udp(client, device)?;
            socket.send(query).await?;
            let mut buf = vec![0; usize::from(u16::MAX)];
            loop {
                let len = socket.recv(&mut buf).await?;
                if buf[..len].starts_with(&id) {
                    buf.truncate(len);
                    return Ok(buf);
                }
            }
        } else {
            let len = u16::try_from(query.len())
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "query exceeds 64 KiB"))?;
            let mut stream = transparent_connect(client, device).await?;
            stream
                .write_all(&[&len.to_be_bytes()[..], query].concat())
                .await?;
            let mut buf = vec![0; usize::from(stream.read_u16().await?)];
            stream.read_exact(&mut buf).await?;
            Ok::<_, io::Error>(buf)
        }
    };
    let reply = tokio::time::timeout(RELAY_TIMEOUT, exchange)
        .await
        .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))??;
    let reply = Message::from_vec(&reply)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    if reply.metadata.id != request.metadata.id {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "reply ID does not match the query",
        ));
    }
    Ok(reply)
}

/// Answers the request with a relayed reply, within the sender's UDP payload size.
async fn send_reply<R: ResponseHandler>(
    request: &Request,
    mut response_handle: R,
    mut reply: Message,
) -> ResponseInfo {
    if let (Some(edns), Some(asked)) = (reply.edns.as_mut(), request.edns.as_ref()) {
        edns.set_max_payload(edns.max_payload().min(asked.max_payload()));
    }
    let metadata = reply.metadata;
    let mut builder = MessageResponseBuilder::from_message_request(request);
    if let Some(edns) = &reply.edns {
        builder.edns(edns);
    }
    response_handle
        .send_response(builder.build(
            metadata,
            &reply.answers,
            &reply.authorities,
            [],
            &reply.additionals,
        ))
        .await
        .unwrap_or_else(|_| fallback_info(metadata))
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

/// Bind UDP + TCP on `addr:53` (and `addr_v6:53` when the subnet carries a
/// prefix) and spawn a `ServerFuture` forwarding through `forwarder`. Binds
/// before spawning so bind failures surface to the caller.
async fn bind_proxy(
    addr: Ipv4Addr,
    addr_v6: Option<Ipv6Addr>,
    forwarder: OriginForwarder,
    injector: Arc<DnsInjector>,
) -> Result<ProxyHandle, Error> {
    let listen = SocketAddrV4::new(addr, DNS_PORT);
    let udp = UdpSocket::bind(listen)
        .await
        .with_kind(ErrorKind::Network)?;
    let tcp = TcpListener::bind(listen)
        .await
        .with_kind(ErrorKind::Network)?;

    let mut server = Server::new(InjectingHandler::new(injector, forwarder));
    server.register_socket(udp);
    server.register_listener(tcp, FORWARD_TIMEOUT, DNS_RESPONSE_BUFFER_SIZE);

    if let Some(v6) = addr_v6 {
        let listen_v6 = SocketAddrV6::new(v6, DNS_PORT, 0, 0);
        match UdpSocket::bind(listen_v6).await {
            Ok(udp_v6) => match TcpListener::bind(listen_v6).await {
                Ok(tcp_v6) => {
                    server.register_socket(udp_v6);
                    server.register_listener(tcp_v6, FORWARD_TIMEOUT, DNS_RESPONSE_BUFFER_SIZE);
                }
                Err(e) => {
                    tracing::warn!("DNS proxy v6 TCP bind on {listen_v6} failed (v4-only): {e}")
                }
            },
            Err(e) => tracing::warn!("DNS proxy v6 UDP bind on {listen_v6} failed (v4-only): {e}"),
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn subnet() -> Ipv4Net {
        "10.59.0.1/24".parse().unwrap()
    }

    fn addr(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    #[test]
    fn origin_devices_requires_every_upstream_on_the_subnet() {
        let device = addr("10.59.0.13:5300");
        assert_eq!(
            origin_devices(subnet(), &[device]),
            vec!["10.59.0.13:5300".parse::<SocketAddrV4>().unwrap()]
        );
        for upstreams in [
            vec![device, addr("1.1.1.1:53")],
            vec![addr("10.59.0.1:53")],
            vec![addr("10.59.1.13:53")],
            vec![addr("[fd00::13]:53")],
            vec![],
        ] {
            assert!(
                origin_devices(subnet(), &upstreams).is_empty(),
                "{upstreams:?}"
            );
        }
    }

    #[test]
    fn relays_peer_queries_to_other_devices() {
        let first = "10.59.0.13:5300".parse::<SocketAddrV4>().unwrap();
        let second = "10.59.0.14:53".parse::<SocketAddrV4>().unwrap();
        let forwarder = OriginForwarder {
            subnet: subnet(),
            devices: vec![first, second],
            fallback: Catalog::new(),
        };
        let targets = |src: &str| forwarder.relay_targets(addr(src)).collect::<Vec<_>>();
        let peer = Ipv4Addr::new(10, 59, 0, 3);
        assert_eq!(
            targets("10.59.0.3:4000"),
            vec![(peer, first), (peer, second)]
        );
        assert_eq!(targets("10.59.0.13:4000"), vec![(*first.ip(), second)]);
        for src in ["10.59.0.1:4000", "10.59.1.3:4000", "[fd00::3]:4000"] {
            assert!(targets(src).is_empty(), "{src}");
        }
    }

    #[cfg(target_os = "linux")]
    mod relay {
        use hickory_server::proto::op::{OpCode, Query, ResponseCode};
        use hickory_server::proto::rr::rdata::A;
        use hickory_server::proto::rr::{Name, RData, Record, RecordType};

        use super::*;
        use crate::tunnel::wg::{WgSubnetConfig, WgSubnetMap};

        const TEST: &str = "tunnel::dns::tests::relay::relays_queries_from_the_sender_address";
        const ROLE: &str = "STARTTUNNEL_DNS_RELAY_ROLE";

        /// Relays a peer's query from the peer's address over UDP and TCP, and
        /// forwards a device's own query from the proxy's address.
        #[test]
        fn relays_queries_from_the_sender_address() {
            use std::process::Command;

            let Ok(role) = std::env::var(ROLE) else {
                let probe = "command -v ip && command -v nft \
                    && mount --make-rprivate / && mount -t tmpfs tmpfs /run && ip netns add probe \
                    && nft add table ip probe \
                    && nft add chain ip probe c '{ type filter hook prerouting priority mangle; }' \
                    && nft add rule ip probe c meta l4proto '{ tcp, udp }' socket transparent 1 accept";
                if !Command::new("unshare")
                    .args(["-rnm", "bash", "-c", probe])
                    .output()
                    .is_ok_and(|o| o.status.success())
                {
                    eprintln!(
                        "skipped: the DNS relay test needs user/network/mount namespaces, iproute2, and nftables socket matching"
                    );
                    return;
                }
                let output = Command::new("unshare")
                    .args([
                        "-rnm",
                        "bash",
                        "-c",
                        include_str!("dns_relay_test.sh"),
                        "dns-relay",
                    ])
                    .arg(std::env::current_exe().unwrap())
                    .arg(TEST)
                    .env("ROLE_VAR", ROLE)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                return;
            };
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                match role.as_str() {
                    "device" => serve_whoami(addr("10.59.0.13:5300")).await,
                    "proxy" => run_proxy().await,
                    "client" => {
                        for tcp in [false, true] {
                            assert_eq!(
                                whoami(tcp).await,
                                Ipv4Addr::new(10, 59, 0, 3),
                                "tcp: {tcp}"
                            );
                        }
                    }
                    "device-self" => assert_eq!(whoami(false).await, subnet().addr()),
                    role => panic!("unknown role {role}"),
                }
            });
        }

        async fn run_proxy() {
            crate::net::forward::nft_ensure_base().await.unwrap();
            let mut server = WgServer::default();
            server.subnets = WgSubnetMap::default();
            server.subnets.0.insert(
                subnet(),
                WgSubnetConfig {
                    name: "net".into(),
                    dns: DnsConfig::Custom {
                        servers: vec![addr("10.59.0.13:5300")],
                    },
                    ..Default::default()
                },
            );
            let injector = DnsInjector::new(
                Vec::new(),
                |_| true,
                |_| None,
                |_| {},
                |_, _, _| async { ResponseCode::NoError },
            );
            let proxy = DnsProxyController::new();
            proxy.sync(&server, injector).await.unwrap();
            std::future::pending::<()>().await;
        }

        /// Answers every query with an A record holding the asker's address.
        async fn serve_whoami(addr: SocketAddr) {
            let udp = UdpSocket::bind(addr).await.unwrap();
            let tcp = TcpListener::bind(addr).await.unwrap();
            let udp_loop = async {
                let mut buf = vec![0; 512];
                while let Ok((len, peer)) = udp.recv_from(&mut buf).await {
                    udp.send_to(&whoami_reply(&buf[..len], peer.ip()), peer)
                        .await
                        .unwrap();
                }
            };
            let tcp_loop = async {
                while let Ok((mut stream, peer)) = tcp.accept().await {
                    let mut query = vec![0; usize::from(stream.read_u16().await.unwrap())];
                    stream.read_exact(&mut query).await.unwrap();
                    let reply = whoami_reply(&query, peer.ip());
                    let len = u16::try_from(reply.len()).unwrap().to_be_bytes();
                    stream
                        .write_all(&[&len[..], &reply].concat())
                        .await
                        .unwrap();
                }
            };
            tokio::join!(udp_loop, tcp_loop);
        }

        fn whoami_reply(query: &[u8], peer: IpAddr) -> Vec<u8> {
            let IpAddr::V4(peer) = peer else {
                panic!("IPv6 asker {peer}");
            };
            let query = Message::from_vec(query).unwrap();
            let mut reply = Message::response(query.metadata.id, OpCode::Query);
            reply.metadata.recursion_desired = query.metadata.recursion_desired;
            reply.metadata.recursion_available = true;
            for q in &query.queries {
                reply.add_answer(Record::from_rdata(q.name().clone(), 60, RData::A(A(peer))));
            }
            reply.queries = query.queries;
            reply.edns = query.edns;
            reply.to_vec().unwrap()
        }

        /// Asks the proxy for an A record until it answers, and returns the address.
        async fn whoami(tcp: bool) -> Ipv4Addr {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                match tokio::time::timeout(Duration::from_secs(2), ask_whoami(tcp)).await {
                    Ok(Ok(ip)) => return ip,
                    result if tokio::time::Instant::now() > deadline => {
                        panic!("no whoami answer (tcp: {tcp}): {result:?}")
                    }
                    _ => tokio::time::sleep(Duration::from_millis(200)).await,
                }
            }
        }

        async fn ask_whoami(tcp: bool) -> io::Result<Ipv4Addr> {
            let proxy = SocketAddr::from((subnet().addr(), DNS_PORT));
            let mut query = Message::query();
            query.metadata.recursion_desired = true;
            query.add_query(Query::query(
                Name::from_ascii("whoami.test.").unwrap(),
                RecordType::A,
            ));
            let query = query.to_vec().map_err(io::Error::other)?;
            let reply = if tcp {
                let mut stream = tokio::net::TcpStream::connect(proxy).await?;
                let len = u16::try_from(query.len()).unwrap().to_be_bytes();
                stream.write_all(&[&len[..], &query].concat()).await?;
                let mut buf = vec![0; usize::from(stream.read_u16().await?)];
                stream.read_exact(&mut buf).await?;
                buf
            } else {
                let socket = UdpSocket::bind("0.0.0.0:0").await?;
                socket.connect(proxy).await?;
                socket.send(&query).await?;
                let mut buf = vec![0; 4096];
                let len = socket.recv(&mut buf).await?;
                buf.truncate(len);
                buf
            };
            let reply = Message::from_vec(&reply).map_err(io::Error::other)?;
            match reply.answers.first().map(|answer| &answer.data) {
                Some(RData::A(A(ip))) => Ok(*ip),
                other => Err(io::Error::other(format!("unexpected answer {other:?}"))),
            }
        }
    }
}
