use std::net::SocketAddr;

use tokio::net::TcpStream;

use crate::lxc::slot::is_os_listen;
use crate::net::web_server::TcpMetadata;

/// Restores the destination a client dialed before DNAT to an OS listen address. A connection
/// dialed to that address directly keeps it. Fails when the original destination is unknown.
///
/// Takes the peer from `accept`; `TcpStream::peer_addr` fails once the peer resets.
pub(crate) fn tcp_metadata(
    stream: &TcpStream,
    peer_addr: SocketAddr,
) -> std::io::Result<TcpMetadata> {
    let socket_addr = stream.local_addr()?;
    let local_addr = if is_os_listen(socket_addr.ip()) {
        original_destination(stream, socket_addr)?
    } else {
        socket_addr
    };
    Ok(TcpMetadata {
        local_addr,
        peer_addr,
    })
}

#[cfg(target_os = "linux")]
fn original_destination(
    stream: &TcpStream,
    socket_addr: SocketAddr,
) -> std::io::Result<SocketAddr> {
    use nix::sys::socket::{SockaddrIn, SockaddrIn6, getsockopt, sockopt};

    if socket_addr.ip().to_canonical().is_ipv4() {
        Ok(SockaddrIn::from(getsockopt(stream, sockopt::OriginalDst)?).into())
    } else {
        Ok(SockaddrIn6::from(getsockopt(stream, sockopt::Ip6tOriginalDst)?).into())
    }
}

#[cfg(not(target_os = "linux"))]
fn original_destination(
    _stream: &TcpStream,
    _socket_addr: SocketAddr,
) -> std::io::Result<SocketAddr> {
    Err(std::io::ErrorKind::Unsupported.into())
}

#[cfg(test)]
#[path = "ingress/tests.rs"]
mod tests;
