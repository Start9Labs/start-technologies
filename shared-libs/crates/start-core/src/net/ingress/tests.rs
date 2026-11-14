#[cfg(target_os = "linux")]
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddrV6};
use std::sync::Arc;
#[cfg(target_os = "linux")]
use std::time::Duration;

use imbl::OrdMap;
use tokio::io::AsyncWriteExt;

use super::*;
use crate::db::model::public::{IpInfo, NetworkInterfaceInfo};
#[cfg(target_os = "linux")]
use crate::net::bridge::{OS_TARGET_IPV4, OS_TARGET_IPV6};
#[cfg(target_os = "linux")]
use crate::net::gateway::NetworkInterfaceListenerAcceptMetadata;
use crate::net::gateway::lookup_info_by_addr;
#[cfg(target_os = "linux")]
use crate::util::Invoke;

#[tokio::test]
async fn direct_ingress_preserves_socket_endpoints() {
    for ip in [
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
    ] {
        let listener = tokio::net::TcpListener::bind((ip, 0)).await.unwrap();
        let endpoint = listener.local_addr().unwrap();
        let (client, accepted) = tokio::join!(TcpStream::connect(endpoint), listener.accept());
        let client = client.unwrap();
        let (server, _) = accepted.unwrap();
        let metadata = tcp_metadata(&server).unwrap();
        assert_eq!(metadata.local_addr, endpoint);
        assert_eq!(metadata.peer_addr, client.local_addr().unwrap());
    }
}

#[test]
fn link_local_ingress_selects_its_zone_not_another_matching_address() {
    let interfaces: OrdMap<crate::GatewayId, NetworkInterfaceInfo> = [("eth0", 2), ("eth1", 3)]
        .into_iter()
        .map(|(id, scope_id)| {
            (
                id.parse::<crate::GatewayId>().unwrap(),
                NetworkInterfaceInfo {
                    ip_info: Some(Arc::new(IpInfo {
                        subnets: ["fe80::1/64".parse::<ipnet::IpNet>().unwrap()]
                            .into_iter()
                            .collect(),
                        scope_id,
                        ..Default::default()
                    })),
                    ..Default::default()
                },
            )
        })
        .collect();
    let ip = "fe80::1".parse().unwrap();
    assert_eq!(
        lookup_info_by_addr(&interfaces, SocketAddrV6::new(ip, 443, 0, 3).into())
            .unwrap()
            .0
            .as_str(),
        "eth1"
    );
    assert!(lookup_info_by_addr(&interfaces, SocketAddrV6::new(ip, 443, 0, 0).into()).is_none());
    assert!(lookup_info_by_addr(&interfaces, SocketAddrV6::new(ip, 443, 0, 4).into()).is_none());
}

