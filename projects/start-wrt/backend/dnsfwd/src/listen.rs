use std::io;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tracing::{debug, warn};

use crate::Forwarder;

const TCP_IDLE: Duration = Duration::from_secs(10);

/// Answers queries on `udp` and `tcp` until either socket fails.
pub async fn serve(forwarder: Forwarder, udp: UdpSocket, tcp: TcpListener) -> io::Result<()> {
    let forwarder = Arc::new(forwarder);
    tokio::try_join!(
        serve_udp(forwarder.clone(), Arc::new(udp)),
        serve_tcp(forwarder, tcp)
    )?;
    Ok(())
}

async fn serve_udp(forwarder: Arc<Forwarder>, socket: Arc<UdpSocket>) -> io::Result<()> {
    let mut buf = vec![0u8; 65_535];
    loop {
        let (len, peer) = socket.recv_from(&mut buf).await?;
        let query = buf[..len].to_vec();
        let forwarder = forwarder.clone();
        let socket = socket.clone();
        tokio::spawn(async move {
            if let Some(reply) = forwarder.answer(&query, true).await {
                if let Err(e) = socket.send_to(&reply, peer).await {
                    debug!(%peer, "UDP reply failed: {e}");
                }
            }
        });
    }
}

async fn serve_tcp(forwarder: Arc<Forwarder>, listener: TcpListener) -> io::Result<()> {
    loop {
        let (stream, peer) = listener.accept().await?;
        let forwarder = forwarder.clone();
        tokio::spawn(async move {
            if let Err(e) = serve_connection(&forwarder, stream).await {
                debug!(%peer, "TCP connection ended: {e}");
            }
        });
    }
}

async fn serve_connection(forwarder: &Forwarder, mut stream: TcpStream) -> io::Result<()> {
    loop {
        let mut len = [0u8; 2];
        match tokio::time::timeout(TCP_IDLE, stream.read_exact(&mut len)).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Ok(Err(e)) => return Err(e),
            Err(_) => return Ok(()),
        }
        let mut query = vec![0u8; usize::from(u16::from_be_bytes(len))];
        tokio::time::timeout(TCP_IDLE, stream.read_exact(&mut query))
            .await
            .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))??;
        let Some(reply) = forwarder.answer(&query, false).await else {
            continue;
        };
        let Ok(len) = u16::try_from(reply.len()) else {
            warn!("dropping an answer larger than 65535 bytes");
            continue;
        };
        stream
            .write_all(&[&len.to_be_bytes(), reply.as_slice()].concat())
            .await?;
    }
}
