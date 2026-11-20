use std::collections::VecDeque;
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use futures::stream::FuturesUnordered;
use futures::{Stream, StreamExt};
use tokio::net::TcpStream;
use tokio::time::Instant;

use crate::db::model::Database;
use crate::net::host::binding::{DerivedAddressInfo, bridge_address};
use crate::net::mdns::resolve_mdns;
use crate::prelude::*;
use crate::util::sync::SyncRwLock;

/// The addresses of the Tor package's SOCKS binding: host `socks`, internal port 9050.
/// tor-startos publishes both as a contract.
const TOR_SOCKS_ADDRESSES: &str = "/public/packageData/tor/hosts/socks/bindings/9050/addresses";

const CONNECT_ATTEMPT_DELAY: Duration = Duration::from_millis(250);
const RESOLUTION_DELAY: Duration = Duration::from_millis(50);

pub fn is_onion(host: &str) -> bool {
    host.ends_with(".onion")
}

fn tor_unavailable(host: &str) -> Error {
    Error::new(
        eyre!("{}", t!("net.dial.tor-unavailable", host = host)),
        ErrorKind::Network,
    )
}

/// The bridge address of Tor's SOCKS binding. `None` while Tor has not bound it.
#[derive(Clone, Debug, Default)]
pub struct TorSocks(Arc<SyncRwLock<Option<SocketAddr>>>);

impl TorSocks {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self) -> Option<SocketAddr> {
        self.0.read()
    }

    #[cfg(test)]
    pub(crate) fn set(&self, socks: SocketAddr) {
        self.0.replace(Some(socks));
    }

    /// Tracks the binding until the database closes.
    pub async fn follow(self, db: TypedPatchDb<Database>) {
        let mut watch = db.watch(TOR_SOCKS_ADDRESSES.parse().unwrap()).await;
        while let Ok(addresses) = watch.peek_and_mark_seen() {
            self.0.replace(
                from_value::<Option<DerivedAddressInfo>>(addresses)
                    .ok()
                    .flatten()
                    .and_then(|addresses| bridge_address(&addresses, false)),
            );
            if watch.changed().await.is_err() {
                break;
            }
        }
    }
}

/// Uses libc resolution; musl reads `/etc/hosts` and `/etc/resolv.conf`.
async fn lookup_family(
    domain: String,
    port: u16,
    family: libc::c_int,
) -> Result<Vec<SocketAddr>, Error> {
    // Cancellation cannot interrupt an in-flight system resolver call.
    tokio::task::spawn_blocking(move || {
        let hints = dns_lookup::AddrInfoHints {
            address: family,
            socktype: libc::SOCK_STREAM,
            ..Default::default()
        };
        dns_lookup::getaddrinfo(Some(&domain), None, Some(hints))
            .map_err(std::io::Error::from)?
            .map(|addr| {
                let mut addr = addr?.sockaddr;
                addr.set_port(port);
                Ok(addr)
            })
            .collect::<std::io::Result<Vec<_>>>()
    })
    .await
    .with_kind(ErrorKind::Network)?
    .with_kind(ErrorKind::Network)
}

/// RFC 8305: usable IPv4 waits at most `RESOLUTION_DELAY` for IPv6 resolution.
fn resolve_families(
    ipv6: impl Future<Output = Result<Vec<SocketAddr>, Error>>,
    ipv4: impl Future<Output = Result<Vec<SocketAddr>, Error>>,
) -> impl Stream<Item = Result<Vec<SocketAddr>, Error>> {
    async_stream::stream! {
        tokio::pin!(ipv6, ipv4);
        tokio::select! {
            biased;
            result = &mut ipv6 => {
                yield result;
                yield ipv4.await;
            }
            result = &mut ipv4 => {
                match tokio::time::timeout(RESOLUTION_DELAY, &mut ipv6).await {
                    Ok(ipv6) => {
                        yield ipv6;
                        yield result;
                    }
                    Err(_) => {
                        yield result;
                        yield ipv6.await;
                    }
                }
            }
        }
    }
}

