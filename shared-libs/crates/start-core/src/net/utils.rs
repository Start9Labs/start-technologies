use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6};
use std::path::Path;
use std::time::Duration;

use async_stream::try_stream;
use color_eyre::eyre::eyre;
use futures::stream::BoxStream;
use futures::{StreamExt, TryStreamExt};
use imbl_value::InternedString;
use ipnet::{IpNet, Ipv4Net, Ipv6Net};
use nix::net::if_::if_nametoindex;
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Command;

use crate::GatewayId;
use crate::db::model::public::{IpInfo, NetworkInterfaceType};
use crate::prelude::*;
use crate::util::Invoke;

// mio/tokio's TcpListener::bind hardcodes listen(128), which overflows the
// accept queue on the vhost proxy under burst load.
const LISTEN_BACKLOG: i32 = 1024;

fn build_listen_socket(
    addr: SocketAddr,
    reuse_port: bool,
) -> std::io::Result<std::net::TcpListener> {
    let domain = match addr {
        SocketAddr::V4(_) => socket2::Domain::IPV4,
        SocketAddr::V6(_) => socket2::Domain::IPV6,
    };
    let socket = socket2::Socket::new(domain, socket2::Type::STREAM, Some(socket2::Protocol::TCP))?;
    socket.set_reuse_address(true)?;
    if reuse_port {
        // SO_REUSEADDR alone does not let a dual-stack wildcard catch-all listener
        // share a TCP port with the per-address specific listeners bound alongside
        // it; that requires SO_REUSEPORT. Used by the DNS :53 binds (`dns_server_on`).
        socket.set_reuse_port(true)?;
    }
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(LISTEN_BACKLOG)?;
    Ok(socket.into())
}

/// Bind a `mio::net::TcpListener` with an explicit listen backlog
/// ([`LISTEN_BACKLOG`]) instead of mio's hardcoded 128. Use everywhere we'd
/// otherwise reach for `mio::net::TcpListener::bind`.
pub fn bind_mio_listener(addr: SocketAddr) -> std::io::Result<mio::net::TcpListener> {
    Ok(mio::net::TcpListener::from_std(build_listen_socket(
        addr, false,
    )?))
}

/// Bind a `tokio::net::TcpListener` with an explicit listen backlog
/// ([`LISTEN_BACKLOG`]) instead of tokio's hardcoded 128. Use everywhere we'd
/// otherwise reach for `tokio::net::TcpListener::bind`.
pub fn bind_tokio_listener(addr: SocketAddr) -> std::io::Result<tokio::net::TcpListener> {
    tokio::net::TcpListener::from_std(build_listen_socket(addr, false)?)
}

/// Like [`bind_tokio_listener`], but with SO_REUSEPORT so a wildcard catch-all
/// listener can share a port with per-address specific listeners (the DNS :53
/// binds in `dns_server_on`).
pub fn bind_tokio_listener_reuse_port(
    addr: SocketAddr,
) -> std::io::Result<tokio::net::TcpListener> {
    tokio::net::TcpListener::from_std(build_listen_socket(addr, true)?)
}

/// Detect silent peer death within ~2 min instead of the Linux default ~2h.
pub fn default_keepalive() -> socket2::TcpKeepalive {
    socket2::TcpKeepalive::new()
        .with_time(Duration::from_secs(60))
        .with_interval(Duration::from_secs(10))
        .with_retries(6)
}

pub async fn load_ip_info() -> Result<BTreeMap<GatewayId, IpInfo>, Error> {
    let output = String::from_utf8(
        Command::new("ip")
            .arg("-o")
            .arg("addr")
            .arg("show")
            .invoke(crate::ErrorKind::Network)
            .await?,
    )?;

    let err_fn = || {
        Error::new(
            eyre!("malformed output from `ip`"),
            crate::ErrorKind::Network,
        )
    };

    let mut res = BTreeMap::<GatewayId, IpInfo>::new();

    for line in output.lines() {
        let split = line.split_ascii_whitespace().collect::<Vec<_>>();
        let iface = GatewayId::from(InternedString::from(*split.get(1).ok_or_else(&err_fn)?));
        let subnet: IpNet = split.get(3).ok_or_else(&err_fn)?.parse()?;
        let ip_info = res.entry(iface.clone()).or_default();
        ip_info.name = iface.into();
        ip_info.scope_id = split
            .get(0)
            .ok_or_else(&err_fn)?
            .strip_suffix(":")
            .ok_or_else(&err_fn)?
            .parse()?;
        ip_info.subnets.insert(subnet);
    }

    for (id, ip_info) in res.iter_mut() {
        ip_info.device_type = probe_iface_type(id.as_str()).await;
    }

    Ok(res)
}

