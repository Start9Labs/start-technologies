//! HTTP→HTTPS redirect for a service's domains on the StartOS UI's port.
//!
//! Port 80 belongs to the StartOS UI, whose listener answers on every address
//! the server holds, and a browser given a bare domain name tries it first. A
//! plaintext request carries no SNI, so that listener has only the `Host`
//! header to tell a service's domain from the names the dashboard answers to.

use std::net::IpAddr;

use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use http::uri::{Authority, Scheme};
use http::{HeaderValue, StatusCode, Uri};
use imbl_value::InternedString;

use crate::GatewayId;
use crate::context::RpcContext;
use crate::db::model::DatabaseModel;
use crate::net::gateway::GatewayInfo;
use crate::net::host::binding::BindInfo;
use crate::net::service_interface::{HostnameMetadata, ServiceInterfaceType};
use crate::prelude::*;

const HTTPS_PORT: u16 = 443;

/// The longest name DNS carries.
const MAX_NAME_LEN: usize = 253;

/// Bounce a request whose `Host` names a domain an installed service is served
/// on.
pub fn layer(ctx: RpcContext, router: Router) -> Router {
    router.layer(axum::middleware::from_fn(
        move |req: Request, next: Next| {
            let ctx = ctx.clone();
            let gateway = arrival_gateway(&req);
            async move {
                if let (Some(name), Some(gateway)) = (host(&req), gateway) {
                    let uri = req.uri().clone();
                    match redirect(&ctx, &gateway, &name, &uri).await {
                        Ok(Some(res)) => return res,
                        Ok(None) => (),
                        Err(e) => {
                            tracing::warn!("failed to check the host {name}: {e}");
                            tracing::debug!("{e:?}");
                        }
                    }
                }
                next.run(req).await
            }
        },
    ))
}

/// Which of the server's networks the connection arrived on. The listener
/// resolves it per accepted connection and the web server puts it in the
/// request extensions.
///
/// The listener names a connection it cannot place with an empty gateway, which
/// is no gateway at all.
fn arrival_gateway(req: &Request) -> Option<GatewayId> {
    req.extensions()
        .get::<GatewayInfo>()
        .map(|g| g.id.clone())
        .filter(|id| !id.as_str().is_empty())
}

/// The host the request named, lowercased and stripped of any port and of the
/// root's trailing dot.
///
/// `None` unless the result is a plausible domain name. An address, an
/// over-long name, or anything outside the DNS character set is rejected here,
/// so nothing else has to defend against a hostile `Host`.
fn host(req: &Request) -> Option<String> {
    // The request target's authority wins over the header, and on HTTP/2 it is
    // the only one of the two hyper fills in.
    let host = match req.uri().host() {
        Some(host) => host,
        None => req.headers().get(http::header::HOST)?.to_str().ok()?,
    };
    let host = host.split_once(':').map_or(host, |(name, _)| name);
    if host.parse::<IpAddr>().is_ok() {
        return None;
    }
    let name = host.trim_end_matches('.').to_ascii_lowercase();
    if name.is_empty() || name.len() > MAX_NAME_LEN || !name.chars().all(is_name_char) {
        return None;
    }
    Some(name)
}

/// The characters a `Host` may hold to be matched against a stored domain.
/// Anything else — userinfo above all — would reach the `Location` header,
/// where a browser reads everything after an `@` as the destination.
fn is_name_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.' || c == '_'
}

async fn redirect(
    ctx: &RpcContext,
    gateway: &GatewayId,
    name: &str,
    uri: &Uri,
) -> Result<Option<Response>, Error> {
    // A target that is not a path has nothing to send across: `CONNECT` names an
    // authority, and `OPTIONS *` names nothing.
    if uri
        .path_and_query()
        .map_or(true, |p| !p.path().starts_with('/'))
    {
        return Ok(None);
    }
    // The dashboard is reached by the server's own `.local` name, so answer that
    // before reading the database.
    let is_own_mdns_name = ctx.account.peek(|a| {
        name.strip_suffix(".local")
            .is_some_and(|host| host == &**a.hostname.hostname)
    });
    if is_own_mdns_name {
        return Ok(None);
    }
    let Some(port) = service_tls_port(&ctx.db.peek().await, gateway, name)? else {
        return Ok(None);
    };
    https_redirect(uri, name, port).map(Some)
}

