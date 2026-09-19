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
//! `:80` listener: a request whose destination is the WAN address is answered
//! with a 307 to the same authority over HTTPS, and everything else reaches
//! the UI untouched. The Internet side is admitted by a WAN ACCEPT rule on
//! tcp/80, which port control keeps in the SNI admission set ([`HTTP_PORT`])
//! while the redirect is wanted.
//!
//! Wanted only while WAN 443 leaves the router — an enabled WAN DNAT covering
//! tcp/443, or a live hostname route on 443 — and it yields to a DNAT covering
//! tcp/80. With nothing published on 443 the gate is shut and port 80 behaves
//! exactly as before.

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::RwLock;

use axum::body::Body;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use axum::Router;
use startos::net::http::{https_redirect_uri, request_authority};
use startos::net::web_server::TcpMetadata;
use startos::tunnel::forward::sni::SniRoute;
use uciedit::openwrt::FirewallRule;
use uciedit::{parse_all, Arena};

use crate::bins::daemon::WebserverListener;
use crate::port_control::{parse_port_range, uci_task, wan_dnat_covers, KIND_SNI};

pub const HTTP_PORT: u16 = 80;
pub const HTTPS_PORT: u16 = 443;

/// What the firewall admits on port 80, and where the router answers it.
/// Read per request by [`redirect_public_http`]; written by port control and
/// seeded from UCI before the listener binds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Gate {
    /// A WAN admission rule for [`HTTP_PORT`] is in the firewall.
    pub admitted: bool,
    /// The router's WAN IPv4, once port control has resolved it.
    pub wan: Option<Ipv4Addr>,
}

static GATE: RwLock<Gate> = RwLock::new(Gate {
    admitted: false,
    wan: None,
});

pub(crate) fn gate() -> Gate {
    *GATE.read().unwrap_or_else(|e| e.into_inner())
}

pub(crate) fn set_admitted(admitted: bool) {
    GATE.write().unwrap_or_else(|e| e.into_inner()).admitted = admitted;
}

pub(crate) fn set_wan(wan: Option<Ipv4Addr>) {
    GATE.write().unwrap_or_else(|e| e.into_inner()).wan = wan;
}

/// Seeds the gate from the firewall fw4 has already loaded, so the decision is
/// live from the first accepted connection rather than from the first
/// reconcile. An admission rule outlives the daemon; the hostname route that
/// earned it does not. A firewall that cannot be read redirects, since it may
/// be admitting port 80.
pub async fn seed_from_uci(uci_root: PathBuf) {
    match uci_task(move || async move {
        let arena = Arena::new();
        let cfgs = parse_all(&uci_root, &arena, &["firewall"]).await?;
        Ok(admission_present(&cfgs["firewall"]))
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

/// Whether a WAN admission rule covers [`HTTP_PORT`]. Port control reserves
/// that port, so an SNI-labelled rule holding it is this redirect's.
pub(crate) fn admission_present(firewall: &uciedit::Config<'_>) -> bool {
    firewall
        .sections
        .iter()
        .filter_map(|sec| sec.get::<FirewallRule>().ok())
        .any(|rule| {
            rule._apf_label.as_deref() == Some(KIND_SNI)
                && rule
                    .dest_port
                    .as_deref()
                    .and_then(parse_port_range)
                    .is_some_and(|(lo, _)| lo == HTTP_PORT)
        })
}

/// Whether the redirect is wanted: WAN 443 leaves the router and no other DNAT
/// claims tcp/80. A Remote Access ACCEPT on 443 does not count — there the
/// router itself answers 443.
pub(crate) fn desired(firewall: &uciedit::Config<'_>, routes: &[SniRoute]) -> bool {
    !wan_dnat_covers(firewall, HTTP_PORT)
        && (wan_dnat_covers(firewall, HTTPS_PORT)
            || routes.iter().any(|route| route.ext_port == HTTPS_PORT))
}

/// Whether a request arriving on the plain-HTTP listener is answered with the
/// redirect. An unknown destination, or a WAN address port control has not
/// resolved yet, redirects: the UI must never answer at the public address,
/// and on a double-NAT WAN its address class does not distinguish it from the
/// LAN.
pub(crate) fn redirects(gate: Gate, dst: Option<IpAddr>) -> bool {
    if !gate.admitted {
        return false;
    }
    match (gate.wan, dst) {
        (Some(wan), Some(dst)) => dst.to_canonical() == IpAddr::V4(wan),
        _ => true,
    }
}

/// Answers plain HTTP at the public address with a 307 to HTTPS. Outermost on
/// the router, so no route can be reached at that address.
pub fn redirect_public_http(router: Router) -> Router {
    router.layer(axum::middleware::from_fn(
        |req: Request, next: Next| async move {
            if arrived_on_http(&req) && redirects(gate(), destination(&req)) {
                if let Some(response) = redirect(&req) {
                    return response;
                }
            }
            next.run(req).await
        },
    ))
}

fn arrived_on_http(req: &Request) -> bool {
    req.extensions().get::<WebserverListener>() == Some(&WebserverListener::Http)
}

fn destination(req: &Request) -> Option<IpAddr> {
    Some(req.extensions().get::<TcpMetadata>()?.local_addr.ip())
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
    async fn the_gate_seeds_from_the_admission_rule() {
        assert!(with_firewall(ADMISSION_80, admission_present).await);
        // The demux's own rule on 443 is not the redirect's.
        assert!(!with_firewall(ADMISSION_443, admission_present).await);
        assert!(!with_firewall(REMOTE_443, admission_present).await);
        assert!(!with_firewall("", admission_present).await);
        // A hostname route's rule survives a restart while its route does not,
        // which is exactly the state the seed has to read.
        assert!(with_firewall(&format!("{ADMISSION_443}{ADMISSION_80}"), admission_present).await);
    }

    #[test]
    fn a_shut_gate_never_redirects() {
        let shut = Gate {
            admitted: false,
            wan: Some(WAN),
        };
        assert!(!redirects(shut, Some(IpAddr::V4(WAN))));
        assert!(!redirects(shut, Some(LAN)));
        assert!(!redirects(shut, None));
    }

    #[test]
    fn an_open_gate_redirects_the_wan_address_alone() {
        let open = Gate {
            admitted: true,
            wan: Some(WAN),
        };
        assert!(redirects(open, Some(IpAddr::V4(WAN))));
        // The dual-stack socket delivers IPv4 clients v4-mapped.
        assert!(redirects(
            open,
            Some(IpAddr::V6(Ipv4Addr::to_ipv6_mapped(&WAN)))
        ));
        assert!(!redirects(open, Some(LAN)));
        assert!(!redirects(open, Some(IpAddr::V4(Ipv4Addr::LOCALHOST))));
    }

    #[test]
    fn an_unresolved_address_fails_closed() {
        let open = Gate {
            admitted: true,
            wan: None,
        };
        assert!(redirects(open, Some(LAN)));
        assert!(redirects(open, None));
        // A destination we cannot read is the same unknown.
        assert!(redirects(
            Gate {
                admitted: true,
                wan: Some(WAN)
            },
            None
        ));
    }
}