pub fn ipv6_is_link_local(addr: Ipv6Addr) -> bool {
    (addr.segments()[0] & 0xffc0) == 0xfe80
}

/// Unique-local address (`fc00::/7`) — the lxcbr0 bridge subnet lives here.
pub fn ipv6_is_ula(addr: Ipv6Addr) -> bool {
    (addr.segments()[0] & 0xfe00) == 0xfc00
}

pub fn ipv6_is_local(addr: Ipv6Addr) -> bool {
    addr.is_loopback() || ipv6_is_ula(addr) || ipv6_is_link_local(addr)
}

pub fn is_private_ip(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
        IpAddr::V6(v6) => ipv6_is_local(v6),
    }
}

fn parse_iface_ip(output: &str) -> Result<Vec<&str>, Error> {
    let output = output.trim();
    if output.is_empty() {
        return Ok(Vec::new());
    }
    let mut res = Vec::new();
    for line in output.lines() {
        if let Some(ip) = line.split_ascii_whitespace().nth(3) {
            res.push(ip)
        } else {
            return Err(Error::new(
                eyre!("malformed output from `ip`"),
                crate::ErrorKind::Network,
            ));
        }
    }
    Ok(res)
}

pub async fn get_iface_ipv4_addr(iface: &str) -> Result<Option<(Ipv4Addr, Ipv4Net)>, Error> {
    Ok(parse_iface_ip(&String::from_utf8(
        Command::new("ip")
            .arg("-4")
            .arg("-o")
            .arg("addr")
            .arg("show")
            .arg(iface)
            .invoke(crate::ErrorKind::Network)
            .await?,
    )?)?
    .into_iter()
    .map(|s| Ok::<_, Error>((s.split("/").next().unwrap().parse()?, s.parse()?)))
    .next()
    .transpose()?)
}

pub async fn get_iface_ipv6_addr(iface: &str) -> Result<Option<(Ipv6Addr, Ipv6Net)>, Error> {
    Ok(parse_iface_ip(&String::from_utf8(
        Command::new("ip")
            .arg("-6")
            .arg("-o")
            .arg("addr")
            .arg("show")
            .arg(iface)
            .invoke(crate::ErrorKind::Network)
            .await?,
    )?)?
    .into_iter()
    .find(|ip| !ip.starts_with("fe80::"))
    .map(|s| Ok::<_, Error>((s.split("/").next().unwrap().parse()?, s.parse()?)))
    .transpose()?)
}

/// True iff the host has an IPv6 default route (`::/0`), i.e. some working IPv6
/// egress. Used to reject delegating an IPv6 prefix on a box that can't route it.
pub async fn has_ipv6_default_route() -> Result<bool, Error> {
    let output = String::from_utf8(
        Command::new("ip")
            .arg("-6")
            .arg("route")
            .arg("show")
            .arg("default")
            .invoke(crate::ErrorKind::Network)
            .await?,
    )?;
    Ok(!output.trim().is_empty())
}

pub async fn probe_iface_type(iface: &str) -> Option<NetworkInterfaceType> {
    match tokio::fs::read_to_string(Path::new("/sys/class/net").join(iface).join("uevent"))
        .await
        .ok()?
        .lines()
        .find_map(|l| l.strip_prefix("DEVTYPE="))
    {
        Some("wlan") => Some(NetworkInterfaceType::Wireless),
        Some("bridge") => Some(NetworkInterfaceType::Bridge),
        Some("wireguard") => Some(NetworkInterfaceType::Wireguard),
        None if iface_is_physical(iface).await => Some(NetworkInterfaceType::Ethernet),
        None if iface_is_loopback(iface).await => Some(NetworkInterfaceType::Loopback),
        _ => None,
    }
}

pub async fn iface_is_physical(iface: &str) -> bool {
    tokio::fs::metadata(Path::new("/sys/class/net").join(iface).join("device"))
        .await
        .is_ok()
}

pub async fn iface_is_wireless(iface: &str) -> bool {
    tokio::fs::metadata(Path::new("/sys/class/net").join(iface).join("wireless"))
        .await
        .is_ok()
}

