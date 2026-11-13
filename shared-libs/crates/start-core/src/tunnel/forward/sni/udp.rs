//! UDP flows retain their selected backend until idle expiry or listener teardown.
//! Connection IDs do not carry routing state across client address changes.

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};
use tokio::task::JoinSet;
use tokio::time::{Instant, timeout_at};

use super::quic::InitialHello;
use super::{HelloStatus, LocalPrefix, PortBindings, PortKey, is_hairpin, select_target};
use crate::util::sync::SyncMutex;

const MAX_DATAGRAMS: usize = 10;
const MAX_BUFFERED_BYTES: usize = 65535;
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
const FLOW_IDLE_TIMEOUT: Duration = Duration::from_secs(120);
const FLOW_QUEUE: usize = 8;

/// Shared by all UDP listeners owned by one demux.
pub(super) struct Limits {
    flows: Arc<Semaphore>,
    pending: Arc<Semaphore>,
    bytes: Arc<Semaphore>,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            flows: Arc::new(Semaphore::new(1024)),
            pending: Arc::new(Semaphore::new(128)),
            bytes: Arc::new(Semaphore::new(8 * 1024 * 1024)),
        }
    }
}

#[derive(Debug)]
struct Datagram {
    bytes: Vec<u8>,
    _budget: OwnedSemaphorePermit,
}

pub(super) fn bind(key: PortKey) -> std::io::Result<UdpSocket> {
    // Wildcard UDP replies need per-datagram destination metadata.
    if key.0.is_unspecified() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "QUIC demux requires a concrete external IPv4 address",
        ));
    }
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.set_nonblocking(true)?;
    #[cfg(target_os = "linux")]
    socket.set_freebind_v4(true)?;
    socket.bind(&SocketAddrV4::new(key.0, key.1).into())?;
    UdpSocket::from_std(socket.into())
}

pub(super) async fn run_listener(
    socket: UdpSocket,
    key: PortKey,
    ports: Arc<SyncMutex<BTreeMap<PortKey, PortBindings>>>,
    local_prefix: Option<LocalPrefix>,
    limits: Arc<Limits>,
) {
    if let Err(error) = crate::net::transparent::ensure_divert_infra_once().await {
        tracing::warn!("QUIC demux reply-path diversion failed: {error}");
    }
    let socket = Arc::new(socket);
    let mut flows = BTreeMap::<SocketAddrV4, mpsc::Sender<Datagram>>::new();
    let mut tasks = JoinSet::new();
    let mut buffer = vec![0; 65535];
    loop {
        tokio::select! {
            biased;
            Some(completed) = tasks.join_next(), if !tasks.is_empty() => {
                if let Ok(peer) = completed {
                    flows.remove(&peer);
                }
            }
            received = socket.recv_from(&mut buffer) => {
                let (length, SocketAddr::V4(peer)) = (match received {
                    Ok(received) => received,
                    Err(error) => {
                        tracing::warn!("QUIC demux receive on {}:{}: {error}", key.0, key.1);
                        tokio::time::sleep(super::ACCEPT_RETRY_DELAY).await;
                        continue;
                    }
                }) else { continue };
                let Ok(budget) = limits.bytes.clone().try_acquire_many_owned(length.max(1) as u32) else {
                    continue;
                };
                let datagram = Datagram { bytes: buffer[..length].to_vec(), _budget: budget };
                if let Some(sender) = flows.get(&peer) {
                    let _ = sender.try_send(datagram);
                    continue;
                }
                let Ok(flow_budget) = limits.flows.clone().try_acquire_owned() else { continue };
                let (sender, receiver) = mpsc::channel(FLOW_QUEUE);
                sender.try_send(datagram).expect("new flow queue has capacity");
                flows.insert(peer, sender);
                let socket = socket.clone();
                let ports = ports.clone();
                let local_prefix = local_prefix.clone();
                let limits = limits.clone();
                tasks.spawn(async move {
                    let _flow_budget = flow_budget;
                    handle_flow(socket, key, peer, receiver, ports, local_prefix, limits).await;
                    peer
                });
            }
        }
    }
}

async fn collect_hello(receiver: &mut mpsc::Receiver<Datagram>) -> (Vec<Datagram>, Option<String>) {
    let deadline = Instant::now() + HELLO_TIMEOUT;
    let mut hello = InitialHello::default();
    let mut datagrams = Vec::new();
    let mut buffered = 0;
    while datagrams.len() < MAX_DATAGRAMS {
        let Ok(Some(datagram)) = timeout_at(deadline, receiver.recv()).await else {
            break;
        };
        buffered += datagram.bytes.len();
        if buffered > MAX_BUFFERED_BYTES {
            datagrams.push(datagram);
            break;
        }
        let status = hello.ingest(&datagram.bytes);
        datagrams.push(datagram);
        match status {
            HelloStatus::Pending => {}
            HelloStatus::Complete(hostname) => return (datagrams, hostname),
            HelloStatus::Invalid => break,
        }
    }
    (datagrams, None)
}

async fn handle_flow(
    frontend: Arc<UdpSocket>,
    key: PortKey,
    peer: SocketAddrV4,
    mut receiver: mpsc::Receiver<Datagram>,
    ports: Arc<SyncMutex<BTreeMap<PortKey, PortBindings>>>,
    local_prefix: Option<LocalPrefix>,
    limits: Arc<Limits>,
) {
    let Ok(pending_budget) = limits.pending.clone().try_acquire_owned() else {
        return;
    };
    let (datagrams, hostname) = collect_hello(&mut receiver).await;
    drop(pending_budget);
    let selected = select_target(&ports, key, hostname.as_deref(), *peer.ip());
    let Some((target, transparent)) = selected else {
        return;
    };
    let transparent = transparent && !is_hairpin(&local_prefix, *peer.ip(), *target.ip()).await;
    let upstream = if transparent {
        crate::net::transparent::transparent_udp_connect(peer.into(), target.into())
            .await
            .map(|(socket, reservation)| (socket, Some(reservation)))
    } else {
        async {
            let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await?;
            socket.connect(target).await?;
            Ok((socket, None))
        }
        .await
    };
    let upstream = match upstream {
        Ok(socket) => socket,
        Err(error) => {
            tracing::warn!("QUIC demux egress to {target} for {peer} failed: {error}");
            return;
        }
    };
    let socket = &upstream.0;
    for datagram in datagrams {
        if socket.send(&datagram.bytes).await.is_err() {
            return;
        }
    }
    let mut deadline = Instant::now() + FLOW_IDLE_TIMEOUT;
    let mut buffer = vec![0; 65535];
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => return,
            incoming = receiver.recv() => {
                let Some(datagram) = incoming else { return };
                if socket.send(&datagram.bytes).await.is_err() { return }
                deadline = Instant::now() + FLOW_IDLE_TIMEOUT;
            }
            received = socket.recv(&mut buffer) => {
                let Ok(length) = received else { return };
                if frontend.send_to(&buffer[..length], peer).await.is_err() { return }
                deadline = Instant::now() + FLOW_IDLE_TIMEOUT;
            }
        }
    }
}

#[cfg(test)]
#[path = "udp_tests.rs"]
mod tests;
