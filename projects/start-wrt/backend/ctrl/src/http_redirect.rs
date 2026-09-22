//! HTTP→HTTPS redirect on port 80 while WAN 443 is published.
//!
//! StartOS publishes only 443 and expects its gateway to redirect plain HTTP
//! at the public address (start-core `net/vhost.rs`).
//!
//! [`redirect_public_http`] serves the UI to a client on a connected subnet
//! off the WAN, at an address that is not the WAN's. Every other IPv4 request
//! gets a 307 to the same authority over HTTPS, or a 400 without one. Port
//! control admits WAN-side tcp/80 at the WAN address with an ACCEPT rule in
//! the SNI admission set.

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Router;
use ipnet::Ipv4Net;
use startos::net::http::{https_redirect_uri, request_authority};
use startos::net::web_server::TcpMetadata;
use startos::tunnel::forward::sni::SniRoute;
use uciedit::openwrt::FirewallRule;
use uciedit::{parse_all, Arena};

use crate::bins::daemon::WebserverListener;
use crate::error::ErrorKind;
use crate::invoke::Invoke;
use crate::port_control::{parse_port_range, uci_task, wan_dnat_covers, KIND_SNI};

pub const HTTP_PORT: u16 = 80;
pub const HTTPS_PORT: u16 = 443;

/// Distinguishes the redirect's admission rule from a hostname route's.
pub(crate) const RULE_NAME: &str = "HTTP to HTTPS redirect";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Gate {
    /// The firewall may be admitting WAN-side [`HTTP_PORT`].
    pub admitted: bool,
    pub wan: Vec<Ipv4Addr>,
    /// Connected IPv4 subnets off the WAN, loopback included.
    pub local: Vec<Ipv4Net>,
}

static GATE: RwLock<Gate> = RwLock::new(Gate {
    admitted: false,
    wan: Vec::new(),
    local: Vec::new(),
});

static REFRESHED: tokio::sync::Mutex<Option<Instant>> = tokio::sync::Mutex::const_new(None);
const REFRESH_FLOOR: Duration = Duration::from_secs(5);

fn gate() -> Gate {
    GATE.read().unwrap_or_else(|e| e.into_inner()).clone()
}

pub(crate) fn set_admitted(admitted: bool) {
    GATE.write().unwrap_or_else(|e| e.into_inner()).admitted = admitted;
}

/// Unreadable addresses leave no client trusted.
pub(crate) async fn refresh_addrs() {
    let mut refreshed = REFRESHED.lock().await;
    read_addrs().await;
    *refreshed = Some(Instant::now());
}

/// Returns whether it reread.
async fn refresh_stale_addrs() -> bool {
    let mut refreshed = REFRESHED.lock().await;
    if refreshed.is_some_and(|at| at.elapsed() < REFRESH_FLOOR) {
        return false;
    }
    read_addrs().await;
    *refreshed = Some(Instant::now());
    true
}

async fn read_addrs() {
    // Subnets first: a WAN address appearing between the reads stays untrusted.
    let connected = tokio::process::Command::new("ip")
        .args(["-j", "-4", "addr", "show"])
        .invoke(ErrorKind::Network.into())
        .await
        .ok()
        .and_then(|out| String::from_utf8(out).ok())
        .map(|json| parse_connected(&json))
        .unwrap_or_default();
    let wan = tokio::task::spawn_blocking(crate::system::wan_ipv4_addrs)
        .await
        .unwrap_or_default();
    let local = off_wan(connected, &wan);
    let mut gate = GATE.write().unwrap_or_else(|e| e.into_inner());
    gate.wan = wan;
    gate.local = local;
}

fn parse_connected(json: &str) -> Vec<Ipv4Net> {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    parsed
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|iface| iface.get("addr_info")?.as_array())
        .flatten()
        .filter_map(|info| {
            let addr = info.get("local")?.as_str()?.parse().ok()?;
            let prefix = info.get("prefixlen")?.as_u64()?;
            Ipv4Net::new(addr, u8::try_from(prefix).ok()?).ok()
        })
        .collect()
}

