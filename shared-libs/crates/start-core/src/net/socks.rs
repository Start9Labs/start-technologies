use std::future::Future;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

use fast_socks5::client::{Config as ClientConfig, Socks5Stream};
use fast_socks5::server::Socks5ServerProtocol;
use fast_socks5::util::target_addr::TargetAddr;
use fast_socks5::{ReplyError, Socks5Command};
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use itertools::Itertools;
use tokio::net::{TcpListener, TcpStream, lookup_host};
use tokio::time::Instant;

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
const TOR_PROXY: (&str, u16) = ("tor.startos", 9050);

/// RFC 8305 connection attempt delay.
const CONNECT_ATTEMPT_DELAY: Duration = Duration::from_millis(250);
const CONNECT_ATTEMPTS_IN_FLIGHT: usize = 2;

fn interleave_families(addrs: impl IntoIterator<Item = SocketAddr>) -> Vec<SocketAddr> {
    let mut addrs = addrs.into_iter().peekable();
    let first_is_v6 = addrs.peek().is_some_and(SocketAddr::is_ipv6);
    let (first, second): (Vec<_>, Vec<_>) = addrs.partition(|a| a.is_ipv6() == first_is_v6);
    first.into_iter().interleave(second).collect()
}

/// Connects to the first address that answers, alternating families. Starts
/// another attempt after [`CONNECT_ATTEMPT_DELAY`] or an earlier failure, with
/// at most [`CONNECT_ATTEMPTS_IN_FLIGHT`] pending.
async fn connect_each<F, Fut>(
    addrs: impl IntoIterator<Item = SocketAddr>,
    mut connect: F,
) -> Result<TcpStream, Error>
where
    F: FnMut(SocketAddr) -> Fut,
    Fut: Future<Output = std::io::Result<TcpStream>>,
{
    let mut pending = interleave_families(addrs).into_iter();
    let mut attempts = FuturesUnordered::new();
    let mut last_err = None;
    let next_attempt = tokio::time::sleep(Duration::ZERO);
    tokio::pin!(next_attempt);
    loop {
        tokio::select! {
            _ = &mut next_attempt, if pending.len() > 0 && attempts.len() < CONNECT_ATTEMPTS_IN_FLIGHT => {
                if let Some(addr) = pending.next() {
                    attempts.push(connect(addr));
                }
                next_attempt.as_mut().reset(Instant::now() + CONNECT_ATTEMPT_DELAY);
            }
            Some(res) = attempts.next() => match res {
                Ok(stream) => return Ok(stream),
                Err(e) => {
                    last_err = Some(e);
                    next_attempt.as_mut().reset(Instant::now());
                }
            },
            else => break,
        }
    }
    match last_err {
        Some(e) => Err(e).with_kind(ErrorKind::Network),
        None => Err(Error::new(
            eyre!("no address to connect to"),
            ErrorKind::Network,
        )),
    }
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
async fn connect_target(addr: TargetAddr) -> Result<TcpStream, Error> {
    match addr {
        TargetAddr::Domain(domain, port) if domain.ends_with(".onion") => {
            // get_socket() unwraps the tunnel once the handshake is done, so the
            // caller still gets a plain TcpStream to set keepalive on.
            Ok(
                Socks5Stream::connect(TOR_PROXY, domain, port, ClientConfig::default())
                    .await
                    .with_kind(ErrorKind::Network)?
                    .get_socket(),
            )
        }
        TargetAddr::Domain(domain, port) if domain.ends_with(".local") => {
            let ip = resolve_mdns(&domain).await?;
            TcpStream::connect((ip, port))
                .await
                .with_kind(ErrorKind::Network)
        }
        TargetAddr::Domain(domain, port) => {
            connect_each(
                lookup_host((domain, port))
                    .await
                    .with_kind(ErrorKind::Network)?,
                |addr| TcpStream::connect(addr),
            )
            .await
        }
        TargetAddr::Ip(addr) => TcpStream::connect(addr).await.with_kind(ErrorKind::Network),
    }
}

pub struct SocksController {
    _thread: NonDetachingJoinHandle<()>,
}
impl SocksController {
    pub fn new(listen: SocketAddr) -> Result<Self, Error> {
        Ok(Self {
            _thread: tokio::spawn(async move {
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
                        loop {
                            match listener.accept().await {
                                Ok((stream, _)) => {
                                    bg.add_job(async move {
                                        if let Err(e) = async {
                                            let (proto, cmd, target) =
                                                Socks5ServerProtocol::accept_no_auth(stream)
                                                    .await
                                                    .with_kind(ErrorKind::Network)?
                                                    .read_command()
                                                    .await
                                                    .with_kind(ErrorKind::Network)?;

                                            if cmd != Socks5Command::TCPConnect {
                                                proto
                                                    .reply_error(&ReplyError::CommandNotSupported)
                                                    .await
                                                    .with_kind(ErrorKind::Network)?;
                                                return Ok(());
                                            }

                                            if let Ok(mut target) = connect_target(target).await {
                                                if let Err(e) = socket2::SockRef::from(&target)
                                                    .set_keepalive(true)
                                                {
                                                    tracing::error!(
                                                        "Failed to set tcp keepalive: {e}"
                                                    );
                                                    tracing::debug!("{e:?}");
                                                }
                                                let mut sock = proto
                                                    .reply_success(SocketAddr::new(
                                                        Ipv4Addr::UNSPECIFIED.into(),
                                                        0,
                                                    ))
                                                    .await
                                                    .with_kind(ErrorKind::Network)?;
                                                tokio::io::copy_bidirectional(
                                                    &mut sock,
                                                    &mut target,
                                                )
                                                .await
                                                .with_kind(ErrorKind::Network)?;
                                            } else {
                                                proto
                                                    .reply_error(&ReplyError::HostUnreachable)
                                                    .await
                                                    .with_kind(ErrorKind::Network)?;
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

#[cfg(test)]
mod test {
    use std::net::Ipv6Addr;

    use futures::FutureExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    const STALLED: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 1), 9));
    const REFUSED: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 2), 9));

    async fn echo_round_trip(sock: &mut TcpStream) -> Result<(), Error> {
        sock.write_all(b"hello")
            .await
            .with_kind(ErrorKind::Network)?;
        let mut buf = [0u8; 5];
        sock.read_exact(&mut buf)
            .await
            .with_kind(ErrorKind::Network)?;
        assert_eq!(&buf, b"hello");
        Ok(())
    }

    #[test]
    fn interleave_families_alternates_from_the_first_family() {
        let v6 = |n| SocketAddr::from((Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, n), 1));
        let v4 = |n| SocketAddr::from((Ipv4Addr::new(192, 0, 2, n), 1));
        assert_eq!(
            interleave_families([v6(1), v6(2), v4(1)]),
            [v6(1), v4(1), v6(2)]
        );
        assert_eq!(
            interleave_families([v4(1), v6(1), v6(2)]),
            [v4(1), v6(1), v6(2)]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn the_second_attempt_starts_one_delay_after_a_stalled_first() -> Result<(), Error> {
        let target = echo_server().await?;
        let started = Instant::now();
        let mut starts = Vec::new();
        let mut sock = connect_each([STALLED, target], |addr| {
            starts.push((addr, started.elapsed()));
            if addr == STALLED {
                futures::future::pending().boxed()
            } else {
                TcpStream::connect(addr).boxed()
            }
        })
        .await?;
        assert_eq!(
            starts,
            [(STALLED, Duration::ZERO), (target, CONNECT_ATTEMPT_DELAY)]
        );
        echo_round_trip(&mut sock).await
    }

    #[tokio::test(start_paused = true)]
    async fn a_third_attempt_waits_for_a_failure() -> Result<(), Error> {
        let target = echo_server().await?;
        let started = Instant::now();
        let mut starts = Vec::new();
        let mut sock = connect_each([STALLED, REFUSED, target], |addr| {
            starts.push((addr, started.elapsed()));
            if addr == STALLED {
                futures::future::pending().boxed()
            } else if addr == REFUSED {
                async {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    Err(std::io::ErrorKind::ConnectionRefused.into())
                }
                .boxed()
            } else {
                TcpStream::connect(addr).boxed()
            }
        })
        .await?;
        assert_eq!(
            starts,
            [
                (STALLED, Duration::ZERO),
                (REFUSED, CONNECT_ATTEMPT_DELAY),
                (target, Duration::from_secs(3) + CONNECT_ATTEMPT_DELAY),
            ]
        );
        echo_round_trip(&mut sock).await
    }

    async fn echo_server() -> Result<SocketAddr, Error> {
        let l = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .with_kind(ErrorKind::Network)?;
        let addr = l.local_addr().with_kind(ErrorKind::Network)?;
        tokio::spawn(async move {
            while let Ok((mut s, _)) = l.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0u8; 64];
                    while let Ok(n) = s.read(&mut buf).await {
                        if n == 0 || s.write_all(&buf[..n]).await.is_err() {
                            break;
                        }
                    }
                });
            }
        });
        Ok(addr)
    }

    async fn start_proxy() -> Result<(SocketAddr, SocksController), Error> {
        let probe = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .with_kind(ErrorKind::Network)?;
        let listen = probe.local_addr().with_kind(ErrorKind::Network)?;
        drop(probe);
        let ctrl = SocksController::new(listen)?;
        for _ in 0..50 {
            if TcpStream::connect(listen).await.is_ok() {
                return Ok((listen, ctrl));
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Err(Error::new(
            color_eyre::eyre::eyre!("socks proxy did not come up"),
            ErrorKind::Network,
        ))
    }

    #[tokio::test]
    async fn connect_proxies_traffic_to_the_target() -> Result<(), Error> {
        let target = echo_server().await?;
        let (listen, _ctrl) = start_proxy().await?;

        let mut sock = Socks5Stream::connect(
            listen,
            target.ip().to_string(),
            target.port(),
            ClientConfig::default(),
        )
        .await
        .with_kind(ErrorKind::Network)?;

        sock.write_all(b"hello")
            .await
            .with_kind(ErrorKind::Network)?;
        let mut buf = [0u8; 5];
        sock.read_exact(&mut buf)
            .await
            .with_kind(ErrorKind::Network)?;
        assert_eq!(&buf, b"hello");
        Ok(())
    }

    #[tokio::test]
    async fn unreachable_target_is_refused_not_hung() -> Result<(), Error> {
        let dead = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .with_kind(ErrorKind::Network)?;
        let addr = dead.local_addr().with_kind(ErrorKind::Network)?;
        drop(dead);
        let (listen, _ctrl) = start_proxy().await?;

        assert!(
            Socks5Stream::connect(
                listen,
                addr.ip().to_string(),
                addr.port(),
                ClientConfig::default(),
            )
            .await
            .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn bind_and_udp_associate_are_rejected() -> Result<(), Error> {
        let (listen, _ctrl) = start_proxy().await?;
        for cmd in [Socks5Command::TCPBind, Socks5Command::UDPAssociate] {
            let label = format!("{cmd:?}");
            assert!(
                Socks5Stream::connect_raw(
                    cmd,
                    listen,
                    Ipv4Addr::LOCALHOST.to_string(),
                    9,
                    None,
                    ClientConfig::default(),
                )
                .await
                .is_err(),
                "{label} should be refused"
            );
        }
        Ok(())
    }
}