#[cfg(target_os = "linux")]
#[test]
fn translated_ingress_keeps_original_destination_gateway_and_peer() {
    if std::env::var_os("STARTOS_INGRESS_NETNS").is_none() {
        let available = std::process::Command::new("unshare")
            .args(["-rnm", "bash", "-c", "command -v ip && command -v nft && mount --make-rprivate / && mount -t tmpfs tmpfs /run"])
            .output().is_ok_and(|output| output.status.success());
        if !available {
            eprintln!(
                "skipped: translated ingress needs user/network/mount namespaces, iproute2 and nftables"
            );
            return;
        }
        let output = std::process::Command::new("unshare")
            .args([
                "-rnm",
                "bash",
                "-c",
                include_str!("namespace.sh"),
                "ingress-test",
            ])
            .arg(std::env::current_exe().unwrap())
            .env("OS_TARGET_V4", OS_TARGET_IPV4.to_string())
            .env("OS_TARGET_V6", OS_TARGET_IPV6.to_string())
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
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        let scope_id = nix::net::if_::if_nametoindex("ingress0").unwrap();
        let interfaces: OrdMap<crate::GatewayId, NetworkInterfaceInfo> = [("ingress0".parse::<crate::GatewayId>().unwrap(), NetworkInterfaceInfo {
            ip_info: Some(Arc::new(IpInfo {
                scope_id,
                subnets: ["192.0.2.1/24", "2001:db8:42::1/64", "fe80::1/64"].into_iter().map(|ip| ip.parse::<ipnet::IpNet>().unwrap()).collect(),
                ..Default::default()
            })),
            ..Default::default()
        })].into_iter().collect();
        let table = crate::net::gateway::gateway_table_id(scope_id);
        crate::net::gateway::reconcile_mangle_rules(&[("ingress0".parse().unwrap(), table)].into()).await.unwrap();
        for (bind, target, original, peer, family) in [
            (IpAddr::V4(OS_TARGET_IPV4), IpAddr::V4(OS_TARGET_IPV4), "192.0.2.1".parse::<IpAddr>().unwrap(), "192.0.2.2".parse::<IpAddr>().unwrap(), "ip"),
            (IpAddr::V6(OS_TARGET_IPV6), IpAddr::V6(OS_TARGET_IPV6), "2001:db8:42::1".parse().unwrap(), "2001:db8:42::2".parse().unwrap(), "ip6"),
            (IpAddr::V6(OS_TARGET_IPV6), IpAddr::V6(OS_TARGET_IPV6), "fe80::1".parse().unwrap(), "fe80::2".parse().unwrap(), "ip6"),
            (IpAddr::V6(OS_TARGET_IPV6), IpAddr::V6(OS_TARGET_IPV6), "fe80::1".parse().unwrap(), "2001:db8:42::2".parse().unwrap(), "ip6"),
            (IpAddr::V6(Ipv6Addr::UNSPECIFIED), IpAddr::V4(OS_TARGET_IPV4), "192.0.2.1".parse().unwrap(), "192.0.2.2".parse().unwrap(), "ip"),
        ] {
            let listener = tokio::net::TcpListener::bind((bind, 41001)).await.unwrap();
            let mut original = SocketAddr::new(original, 41002);
            if let SocketAddr::V6(address) = &mut original {
                if address.ip().is_unicast_link_local() { address.set_scope_id(scope_id); }
            }
            let target = SocketAddr::new(target, 41001);
            tokio::process::Command::new("nft").arg(format!("add rule inet ingress_test prerouting {family} daddr {} tcp dport {} dnat {family} to {target}", original.ip(), original.port()))
                .invoke(crate::ErrorKind::Network).await.unwrap();
            let mut client = client_command(original, Some(peer), "ok");
            tokio::time::timeout(Duration::from_secs(10), async {
                let (accepted, result) = tokio::join!(async {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    assert_eq!(stream.local_addr().unwrap().ip().to_canonical(), target.ip());
                    assert_eq!(socket2::SockRef::from(&stream).mark().unwrap(), table);
                    let metadata = NetworkInterfaceListenerAcceptMetadata::from_stream(&stream, &interfaces).unwrap();
                    assert_eq!(metadata.inner.local_addr, original);
                    assert_eq!(metadata.inner.peer_addr.ip().to_canonical(), peer);
                    assert_eq!(metadata.info.id.as_str(), "ingress0");
                    stream.write_all(b"ok").await.unwrap();
                }, client.invoke(crate::ErrorKind::Network));
                let _ = accepted;
                result.unwrap();
            }).await.unwrap();
            let mut direct = client_command(target, None, "");
            tokio::time::timeout(Duration::from_secs(10), async {
                let (_, result) = tokio::join!(async {
                    let (stream, _) = listener.accept().await.unwrap();
                    assert!(tcp_metadata(&stream).is_err());
                }, direct.invoke(crate::ErrorKind::Network));
                result.unwrap();
            }).await.unwrap();
        }
    });
}

#[cfg(target_os = "linux")]
fn client_command(
    destination: SocketAddr,
    source: Option<IpAddr>,
    expected: &str,
) -> tokio::process::Command {
    let mut command = tokio::process::Command::new("ip");
    command
        .args(["netns", "exec", "ingress-peer"])
        .arg(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "net::ingress::tests::ingress_peer",
            "--ignored",
            "--nocapture",
        ])
        .env("INGRESS_DESTINATION", destination.to_string())
        .env("INGRESS_EXPECTED", expected)
        .kill_on_drop(true);
    if let Some(source) = source {
        command.env("INGRESS_SOURCE", source.to_string());
    }
    command
}

#[cfg(target_os = "linux")]
fn peer_zone(mut address: SocketAddr) -> SocketAddr {
    if let SocketAddr::V6(address) = &mut address {
        if address.ip().is_unicast_link_local() {
            address.set_scope_id(nix::net::if_::if_nametoindex("peer0").unwrap());
        }
    }
    address
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "invoked by the isolated translated-ingress fixture"]
fn ingress_peer() {
    let destination = peer_zone(
        std::env::var("INGRESS_DESTINATION")
            .unwrap()
            .parse()
            .unwrap(),
    );
    let socket = socket2::Socket::new(
        socket2::Domain::for_address(destination),
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )
    .unwrap();
    if let Ok(source) = std::env::var("INGRESS_SOURCE") {
        socket
            .bind(&peer_zone(SocketAddr::new(source.parse().unwrap(), 0)).into())
            .unwrap();
    }
    socket
        .connect_timeout(&destination.into(), Duration::from_secs(5))
        .unwrap();
    let mut stream: std::net::TcpStream = socket.into();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut data = String::new();
    stream.read_to_string(&mut data).unwrap();
    assert_eq!(data, std::env::var("INGRESS_EXPECTED").unwrap());
}