pub async fn iface_is_bridge(iface: &str) -> bool {
    tokio::fs::metadata(Path::new("/sys/class/net").join(iface).join("bridge"))
        .await
        .is_ok()
}

pub async fn iface_is_loopback(iface: &str) -> bool {
    tokio::fs::read_to_string(Path::new("/sys/class/net").join(iface).join("type"))
        .await
        .ok()
        .map_or(false, |x| x.trim() == "772")
}

pub fn list_interfaces() -> BoxStream<'static, Result<String, Error>> {
    try_stream! {
        let mut ifaces = tokio::fs::read_dir("/sys/class/net").await?;
        while let Some(iface) = ifaces.next_entry().await? {
            if let Some(iface) = iface.file_name().into_string().ok() {
                yield iface;
            }
        }
    }
    .boxed()
}

pub async fn find_wifi_iface() -> Result<Option<String>, Error> {
    let mut ifaces = list_interfaces();
    while let Some(iface) = ifaces.try_next().await? {
        if iface_is_wireless(&iface).await {
            return Ok(Some(iface));
        }
    }
    Ok(None)
}

pub async fn find_eth_iface() -> Result<String, Error> {
    let mut ifaces = list_interfaces();
    while let Some(iface) = ifaces.try_next().await? {
        if iface_is_physical(&iface).await && !iface_is_wireless(&iface).await {
            return Ok(iface);
        }
    }
    Err(Error::new(
        eyre!("Could not detect ethernet interface"),
        crate::ErrorKind::Network,
    ))
}

pub async fn all_socket_addrs_for(port: u16) -> Result<Vec<(InternedString, SocketAddr)>, Error> {
    let mut res = Vec::new();

    let raw = String::from_utf8(
        Command::new("ip")
            .arg("-o")
            .arg("addr")
            .arg("show")
            .invoke(ErrorKind::ParseSysInfo)
            .await?,
    )?;
    let err = |item: &str, lineno: usize, line: &str| {
        Error::new(
            eyre!("failed to parse ip info ({item}[line:{lineno}]) from {line:?}"),
            ErrorKind::ParseSysInfo,
        )
    };
    for (idx, line) in raw
        .lines()
        .map(|l| l.trim())
        .enumerate()
        .filter(|(_, l)| !l.is_empty())
    {
        let mut split = line.split_whitespace();
        let _num = split.next();
        let ifname = split.next().ok_or_else(|| err("ifname", idx, line))?;
        let _kind = split.next();
        let ipnet_str = split.next().ok_or_else(|| err("ipnet", idx, line))?;
        let ipnet = ipnet_str
            .parse::<IpNet>()
            .with_ctx(|_| (ErrorKind::ParseSysInfo, err("ipnet", idx, ipnet_str)))?;
        match ipnet.addr() {
            IpAddr::V4(ip4) => res.push((ifname.into(), SocketAddr::new(ip4.into(), port))),
            IpAddr::V6(ip6) => res.push((
                ifname.into(),
                SocketAddr::V6(SocketAddrV6::new(
                    ip6,
                    port,
                    0,
                    if_nametoindex(ifname)
                        .with_ctx(|_| (ErrorKind::ParseSysInfo, "reading scope_id"))?,
                )),
            )),
        }
    }

    Ok(res)
}

pub struct TcpListeners {
    listeners: Vec<TcpListener>,
}
impl TcpListeners {
    pub fn new(listeners: impl IntoIterator<Item = TcpListener>) -> Self {
        Self {
            listeners: listeners.into_iter().collect(),
        }
    }

    pub async fn accept(&self) -> std::io::Result<(TcpStream, SocketAddr)> {
        futures::future::select_all(self.listeners.iter().map(|l| Box::pin(l.accept())))
            .await
            .0
    }
}
// impl hyper::server::accept::Accept for TcpListeners {
//     type Conn = TcpStream;
//     type Error = std::io::Error;

//     fn poll_accept(
//         self: std::pin::Pin<&mut Self>,
//         cx: &mut std::task::Context<'_>,
//     ) -> std::task::Poll<Option<Result<Self::Conn, Self::Error>>> {
//         for listener in self.listeners.iter() {
//             let poll = listener.poll_accept(cx);
//             if poll.is_ready() {
//                 return poll.map(|a| a.map(|a| a.0)).map(Some);
//             }
//         }
//         std::task::Poll::Pending
//     }
// }
// TODO
