//! HTTP→HTTPS redirect for the router's public IPv4 address, port 80.
//!
//! StartOS asks its IPv4 gateway for no port-80 mapping: it publishes only
//! 443 and expects the gateway to answer plain HTTP at the public address with
//! a redirect to HTTPS (start-core `net/vhost.rs`; StartTunnel implements its
//! half in `tunnel/redirect.rs`, which this mirrors). Without it, a LAN client
//! opening `http://sub.example.com` for a domain that points at the WAN
//! address hairpins to the router's own port 80 and gets the router UI, and a
//! client on the Internet is refused unless Remote Access admits it.
//!
//! Two sockets serve the redirect. Neither is wired into the axum app, so
//! they can serve nothing but a 307:
//!
//! - `<WAN IPv4>:80`, beside the daemon's wildcard `:80` (both SO_REUSEPORT;
//!   the kernel delivers a connection to the most specific bound socket). LAN
//!   hairpins arrive from the lan zone, never through WAN-side DNAT, and land
//!   here. The LAN gateway addresses and `router.lan` still reach the UI.
//! - `0.0.0.0:REDIRECT_PORT`, the target of a wan-zone DNAT that rewrites
//!   tcp/80 to it ([`section`]). fw4 accepts a DNAT'd connection at input, so
//!   the Internet side reaches the redirect under every Remote Access mode and
//!   the UI's wildcard socket never sees WAN:80. An ACCEPT rule would be
//!   fail-open — fw4 loads UCI before the daemon binds — where the DNAT is
//!   fail-closed: with nothing bound, the connection is reset.
//!
//! Active only while WAN 443 leaves the router — an enabled WAN DNAT covering
//! tcp/443, or a live hostname route on 443 — so a router publishing nothing
//! on 443 behaves exactly as before. Yields to a DNAT covering tcp/80.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::Path;
use std::time::Duration;

use startos::tunnel::forward::sni::SniRoute;
use startos::util::future::NonDetachingJoinHandle;
use tokio::net::TcpListener;
use uciedit::openwrt::FirewallRedirect;
use uciedit::{dump_all, parse_all, Arena};

use crate::port_control::{wan_dnat_covers, UCI_RETRIES};
use crate::prelude::*;

pub const HTTP_PORT: u16 = 80;
pub const HTTPS_PORT: u16 = 443;
/// Router-local target of the WAN tcp/80 DNAT. Nothing else may bind it.
pub(crate) const REDIRECT_PORT: u16 = 8880;
/// UCI name of the WAN tcp/80 DNAT. Nothing else may hold it.
pub(crate) const SECTION: &str = "startwrt_http_redirect";

/// A redirect is a single exchange, and the WAN side may be public, so an idle
/// or dribbling connection must not hold a task open.
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(15);
const ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Desired {
    /// The WAN tcp/80 DNAT and the socket behind it.
    pub section: bool,
    /// The address-specific port-80 socket for LAN hairpins.
    pub hairpin: Option<Ipv4Addr>,
}

/// Active while WAN 443 leaves the router — an enabled WAN DNAT whose protocol
/// includes TCP and whose external range covers 443, or a live hostname route
/// on 443 — and no other DNAT claims tcp/80. A Remote Access ACCEPT on 443
/// does not count: there the router itself answers 443. The hairpin socket
/// needs the WAN address; the DNAT follows the interface and does not.
pub(crate) fn desired(
    firewall: &uciedit::Config<'_>,
    routes: &[SniRoute],
    wan: Option<Ipv4Addr>,
) -> Desired {
    let section = !wan_dnat_covers(firewall, HTTP_PORT)
        && (wan_dnat_covers(firewall, HTTPS_PORT)
            || routes.iter().any(|route| route.ext_port == HTTPS_PORT));
    Desired {
        section,
        hairpin: wan.filter(|_| section),
    }
}

/// The WAN tcp/80 DNAT. Without `dest_ip`, fw4 renders it as an nft
/// `redirect` to the incoming interface's own address, builds no reflection
/// rule, and needs no `dest` zone. `family` keeps it off IPv6.
fn section() -> FirewallRedirect {
    FirewallRedirect {
        name: "HTTP to HTTPS redirect".into(),
        src: "wan".into(),
        proto: vec!["tcp".into()],
        src_dport: Some(HTTP_PORT.to_string()),
        dest_port: Some(REDIRECT_PORT.to_string()),
        target: "DNAT".into(),
        enabled: Some("1".into()),
        family: Some("ipv4".into()),
        reflection: Some(false),
        _startwrt_http_redirect: Some("1".into()),
        ..Default::default()
    }
}