fn off_wan(connected: Vec<Ipv4Net>, wan: &[Ipv4Addr]) -> Vec<Ipv4Net> {
    connected
        .into_iter()
        .filter(|net| !wan.contains(&net.addr()))
        .collect()
}

/// An unreadable firewall counts as admitting.
pub async fn seed(uci_root: PathBuf) {
    refresh_addrs().await;
    match uci_task(move || async move {
        let arena = Arena::new();
        let cfgs = parse_all(&uci_root, &arena, &["firewall"]).await?;
        Ok(port_admitted(&cfgs["firewall"]))
    })
    .await
    {
        Ok(admitted) => set_admitted(admitted),
        Err(e) => {
            tracing::warn!("http redirect: reading the firewall failed: {e}");
            set_admitted(true);
        }
    }
}

fn sni_rules_on_http_port<'a>(
    firewall: &'a uciedit::Config<'_>,
) -> impl Iterator<Item = FirewallRule> + 'a {
    firewall
        .sections
        .iter()
        .filter_map(|sec| sec.get::<FirewallRule>().ok())
        .filter(|rule| {
            rule._apf_label.as_deref() == Some(KIND_SNI)
                && rule
                    .dest_port
                    .as_deref()
                    .and_then(parse_port_range)
                    .is_some_and(|(lo, _)| lo == HTTP_PORT)
        })
}

/// Any SNI-labelled rule on [`HTTP_PORT`], a hostname route's included.
fn port_admitted(firewall: &uciedit::Config<'_>) -> bool {
    sni_rules_on_http_port(firewall).next().is_some()
}

pub(crate) fn admission_present(firewall: &uciedit::Config<'_>) -> bool {
    sni_rules_on_http_port(firewall).any(|rule| rule.name == RULE_NAME)
}

/// A DNAT or a hostname route forwards WAN tcp/443, and neither holds tcp/80.
pub(crate) fn desired(firewall: &uciedit::Config<'_>, routes: &[SniRoute]) -> bool {
    !wan_dnat_covers(firewall, HTTP_PORT)
        && !routes.iter().any(|route| route.ext_port == HTTP_PORT)
        && (wan_dnat_covers(firewall, HTTPS_PORT)
            || routes.iter().any(|route| route.ext_port == HTTPS_PORT))
}

/// An unreadable address redirects. IPv6 never does.
pub(crate) fn redirects(gate: &Gate, peer: Option<IpAddr>, dst: Option<IpAddr>) -> bool {
    if !gate.admitted {
        return false;
    }
    let (Some(peer), Some(dst)) = (peer, dst) else {
        return true;
    };
    match (peer.to_canonical(), dst.to_canonical()) {
        (IpAddr::V4(peer), IpAddr::V4(dst)) => {
            gate.wan.contains(&dst) || !gate.local.iter().any(|net| net.contains(&peer))
        }
        _ => false,
    }
}

/// Must be the outermost layer.
pub fn redirect_public_http(router: Router) -> Router {
    router.layer(axum::middleware::from_fn(
        |req: Request, next: Next| async move {
            let mut response = respond(&gate(), &req);
            // A subnet that came up after the last read is not yet trusted.
            if response.is_some() && refresh_stale_addrs().await {
                response = respond(&gate(), &req);
            }
            match response {
                Some(response) => response,
                None => next.run(req).await,
            }
        },
    ))
}

fn respond(gate: &Gate, req: &Request) -> Option<Response> {
    if req.extensions().get::<WebserverListener>() != Some(&WebserverListener::Http) {
        return None;
    }
    let tcp = req.extensions().get::<TcpMetadata>();
    redirects(
        gate,
        tcp.map(|tcp| tcp.peer_addr.ip()),
        tcp.map(|tcp| tcp.local_addr.ip()),
    )
    .then(|| redirect(req))
}

