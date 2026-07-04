use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;

use socks5_impl::protocol::{Address, Reply};
use socks5_impl::server::auth::NoAuth;
use socks5_impl::server::{AuthAdaptor, ClientConnection, Server};
use tokio::net::{TcpListener, TcpStream};

use crate::HOST_IP;
use crate::net::mdns::resolve_mdns;
use crate::prelude::*;
use crate::util::actor::background::BackgroundJobQueue;
use crate::util::future::NonDetachingJoinHandle;

pub const DEFAULT_SOCKS_LISTEN: SocketAddr = SocketAddr::V4(SocketAddrV4::new(
    Ipv4Addr::new(HOST_IP[0], HOST_IP[1], HOST_IP[2], HOST_IP[3]),
    1080,
));

/// SOCKS5 proxy exposed by the `tor` service (the user-installable Tor plugin),
/// reachable from the host via the embedded DNS once that service is running.
/// Matches the default the registry server uses for its own onion requests.
fn tor_proxy() -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::new(HOST_IP[0], HOST_IP[1], HOST_IP[2], HOST_IP[3]),
        9050,
    ))
}

/// Open a connection to a SOCKS `CONNECT` target, special-casing the two address
/// families the host's resolver/router can't reach on its own:
///
/// - `*.onion` is tunneled through the Tor service's SOCKS5 proxy ([`TOR_PROXY`]).
///   Requires the `tor` service installed and running; otherwise `tor.startos`
///   doesn't resolve and the connection fails.
/// - `*.local` is resolved over mDNS via Avahi. The host runs systemd-resolved
///   with `MulticastDNS=no` and forwards `.local` to unicast upstreams, so
///   `getaddrinfo` never sees it — [`resolve_mdns`] queries `avahi` directly.
///
/// Everything else (clearnet hostnames, `*.startos`, raw IPs) connects directly
/// through the host's normal resolution path.
async fn connect_target(addr: Address) -> Result<TcpStream, Error> {
    match addr {
        Address::DomainAddress(domain, port) if domain.ends_with(".onion") => {
            let mut tor = TcpStream::connect(tor_proxy())
                .await
                .with_kind(ErrorKind::Network)?;
            socks5_impl::client::connect(&mut tor, Address::DomainAddress(domain, port), None)
                .await
                .with_kind(ErrorKind::Network)?;
            Ok(tor)
        }
        Address::DomainAddress(domain, port) if domain.ends_with(".local") => {
            let ip = resolve_mdns(&domain).await?;
            TcpStream::connect((ip, port))
                .await
                .with_kind(ErrorKind::Network)
        }
        Address::DomainAddress(domain, port) => TcpStream::connect((domain, port))
            .await
            .with_kind(ErrorKind::Network),
        Address::SocketAddress(addr) => {
            TcpStream::connect(addr).await.with_kind(ErrorKind::Network)
        }
    }
}

pub struct SocksController {
    _thread: NonDetachingJoinHandle<()>,
}
impl SocksController {
    pub fn new(listen: SocketAddr) -> Result<Self, Error> {
        Ok(Self {
            _thread: tokio::spawn(async move {
                let auth: AuthAdaptor<()> = Arc::new(NoAuth);
                let listener;
                loop {
                    if let Some(l) = TcpListener::bind(listen)
                        .await
                        .with_kind(ErrorKind::Network)
                        .log_err()
                    {
                        listener = l;
                        break;
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                let (bg, mut runner) = BackgroundJobQueue::new();
                runner
                    .run_while(async {
                        let server = Server::new(listener, auth);
                        loop {
                            match server.accept().await {
                                Ok((stream, _)) => {
                                    bg.add_job(async move {
                                        if let Err(e) = async {
                                            match stream
                                                .authenticate()
                                                .await
                                                .with_kind(ErrorKind::Network)?
                                                .0
                                                .wait_request()
                                                .await
                                                .with_kind(ErrorKind::Network)?
                                            {
                                                ClientConnection::Connect(reply, addr) => {
                                                    if let Ok(mut target) =
                                                        connect_target(addr).await
                                                    {
                                                        if let Err(e) =
                                                            socket2::SockRef::from(&target)
                                                                .set_keepalive(true)
                                                        {
                                                            tracing::error!(
                                                                "Failed to set tcp keepalive: {e}"
                                                            );
                                                            tracing::debug!("{e:?}");
                                                        }
                                                        let mut sock = reply
                                                            .reply(
                                                                Reply::Succeeded,
                                                                Address::unspecified(),
                                                            )
                                                            .await
                                                            .with_kind(ErrorKind::Network)?;
                                                        tokio::io::copy_bidirectional(
                                                            &mut sock,
                                                            &mut target,
                                                        )
                                                        .await
                                                        .with_kind(ErrorKind::Network)?;
                                                    } else {
                                                        let mut sock = reply
                                                            .reply(
                                                                Reply::HostUnreachable,
                                                                Address::unspecified(),
                                                            )
                                                            .await
                                                            .with_kind(ErrorKind::Network)?;
                                                        sock.shutdown()
                                                            .await
                                                            .with_kind(ErrorKind::Network)?;
                                                    }
                                                }
                                                ClientConnection::Bind(bind, _) => {
                                                    let mut sock = bind
                                                        .reply(
                                                            Reply::CommandNotSupported,
                                                            Address::unspecified(),
                                                        )
                                                        .await
                                                        .with_kind(ErrorKind::Network)?;
                                                    sock.shutdown()
                                                        .await
                                                        .with_kind(ErrorKind::Network)?;
                                                }
                                                ClientConnection::UdpAssociate(associate, _) => {
                                                    let mut sock = associate
                                                        .reply(
                                                            Reply::CommandNotSupported,
                                                            Address::unspecified(),
                                                        )
                                                        .await
                                                        .with_kind(ErrorKind::Network)?;
                                                    sock.shutdown()
                                                        .await
                                                        .with_kind(ErrorKind::Network)?;
                                                }
                                            }

                                            Ok::<_, Error>(())
                                        }
                                        .await
                                        {
                                            tracing::trace!("SOCKS5 Stream Error: {e}");
                                            tracing::trace!("{e:?}");
                                        }
                                    });
                                }
                                Err(e) => {
                                    tracing::error!("SOCKS5 TCP Accept Error: {e}");
                                    tracing::debug!("{e:?}");
                                }
                            }
                        }
                    })
                    .await;
            })
            .into(),
        })
    }
}
