//! HTTP→HTTPS redirect for the router's public IPv4 address, port 80.
//!
//! StartOS asks its IPv4 gateway for no port-80 mapping: it publishes only
//! 443 and expects the gateway to answer plain HTTP at the public address with
//! a redirect to HTTPS (start-core `net/vhost.rs`; StartTunnel implements its
//! half in `tunnel/redirect.rs`). Without it, a LAN client opening
//! `http://sub.example.com` for a domain that points at the WAN address
//! hairpins to the router's own port 80 and gets the router UI, and a client
//! on the Internet is refused unless Remote Access admits it.
//!
//! [`redirect_public_http`] decides this per request on the daemon's wildcard
//! `:80` listener. The UI is served to a client on a subnet the router is
//! connected to off the WAN, at an address that is not the WAN's; every other
//! IPv4 request is answered with a 307 to the same authority over HTTPS. The
//! Internet side is admitted by a WAN ACCEPT rule on tcp/80, which port
//! control keeps in the SNI admission set ([`HTTP_PORT`]) while the redirect
//! is wanted. That rule matches the zone, not the destination, so the client's
//! address decides alongside the address it dialed.
//!
//! Wanted only while WAN 443 leaves the router — an enabled WAN DNAT covering
//! tcp/443, or a live hostname route on 443 — and it yields to a DNAT or a
//! hostname route holding tcp/80. With nothing published on 443 the gate is
//! shut and port 80 behaves exactly as before.

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
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

/// The admission rule's `name`, which tells it from a hostname route's rule on
/// the same port under the same label.
pub(crate) const RULE_NAME: &str = "HTTP to HTTPS redirect";

/// Read per request by [`redirect_public_http`]; written by port control and
/// seeded before the listener binds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Gate {
    /// The firewall may be admitting WAN-side [`HTTP_PORT`].
    pub admitted: bool,
    pub wan: Vec<Ipv4Addr>,
    /// Connected IPv4 subnets off the WAN, loopback among them.
    pub local: Vec<Ipv4Net>,
}

static GATE: RwLock<Gate> = RwLock::new(Gate {
    admitted: false,
    wan: Vec::new(),
    local: Vec::new(),
});

/// When the addresses were last read.
static REFRESHED: tokio::sync::Mutex<Option<Instant>> = tokio::sync::Mutex::const_new(None);
const REFRESH_FLOOR: Duration = Duration::from_secs(5);

fn gate() -> Gate {
    GATE.read().unwrap_or_else(|e| e.into_inner()).clone()
}

pub(crate) fn set_admitted(admitted: bool) {
    GATE.write().unwrap_or_else(|e| e.into_inner()).admitted = admitted;
}

/// Rereads the router's addresses. Unreadable addresses leave no client
/// trusted.
pub(crate) async fn refresh_addrs() {
    let mut refreshed = REFRESHED.lock().await;
    read_addrs().await;
    *refreshed = Some(Instant::now());
}

/// Rereads addresses older than [`REFRESH_FLOOR`]. Returns whether it did.
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
    // Subnets first: a WAN address that comes up between the two reads is
    // then missing from the subnets rather than trusted among them.
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

/// Seeds the gate from the firewall fw4 has already loaded and the addresses
/// the router already holds, so the decision is live from the first accepted
/// connection rather than from the first reconcile. A firewall that cannot be
/// read redirects, since it may be admitting port 80.
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

/// Whether any SNI-labelled rule admits [`HTTP_PORT`]. A hostname route's rule
/// outlives the daemon and its route, and admits the same traffic.
fn port_admitted(firewall: &uciedit::Config<'_>) -> bool {
    sni_rules_on_http_port(firewall).next().is_some()
}

/// Whether the redirect's own admission rule is in the firewall.
pub(crate) fn admission_present(firewall: &uciedit::Config<'_>) -> bool {
    sni_rules_on_http_port(firewall).any(|rule| rule.name == RULE_NAME)
}

/// Whether the redirect is wanted: WAN 443 leaves the router and neither a
/// DNAT nor a hostname route holds tcp/80. A Remote Access ACCEPT on 443 does
/// not count — there the router itself answers 443.
pub(crate) fn desired(firewall: &uciedit::Config<'_>, routes: &[SniRoute]) -> bool {
    !wan_dnat_covers(firewall, HTTP_PORT)
        && !routes.iter().any(|route| route.ext_port == HTTP_PORT)
        && (wan_dnat_covers(firewall, HTTPS_PORT)
            || routes.iter().any(|route| route.ext_port == HTTPS_PORT))
}

/// Whether a request on the plain-HTTP listener is answered with the redirect.
/// An unreadable address redirects. IPv6 is outside the admission rule.
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

/// Answers plain HTTP at the public address with a 307 to HTTPS. Outermost on
/// the router, so no route can be reached at that address.
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
    .flatten()
}

/// Mirrors start-core's `handle_http_on_https`: the client's own authority,
/// the same path, and no body.
fn redirect(req: &Request) -> Option<Response> {
    let authority = request_authority(req)?;
    let target = https_redirect_uri(req.uri(), authority).ok()?;
    Response::builder()
        .status(http::StatusCode::TEMPORARY_REDIRECT)
        .header(http::header::LOCATION, target.to_string())
        .body(Body::empty())
        .ok()
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddrV4;

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

    /// The admission rule port control writes for the redirect.
    const ADMISSION_80: &str = "config rule 'apf_sni_80'\n\
        \toption name 'HTTP to HTTPS redirect'\n\
        \toption src 'wan'\n\
        \tlist proto 'tcp'\n\
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
        // The router answering 443 itself (Remote Access) is not "published".
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
        // A route keyed to a stale WAN address is re-keyed by maintenance;
        // its 443 is still published.
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

    /// A hostname route's rule on 80: the same section, under the demux's name.
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
        // A route's rule survives a restart while its route does not, and
        // admits WAN-side HTTP all the same.
        assert!(with_firewall(&route_rule_80(), port_admitted).await);
        assert!(!with_firewall(ADMISSION_443, port_admitted).await);
        assert!(!with_firewall(REMOTE_443, port_admitted).await);
        assert!(!with_firewall("", port_admitted).await);
    }

    const UPSTREAM: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 0, 50));
    const CLIENT: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50));
    const INTERNET: IpAddr = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 9));

    /// A double-NAT router: the WAN sits on someone else's private LAN.
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

    /// The admission rule matches the zone, so a WAN-side host that routes the
    /// LAN subnet through the router arrives at the LAN address.
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
        // The WAN is down: nothing to exclude, and the LAN still gets the UI.
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
        let mut req = Request::builder()
            .uri("/luci?x=1")
            .header(http::header::HOST, "nas.example.com")
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
}
