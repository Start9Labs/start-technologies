use std::future::Future;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::time::Duration;

use hickory_server::net::runtime::{RuntimeProvider, TokioRuntimeProvider};

use crate::GatewayId;
use crate::db::model::public::{DnsSettings, DnsSource, IpInfo, NetworkInterfaceInfo};

pub(super) const PUBLIC_DNS_FALLBACK: [SocketAddr; 2] = [
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), 53),
    SocketAddr::new(
        IpAddr::V6(Ipv6Addr::new(0x2606, 0x4700, 0x4700, 0, 0, 0, 0, 0x1111)),
        53,
    ),
];

impl Default for DnsSettings {
    fn default() -> Self {
        Self {
            static_servers: None,
            servers: PUBLIC_DNS_FALLBACK.into(),
            source: DnsSource::PublicFallback,
        }
    }
}

impl DnsSettings {
    pub(crate) fn refresh(&mut self, info: Option<&IpInfo>) {
        let provided: std::collections::VecDeque<_> = info
            .into_iter()
            .flat_map(|i| i.dns_servers.iter())
            .map(|ip| SocketAddr::new(*ip, 53))
            .filter(|addr| valid_upstream(*addr) && !is_host_address(addr.ip(), info))
            .collect();
        let (servers, source) = match &self.static_servers {
            Some(servers) => (servers.clone(), DnsSource::Static),
            _ if !provided.is_empty() => (provided, DnsSource::Gateway),
            _ => (PUBLIC_DNS_FALLBACK.into(), DnsSource::PublicFallback),
        };
        self.servers = servers;
        self.source = source;
    }
}

pub(super) fn is_host_address<'a>(
    ip: IpAddr,
    interfaces: impl IntoIterator<Item = &'a IpInfo>,
) -> bool {
    let ip = ip.to_canonical();
    ip.is_loopback()
        || ip == IpAddr::V4(crate::HOST_IP.into())
        || interfaces.into_iter().any(|info| {
            info.subnets
                .iter()
                .any(|subnet| subnet.addr().to_canonical() == ip)
        })
}

pub(super) fn is_host_source<'a>(
    source: SocketAddr,
    interfaces: impl IntoIterator<Item = &'a IpInfo>,
) -> bool {
    is_host_address(
        source.ip(),
        interfaces
            .into_iter()
            .filter(|info| source_zone_matches_interface(source, info)),
    )
}

pub(super) fn source_zone_matches_interface(source: SocketAddr, interface: &IpInfo) -> bool {
    match source {
        SocketAddr::V6(addr) if crate::net::utils::ipv6_is_link_local(*addr.ip()) => {
            addr.scope_id() == interface.scope_id
        }
        _ => true,
    }
}

pub(super) fn valid_upstream(addr: SocketAddr) -> bool {
    let ip = addr.ip().to_canonical();
    addr.port() != 0 && !ip.is_unspecified() && !ip.is_loopback()
}

pub(super) fn auto_gateway<'a>(
    interfaces: impl IntoIterator<Item = (&'a GatewayId, &'a NetworkInterfaceInfo)>,
) -> Option<GatewayId> {
    interfaces
        .into_iter()
        .filter_map(|(id, info)| {
            let info = info.ip_info.as_ref()?;
            info.default_route_v4
                .map(|metric| (0, metric))
                .or_else(|| info.default_route_v6.map(|metric| (1, metric)))
                .map(|(family, metric)| ((family, metric, info.scope_id), id.clone()))
        })
        .min_by_key(|(key, _)| *key)
        .map(|(_, id)| id)
}

pub(super) fn underlay_gateway(
    interfaces: &imbl::OrdMap<GatewayId, NetworkInterfaceInfo>,
) -> Option<GatewayId> {
    auto_gateway(interfaces.iter().filter(|(_, info)| !info.is_wireguard()))
}

#[derive(Clone)]
pub(crate) struct GatewayRuntime {
    runtime: TokioRuntimeProvider,
    gateway: GatewayId,
    index: u32,
}

impl GatewayRuntime {
    pub(crate) fn new(gateway: GatewayId, index: u32) -> Self {
        Self {
            runtime: TokioRuntimeProvider::default(),
            gateway,
            index,
        }
    }

