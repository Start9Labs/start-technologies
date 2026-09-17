//! HTTP→HTTPS redirect on the WAN IPv4 address, port 80.
//!
//! StartOS asks its IPv4 gateway for no port-80 mapping: it publishes only
//! 443 and expects the gateway to answer plain HTTP at the public address with
//! a redirect to HTTPS (start-core `net/vhost.rs`; StartTunnel implements its
//! half in `tunnel/redirect.rs`, which this mirrors). Without it, a LAN client
//! opening `http://sub.example.com` for a domain that points at the WAN
//! address hairpins to the router's own port 80 and gets the router UI.
//!
//! The listener is bound to the WAN address specifically, beside the daemon's
//! wildcard `:80` (both SO_REUSEPORT). The kernel delivers a connection to the
//! most specific bound socket, so only connections addressed to the WAN
//! address reach the redirect; the LAN gateway addresses and `router.lan`
//! still reach the UI. The socket is never wired into the axum app, so it can
//! serve nothing but the redirect.
//!
//! No firewall rule is touched. Who may reach WAN:80 remains the Remote Access
//! policy; this changes only what answers there.
//!
//! Bound only while WAN 443 leaves the router — an enabled WAN DNAT covering
//! tcp/443, or a live hostname route on 443 — so a router publishing nothing
//! on 443 behaves exactly as before. Yields to a DNAT covering tcp/80: fw4's
//! prerouting DNAT would win anyway, and not binding keeps the state honest.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

use startos::tunnel::forward::sni::SniRoute;
use startos::util::future::NonDetachingJoinHandle;
use tokio::net::TcpListener;

use crate::port_control::wan_dnat_covers;

pub const HTTP_PORT: u16 = 80;
pub const HTTPS_PORT: u16 = 443;

/// A redirect is a single exchange, and the WAN side may be public, so an idle
/// or dribbling connection must not hold a task open.
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(15);
const ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(250);

/// The address the redirect should listen on, if any: `wan`, while WAN 443
/// leaves the router — an enabled WAN DNAT whose protocol includes TCP and
/// whose external range covers 443, or a live hostname route on 443 keyed to
/// `wan` — and no such DNAT claims tcp/80. A Remote Access ACCEPT on 443 does
/// not count: there the router itself answers 443.
pub(crate) fn desired(
    firewall: &uciedit::Config<'_>,
    routes: &[SniRoute],
    wan: Option<Ipv4Addr>,
) -> Option<Ipv4Addr> {
    let wan = wan?;
    if wan_dnat_covers(firewall, HTTP_PORT) {
        return None;
    }
    let published = wan_dnat_covers(firewall, HTTPS_PORT)
        || routes
            .iter()
            .any(|route| route.ext_port == HTTPS_PORT && route.ext_ip == wan);
    published.then_some(wan)
}

/// A live redirect listener. Dropping it aborts the accept loop, which closes
/// the socket.
pub(crate) struct Redirect {
    addr: SocketAddrV4,
    _task: NonDetachingJoinHandle<()>,
}

impl Redirect {
    /// Binds `ip:80` beside the daemon's wildcard listener.
    pub(crate) fn bind(ip: Ipv4Addr) -> std::io::Result<Self> {
        let addr = SocketAddrV4::new(ip, HTTP_PORT);
        // Tests can't bind a WAN address on a privileged port; the accept loop
        // is exercised on loopback through `spawn`.
        #[cfg(test)]
        let bind_addr = SocketAddr::from(([127, 0, 0, 1], 0));
        #[cfg(not(test))]
        let bind_addr = SocketAddr::V4(addr);
        let listener = startos::net::utils::bind_tokio_listener_reuse_port(bind_addr)?;
        Ok(Self::spawn(addr, listener))
    }