/// Compares the fields fw4 reads.
fn section_matches(existing: &FirewallRedirect) -> bool {
    let want = section();
    existing.src == want.src
        && existing.target == want.target
        && existing.proto == want.proto
        && existing.src_dport == want.src_dport
        && existing.dest_port == want.dest_port
        && existing.dest.is_none()
        && existing.dest_ip.is_none()
        && existing.src_ip.is_none()
        && existing.enabled == want.enabled
        && existing.family == want.family
        && existing.reflection == want.reflection
        && existing.reflection_zone.is_empty()
}

/// Writes or removes the WAN tcp/80 DNAT. Returns whether UCI changed. A
/// section carrying the name without the marker, or the marker under another
/// name, is not ours and is replaced.
pub(crate) async fn reconcile_section_uci(uci_root: &Path, want: bool) -> Result<bool, Error> {
    let mut retries = UCI_RETRIES;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(uci_root, &arena, &["firewall"]).await?;
        let mut kept = false;
        let mut changed = false;
        cfgs["firewall"].sections.retain(|sec| {
            let named = sec.name().as_deref() == Some(SECTION);
            let redirect = sec.get::<FirewallRedirect>().ok();
            let marked = redirect
                .as_ref()
                .is_some_and(|r| r._startwrt_http_redirect.is_some());
            if !named && !marked {
                return true;
            }
            let keep =
                want && !kept && named && marked && redirect.as_ref().is_some_and(section_matches);
            if keep {
                kept = true;
            } else {
                changed = true;
            }
            keep
        });
        if want && !kept {
            cfgs["firewall"].append(&section(), Some(SECTION))?;
            changed = true;
        }
        if !changed {
            return Ok(false);
        }
        match dump_all(uci_root, cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(e) => return Err(e.into()),
            Ok(()) => return Ok(true),
        }
    }
}

/// The live redirect sockets. Dropping it aborts the accept loops, which
/// closes the sockets.
pub(crate) struct Redirect {
    local: Listener,
    hairpin: Option<Listener>,
}

impl Redirect {
    /// Binds `0.0.0.0:REDIRECT_PORT` and, given a WAN address, `wan:80`
    /// beside the daemon's wildcard listener.
    pub(crate) fn bind(hairpin: Option<Ipv4Addr>) -> std::io::Result<Self> {
        let local = Listener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, REDIRECT_PORT))?;
        let mut redirect = Self {
            local,
            hairpin: None,
        };
        redirect.rebind_hairpin(hairpin)?;
        Ok(redirect)
    }

    /// Replaces the `wan:80` socket; `None` drops it. On failure the old
    /// socket is gone and the DNAT half keeps serving.
    pub(crate) fn rebind_hairpin(&mut self, ip: Option<Ipv4Addr>) -> std::io::Result<()> {
        if self.hairpin_ip() == ip {
            return Ok(());
        }
        self.hairpin = None;
        if let Some(ip) = ip {
            self.hairpin = Some(Listener::bind(SocketAddrV4::new(ip, HTTP_PORT))?);
        }
        Ok(())
    }

    pub(crate) fn hairpin_ip(&self) -> Option<Ipv4Addr> {
        self.hairpin.as_ref().map(|listener| *listener.addr.ip())
    }
}

struct Listener {
    addr: SocketAddrV4,
    /// Where the socket actually is; differs from `addr` only in tests.
    #[cfg_attr(not(test), allow(dead_code))]
    bound: SocketAddr,
    _task: NonDetachingJoinHandle<()>,
}

impl Listener {
    fn bind(addr: SocketAddrV4) -> std::io::Result<Self> {
        // Tests can't bind a WAN address on a privileged port; the accept
        // loop is exercised on loopback instead.
        #[cfg(test)]
        let bind_addr = SocketAddr::from(([127, 0, 0, 1], 0));
        #[cfg(not(test))]
        let bind_addr = SocketAddr::V4(addr);
        let listener = startos::net::utils::bind_tokio_listener_reuse_port(bind_addr)?;
        Ok(Self::spawn(addr, listener))
    }

