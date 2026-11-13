use super::super::quic::tests::{client_hello, initial};
use super::super::tests::reserve_protocol_pair;
use super::*;
use crate::net::port_map::TransportProtocol;
use crate::tunnel::forward::sni::SniDemux;

fn v4(address: SocketAddr) -> SocketAddrV4 {
    let SocketAddr::V4(address) = address else {
        unreachable!()
    };
    address
}

async fn socket() -> UdpSocket {
    UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap()
}

async fn receive(socket: &UdpSocket) -> (Vec<u8>, SocketAddr) {
    let mut buffer = vec![0; 65535];
    let (length, peer) =
        tokio::time::timeout(Duration::from_secs(3), socket.recv_from(&mut buffer))
            .await
            .unwrap_or_else(|error| panic!("UDP receive on {:?}: {error}", socket.local_addr()))
            .unwrap();
    buffer.truncate(length);
    (buffer, peer)
}

#[tokio::test]
async fn fragmented_initials_are_verbatim_and_flows_remain_pinned() {
    for wire in [1, 0x6b3343cf] {
        let backend = socket().await;
        let replacement = socket().await;
        let probe = reserve_protocol_pair();
        let source = v4(probe.1.local_addr().unwrap());
        let demux = SniDemux::new();
        let names = ["quic.example.com".to_owned()];
        let target = v4(backend.local_addr().unwrap());
        demux
            .register_transport(
                TransportProtocol::Udp,
                *source.ip(),
                source.port(),
                &names,
                target,
                None,
            )
            .unwrap();
        drop(probe);
        let client = socket().await;
        let hello = client_hello(wire, true);
        let packets = [
            initial(wire, 1, 100, &hello[100..]),
            initial(wire, 0, 0, &hello[..100]),
        ];
        for packet in &packets {
            client.send_to(packet, source).await.unwrap();
        }
        let mut backend_peer = None;
        for packet in &packets {
            let (received, peer) = receive(&backend).await;
            assert_eq!(&received, packet);
            backend_peer = Some(peer);
        }
        backend
            .send_to(b"retry-or-version-negotiation", backend_peer.unwrap())
            .await
            .unwrap();
        let (received, peer) = receive(&client).await;
        assert_eq!(received, b"retry-or-version-negotiation");
        assert_eq!(peer, SocketAddr::V4(source));
        let new_target = v4(replacement.local_addr().unwrap());
        demux
            .register_transport(
                TransportProtocol::Udp,
                *source.ip(),
                source.port(),
                &names,
                new_target,
                None,
            )
            .unwrap();
        client
            .send_to(b"\x40application-packet", source)
            .await
            .unwrap();
        assert_eq!(receive(&backend).await.0, b"\x40application-packet");
        let new_client = socket().await;
        new_client
            .send_to(&initial(wire, 0, 0, &hello), source)
            .await
            .unwrap();
        assert_eq!(receive(&replacement).await.0, initial(wire, 0, 0, &hello));
    }
}

fn fallback_demux(backend: &UdpSocket) -> (Arc<SniDemux>, SocketAddrV4) {
    let probe = reserve_protocol_pair();
    let source = v4(probe.1.local_addr().unwrap());
    let demux = SniDemux::new();
    demux
        .register_fallback_transport(
            TransportProtocol::Udp,
            *source.ip(),
            source.port(),
            v4(backend.local_addr().unwrap()),
        )
        .unwrap();
    (demux, source)
}

#[tokio::test]
async fn fallback_relays_unknown_versions_plain_udp_and_no_sni() {
    let backend = socket().await;
    let (_demux, source) = fallback_demux(&backend);
    for payload in [
        b"plain UDP".to_vec(),
        vec![0xc0, 0xff, 0xff, 0xff, 0xff],
        initial(1, 0, 0, &client_hello(1, false)),
        Vec::new(),
    ] {
        let client = socket().await;
        client.send_to(&payload, source).await.unwrap();
        let (received, peer) = receive(&backend).await;
        assert_eq!(received, payload);
        backend.send_to(b"reply", peer).await.unwrap();
        assert_eq!(receive(&client).await.0, b"reply");
    }
}

fn budgeted(bytes: Vec<u8>) -> Datagram {
    let budget = Arc::new(Semaphore::new(bytes.len().max(1)))
        .try_acquire_many_owned(bytes.len().max(1) as u32)
        .unwrap();
    Datagram {
        bytes,
        _budget: budget,
    }
}

#[tokio::test(start_paused = true)]
async fn collection_deadline_does_not_reset_and_count_is_bounded() {
    let hello = client_hello(1, true);
    let first = initial(1, 0, 0, &hello[..100]);
    let (sender, mut receiver) = mpsc::channel(16);
    sender.send(budgeted(first.clone())).await.unwrap();
    let start = Instant::now();
    let (packets, name) = collect_hello(&mut receiver).await;
    assert_eq!(name, None);
    assert_eq!(packets.len(), 1);
    assert_eq!(Instant::now() - start, HELLO_TIMEOUT);
    for _ in 0..MAX_DATAGRAMS {
        sender.send(budgeted(first.clone())).await.unwrap();
    }
    let (packets, name) = collect_hello(&mut receiver).await;
    assert_eq!(name, None);
    assert_eq!(packets.len(), MAX_DATAGRAMS);
}