    #[cfg(target_os = "linux")]
    fn socket(&self, addr: SocketAddr, kind: socket2::Type) -> io::Result<socket2::Socket> {
        use nix::net::if_::if_nametoindex;

        if if_nametoindex(self.gateway.as_str()).ok() != Some(self.index) {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "DNS gateway is unavailable",
            ));
        }
        let socket = socket2::Socket::new(socket2::Domain::for_address(addr), kind, None)?;
        let interface = std::num::NonZeroU32::new(self.index);
        match addr {
            SocketAddr::V4(_) => socket.bind_device_by_index_v4(interface)?,
            SocketAddr::V6(_) => socket.bind_device_by_index_v6(interface)?,
        }
        socket.set_mark(crate::net::gateway::gateway_table_id(self.index))?;
        socket.set_nonblocking(true)?;
        Ok(socket)
    }

    #[cfg(not(target_os = "linux"))]
    fn socket(&self, _: SocketAddr, _: socket2::Type) -> io::Result<socket2::Socket> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "gateway DNS requires Linux",
        ))
    }
}

impl RuntimeProvider for GatewayRuntime {
    type Handle = <TokioRuntimeProvider as RuntimeProvider>::Handle;
    type Timer = <TokioRuntimeProvider as RuntimeProvider>::Timer;
    type Udp = <TokioRuntimeProvider as RuntimeProvider>::Udp;
    type Tcp = <TokioRuntimeProvider as RuntimeProvider>::Tcp;

    fn create_handle(&self) -> Self::Handle {
        self.runtime.create_handle()
    }

    fn connect_tcp(
        &self,
        server_addr: SocketAddr,
        bind_addr: Option<SocketAddr>,
        timeout: Option<Duration>,
    ) -> Pin<Box<dyn Send + Future<Output = io::Result<Self::Tcp>>>> {
        let runtime = self.clone();
        Box::pin(async move {
            use std::os::fd::{FromRawFd, IntoRawFd};

            use hickory_server::net::runtime::iocompat::AsyncIoTokioAsStd;

            let socket = runtime.socket(server_addr, socket2::Type::STREAM)?;
            if let Some(addr) = bind_addr {
                socket.bind(&addr.into())?;
            }
            socket.set_tcp_nodelay(true)?;
            let socket = unsafe { tokio::net::TcpSocket::from_raw_fd(socket.into_raw_fd()) };
            tokio::time::timeout(
                timeout.unwrap_or(Duration::from_secs(5)),
                socket.connect(server_addr),
            )
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "DNS TCP connect timed out"))?
            .map(AsyncIoTokioAsStd)
        })
    }

    fn bind_udp(
        &self,
        local_addr: SocketAddr,
        _: SocketAddr,
    ) -> Pin<Box<dyn Send + Future<Output = io::Result<Self::Udp>>>> {
        let runtime = self.clone();
        Box::pin(async move {
            let socket = runtime.socket(local_addr, socket2::Type::DGRAM)?;
            socket.bind(&local_addr.into())?;
            tokio::net::UdpSocket::from_std(socket.into())
        })
    }
}

#[cfg(test)]
mod tests {
    use imbl::OrdMap;

    use super::*;
    use crate::db::model::public::NetworkInterfaceType;