/// The same request, addressed to `name` over TLS on `port`.
fn https_redirect(uri: &Uri, name: &str, port: u16) -> Result<Response, Error> {
    let authority = if port == HTTPS_PORT {
        name.to_owned()
    } else {
        format!("{name}:{port}")
    };
    let mut parts = uri.to_owned().into_parts();
    parts.scheme = Some(Scheme::HTTPS);
    parts.authority = Some(
        authority
            .parse::<Authority>()
            .with_kind(ErrorKind::ParseUrl)?,
    );
    let target = Uri::from_parts(parts).with_kind(ErrorKind::ParseUrl)?;
    Response::builder()
        .status(StatusCode::TEMPORARY_REDIRECT)
        .header(
            http::header::LOCATION,
            HeaderValue::from_str(&target.to_string()).with_kind(ErrorKind::ParseUrl)?,
        )
        .body(Body::empty())
        .with_kind(ErrorKind::Network)
}

/// The TLS port an installed service answers `name` on over `gateway`.
///
/// Only installed services are searched. The server's own host answers its names
/// through a TLS listener that proxies back to this very port, so redirecting one
/// of those would loop.
fn service_tls_port(
    db: &DatabaseModel,
    gateway: &GatewayId,
    name: &str,
) -> Result<Option<u16>, Error> {
    let key = InternedString::from(name);
    for (_, package) in db.as_public().as_package_data().as_entries()? {
        for (_, host) in package.as_hosts().as_entries()? {
            if !host.as_private_domains().contains_key(&key)?
                && !host.as_public_domains().contains_key(&key)?
            {
                continue;
            }
            if let Some(port) = tls_port(host.as_bindings().de()?.values(), gateway, name) {
                return Ok(Some(port));
            }
        }
    }
    Ok(None)
}

/// The port to send a browser that asked for `name` over `gateway` in plaintext.
///
/// Only a domain qualifies, and only over a gateway it is served on. The
/// server's IP addresses and its `.local` name are how the dashboard itself is
/// reached, and every host carries them.
fn tls_port<'a>(
    bindings: impl IntoIterator<Item = &'a BindInfo>,
    gateway: &GatewayId,
    name: &str,
) -> Option<u16> {
    bindings
        .into_iter()
        .filter(|bind| bind.enabled && serves_https(bind))
        .flat_map(|bind| bind.enabled_addresses())
        .filter(|addr| {
            addr.ssl
                && *addr.hostname == *name
                && matches!(
                    addr.metadata,
                    HostnameMetadata::PrivateDomain { .. } | HostnameMetadata::PublicDomain { .. }
                )
                // A domain scoped to another gateway has no listener on this
                // one, so sending a browser there ends in a refused handshake.
                && addr.metadata.gateways().any(|g| g == gateway)
        })
        .filter_map(|addr| addr.port)
        // A browser with no port in its address bar goes to 443, so prefer it.
        .min_by_key(|port| (*port != HTTPS_PORT, *port))
}

/// Whether a browser can open this binding's TLS port. A binding that carries
/// another protocol over TLS — an Electrum server, a TURN server — has a domain
/// and a TLS port like any other. An `https` scheme separates the two, and so
/// does a `ui` interface where the package declared no scheme at all.
fn serves_https(bind: &BindInfo) -> bool {
    bind.interfaces
        .values()
        .any(|iface| match iface.address_info.ssl_scheme.as_deref() {
            Some(scheme) => scheme == Scheme::HTTPS.as_str(),
            None => matches!(iface.interface_type, ServiceInterfaceType::Ui),
        })
}

#[cfg(test)]
mod test {
    use std::collections::BTreeMap;

    use super::*;
    use crate::net::host::binding::{BindOptions, DerivedAddressInfo, NetInfo};
    use crate::net::service_interface::{AddressInfo, HostnameInfo, ServiceInterface};
    use crate::{HostId, Id, ServiceInterfaceId};

    fn gateway(id: &'static str) -> GatewayId {
        GatewayId::from(InternedString::from_static(id))
    }

    fn gateways() -> std::collections::BTreeSet<GatewayId> {
        [gateway("eth0")].into_iter().collect()
    }

    fn private(hostname: &str, ssl: bool, port: u16) -> HostnameInfo {
        HostnameInfo {
            ssl,
            public: false,
            hostname: InternedString::from(hostname),
            port: Some(port),
            metadata: HostnameMetadata::PrivateDomain {
                gateways: gateways(),
            },
        }
    }

    fn public(hostname: &str, port: u16) -> HostnameInfo {
        HostnameInfo {
            ssl: true,
            public: true,
            hostname: InternedString::from(hostname),
            port: Some(port),
            metadata: HostnameMetadata::PublicDomain {
                gateway: gateway("eth0"),
            },
        }
    }

