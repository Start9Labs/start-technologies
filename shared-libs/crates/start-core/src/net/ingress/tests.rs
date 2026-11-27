#[cfg(target_os = "linux")]
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddrV6};
use std::sync::Arc;
#[cfg(target_os = "linux")]
use std::time::Duration;

use imbl::OrdMap;
#[cfg(target_os = "linux")]
use tokio::io::AsyncWriteExt;

use super::*;
use crate::db::model::public::{IpInfo, NetworkInterfaceInfo};
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
        let (server, peer_addr) = accepted.unwrap();
        let metadata = tcp_metadata(&server, peer_addr).unwrap();
        assert_eq!(metadata.local_addr, endpoint);
        assert_eq!(metadata.peer_addr, client.local_addr().unwrap());
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn reset_ingress_keeps_accepted_peer() {
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let endpoint = listener.local_addr().unwrap();
    let client = std::net::TcpStream::connect(endpoint).unwrap();
    let client_addr = client.local_addr().unwrap();
    let (server, peer_addr) = listener.accept().await.unwrap();
    socket2::SockRef::from(&client)
        .set_linger(Some(Duration::ZERO))
        .unwrap();
    drop(client);
    tokio::time::timeout(Duration::from_secs(5), async {
        while server.peer_addr().is_ok() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let metadata = tcp_metadata(&server, peer_addr).unwrap();
    assert_eq!(metadata.local_addr, endpoint);
    assert_eq!(metadata.peer_addr, client_addr);
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
fn translated_ingress_keeps_the_dialed_destination_gateway_and_peer() {
    use crate::lxc::slot::{OS_LISTEN_IP, OS_LISTEN_IPV6};
    use crate::net::gateway::{NetworkInterfaceListenerAcceptMetadata, poll_accept_described};

    if std::env::var_os("STARTOS_INGRESS_NETNS").is_none() {
        let available = std::process::Command::new("unshare")
            .args([
                "-rnm",
                "bash",
                "-c",
                "command -v ip && command -v nft && mount --make-rprivate / && mount -t tmpfs tmpfs /run",
            ])
            .output()
            .is_ok_and(|output| output.status.success());
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
            .env("OS_LISTEN_V4", OS_LISTEN_IP.to_string())
            .env("OS_LISTEN_V6", OS_LISTEN_IPV6.to_string())
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
    let timeout = Duration::from_secs(10);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let scope_id = nix::net::if_::if_nametoindex("ingress0").unwrap();
            let gateway: crate::GatewayId = "ingress0".parse().unwrap();
            let interfaces: OrdMap<crate::GatewayId, NetworkInterfaceInfo> = [(
                gateway.clone(),
                NetworkInterfaceInfo {
                    ip_info: Some(Arc::new(IpInfo {
                        scope_id,
                        subnets: ["192.0.2.1/24", "2001:db8:42::1/64"]
                            .into_iter()
                            .map(|ip| ip.parse::<ipnet::IpNet>().unwrap())
                            .collect(),
                        ..Default::default()
                    })),
                    ..Default::default()
                },
            )]
            .into_iter()
            .collect();
            let table = crate::net::gateway::gateway_table_id(scope_id);
            crate::net::gateway::reconcile_mangle_rules(&[(gateway, table)].into())
                .await
                .unwrap();
            let v4_peer: IpAddr = "192.0.2.2".parse().unwrap();
            for (bind, target, dialed, peer, family) in [
                (
                    IpAddr::V4(OS_LISTEN_IP),
                    IpAddr::V4(OS_LISTEN_IP),
                    "192.0.2.1".parse::<IpAddr>().unwrap(),
                    v4_peer,
                    "ip",
                ),
                (
                    IpAddr::V6(OS_LISTEN_IPV6),
                    IpAddr::V6(OS_LISTEN_IPV6),
                    "2001:db8:42::1".parse().unwrap(),
                    "2001:db8:42::2".parse().unwrap(),
                    "ip6",
                ),
                (
                    IpAddr::V6(Ipv6Addr::UNSPECIFIED),
                    IpAddr::V4(OS_LISTEN_IP),
                    "192.0.2.1".parse().unwrap(),
                    v4_peer,
                    "ip",
                ),
            ] {
                let listener = tokio::net::TcpListener::bind((bind, 41001)).await.unwrap();
                let dialed = SocketAddr::new(dialed, 41002);
                let target = SocketAddr::new(target, 41001);
                tokio::process::Command::new("nft")
                    .arg(format!(
                        "add rule inet ingress_test prerouting {family} daddr {} tcp dport {} dnat {family} to {target}",
                        dialed.ip(),
                        dialed.port()
                    ))
                    .invoke(crate::ErrorKind::Network)
                    .await
                    .unwrap();

                let mut client = peer_command(dialed, Some(peer), "translated");
                tokio::time::timeout(timeout, async {
                    let (_, result) = tokio::join!(
                        async {
                            let (mut stream, peer_addr) = listener.accept().await.unwrap();
                            assert_eq!(stream.local_addr().unwrap().ip().to_canonical(), target.ip());
                            assert_eq!(socket2::SockRef::from(&stream).mark().unwrap(), table);
                            let metadata = NetworkInterfaceListenerAcceptMetadata::from_stream(
                                &stream,
                                peer_addr,
                                &interfaces,
                            )
                            .unwrap();
                            assert_eq!(metadata.inner.local_addr, dialed);
                            assert_eq!(metadata.inner.peer_addr.ip().to_canonical(), peer);
                            assert_eq!(metadata.info.id.as_str(), "ingress0");
                            stream.write_all(b"translated").await.unwrap();
                        },
                        client.invoke(crate::ErrorKind::Network)
                    );
                    result.unwrap();
                })
                .await
                .unwrap();

                let mut direct = peer_command(target, Some(peer), "direct");
                tokio::time::timeout(timeout, async {
                    let (_, result) = tokio::join!(
                        async {
                            let (mut stream, peer_addr) = listener.accept().await.unwrap();
                            let metadata = NetworkInterfaceListenerAcceptMetadata::from_stream(
                                &stream,
                                peer_addr,
                                &interfaces,
                            )
                            .unwrap();
                            assert_eq!(metadata.inner.local_addr.ip().to_canonical(), target.ip());
                            assert_eq!(metadata.inner.local_addr.port(), target.port());
                            assert!(metadata.info.id.as_str().is_empty());
                            stream.write_all(b"direct").await.unwrap();
                        },
                        direct.invoke(crate::ErrorKind::Network)
                    );
                    result.unwrap();
                })
                .await
                .unwrap();

                if target.is_ipv4() {
                    let watch = crate::util::sync::Watch::new(interfaces.clone());
                    let mut accepting = Box::pin(std::future::poll_fn(|cx| {
                        poll_accept_described(&listener, cx, |stream, peer_addr| {
                            watch.peek(|interfaces| {
                                NetworkInterfaceListenerAcceptMetadata::from_stream(
                                    stream, peer_addr, interfaces,
                                )
                            })
                        })
                    }));
                    let mut untracked =
                        peer_command(target, Some("192.0.2.3".parse().unwrap()), "");
                    tokio::time::timeout(timeout, async {
                        tokio::select! {
                            accepted = &mut accepting => panic!(
                                "an untracked connection must be dropped, not accepted: {:?}",
                                accepted.map(|(metadata, _)| metadata)
                            ),
                            result = untracked.invoke(crate::ErrorKind::Network) => result.unwrap(),
                        };
                        let mut next = peer_command(dialed, Some(peer), "translated");
                        let (_, result) = tokio::join!(
                            async {
                                let (metadata, mut stream) = (&mut accepting).await.unwrap();
                                assert_eq!(metadata.inner.local_addr, dialed);
                                assert_eq!(metadata.info.id.as_str(), "ingress0");
                                stream.write_all(b"translated").await.unwrap();
                            },
                            next.invoke(crate::ErrorKind::Network)
                        );
                        result.unwrap();
                    })
                    .await
                    .unwrap();
                }
            }

            let loopback: OrdMap<crate::GatewayId, NetworkInterfaceInfo> = [(
                "lo".parse::<crate::GatewayId>().unwrap(),
                NetworkInterfaceInfo {
                    ip_info: Some(Arc::new(IpInfo {
                        subnets: ["127.0.0.1/8", "::1/128"]
                            .into_iter()
                            .map(|ip| ip.parse::<ipnet::IpNet>().unwrap())
                            .collect(),
                        ..Default::default()
                    })),
                    ..Default::default()
                },
            )]
            .into_iter()
            .collect();
            for (script, dialed, target, prefix) in [
                (
                    include_str!("../../../../../../build/lib/scripts/forward-port"),
                    SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 41004),
                    SocketAddr::new(OS_LISTEN_IP.into(), 41003),
                    "16",
                ),
                (
                    include_str!("../../../../../../build/lib/scripts/forward-port6"),
                    SocketAddr::new(Ipv6Addr::LOCALHOST.into(), 41004),
                    SocketAddr::new(OS_LISTEN_IPV6.into(), 41003),
                    "64",
                ),
            ] {
                tokio::process::Command::new("bash")
                    .args(["-c", script])
                    .env("sip", dialed.ip().to_string())
                    .env("sport", dialed.port().to_string())
                    .env("dip", target.ip().to_string())
                    .env("dport", target.port().to_string())
                    .env("dprefix", prefix)
                    .env("transport", "tcp")
                    .invoke(crate::ErrorKind::Network)
                    .await
                    .unwrap();
                let listener = tokio::net::TcpListener::bind(target).await.unwrap();
                let (client, accepted) = tokio::time::timeout(timeout, async {
                    tokio::join!(TcpStream::connect(dialed), listener.accept())
                })
                .await
                .unwrap();
                let client = client.unwrap();
                let (stream, peer_addr) = accepted.unwrap();
                assert_eq!(peer_addr, client.local_addr().unwrap(), "{dialed}");
                let metadata = NetworkInterfaceListenerAcceptMetadata::from_stream(
                    &stream, peer_addr, &loopback,
                )
                .unwrap();
                assert_eq!(metadata.inner.local_addr, dialed);
                assert_eq!(metadata.info.id.as_str(), "lo");
                let origin = crate::net::request_origin::RequestOrigin::from_connection(
                    Some(&metadata.inner),
                    Some(&metadata.info),
                );
                assert!(origin.peer_ip.is_some_and(|ip| ip.is_loopback()), "{dialed}");
                assert!(origin.is_internal(), "{dialed}");
                assert!(
                    crate::middleware::auth::signature::startos_signing_identities(None, &origin)
                        .contains("localhost")
                );
            }
        });
}

#[cfg(target_os = "linux")]
fn peer_command(
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
#[test]
#[ignore = "invoked by the isolated translated-ingress fixture"]
fn ingress_peer() {
    let destination: SocketAddr = std::env::var("INGRESS_DESTINATION")
        .unwrap()
        .parse()
        .unwrap();
    let socket = socket2::Socket::new(
        socket2::Domain::for_address(destination),
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )
    .unwrap();
    if let Ok(source) = std::env::var("INGRESS_SOURCE") {
        socket
            .bind(&SocketAddr::new(source.parse().unwrap(), 0).into())
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