    #[test]
    fn gateway_dns_defaults_and_overrides() {
        let mut dns = DnsSettings::default();
        assert_eq!(dns.source, DnsSource::PublicFallback);
        assert_eq!(
            dns.servers,
            [
                "1.1.1.1:53".parse().unwrap(),
                "[2606:4700:4700::1111]:53".parse().unwrap()
            ]
        );
        let ip = IpInfo {
            dns_servers: [
                "10.64.0.1".parse::<IpAddr>().unwrap(),
                "2001:db8::53".parse::<IpAddr>().unwrap(),
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        };
        dns.refresh(Some(&ip));
        assert_eq!(dns.source, DnsSource::Gateway);
        assert_eq!(
            dns.servers,
            [
                "10.64.0.1:53".parse().unwrap(),
                "[2001:db8::53]:53".parse().unwrap()
            ]
        );
        dns.static_servers = Some(["192.0.2.53:5353".parse().unwrap()].into());
        dns.refresh(Some(&ip));
        assert_eq!(dns.source, DnsSource::Static);
        assert_eq!(dns.servers, ["192.0.2.53:5353".parse().unwrap()]);
        dns.refresh(None);
        assert_eq!(dns.source, DnsSource::Static);
        dns.static_servers = None;
        dns.refresh(Some(&ip));
        assert_eq!(dns.source, DnsSource::Gateway);
        dns.refresh(None);
        assert_eq!(dns.source, DnsSource::PublicFallback);
    }

    #[test]
    fn host_source_matching_preserves_link_local_zones_and_ipv4_mapping() {
        let interface = IpInfo {
            scope_id: 2,
            subnets: [
                "fe80::1/64".parse::<ipnet::IpNet>().unwrap(),
                "192.0.2.2/24".parse::<ipnet::IpNet>().unwrap(),
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        };
        for source in [
            "[fe80::1%2]:53",
            "[::ffff:192.0.2.2]:53",
            "127.0.0.1:53",
            "[::1]:53",
        ] {
            assert!(is_host_source(source.parse().unwrap(), [&interface]));
        }
        for source in ["[fe80::1%3]:53", "[fe80::1]:53", "192.0.2.3:53"] {
            assert!(!is_host_source(source.parse().unwrap(), [&interface]));
        }
    }

    #[test]
    fn auto_dns_follows_main_default_metrics() {
        let iface = |index, v4, v6, kind| NetworkInterfaceInfo {
            ip_info: Some(std::sync::Arc::new(IpInfo {
                scope_id: index,
                default_route_v4: v4,
                default_route_v6: v6,
                device_type: Some(kind),
                ..Default::default()
            })),
            ..Default::default()
        };
        let mut interfaces: OrdMap<GatewayId, _> = [
            (
                "eth0".parse::<GatewayId>().unwrap(),
                iface(2, Some(100), Some(100), NetworkInterfaceType::Ethernet),
            ),
            (
                "wlan0".parse::<GatewayId>().unwrap(),
                iface(3, Some(600), None, NetworkInterfaceType::Wireless),
            ),
            (
                "wg0".parse::<GatewayId>().unwrap(),
                iface(4, None, None, NetworkInterfaceType::Wireguard),
            ),
        ]
        .into_iter()
        .collect();
        assert_eq!(auto_gateway(&interfaces), Some("eth0".parse().unwrap()));
        interfaces.remove(&"eth0".parse::<GatewayId>().unwrap());
        assert_eq!(auto_gateway(&interfaces), Some("wlan0".parse().unwrap()));
        interfaces.remove(&"wlan0".parse::<GatewayId>().unwrap());
        assert_eq!(auto_gateway(&interfaces), None);
        interfaces.insert(
            "eth1".parse::<GatewayId>().unwrap(),
            iface(5, None, Some(50), NetworkInterfaceType::Ethernet),
        );
        assert_eq!(auto_gateway(&interfaces), Some("eth1".parse().unwrap()));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn gateway_dns_transport() {
        use std::process::Command;

        if std::env::var_os("STARTOS_DNS_NETNS").is_none() {
            if !Command::new("unshare").args(["-rnm", "bash", "-c", "command -v ip && command -v socat && command -v nft && mount --make-rprivate / && mount -t tmpfs tmpfs /run"])
                .output().is_ok_and(|o| o.status.success()) {
                eprintln!("skipped: gateway DNS transport needs user/network/mount namespaces, iproute2, nftables and socat");
                return;
            }
            let output = Command::new("unshare")
                .args([
                    "-rnm",
                    "bash",
                    "-c",
                    include_str!("transport_test.sh"),
                    "dns-transport",
                ])
                .arg(std::env::current_exe().unwrap())
                .env("MAIN", crate::net::MAIN_RULE_PRIORITY.to_string())
                .env("REPLY", crate::net::REPLY_RULE_PRIORITY.to_string())
                .env(
                    "DEFAULT_OUTBOUND",
                    crate::net::DEFAULT_OUTBOUND_RULE_PRIORITY.to_string(),
                )
                .env(
                    "DEFAULT_OUTBOUND_REJECT",
                    crate::net::DEFAULT_OUTBOUND_REJECT_RULE_PRIORITY.to_string(),
                )
                .env(
                    "LOCAL_OUTBOUND_MARK",
                    crate::net::LOCAL_OUTBOUND_MARK.to_string(),
                )
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let index: u32 = std::env::var("DNS_GATEWAY_INDEX").unwrap().parse().unwrap();
        let provider = GatewayRuntime::new("gw0".parse().unwrap(), index);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            use futures::{AsyncReadExt, AsyncWriteExt};

            for server in ["192.0.2.53:5353", "[2001:db8::53]:5353", "[fe80::53]:5353"] {
                let server: SocketAddr = server.parse().unwrap();
                let local = SocketAddr::new(
                    if server.is_ipv4() {
                        "0.0.0.0".parse().unwrap()
                    } else {
                        "::".parse().unwrap()
                    },
                    0,
                );
                let socket = provider.bind_udp(local, server).await.unwrap();
                assert_eq!(
                    socket2::SockRef::from(&socket).mark().unwrap(),
                    1000 + index
                );
                socket.send_to(b"dns", server).await.unwrap();
                let mut buf = [0; 3];
                tokio::time::timeout(Duration::from_secs(3), socket.recv_from(&mut buf))
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(&buf, b"dns");
                let mut tcp = provider.connect_tcp(server, None, None).await.unwrap();
                tcp.write_all(b"tcp").await.unwrap();
                tokio::time::timeout(Duration::from_secs(3), tcp.read_exact(&mut buf))
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(&buf, b"tcp");
            }
            for family in ["-4", "-6"] {
                assert!(
                    Command::new("ip")
                        .args([
                            family,
                            "route",
                            "flush",
                            "table",
                            &(1000 + index).to_string()
                        ])
                        .status()
                        .unwrap()
                        .success()
                );
            }
            let server = "192.0.2.53:5353".parse().unwrap();
            let socket = provider
                .bind_udp("0.0.0.0:0".parse().unwrap(), server)
                .await
                .unwrap();
            let _ = socket.send_to(b"empty-table", server).await;
            let _ = provider
                .connect_tcp(server, None, Some(Duration::from_millis(250)))
                .await;
            let counters = Command::new("nft")
                .args(["list", "chain", "inet", "dns_test", "output"])
                .output()
                .unwrap();
            assert!(counters.status.success());
            let counters = String::from_utf8(counters.stdout).unwrap();
            let counters: Vec<_> = counters
                .lines()
                .filter(|l| l.contains("oifname \"eth0\""))
                .collect();
            assert_eq!(counters.len(), 2);
            assert!(
                counters.iter().all(|l| l.contains("counter packets 0")),
                "{counters:?}"
            );
            assert!(
                Command::new("ip")
                    .args(["link", "set", "gw0", "down"])
                    .status()
                    .unwrap()
                    .success()
            );
            let socket = provider
                .bind_udp("0.0.0.0:0".parse().unwrap(), server)
                .await
                .unwrap();
            let escaped = async {
                socket.send_to(b"leak", server).await?;
                socket.recv_from(&mut [0; 4]).await
            };
            assert!(!matches!(
                tokio::time::timeout(Duration::from_millis(250), escaped).await,
                Ok(Ok(_))
            ));
            assert!(
                provider
                    .connect_tcp(server, None, Some(Duration::from_millis(250)))
                    .await
                    .is_err()
            );
            assert!(
                Command::new("ip")
                    .args(["link", "del", "gw0"])
                    .status()
                    .unwrap()
                    .success()
            );
            assert!(
                Command::new("ip")
                    .args(["link", "add", "gw0", "type", "dummy"])
                    .status()
                    .unwrap()
                    .success()
            );
            assert!(
                provider
                    .bind_udp("0.0.0.0:0".parse().unwrap(), server)
                    .await
                    .is_err()
            );
            assert!(provider.connect_tcp(server, None, None).await.is_err());
        });
    }
}