    fn mdns(hostname: &str, port: u16) -> HostnameInfo {
        HostnameInfo {
            ssl: true,
            public: false,
            hostname: InternedString::from(hostname),
            port: Some(port),
            metadata: HostnameMetadata::Mdns {
                gateways: gateways(),
            },
        }
    }

    /// A binding serves its domains only through an exported interface, so
    /// every fixture needs one: `BindInfo::enabled_addresses` drops every
    /// address but the internal ones when `interfaces` is empty.
    fn interface(ssl_scheme: Option<&str>, kind: ServiceInterfaceType) -> ServiceInterface {
        let id = ServiceInterfaceId::from(Id::try_from("ui".to_owned()).unwrap());
        ServiceInterface {
            id,
            name: "UI".to_owned(),
            description: String::new(),
            masked: false,
            address_info: AddressInfo {
                username: None,
                host_id: HostId::from(Id::try_from("ui".to_owned()).unwrap()),
                internal_port: 80,
                scheme: Some(InternedString::intern("http")),
                ssl_scheme: ssl_scheme.map(InternedString::intern),
                suffix: String::new(),
            },
            interface_type: kind,
        }
    }

    fn binding_serving(
        ssl_scheme: Option<&str>,
        kind: ServiceInterfaceType,
        available: impl IntoIterator<Item = HostnameInfo>,
    ) -> BindInfo {
        let iface = interface(ssl_scheme, kind);
        BindInfo {
            enabled: true,
            options: BindOptions {
                preferred_external_port: 80,
                add_ssl: None,
                secure: None,
            },
            net: NetInfo {
                assigned_port: Some(8080),
                assigned_ssl_port: Some(HTTPS_PORT),
            },
            addresses: DerivedAddressInfo {
                available: available.into_iter().collect(),
                ..Default::default()
            },
            interfaces: [(iface.id.clone(), iface)]
                .into_iter()
                .collect::<BTreeMap<_, _>>(),
        }
    }

    fn binding(available: impl IntoIterator<Item = HostnameInfo>) -> BindInfo {
        binding_serving(
            Some(Scheme::HTTPS.as_str()),
            ServiceInterfaceType::Ui,
            available,
        )
    }

    #[test]
    fn sends_a_service_domain_to_its_tls_port() {
        let binds = [binding([
            private("cloud.mydomain.com", false, 8080),
            private("cloud.mydomain.com", true, HTTPS_PORT),
        ])];
        assert_eq!(
            tls_port(binds.iter(), &gateway("eth0"), "cloud.mydomain.com"),
            Some(HTTPS_PORT)
        );
    }

    #[test]
    fn sends_a_public_domain_to_its_tls_port() {
        let binds = [binding([public("cloud.mydomain.com", HTTPS_PORT)])];
        assert_eq!(
            tls_port(binds.iter(), &gateway("eth0"), "cloud.mydomain.com"),
            Some(HTTPS_PORT)
        );
    }

    // The dashboard is reached by the server's `.local` name over plaintext, and
    // every host carries that name.
    #[test]
    fn leaves_the_mdns_name_alone() {
        let binds = [binding([mdns("myserver.local", HTTPS_PORT)])];
        assert_eq!(
            tls_port(binds.iter(), &gateway("eth0"), "myserver.local"),
            None
        );
    }

    #[test]
    fn ignores_a_domain_served_only_in_plaintext() {
        let binds = [binding([private("cloud.mydomain.com", false, 8080)])];
        assert_eq!(
            tls_port(binds.iter(), &gateway("eth0"), "cloud.mydomain.com"),
            None
        );
    }

    // An Electrum server's TLS port speaks its own protocol, and a browser sent
    // there would speak HTTP at it.
    #[test]
    fn ignores_a_tls_port_that_is_not_https() {
        let binds = [binding_serving(
            Some("ssl"),
            ServiceInterfaceType::Api,
            [private("electrum.mydomain.com", true, 50002)],
        )];
        assert_eq!(
            tls_port(binds.iter(), &gateway("eth0"), "electrum.mydomain.com"),
            None
        );
    }

    #[test]
    fn ignores_a_non_ui_port_with_no_declared_scheme() {
        let binds = [binding_serving(
            None,
            ServiceInterfaceType::P2p,
            [private("p2p.mydomain.com", true, 8443)],
        )];
        assert_eq!(
            tls_port(binds.iter(), &gateway("eth0"), "p2p.mydomain.com"),
            None
        );
    }