/// Mirrors start-core's `handle_http_on_https`. Never falls through.
fn redirect(req: &Request) -> Response {
    match request_authority(req).and_then(|authority| https_redirect_uri(req.uri(), authority).ok())
    {
        Some(target) => (
            http::StatusCode::TEMPORARY_REDIRECT,
            [(http::header::LOCATION, target.to_string())],
        )
            .into_response(),
        None => (http::StatusCode::BAD_REQUEST, "Host header required").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddrV4;

    use axum::body::Body;

    use super::*;

    const WAN: Ipv4Addr = Ipv4Addr::new(203, 0, 113, 7);
    const LAN: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

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

    const ADMISSION_80: &str = "config rule 'apf_sni_80'\n\
        \toption name 'HTTP to HTTPS redirect'\n\
        \toption src 'wan'\n\
        \tlist proto 'tcp'\n\
        \toption dest_ip '192.168.0.2'\n\
        \toption dest_port '80'\n\
        \toption target 'ACCEPT'\n\
        \toption family 'ipv4'\n\
        \toption enabled '1'\n\
        \toption _apf_label 'SNI'\n\n";

    const ADMISSION_443: &str = "config rule 'apf_sni_443'\n\
        \toption name 'SNI demux (hostname routes)'\n\
        \toption src 'wan'\n\
        \tlist proto 'tcp'\n\
        \toption dest_port '443'\n\
        \toption target 'ACCEPT'\n\
        \toption family 'ipv4'\n\
        \toption enabled '1'\n\
        \toption _apf_label 'SNI'\n\n";

    fn route(ext_ip: Ipv4Addr, ext_port: u16) -> SniRoute {
        SniRoute {
            ext_ip,
            ext_port,
            hostname: "nas.example.com".into(),
            target: SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 50), 443),
            remaining_secs: Some(3600),
        }
    }

    async fn with_firewall<T>(firewall: &str, f: impl FnOnce(&uciedit::Config<'_>) -> T) -> T {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("firewall"), firewall).unwrap();
        let arena = uciedit::Arena::new();
        let cfgs = uciedit::parse_all(dir.path(), &arena, &["firewall"])
            .await
            .unwrap();
        f(&cfgs["firewall"])
    }

    async fn desired_for(firewall: &str, routes: &[SniRoute]) -> bool {
        with_firewall(firewall, |fw| desired(fw, routes)).await
    }

    #[tokio::test]
    async fn nothing_published_means_no_redirect() {
        assert!(!desired_for("", &[]).await);
        // Remote Access: the router answers 443 itself.
        assert!(!desired_for(REMOTE_443, &[]).await);
    }

    #[tokio::test]
    async fn wan_dnat_on_443_activates() {
        assert!(desired_for(&dnat("443", TCP, "1"), &[]).await);
        // fw4 reads an empty protocol list as TCP and UDP.
        assert!(desired_for(&dnat("443", "", "1"), &[]).await);
        assert!(desired_for(&dnat("400-500", TCP, "1"), &[]).await);
    }

    #[tokio::test]
    async fn only_a_live_tcp_dnat_covering_443_counts() {
        assert!(!desired_for(&dnat("443", TCP, "0"), &[]).await);
        assert!(!desired_for(&dnat("443", UDP, "1"), &[]).await);
        assert!(!desired_for(&dnat("8443", TCP, "1"), &[]).await);
    }

    #[tokio::test]
    async fn hostname_route_on_443_activates() {
        assert!(desired_for("", &[route(WAN, 443)]).await);
        assert!(!desired_for("", &[route(WAN, 8443)]).await);
        // A route keyed to a stale WAN address still counts.
        assert!(desired_for("", &[route(Ipv4Addr::new(198, 51, 100, 9), 443)]).await);
    }

    #[tokio::test]
    async fn a_dnat_on_80_takes_precedence() {
        let fw = format!("{}{}", dnat("443", TCP, "1"), dnat("80", TCP, "1"));
        assert!(!desired_for(&fw, &[route(WAN, 443)]).await);
    }

    #[tokio::test]
    async fn a_hostname_route_on_80_takes_precedence() {
        assert!(!desired_for("", &[route(WAN, 443), route(WAN, 80)]).await);
        assert!(!desired_for(&dnat("443", TCP, "1"), &[route(WAN, 80)]).await);
    }

    fn route_rule_80() -> String {
        ADMISSION_80.replace(RULE_NAME, "SNI demux (hostname routes)")
    }

    #[tokio::test]
    async fn the_redirects_rule_is_told_from_a_hostname_routes() {
        assert!(with_firewall(ADMISSION_80, admission_present).await);
        assert!(!with_firewall(&route_rule_80(), admission_present).await);
        assert!(!with_firewall(ADMISSION_443, admission_present).await);
        assert!(!with_firewall(REMOTE_443, admission_present).await);
        assert!(!with_firewall("", admission_present).await);
    }

    #[tokio::test]
    async fn the_gate_seeds_from_any_rule_admitting_80() {
        assert!(with_firewall(ADMISSION_80, port_admitted).await);
        assert!(with_firewall(&route_rule_80(), port_admitted).await);
        assert!(!with_firewall(ADMISSION_443, port_admitted).await);
        assert!(!with_firewall(REMOTE_443, port_admitted).await);
        assert!(!with_firewall("", port_admitted).await);
    }

    const UPSTREAM: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 0, 50));
    const CLIENT: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50));
    const INTERNET: IpAddr = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 9));

    /// Double NAT: the WAN address is private.
    fn open_gate() -> Gate {
        let wan = Ipv4Addr::new(192, 168, 0, 2);
        Gate {
            admitted: true,
            wan: vec![wan],
            local: off_wan(
                vec![
                    "127.0.0.1/8".parse().unwrap(),
                    Ipv4Net::new(wan, 24).unwrap(),
                    "192.168.1.1/24".parse().unwrap(),
                    "10.59.0.1/24".parse().unwrap(),
                ],
                &[wan],
            ),
        }
    }

    #[test]
    fn a_shut_gate_never_redirects() {
        let shut = Gate {
            admitted: false,
            ..open_gate()
        };
        assert!(!redirects(&shut, Some(INTERNET), Some(shut.wan[0].into())));
        assert!(!redirects(&shut, Some(CLIENT), Some(LAN)));
        assert!(!redirects(&shut, None, None));
    }

    #[test]
    fn the_wan_address_redirects_for_every_client() {
        let gate = open_gate();
        let wan = IpAddr::V4(gate.wan[0]);
        assert!(redirects(&gate, Some(INTERNET), Some(wan)));
        assert!(redirects(&gate, Some(CLIENT), Some(wan)));
        // The dual-stack socket delivers IPv4 v4-mapped.
        assert!(redirects(
            &gate,
            Some(IpAddr::V6(Ipv4Addr::new(192, 168, 1, 50).to_ipv6_mapped())),
            Some(IpAddr::V6(gate.wan[0].to_ipv6_mapped()))
        ));
    }

    #[test]
    fn a_connected_client_off_the_wan_reaches_the_ui() {
        let gate = open_gate();
        assert!(!redirects(&gate, Some(CLIENT), Some(LAN)));
        assert!(!redirects(
            &gate,
            Some(IpAddr::V4(Ipv4Addr::new(10, 59, 0, 2))),
            Some(LAN)
        ));
        assert!(!redirects(
            &gate,
            Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            Some(IpAddr::V4(Ipv4Addr::LOCALHOST))
        ));
    }

    /// A WAN-side host can route the LAN subnet through the router.
    #[test]
    fn a_wan_side_client_never_reaches_the_ui() {
        let gate = open_gate();
        assert!(redirects(&gate, Some(UPSTREAM), Some(LAN)));
        assert!(redirects(&gate, Some(INTERNET), Some(LAN)));
        // A second WAN address the gate has not learned.
        assert!(redirects(
            &gate,
            Some(INTERNET),
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 0, 3)))
        ));
    }

    #[test]
    fn unresolved_addresses_fail_closed_without_a_wan() {
        let unread = Gate {
            admitted: true,
            ..Default::default()
        };
        assert!(redirects(&unread, Some(CLIENT), Some(LAN)));
        assert!(redirects(&open_gate(), None, None));
        // WAN down.
        let wan_down = Gate {
            admitted: true,
            wan: Vec::new(),
            local: vec!["192.168.1.1/24".parse().unwrap()],
        };
        assert!(!redirects(&wan_down, Some(CLIENT), Some(LAN)));
    }

    #[test]
    fn ipv6_is_outside_the_admission_rule() {
        let peer = IpAddr::V6("2001:db8::50".parse().unwrap());
        let dst = IpAddr::V6("2001:db8::1".parse().unwrap());
        assert!(!redirects(&open_gate(), Some(peer), Some(dst)));
    }

    #[test]
    fn connected_subnets_parse_from_ip_addr() {
        let json = r#"[
            {"ifname":"lo","addr_info":[{"family":"inet","local":"127.0.0.1","prefixlen":8}]},
            {"ifname":"eth0","addr_info":[{"family":"inet","local":"192.168.0.2","prefixlen":24}]},
            {"ifname":"br-lan","addr_info":[{"family":"inet","local":"192.168.1.1","prefixlen":24}]},
            {"ifname":"wg0","addr_info":[{"family":"inet","local":"10.59.0.1","prefixlen":24}]}
        ]"#;
        let local = off_wan(parse_connected(json), &[Ipv4Addr::new(192, 168, 0, 2)]);
        assert_eq!(local, open_gate().local);
        assert!(parse_connected("").is_empty());
    }

    fn request(listener: WebserverListener, peer: IpAddr, dst: IpAddr) -> Request {
        let mut req = request_without_host(listener, peer, dst);
        req.headers_mut()
            .insert(http::header::HOST, "nas.example.com".parse().unwrap());
        req
    }

    fn request_without_host(listener: WebserverListener, peer: IpAddr, dst: IpAddr) -> Request {
        let mut req = Request::builder()
            .uri("/luci?x=1")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(listener);
        req.extensions_mut().insert(TcpMetadata {
            peer_addr: (peer, 40000).into(),
            local_addr: (dst, 80).into(),
        });
        req
    }

    #[test]
    fn the_layer_answers_with_the_clients_own_authority() {
        let gate = open_gate();
        let wan = IpAddr::V4(gate.wan[0]);
        let response = respond(&gate, &request(WebserverListener::Http, INTERNET, wan)).unwrap();
        assert_eq!(response.status(), http::StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(
            response.headers()[http::header::LOCATION],
            "https://nas.example.com/luci?x=1"
        );
        assert!(respond(&gate, &request(WebserverListener::Http, CLIENT, LAN)).is_none());
        assert!(respond(&gate, &request(WebserverListener::Https, INTERNET, wan)).is_none());
    }

    #[test]
    fn a_request_without_a_host_never_reaches_the_ui() {
        let gate = open_gate();
        let wan = IpAddr::V4(gate.wan[0]);
        for (peer, dst) in [(INTERNET, wan), (INTERNET, LAN), (CLIENT, wan)] {
            let response = respond(
                &gate,
                &request_without_host(WebserverListener::Http, peer, dst),
            )
            .unwrap();
            assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);
        }
        assert!(respond(
            &gate,
            &request_without_host(WebserverListener::Http, CLIENT, LAN)
        )
        .is_none());
    }
}