    fn spawn(addr: SocketAddrV4, listener: TcpListener) -> Self {
        let task = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, peer)) => {
                        tokio::spawn(async move {
                            match tokio::time::timeout(
                                CONNECTION_TIMEOUT,
                                startos::net::http::handle_http_on_https(stream),
                            )
                            .await
                            {
                                Ok(Ok(())) => {}
                                Ok(Err(e)) => tracing::debug!(
                                    "http redirect on {addr}: connection from {peer} closed: {e}"
                                ),
                                Err(_) => tracing::debug!(
                                    "http redirect on {addr}: connection from {peer} timed out"
                                ),
                            }
                        });
                    }
                    Err(e) => {
                        tracing::warn!("http redirect on {addr}: accept failed: {e}");
                        tokio::time::sleep(ACCEPT_RETRY_DELAY).await;
                    }
                }
            }
        });
        Self {
            addr,
            _task: task.into(),
        }
    }

    pub(crate) fn addr(&self) -> SocketAddrV4 {
        self.addr
    }

    pub(crate) fn ip(&self) -> Ipv4Addr {
        *self.addr.ip()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    const WAN: Ipv4Addr = Ipv4Addr::new(203, 0, 113, 7);

    fn dnat(port: &str, proto: &str, enabled: &str) -> String {
        format!(
            "config redirect 'pp_a'\n\
             \toption name 'NAS'\n\
             \toption src 'wan'\n\
             \toption dest 'lan'\n\
             \toption target 'DNAT'\n\
             {proto}\
             \toption src_dport '{port}'\n\
             \toption dest_port '{port}'\n\
             \toption dest_ip '192.168.1.50'\n\
             \toption enabled '{enabled}'\n\
             \toption _pp_id 'a'\n\
             \toption _pp_mac 'AA:AA:AA:AA:AA:AA'\n\n"
        )
    }

    const TCP: &str = "\tlist proto 'tcp'\n";
    const UDP: &str = "\tlist proto 'udp'\n";

    const REMOTE_443: &str = "config rule 'startwrt_remote_443'\n\
        \toption name 'startwrt_remote_443'\n\
        \toption src 'wan'\n\
        \tlist proto 'tcp'\n\
        \toption dest_port '443'\n\
        \toption target 'ACCEPT'\n\n";

    fn route(ext_ip: Ipv4Addr, ext_port: u16) -> SniRoute {
        SniRoute {
            ext_ip,
            ext_port,
            hostname: "nas.example.com".into(),
            target: SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 50), 443),
            remaining_secs: Some(3600),
        }
    }

    async fn desired_for(
        firewall: &str,
        routes: &[SniRoute],
        wan: Option<Ipv4Addr>,
    ) -> Option<Ipv4Addr> {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("firewall"), firewall).unwrap();
        let arena = uciedit::Arena::new();
        let cfgs = uciedit::parse_all(dir.path(), &arena, &["firewall"])
            .await
            .unwrap();
        desired(&cfgs["firewall"], routes, wan)
    }

    #[tokio::test]
    async fn nothing_published_means_no_redirect() {
        assert_eq!(desired_for("", &[], Some(WAN)).await, None);
        // The router answering 443 itself (Remote Access) is not "published".
        assert_eq!(desired_for(REMOTE_443, &[], Some(WAN)).await, None);
    }

    #[tokio::test]
    async fn no_wan_address_means_no_redirect() {
        assert_eq!(
            desired_for(&dnat("443", TCP, "1"), &[route(WAN, 443)], None).await,
            None
        );
    }

    #[tokio::test]
    async fn wan_dnat_on_443_activates() {
        assert_eq!(
            desired_for(&dnat("443", TCP, "1"), &[], Some(WAN)).await,
            Some(WAN)
        );
        // fw4 reads an empty protocol list as TCP and UDP.
        assert_eq!(
            desired_for(&dnat("443", "", "1"), &[], Some(WAN)).await,
            Some(WAN)
        );
        assert_eq!(
            desired_for(&dnat("400-500", TCP, "1"), &[], Some(WAN)).await,
            Some(WAN)
        );
    }

    #[tokio::test]
    async fn only_a_live_tcp_dnat_covering_443_counts() {
        assert_eq!(
            desired_for(&dnat("443", TCP, "0"), &[], Some(WAN)).await,
            None
        );
        assert_eq!(
            desired_for(&dnat("443", UDP, "1"), &[], Some(WAN)).await,
            None
        );
        assert_eq!(
            desired_for(&dnat("8443", TCP, "1"), &[], Some(WAN)).await,
            None
        );
    }

    #[tokio::test]
    async fn hostname_route_on_443_activates() {
        assert_eq!(
            desired_for("", &[route(WAN, 443)], Some(WAN)).await,
            Some(WAN)
        );
        assert_eq!(desired_for("", &[route(WAN, 8443)], Some(WAN)).await, None);
        // A route keyed to a stale WAN address is not this address's 443.
        assert_eq!(
            desired_for("", &[route(Ipv4Addr::new(198, 51, 100, 9), 443)], Some(WAN)).await,
            None
        );
    }

    #[tokio::test]
    async fn a_dnat_on_80_takes_precedence() {
        let fw = format!("{}{}", dnat("443", TCP, "1"), dnat("80", TCP, "1"));
        assert_eq!(desired_for(&fw, &[], Some(WAN)).await, None);
        let fw = format!("{}{}", dnat("443", TCP, "1"), dnat("80", TCP, "0"));
        assert_eq!(desired_for(&fw, &[], Some(WAN)).await, Some(WAN));
    }

    fn spawn_loopback() -> (SocketAddr, Redirect) {
        let listener = startos::net::utils::bind_tokio_listener_reuse_port(SocketAddr::from((
            [127, 0, 0, 1],
            0,
        )))
        .unwrap();
        let local = listener.local_addr().unwrap();
        (
            local,
            Redirect::spawn(SocketAddrV4::new(WAN, HTTP_PORT), listener),
        )
    }

    async fn exchange(addr: SocketAddr, request: &str) -> String {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut out = Vec::new();
        stream.read_to_end(&mut out).await.unwrap();
        String::from_utf8_lossy(&out).into_owned()
    }

    fn location(response: &str) -> Option<String> {
        response
            .lines()
            .find_map(|line| {
                line.split_once(':')
                    .filter(|(k, _)| k.eq_ignore_ascii_case("location"))
            })
            .map(|(_, v)| v.trim().to_string())
    }

    #[tokio::test]
    async fn redirects_to_https_keeping_host_and_path() {
        let (local, _redirect) = spawn_loopback();
        let res = exchange(
            local,
            "GET /x/y?z=1 HTTP/1.1\r\nHost: sub.example.com\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(res.starts_with("HTTP/1.1 307"), "{res}");
        assert_eq!(
            location(&res).as_deref(),
            Some("https://sub.example.com/x/y?z=1"),
            "{res}"
        );
    }

    #[tokio::test]
    async fn never_serves_anything_but_a_redirect() {
        // A request naming the router's own WAN address must not reach the UI:
        // the listener is not the app router, whatever the Host says.
        let (local, _redirect) = spawn_loopback();
        let res = exchange(
            local,
            "GET / HTTP/1.1\r\nHost: 203.0.113.7\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(res.starts_with("HTTP/1.1 307"), "{res}");
        assert_eq!(
            location(&res).as_deref(),
            Some("https://203.0.113.7/"),
            "{res}"
        );
        assert!(!res.contains("<html"), "{res}");
    }

    #[tokio::test]
    async fn requires_a_host() {
        let (local, _redirect) = spawn_loopback();
        let res = exchange(local, "GET / HTTP/1.0\r\n\r\n").await;
        assert!(res.contains(" 400 "), "{res}");
    }

    #[tokio::test]
    async fn dropping_the_handle_closes_the_socket() {
        let (local, redirect) = spawn_loopback();
        assert!(TcpStream::connect(local).await.is_ok());
        drop(redirect);
        for _ in 0..40 {
            if TcpStream::connect(local).await.is_err() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("listener still accepting after drop");
    }
}
