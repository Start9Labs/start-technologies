use std::net::SocketAddr;

use tokio::net::TcpStream;

use crate::net::bridge::is_os_target;
use crate::net::web_server::TcpMetadata;

/// Restores the destination before OS-backend DNAT; peer addresses remain unchanged.
/// Requires an original destination outside the reserved backend targets.
pub(crate) fn tcp_metadata(stream: &TcpStream) -> std::io::Result<TcpMetadata> {
    let destination = stream.local_addr()?;
    let local_addr = if is_os_target(destination.ip()) {
        let original = original_destination(stream, destination)?;
        if is_os_target(original.ip()) {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
        original
    } else {
        destination
    };
    Ok(TcpMetadata {
        local_addr,
        peer_addr: stream.peer_addr()?,
    })
}

#[cfg(target_os = "linux")]
fn original_destination(
    stream: &TcpStream,
    destination: SocketAddr,
) -> std::io::Result<SocketAddr> {
    use nix::sys::socket::{SockaddrIn, SockaddrIn6, getsockopt, sockopt};

    if destination.ip().to_canonical().is_ipv4() {
        Ok(SockaddrIn::from(getsockopt(stream, sockopt::OriginalDst)?).into())
    } else {
        Ok(SockaddrIn6::from(getsockopt(stream, sockopt::Ip6tOriginalDst)?).into())
    }
}

#[cfg(not(target_os = "linux"))]
fn original_destination(
    _stream: &TcpStream,
    _destination: SocketAddr,
) -> std::io::Result<SocketAddr> {
    Err(std::io::ErrorKind::Unsupported.into())
}

#[cfg(test)]
#[path = "ingress/tests.rs"]
mod tests;
