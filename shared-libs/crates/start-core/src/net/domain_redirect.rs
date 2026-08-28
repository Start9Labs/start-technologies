//! Redirects service-domain HTTP requests from the shared UI listener.

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

const MAX_DNS_NAME_LEN: usize = 253;

pub fn redirect_service_domains(ctx: RpcContext, router: Router) -> Router {
    router.layer(axum::middleware::from_fn(
        move |req: Request, next: Next| {
            let ctx = ctx.clone();
            let gateway = arrival_gateway_id(&req);
            async move {
                if let (Some(name), Some(gateway)) = (request_domain(&req), gateway) {
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

/// An empty `GatewayId` represents a connection from an unknown origin.
fn arrival_gateway_id(req: &Request) -> Option<GatewayId> {
    req.extensions()
        .get::<GatewayInfo>()
        .map(|g| g.id.clone())
        .filter(|id| !id.as_str().is_empty())
}

fn request_domain(req: &Request) -> Option<String> {
    // Absolute-form requests use the URI authority instead of `Host`.
    let host = match req.uri().host() {
        Some(host) => host,
        None => req.headers().get(http::header::HOST)?.to_str().ok()?,
    };
    let host = host.split_once(':').map_or(host, |(name, _)| name);
    if host.parse::<IpAddr>().is_ok() {
        return None;
    }
    let name = host.trim_end_matches('.').to_ascii_lowercase();
    // The redirect authority excludes userinfo and URI delimiters.
    if name.is_empty() || name.len() > MAX_DNS_NAME_LEN || !name.chars().all(is_safe_domain_char) {
        return None;
    }
    Some(name)
}

fn is_safe_domain_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.' || c == '_'
}

async fn redirect(
    ctx: &RpcContext,
    gateway: &GatewayId,
    name: &str,
    uri: &Uri,
) -> Result<Option<Response>, Error> {
    let Some(port) = package_service_tls_port(&ctx.db.peek().await, gateway, name)? else {
        return Ok(None);
    };
    let is_dashboard = ctx
        .account
        .peek(|a| is_dashboard_mdns(name, port, &a.hostname.hostname));
    if is_dashboard {
        return Ok(None);
    }
    let path_and_query = uri.path_and_query().filter(|path| path.as_str() != "*");
    https_redirect(path_and_query, name, port).map(Some)
}

fn is_dashboard_mdns(name: &str, port: u16, hostname: &str) -> bool {
    port == HTTPS_PORT && name.strip_suffix(".local") == Some(hostname)
}

fn https_redirect(
    path_and_query: Option<&http::uri::PathAndQuery>,
    name: &str,
    port: u16,
) -> Result<Response, Error> {
    let authority = if port == HTTPS_PORT {
        name.to_owned()
    } else {
        format!("{name}:{port}")
    };
    let target = Uri::builder()
        .scheme(Scheme::HTTPS)
        .authority(
            authority
                .parse::<Authority>()
                .with_kind(ErrorKind::ParseUrl)?,
        )
        .path_and_query(path_and_query.map_or("/", |path| path.as_str()))
        .build()
        .with_kind(ErrorKind::ParseUrl)?;
    Response::builder()
        .status(StatusCode::TEMPORARY_REDIRECT)
        .header(
            http::header::LOCATION,
            HeaderValue::from_str(&target.to_string()).with_kind(ErrorKind::ParseUrl)?,
        )
        .body(Body::empty())
        .with_kind(ErrorKind::Network)
}

/// Excludes server-owned domains to avoid the TLS proxy redirect loop.
fn package_service_tls_port(
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
            if let Some(port) = browser_https_port(host.as_bindings().de()?.values(), gateway, name)
            {
                return Ok(Some(port));
            }
        }
    }
    Ok(None)
}

fn browser_https_port<'a>(
    bindings: impl IntoIterator<Item = &'a BindInfo>,
    gateway: &GatewayId,
    name: &str,
) -> Option<u16> {
    bindings
        .into_iter()
        .filter(|bind| bind.enabled && has_browser_https_interface(bind))
        .flat_map(|bind| bind.enabled_addresses())
        .filter(|addr| {
            addr.ssl
                && *addr.hostname == *name
                && matches!(
                    addr.metadata,
                    HostnameMetadata::PrivateDomain { .. } | HostnameMetadata::PublicDomain { .. }
                )
                && addr.metadata.gateways().any(|g| g == gateway)
        })
        .filter_map(|addr| addr.port)
        .min_by_key(|port| (*port != HTTPS_PORT, *port))
}

/// A scheme-less UI interface is browser-addressable over TLS.
fn has_browser_https_interface(bind: &BindInfo) -> bool {
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

    fn plain_scheme(ssl_scheme: Option<&str>) -> Option<&'static str> {
        match ssl_scheme {
            Some("https") => Some("http"),
            Some("wss") => Some("ws"),
            _ => None,
        }
    }

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
                scheme: plain_scheme(ssl_scheme).map(InternedString::intern),
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
            browser_https_port(binds.iter(), &gateway("eth0"), "cloud.mydomain.com"),
            Some(HTTPS_PORT)
        );
    }

    #[test]
    fn sends_a_public_domain_to_its_tls_port() {
        let binds = [binding([public("cloud.mydomain.com", HTTPS_PORT)])];
        assert_eq!(
            browser_https_port(binds.iter(), &gateway("eth0"), "cloud.mydomain.com"),
            Some(HTTPS_PORT)
        );
    }

    #[test]
    fn leaves_the_mdns_name_alone() {
        let binds = [binding([mdns("myserver.local", HTTPS_PORT)])];
        assert_eq!(
            browser_https_port(binds.iter(), &gateway("eth0"), "myserver.local"),
            None
        );
    }

    #[test]
    fn ignores_a_domain_served_only_in_plaintext() {
        let binds = [binding([private("cloud.mydomain.com", false, 8080)])];
        assert_eq!(
            browser_https_port(binds.iter(), &gateway("eth0"), "cloud.mydomain.com"),
            None
        );
    }

    #[test]
    fn ignores_a_tls_port_that_is_not_https() {
        let binds = [binding_serving(
            Some("ssl"),
            ServiceInterfaceType::Api,
            [private("electrum.mydomain.com", true, 50002)],
        )];
        assert_eq!(
            browser_https_port(binds.iter(), &gateway("eth0"), "electrum.mydomain.com"),
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
            browser_https_port(binds.iter(), &gateway("eth0"), "p2p.mydomain.com"),
            None
        );
    }

    #[test]
    fn ignores_a_ui_that_declares_another_scheme() {
        let binds = [binding_serving(
            Some("wss"),
            ServiceInterfaceType::Ui,
            [private("socket.mydomain.com", true, HTTPS_PORT)],
        )];
        assert_eq!(
            browser_https_port(binds.iter(), &gateway("eth0"), "socket.mydomain.com"),
            None
        );
    }

    #[test]
    fn sends_a_ui_with_no_declared_scheme() {
        let binds = [binding_serving(
            None,
            ServiceInterfaceType::Ui,
            [private("cloud.mydomain.com", true, 8443)],
        )];
        assert_eq!(
            browser_https_port(binds.iter(), &gateway("eth0"), "cloud.mydomain.com"),
            Some(8443)
        );
    }

    #[test]
    fn ignores_a_domain_scoped_to_another_gateway() {
        let binds = [binding([private("cloud.mydomain.com", true, HTTPS_PORT)])];
        assert_eq!(
            browser_https_port(binds.iter(), &gateway("wg0"), "cloud.mydomain.com"),
            None
        );
    }

    // Disabled bindings retain their domains in the database.
    #[test]
    fn ignores_a_disabled_binding() {
        let mut bind = binding([private("cloud.mydomain.com", true, HTTPS_PORT)]);
        bind.enabled = false;
        assert_eq!(
            browser_https_port([&bind], &gateway("eth0"), "cloud.mydomain.com"),
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
            browser_https_port([&bind], &gateway("eth0"), "cloud.mydomain.com"),
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
            browser_https_port(binds.iter(), &gateway("eth0"), "cloud.mydomain.com"),
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
            browser_https_port(binds.iter(), &gateway("eth0"), "cloud.mydomain.com"),
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

    // A derived address does not establish domain ownership.
    #[test]
    fn ignores_a_name_no_host_lists_as_a_domain() {
        let bind = binding([private("cloud.mydomain.com", true, HTTPS_PORT)]);
        let db = db(empty_host(), host_holding("other.example", &bind));
        assert_eq!(
            package_service_tls_port(&db, &gateway("eth0"), "cloud.mydomain.com").unwrap(),
            None
        );
    }

    #[test]
    fn finds_a_service_domain_in_the_database() {
        let bind = binding([private("cloud.mydomain.com", true, HTTPS_PORT)]);
        let db = db(empty_host(), host_holding("cloud.mydomain.com", &bind));
        assert_eq!(
            package_service_tls_port(&db, &gateway("eth0"), "cloud.mydomain.com").unwrap(),
            Some(HTTPS_PORT)
        );
    }

    #[test]
    fn does_not_search_the_servers_own_host() {
        let bind = binding([private("home.mydomain.com", true, HTTPS_PORT)]);
        let db = db(host_holding("home.mydomain.com", &bind), empty_host());
        assert_eq!(
            package_service_tls_port(&db, &gateway("eth0"), "home.mydomain.com").unwrap(),
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
        assert_eq!(
            arrival_gateway_id(&arriving_on("eth0")),
            Some(gateway("eth0"))
        );
    }

    #[test]
    fn reads_an_unplaced_connection_as_no_gateway() {
        assert_eq!(arrival_gateway_id(&arriving_on("")), None);
        assert_eq!(arrival_gateway_id(&request("cloud.mydomain.com")), None);
    }

    #[test]
    fn reads_a_host_name() {
        assert_eq!(
            request_domain(&request("Cloud.MyDomain.com:80")).as_deref(),
            Some("cloud.mydomain.com")
        );
    }

    #[test]
    fn reads_a_host_name_written_from_the_root() {
        assert_eq!(
            request_domain(&request("cloud.mydomain.com.")).as_deref(),
            Some("cloud.mydomain.com")
        );
    }

    #[test]
    fn reads_the_authority_of_a_request_with_no_host_header() {
        let req = Request::builder()
            .uri("https://cloud.mydomain.com/photos")
            .body(Body::empty())
            .unwrap();
        assert_eq!(request_domain(&req).as_deref(), Some("cloud.mydomain.com"));
    }

    #[test]
    fn prefers_the_authority_to_the_header() {
        let req = Request::builder()
            .uri("http://cloud.mydomain.com/photos")
            .header(http::header::HOST, "other.mydomain.com")
            .body(Body::empty())
            .unwrap();
        assert_eq!(request_domain(&req).as_deref(), Some("cloud.mydomain.com"));
    }

    #[test]
    fn skips_a_host_named_by_address() {
        assert_eq!(request_domain(&request("192.168.1.5")), None);
        assert_eq!(request_domain(&request("192.168.1.5:80")), None);
        assert_eq!(request_domain(&request("[fd00::1]:80")), None);
    }

    #[test]
    fn skips_a_host_outside_the_name_character_set() {
        assert_eq!(request_domain(&request("admin@evil.example")), None);
        assert_eq!(request_domain(&request("cloud.mydomain.com!")), None);
        assert_eq!(
            request_domain(&request(&format!("{}.com", "a".repeat(250)))),
            None
        );
    }

    #[test]
    fn leaves_the_servers_mdns_name_on_the_dashboard_only_on_443() {
        assert!(is_dashboard_mdns("myserver.local", HTTPS_PORT, "myserver"));
        assert!(!is_dashboard_mdns("myserver.local", 8443, "myserver"));
    }

    fn location(uri: Option<&str>, port: u16) -> String {
        let path_and_query = uri
            .filter(|path| *path != "*")
            .map(|uri| uri.parse().unwrap());
        let target = https_redirect(path_and_query.as_ref(), "cloud.mydomain.com", port).unwrap();
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
            location(Some("/photos?share=1"), HTTPS_PORT),
            "https://cloud.mydomain.com/photos?share=1"
        );
    }

    #[test]
    fn redirects_a_target_without_a_path_to_the_root() {
        assert_eq!(location(None, HTTPS_PORT), "https://cloud.mydomain.com/");
        assert_eq!(
            location(Some("*"), HTTPS_PORT),
            "https://cloud.mydomain.com/"
        );
    }

    #[test]
    fn names_a_tls_port_that_is_not_443() {
        assert_eq!(
            location(Some("/photos"), 8443),
            "https://cloud.mydomain.com:8443/photos"
        );
    }
}