    fn spawn(addr: SocketAddrV4, listener: TcpListener) -> Self {
        let bound = listener.local_addr().unwrap_or(SocketAddr::V4(addr));
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
            bound,
            _task: task.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::*;

    const WAN: Ipv4Addr = Ipv4Addr::new(203, 0, 113, 7);
    const ON: Desired = Desired {
        section: true,
        hairpin: Some(WAN),
    };
    const OFF: Desired = Desired {
        section: false,
        hairpin: None,
    };

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

    const HEADER: &str = "config redirect startwrt_http_redirect";

    /// What `section()` serializes to.
    const SECTION_UCI: &str = "config redirect startwrt_http_redirect\n\
        \toption name 'HTTP to HTTPS redirect'\n\
        \toption src 'wan'\n\
        \tlist proto 'tcp'\n\
        \toption src_dport '80'\n\
        \toption dest_port '8880'\n\
        \toption target 'DNAT'\n\
        \toption enabled '1'\n\
        \toption family 'ipv4'\n\
        \toption reflection '0'\n\
        \toption _startwrt_http_redirect '1'\n\n";

    fn route(ext_ip: Ipv4Addr, ext_port: u16) -> SniRoute {
        SniRoute {
            ext_ip,
            ext_port,
            hostname: "nas.example.com".into(),
            target: SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 50), 443),
            remaining_secs: Some(3600),
        }
    }

    async fn desired_for(firewall: &str, routes: &[SniRoute], wan: Option<Ipv4Addr>) -> Desired {
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
        assert_eq!(desired_for("", &[], Some(WAN)).await, OFF);
        // The router answering 443 itself (Remote Access) is not "published".
        assert_eq!(desired_for(REMOTE_443, &[], Some(WAN)).await, OFF);
    }

    #[tokio::test]
    async fn no_wan_address_keeps_the_section_and_skips_the_hairpin() {
        assert_eq!(
            desired_for(&dnat("443", TCP, "1"), &[route(WAN, 443)], None).await,
            Desired {
                section: true,
                hairpin: None
            }
        );
    }

    #[tokio::test]
    async fn wan_dnat_on_443_activates() {
        assert_eq!(
            desired_for(&dnat("443", TCP, "1"), &[], Some(WAN)).await,
            ON
        );
        // fw4 reads an empty protocol list as TCP and UDP.
        assert_eq!(desired_for(&dnat("443", "", "1"), &[], Some(WAN)).await, ON);
        assert_eq!(
            desired_for(&dnat("400-500", TCP, "1"), &[], Some(WAN)).await,
            ON
        );
    }

    #[tokio::test]
    async fn only_a_live_tcp_dnat_covering_443_counts() {
        assert_eq!(
            desired_for(&dnat("443", TCP, "0"), &[], Some(WAN)).await,
            OFF
        );
        assert_eq!(
            desired_for(&dnat("443", UDP, "1"), &[], Some(WAN)).await,
            OFF
        );
        assert_eq!(
            desired_for(&dnat("8443", TCP, "1"), &[], Some(WAN)).await,
            OFF
        );
    }

    #[tokio::test]
    async fn hostname_route_on_443_activates() {
        assert_eq!(desired_for("", &[route(WAN, 443)], Some(WAN)).await, ON);
        assert_eq!(desired_for("", &[route(WAN, 8443)], Some(WAN)).await, OFF);
        // A route keyed to a stale WAN address is re-keyed by maintenance;
        // its 443 is still published.
        assert_eq!(
            desired_for("", &[route(Ipv4Addr::new(198, 51, 100, 9), 443)], Some(WAN)).await,
            ON
        );
    }

    #[tokio::test]
    async fn a_dnat_on_80_takes_precedence() {
        let fw = format!("{}{}", dnat("443", TCP, "1"), dnat("80", TCP, "1"));
        assert_eq!(desired_for(&fw, &[], Some(WAN)).await, OFF);
        let fw = format!("{}{}", dnat("443", TCP, "1"), dnat("80", TCP, "0"));
        assert_eq!(desired_for(&fw, &[], Some(WAN)).await, ON);
    }

    #[tokio::test]
    async fn own_section_does_not_gate_itself() {
        let fw = format!("{}{}", dnat("443", TCP, "1"), SECTION_UCI);
        assert_eq!(desired_for(&fw, &[], Some(WAN)).await, ON);
        assert_eq!(desired_for(SECTION_UCI, &[], Some(WAN)).await, OFF);
    }

    #[tokio::test]
    async fn reconcile_section_writes_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("firewall");
        let firewall = || std::fs::read_to_string(&path).unwrap();
        let foreign = format!("{}{}", REMOTE_443, dnat("443", TCP, "1"));
        std::fs::write(&path, &foreign).unwrap();

        assert!(reconcile_section_uci(dir.path(), true).await.unwrap());
        let written = firewall();
        for line in SECTION_UCI.trim_end().lines() {
            assert!(written.contains(line), "missing {line:?} in:\n{written}");
        }
        let ours = &written[written.find(HEADER).unwrap()..];
        assert!(!ours.contains("dest_ip"), "{written}");
        assert!(written.contains("startwrt_remote_443"), "{written}");
        assert!(written.contains("config redirect 'pp_a'"), "{written}");

        assert!(!reconcile_section_uci(dir.path(), true).await.unwrap());
        assert_eq!(firewall(), written, "a settled section is not rewritten");

        std::fs::write(
            &path,
            written.replace("dest_port '8880'", "dest_port '9999'"),
        )
        .unwrap();
        assert!(reconcile_section_uci(dir.path(), true).await.unwrap());
        let healed = firewall();
        assert_eq!(healed.matches(HEADER).count(), 1, "{healed}");
        assert!(healed.contains("dest_port '8880'"), "{healed}");

        assert!(reconcile_section_uci(dir.path(), false).await.unwrap());
        let removed = firewall();
        assert!(!removed.contains(SECTION), "{removed}");
        assert!(removed.contains("startwrt_remote_443"), "{removed}");
        assert!(!reconcile_section_uci(dir.path(), false).await.unwrap());
    }

    #[tokio::test]
    async fn reconcile_replaces_impostors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("firewall");
        let unmarked = SECTION_UCI.replace("\toption _startwrt_http_redirect '1'\n", "");
        let stray = SECTION_UCI.replacen(HEADER, "config redirect stray", 1);
        std::fs::write(&path, format!("{unmarked}{stray}")).unwrap();

        assert!(reconcile_section_uci(dir.path(), true).await.unwrap());
        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(written.matches(HEADER).count(), 1, "{written}");
        assert_eq!(
            written.matches("_startwrt_http_redirect '1'").count(),
            1,
            "{written}"
        );
        assert!(!written.contains("stray"), "{written}");

        assert!(reconcile_section_uci(dir.path(), false).await.unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap().trim(), "");
    }

    fn spawn_loopback() -> (SocketAddr, Listener) {
        let listener = startos::net::utils::bind_tokio_listener_reuse_port(SocketAddr::from((
            [127, 0, 0, 1],
            0,
        )))
        .unwrap();
        let local = listener.local_addr().unwrap();
        (
            local,
            Listener::spawn(SocketAddrV4::new(WAN, HTTP_PORT), listener),
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
        let (local, _listener) = spawn_loopback();
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
        let (local, _listener) = spawn_loopback();
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
        let (local, _listener) = spawn_loopback();
        let res = exchange(local, "GET / HTTP/1.0\r\n\r\n").await;
        assert!(res.contains(" 400 "), "{res}");
    }

    async fn closed(addr: SocketAddr) -> bool {
        for _ in 0..40 {
            if TcpStream::connect(addr).await.is_err() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        false
    }

    #[tokio::test]
    async fn both_sockets_serve_the_redirect() {
        let redirect = Redirect::bind(Some(WAN)).unwrap();
        assert_eq!(redirect.hairpin_ip(), Some(WAN));
        let request = "GET / HTTP/1.1\r\nHost: sub.example.com\r\nConnection: close\r\n\r\n";
        for addr in [
            redirect.local.bound,
            redirect.hairpin.as_ref().unwrap().bound,
        ] {
            let res = exchange(addr, request).await;
            assert!(res.starts_with("HTTP/1.1 307"), "{addr}: {res}");
        }
    }

    #[tokio::test]
    async fn rebinding_the_hairpin_keeps_the_local_socket() {
        let mut redirect = Redirect::bind(Some(WAN)).unwrap();
        let local = redirect.local.bound;
        let old = redirect.hairpin.as_ref().unwrap().bound;
        redirect.rebind_hairpin(None).unwrap();
        assert_eq!(redirect.hairpin_ip(), None);
        assert!(closed(old).await, "old hairpin socket still accepting");
        assert!(TcpStream::connect(local).await.is_ok());
        redirect
            .rebind_hairpin(Some(Ipv4Addr::new(198, 51, 100, 9)))
            .unwrap();
        assert_eq!(redirect.hairpin_ip(), Some(Ipv4Addr::new(198, 51, 100, 9)));
        assert!(TcpStream::connect(redirect.hairpin.as_ref().unwrap().bound)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn dropping_the_handle_closes_both_sockets() {
        let redirect = Redirect::bind(Some(WAN)).unwrap();
        let local = redirect.local.bound;
        let hairpin = redirect.hairpin.as_ref().unwrap().bound;
        assert!(TcpStream::connect(local).await.is_ok());
        drop(redirect);
        assert!(
            closed(local).await,
            "local socket still accepting after drop"
        );
        assert!(
            closed(hairpin).await,
            "hairpin socket still accepting after drop"
        );
    }
}