pub(crate) async fn connect_domain(domain: String, port: u16) -> Result<TcpStream, Error> {
    connect_each(
        resolve_families(
            lookup_family(domain.clone(), port, libc::AF_INET6),
            lookup_family(domain, port, libc::AF_INET),
        ),
        |addr| TcpStream::connect(addr),
    )
    .await
}

/// Alternates families, incorporating late DNS answers without restarting attempts.
async fn connect_each<S, F, Fut, T>(resolutions: S, mut connect: F) -> Result<T, Error>
where
    S: Stream<Item = Result<Vec<SocketAddr>, Error>>,
    F: FnMut(SocketAddr) -> Fut,
    Fut: Future<Output = std::io::Result<T>>,
{
    let resolutions = resolutions.fuse();
    tokio::pin!(resolutions);
    let mut pending = [VecDeque::new(), VecDeque::new()];
    let mut next_family = 1;
    let mut attempts = FuturesUnordered::new();
    let mut last_err = None;
    let next_attempt = tokio::time::sleep(Duration::ZERO);
    tokio::pin!(next_attempt);
    loop {
        tokio::select! {
            biased;
            Some(result) = resolutions.next() => match result {
                Ok(addrs) => {
                    for addr in addrs {
                        pending[usize::from(addr.is_ipv6())].push_back(addr);
                    }
                }
                Err(e) => last_err = Some(e),
            },
            Some(res) = attempts.next() => match res {
                Ok(stream) => return Ok(stream),
                Err(e) => {
                    last_err = Some(Error::new(e, ErrorKind::Network));
                    next_attempt.as_mut().reset(Instant::now());
                }
            },
            _ = &mut next_attempt, if pending.iter().any(|q| !q.is_empty()) => {
                let family = if pending[next_family].is_empty() { 1 - next_family } else { next_family };
                if let Some(addr) = pending[family].pop_front() {
                    attempts.push(connect(addr));
                }
                next_family = 1 - family;
                next_attempt.as_mut().reset(Instant::now() + CONNECT_ATTEMPT_DELAY);
            }
            else => break,
        }
    }
    Err(last_err
        .unwrap_or_else(|| Error::new(eyre!("no address to connect to"), ErrorKind::Network)))
}

/// Connects through Avahi for `.local`; other names use libc resolution. Refuses an onion.
pub async fn dial(host: &str, port: u16) -> Result<TcpStream, Error> {
    if is_onion(host) {
        Err(tor_unavailable(host))
    } else if let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>() {
        TcpStream::connect((ip, port))
            .await
            .with_kind(ErrorKind::Network)
    } else if host.ends_with(".local") {
        TcpStream::connect((resolve_mdns(host).await?, port))
            .await
            .with_kind(ErrorKind::Network)
    } else {
        connect_domain(host.to_owned(), port).await
    }
}

/// Resolves `.local` through Avahi and other names through libc, both families.
/// Refuses an onion, which never reaches DNS.
pub struct OsResolver;