    // A declared scheme decides on its own: a `ws` interface is typed `ui` and
    // carries `wss`, which a browser cannot navigate to.
    #[test]
    fn ignores_a_ui_that_declares_another_scheme() {
        let binds = [binding_serving(
            Some("wss"),
            ServiceInterfaceType::Ui,
            [private("socket.mydomain.com", true, HTTPS_PORT)],
        )];
        assert_eq!(
            tls_port(binds.iter(), &gateway("eth0"), "socket.mydomain.com"),
            None
        );
    }

    // A package may bind a port the SDK knows no protocol for and still export a
    // ui from it, which leaves the scheme unset.
    #[test]
    fn sends_a_ui_with_no_declared_scheme() {
        let binds = [binding_serving(
            None,
            ServiceInterfaceType::Ui,
            [private("cloud.mydomain.com", true, 8443)],
        )];
        assert_eq!(
            tls_port(binds.iter(), &gateway("eth0"), "cloud.mydomain.com"),
            Some(8443)
        );
    }

    // A domain is served only over the gateways it was added on, and the vhost
    // refuses the handshake anywhere else.
    #[test]
    fn ignores_a_domain_scoped_to_another_gateway() {
        let binds = [binding([private("cloud.mydomain.com", true, HTTPS_PORT)])];
        assert_eq!(
            tls_port(binds.iter(), &gateway("wg0"), "cloud.mydomain.com"),
            None
        );
    }

    // `setupInterfaces` ends by disabling every binding the pass did not
    // declare, and a disabled binding keeps its domains in the database.
    #[test]
    fn ignores_a_disabled_binding() {
        let mut bind = binding([private("cloud.mydomain.com", true, HTTPS_PORT)]);
        bind.enabled = false;
        assert_eq!(
            tls_port([&bind], &gateway("eth0"), "cloud.mydomain.com"),
            None
        );
    }

    #[test]
    fn ignores_an_address_the_operator_switched_off() {
        let mut bind = binding([private("cloud.mydomain.com", true, HTTPS_PORT)]);
        bind.addresses.disabled.insert((
            InternedString::from_static("cloud.mydomain.com"),
            HTTPS_PORT,
        ));
        assert_eq!(
            tls_port([&bind], &gateway("eth0"), "cloud.mydomain.com"),
            None
        );
    }

    #[test]
    fn prefers_443_to_another_bindings_tls_port() {
        let binds = [
            binding([private("cloud.mydomain.com", true, 8443)]),
            binding([private("cloud.mydomain.com", true, HTTPS_PORT)]),
        ];
        assert_eq!(
            tls_port(binds.iter(), &gateway("eth0"), "cloud.mydomain.com"),
            Some(HTTPS_PORT)
        );
    }

    #[test]
    fn falls_back_to_the_lowest_tls_port() {
        let binds = [
            binding([private("cloud.mydomain.com", true, 9443)]),
            binding([private("cloud.mydomain.com", true, 8443)]),
        ];
        assert_eq!(
            tls_port(binds.iter(), &gateway("eth0"), "cloud.mydomain.com"),
            Some(8443)
        );
    }

    fn host_holding(domain: &str, bind: &BindInfo) -> imbl_value::Value {
        imbl_value::json!({
            "bindings": { "80": imbl_value::to_value(bind).unwrap() },
            "bindingRanges": {},
            "publicDomains": {},
            "privateDomains": { domain: ["eth0"] },
            "portForwards": [],
        })
    }

    fn db(server_host: imbl_value::Value, package_host: imbl_value::Value) -> DatabaseModel {
        DatabaseModel::from(imbl_value::json!({
            "public": {
                "serverInfo": { "network": { "host": server_host } },
                "packageData": { "nextcloud": { "hosts": { "ui": package_host } } },
            }
        }))
    }

    fn empty_host() -> imbl_value::Value {
        imbl_value::json!({
            "bindings": {},
            "bindingRanges": {},
            "publicDomains": {},
            "privateDomains": {},
            "portForwards": [],
        })
    }

    // The host's domain map is the authority, so a derived address alone never
    // qualifies a name.
    #[test]
    fn ignores_a_name_no_host_lists_as_a_domain() {
        let bind = binding([private("cloud.mydomain.com", true, HTTPS_PORT)]);
        let db = db(empty_host(), host_holding("other.example", &bind));
        assert_eq!(
            service_tls_port(&db, &gateway("eth0"), "cloud.mydomain.com").unwrap(),
            None
        );
    }