#[tokio::test]
async fn teardown_releases_all_flow_and_buffer_budgets() {
    let backend = socket().await;
    let (demux, source) = fallback_demux(&backend);
    let limits = demux.udp_limits.clone();
    let client = socket().await;
    client.send_to(b"hello", source).await.unwrap();
    receive(&backend).await;
    assert_eq!(limits.flows.available_permits(), 1023);
    drop(demux);
    let _all_flows = tokio::time::timeout(
        Duration::from_secs(3),
        limits.flows.clone().acquire_many_owned(1024),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(limits.bytes.available_permits(), 8 * 1024 * 1024);
    assert_eq!(limits.pending.available_permits(), 128);
}

#[tokio::test]
async fn idle_flows_release_their_session_budget() {
    let backend = socket().await;
    let (demux, source) = fallback_demux(&backend);
    let limits = demux.udp_limits.clone();
    let client = socket().await;
    client.send_to(b"idle", source).await.unwrap();
    receive(&backend).await;
    assert_eq!(limits.flows.available_permits(), 1023);
    tokio::time::pause();
    tokio::time::advance(FLOW_IDLE_TIMEOUT + Duration::from_millis(1)).await;
    let _all_flows = tokio::time::timeout(
        Duration::from_secs(1),
        limits.flows.clone().acquire_many_owned(1024),
    )
    .await
    .unwrap()
    .unwrap();
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn transparent_udp_dataplane_preserves_sources() {
    use tokio::process::Command;

    use crate::util::Invoke;

    let ready = || {
        std::fs::write(std::env::var("STARTOS_UDP_READY").unwrap(), b"ready\n").unwrap();
    };
    match std::env::var("STARTOS_UDP_ROLE").as_deref() {
        Ok("backend") => {
            let backend = UdpSocket::bind("10.0.0.2:8443").await.unwrap();
            ready();
            let (payload, peer) = receive(&backend).await;
            assert_eq!(payload, b"source-preserving UDP");
            assert_eq!(peer, "192.0.2.2:55000".parse::<SocketAddr>().unwrap());
            backend.send_to(b"backend reply", peer).await.unwrap();
        }
        Ok("gateway") => {
            let demux = SniDemux::new();
            demux.prepare().await.unwrap();
            // Unit-test builds bypass prepare; exercise the production infra here.
            crate::net::transparent::ensure_divert_infra_once()
                .await
                .unwrap();
            demux
                .register_fallback_transport(
                    TransportProtocol::Udp,
                    "192.0.2.1".parse().unwrap(),
                    44300,
                    "10.0.0.2:8443".parse().unwrap(),
                )
                .unwrap();
            ready();
            let done = std::env::var("STARTOS_UDP_DONE").unwrap();
            assert_eq!(
                tokio::task::spawn_blocking(move || std::fs::read_to_string(done))
                    .await
                    .unwrap()
                    .unwrap(),
                "done"
            );
            let client: SocketAddr = "192.0.2.2:55001".parse().unwrap();
            let target: SocketAddr = "10.0.0.2:8443".parse().unwrap();
            let upstream = crate::net::transparent::transparent_udp_connect(client, target)
                .await
                .unwrap();
            assert_eq!(
                crate::net::transparent::transparent_udp_connect(client, target)
                    .await
                    .err()
                    .unwrap()
                    .kind(),
                std::io::ErrorKind::AddrInUse
            );
            drop(upstream);
            assert!(
                crate::net::transparent::transparent_udp_connect(client, target)
                    .await
                    .is_ok()
            );
        }
        Ok("client") => {
            let client = UdpSocket::bind("192.0.2.2:55000").await.unwrap();
            client
                .send_to(b"source-preserving UDP", "192.0.2.1:44300")
                .await
                .unwrap();
            let (payload, peer) = receive(&client).await;
            assert_eq!(payload, b"backend reply");
            assert_eq!(peer, "192.0.2.1:44300".parse::<SocketAddr>().unwrap());
        }
        Err(_) => {
            if Command::new("unshare")
                .args(["-Urn", "true"])
                .invoke(crate::ErrorKind::Network)
                .await
                .is_err()
            {
                eprintln!(
                    "skipping transparent UDP transport fixture: user/network namespaces unavailable"
                );
                return;
            }
            Command::new("timeout")
                .args([
                    "--signal=TERM",
                    "--kill-after=2s",
                    "20s",
                    "unshare",
                    "-Urnm",
                    "bash",
                    "-c",
                    include_str!("udp_transport_test.sh"),
                    "bash",
                ])
                .arg(std::env::current_exe().unwrap())
                .arg(
                    "tunnel::forward::sni::udp::tests::transparent_udp_dataplane_preserves_sources",
                )
                .invoke(crate::ErrorKind::Network)
                .await
                .unwrap();
        }
        Ok(role) => panic!("unknown UDP test role: {role}"),
    }
}