impl reqwest::dns::Resolve for OsResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let name = name.as_str().to_owned();
        Box::pin(async move {
            if is_onion(&name) {
                return Err(tor_unavailable(&name).source.into());
            }
            let addrs: Vec<SocketAddr> = if name.ends_with(".local") {
                vec![SocketAddr::new(
                    resolve_mdns(&name).await.map_err(|e| e.source)?.into(),
                    0,
                )]
            } else {
                let (v6, v4) = futures::join!(
                    lookup_family(name.clone(), 0, libc::AF_INET6),
                    lookup_family(name, 0, libc::AF_INET)
                );
                match (v6, v4) {
                    (Err(e), Err(_)) => return Err(e.source.into()),
                    (v6, v4) => v6
                        .unwrap_or_default()
                        .into_iter()
                        .chain(v4.unwrap_or_default())
                        .collect(),
                }
            };
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

#[cfg(test)]
mod test {
    use imbl_value::InternedString;

    use super::{Database, TorSocks};
    use crate::net::forward::START9_BRIDGE_IFACE;
    use crate::net::host::binding::DerivedAddressInfo;
    use crate::net::service_interface::{HostnameInfo, HostnameMetadata};

    #[tokio::test]
    async fn tor_socks_follows_the_tor_bridge_binding() {
        let temp = tempfile::tempdir().unwrap();
        let db = crate::prelude::TypedPatchDb::<Database>::load_unchecked(
            patch_db::PatchDb::open(temp.path().join("db"))
                .await
                .unwrap(),
        );
        db.put(
            &patch_db::json_ptr::ROOT,
            &imbl_value::json!({ "public": { "packageData": {} } }),
        )
        .await
        .unwrap();
        let tor = TorSocks::new();
        let follower = tokio::spawn(tor.clone().follow(db.clone()));
        let settled = |want: Option<std::net::SocketAddr>| {
            let tor = tor.clone();
            async move {
                tokio::time::timeout(std::time::Duration::from_secs(5), async {
                    while tor.get() != want {
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    }
                })
                .await
                .is_ok()
            }
        };
        let row = |hostname: &str, port, gateway: &str| HostnameInfo {
            ssl: false,
            public: false,
            hostname: InternedString::intern(hostname),
            port: Some(port),
            metadata: HostnameMetadata::Ipv4 {
                gateway: gateway.parse().unwrap(),
            },
        };
        let addresses = DerivedAddressInfo {
            available: [
                row("192.168.1.5", 9050, "eth0"),
                row("172.16.0.1", 9051, START9_BRIDGE_IFACE),
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        };
        db.put(
            &"/public/packageData/tor".parse().unwrap(),
            &imbl_value::json!({ "hosts": { "socks": { "bindings": { "9050": {
                "addresses": imbl_value::to_value(&addresses).unwrap()
            } } } } }),
        )
        .await
        .unwrap();
        assert!(settled(Some("172.16.0.1:9051".parse().unwrap())).await);
        db.put(
            &"/public/packageData".parse().unwrap(),
            &imbl_value::json!({}),
        )
        .await
        .unwrap();
        assert!(settled(None).await);
        follower.abort();
    }

    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4};

    use futures::FutureExt;
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    const STALLED: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 1), 9));
    const STALLED_TOO: SocketAddr =
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 2), 9));
    const REFUSED: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 3), 9));

    async fn echo_round_trip(
        sock: &mut (impl AsyncRead + AsyncWrite + Unpin),
    ) -> Result<(), Error> {
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

    fn resolved(
        addrs: impl IntoIterator<Item = SocketAddr>,
    ) -> impl Stream<Item = Result<Vec<SocketAddr>, Error>> {
        futures::stream::iter([Ok(addrs.into_iter().collect())])
    }

    const V6: SocketAddr = SocketAddr::new(std::net::IpAddr::V6(Ipv6Addr::LOCALHOST), 9);

    async fn answer_after(
        delay: Duration,
        addrs: Vec<SocketAddr>,
    ) -> Result<Vec<SocketAddr>, Error> {
        tokio::time::sleep(delay).await;
        Ok(addrs)
    }

    #[tokio::test(start_paused = true)]
    async fn ipv4_connects_after_resolution_delay_without_waiting_for_aaaa() -> Result<(), Error> {
        let started = Instant::now();
        let target = connect_each(
            resolve_families(futures::future::pending(), async { Ok(vec![STALLED]) }),
            |addr| std::future::ready(Ok(addr)),
        )
        .await?;
        assert_eq!(target, STALLED);
        assert_eq!(started.elapsed(), RESOLUTION_DELAY);
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn ipv6_connects_without_waiting_for_a() -> Result<(), Error> {
        let started = Instant::now();
        let target = connect_each(
            resolve_families(async { Ok(vec![V6]) }, futures::future::pending()),
            |addr| std::future::ready(Ok(addr)),
        )
        .await?;
        assert_eq!(target, V6);
        assert_eq!(started.elapsed(), Duration::ZERO);
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn ipv6_arriving_during_resolution_delay_is_preferred() -> Result<(), Error> {
        let started = Instant::now();
        let delay = Duration::from_millis(30);
        let target = connect_each(
            resolve_families(answer_after(delay, vec![V6]), async { Ok(vec![STALLED]) }),
            |addr| std::future::ready(Ok(addr)),
        )
        .await?;
        assert_eq!(target, V6);
        assert_eq!(started.elapsed(), delay);
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn resolution_delay_begins_when_a_arrives() -> Result<(), Error> {
        let started = Instant::now();
        let delay = Duration::from_millis(100);
        connect_each(
            resolve_families(
                futures::future::pending(),
                answer_after(delay, vec![STALLED]),
            ),
            |addr| std::future::ready(Ok(addr)),
        )
        .await?;
        assert_eq!(started.elapsed(), delay + RESOLUTION_DELAY);
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn negative_or_failed_aaaa_releases_ipv4_without_the_remaining_delay() -> Result<(), Error>
    {
        for ipv6 in [
            Ok(Vec::new()),
            Err(Error::new(eyre!("AAAA failed"), ErrorKind::Network)),
        ] {
            let started = Instant::now();
            let delay = Duration::from_millis(10);
            let target = connect_each(
                resolve_families(
                    async {
                        tokio::time::sleep(delay).await;
                        ipv6
                    },
                    async { Ok(vec![STALLED]) },
                ),
                |addr| std::future::ready(Ok(addr)),
            )
            .await?;
            assert_eq!(target, STALLED);
            assert_eq!(started.elapsed(), delay);
        }
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn late_aaaa_precedes_remaining_ipv4_and_preserves_attempt_spacing() -> Result<(), Error>
    {
        let started = Instant::now();
        let mut starts = Vec::new();
        let target = connect_each(
            resolve_families(answer_after(Duration::from_millis(100), vec![V6]), async {
                Ok(vec![STALLED, STALLED_TOO])
            }),
            |addr| {
                starts.push((addr, started.elapsed()));
                if addr == V6 {
                    std::future::ready(Ok(addr)).boxed()
                } else {
                    futures::future::pending().boxed()
                }
            },
        )
        .await?;
        assert_eq!(target, V6);
        assert_eq!(
            starts,
            [
                (STALLED, RESOLUTION_DELAY),
                (V6, RESOLUTION_DELAY + CONNECT_ATTEMPT_DELAY)
            ]
        );
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn late_a_precedes_remaining_ipv6() -> Result<(), Error> {
        let started = Instant::now();
        let mut starts = Vec::new();
        let target = connect_each(
            resolve_families(
                async { Ok(vec![V6, SocketAddr::new(Ipv6Addr::LOCALHOST.into(), 10)]) },
                answer_after(Duration::from_millis(100), vec![STALLED]),
            ),
            |addr| {
                starts.push((addr, started.elapsed()));
                if addr == STALLED {
                    std::future::ready(Ok(addr)).boxed()
                } else {
                    futures::future::pending().boxed()
                }
            },
        )
        .await?;
        assert_eq!(target, STALLED);
        assert_eq!(
            starts,
            [(V6, Duration::ZERO), (STALLED, CONNECT_ATTEMPT_DELAY)]
        );
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn ready_answers_alternate_families_and_preserve_each_familys_order() -> Result<(), Error>
    {
        let second_v6 = SocketAddr::new(Ipv6Addr::LOCALHOST.into(), 10);
        let mut starts = Vec::new();
        let target = connect_each(
            resolve_families(async { Ok(vec![V6, second_v6]) }, async {
                Ok(vec![STALLED, STALLED_TOO])
            }),
            |addr| {
                starts.push(addr);
                std::future::ready(if addr == STALLED_TOO {
                    Ok(addr)
                } else {
                    Err(std::io::ErrorKind::ConnectionRefused.into())
                })
            },
        )
        .await?;
        assert_eq!(target, STALLED_TOO);
        assert_eq!(starts, [V6, STALLED, second_v6, STALLED_TOO]);
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn connection_failure_keeps_waiting_for_the_other_family() -> Result<(), Error> {
        let target = connect_each(
            resolve_families(answer_after(Duration::from_millis(100), vec![V6]), async {
                Ok(vec![STALLED])
            }),
            |addr| {
                std::future::ready(if addr == V6 {
                    Ok(addr)
                } else {
                    Err(std::io::ErrorKind::ConnectionRefused.into())
                })
            },
        )
        .await?;
        assert_eq!(target, V6);
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn all_resolution_failures_return_an_error() {
        let error = || Error::new(eyre!("lookup failed"), ErrorKind::Network);
        let result = connect_each(
            resolve_families(async { Err(error()) }, async { Err(error()) }),
            |_| std::future::ready(Ok(())),
        )
        .await;
        assert!(result.is_err());
        assert!(
            connect_each(resolved([]), |_| std::future::ready(Ok(())))
                .await
                .is_err()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn success_drops_unfinished_resolution() -> Result<(), Error> {
        let (sender, receiver) = tokio::sync::oneshot::channel::<()>();
        connect_each(
            resolve_families(
                async move {
                    let _sender = sender;
                    futures::future::pending().await
                },
                async { Ok(vec![STALLED]) },
            ),
            |addr| std::future::ready(Ok(addr)),
        )
        .await?;
        assert!(receiver.await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn system_lookup_preserves_family_and_requested_port() -> Result<(), Error> {
        for (host, family, ip) in [
            (
                "127.0.0.1",
                libc::AF_INET,
                std::net::IpAddr::V4(Ipv4Addr::LOCALHOST),
            ),
            (
                "::1",
                libc::AF_INET6,
                std::net::IpAddr::V6(Ipv6Addr::LOCALHOST),
            ),
        ] {
            assert_eq!(
                lookup_family(host.to_owned(), 1234, family).await?,
                vec![SocketAddr::new(ip, 1234)]
            );
        }
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn the_second_attempt_starts_one_delay_after_a_stalled_first() -> Result<(), Error> {
        let target = echo_server().await?;
        let started = Instant::now();
        let mut starts = Vec::new();
        let mut sock = connect_each(resolved([STALLED, target]), |addr| {
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
    async fn stalled_attempts_do_not_hold_up_the_next() -> Result<(), Error> {
        let target = echo_server().await?;
        let started = Instant::now();
        let mut starts = Vec::new();
        let mut sock = connect_each(resolved([STALLED, STALLED_TOO, target]), |addr| {
            starts.push((addr, started.elapsed()));
            if addr == target {
                TcpStream::connect(addr).boxed()
            } else {
                futures::future::pending().boxed()
            }
        })
        .await?;
        assert_eq!(
            starts,
            [
                (STALLED, Duration::ZERO),
                (STALLED_TOO, CONNECT_ATTEMPT_DELAY),
                (target, 2 * CONNECT_ATTEMPT_DELAY),
            ]
        );
        echo_round_trip(&mut sock).await
    }

    #[tokio::test(start_paused = true)]
    async fn a_failure_starts_the_next_attempt_at_once() -> Result<(), Error> {
        let target = echo_server().await?;
        let started = Instant::now();
        let mut starts = Vec::new();
        let mut sock = connect_each(resolved([STALLED, REFUSED, target]), |addr| {
            starts.push((addr, started.elapsed()));
            if addr == STALLED {
                futures::future::pending().boxed()
            } else if addr == REFUSED {
                async {
                    tokio::time::sleep(Duration::from_millis(100)).await;
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
                (target, CONNECT_ATTEMPT_DELAY + Duration::from_millis(100)),
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
}