    #[test]
    fn finds_a_service_domain_in_the_database() {
        let bind = binding([private("cloud.mydomain.com", true, HTTPS_PORT)]);
        let db = db(empty_host(), host_holding("cloud.mydomain.com", &bind));
        assert_eq!(
            service_tls_port(&db, &gateway("eth0"), "cloud.mydomain.com").unwrap(),
            Some(HTTPS_PORT)
        );
    }

    // The server's own TLS listener proxies back to the port this redirect runs
    // on, so a name it answers to must never be redirected.
    #[test]
    fn does_not_search_the_servers_own_host() {
        let bind = binding([private("home.mydomain.com", true, HTTPS_PORT)]);
        let db = db(host_holding("home.mydomain.com", &bind), empty_host());
        assert_eq!(
            service_tls_port(&db, &gateway("eth0"), "home.mydomain.com").unwrap(),
            None
        );
    }

    fn request(host: &str) -> Request {
        Request::builder()
            .uri("/photos?share=1")
            .header(http::header::HOST, host)
            .body(Body::empty())
            .unwrap()
    }

    fn arriving_on(id: &'static str) -> Request {
        let mut req = request("cloud.mydomain.com");
        req.extensions_mut().insert(GatewayInfo {
            id: gateway(id),
            info: Default::default(),
        });
        req
    }

    #[test]
    fn reads_the_gateway_the_listener_resolved() {
        assert_eq!(arrival_gateway(&arriving_on("eth0")), Some(gateway("eth0")));
    }

    // The listener hands on an empty gateway rather than nothing when it cannot
    // place the connection, and no domain is served on it.
    #[test]
    fn reads_an_unplaced_connection_as_no_gateway() {
        assert_eq!(arrival_gateway(&arriving_on("")), None);
        assert_eq!(arrival_gateway(&request("cloud.mydomain.com")), None);
    }

    #[test]
    fn reads_a_host_name() {
        assert_eq!(
            host(&request("Cloud.MyDomain.com:80")).as_deref(),
            Some("cloud.mydomain.com")
        );
    }

    #[test]
    fn reads_a_host_name_written_from_the_root() {
        assert_eq!(
            host(&request("cloud.mydomain.com.")).as_deref(),
            Some("cloud.mydomain.com")
        );
    }

    #[test]
    fn reads_the_authority_of_a_request_with_no_host_header() {
        let req = Request::builder()
            .uri("https://cloud.mydomain.com/photos")
            .body(Body::empty())
            .unwrap();
        assert_eq!(host(&req).as_deref(), Some("cloud.mydomain.com"));
    }

    // RFC 9112 gives the request target's authority precedence over the header.
    #[test]
    fn prefers_the_authority_to_the_header() {
        let req = Request::builder()
            .uri("http://cloud.mydomain.com/photos")
            .header(http::header::HOST, "other.mydomain.com")
            .body(Body::empty())
            .unwrap();
        assert_eq!(host(&req).as_deref(), Some("cloud.mydomain.com"));
    }

    #[test]
    fn skips_a_host_named_by_address() {
        assert_eq!(host(&request("192.168.1.5")), None);
        assert_eq!(host(&request("192.168.1.5:80")), None);
        assert_eq!(host(&request("[fd00::1]:80")), None);
    }

    // Userinfo in the authority would otherwise reach a `Location` header, where
    // the browser reads everything after the `@` as the destination.
    #[test]
    fn skips_a_host_outside_the_name_character_set() {
        assert_eq!(host(&request("admin@evil.example")), None);
        assert_eq!(host(&request("cloud.mydomain.com!")), None);
        assert_eq!(host(&request(&format!("{}.com", "a".repeat(250)))), None);
    }

    fn location(uri: &str, port: u16) -> String {
        let target = https_redirect(&uri.parse().unwrap(), "cloud.mydomain.com", port).unwrap();
        assert_eq!(target.status(), StatusCode::TEMPORARY_REDIRECT);
        target
            .headers()
            .get(http::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn keeps_the_path_and_query() {
        assert_eq!(
            location("/photos?share=1", HTTPS_PORT),
            "https://cloud.mydomain.com/photos?share=1"
        );
    }

    #[test]
    fn names_a_tls_port_that_is_not_443() {
        assert_eq!(
            location("/photos", 8443),
            "https://cloud.mydomain.com:8443/photos"
        );
    }
}
