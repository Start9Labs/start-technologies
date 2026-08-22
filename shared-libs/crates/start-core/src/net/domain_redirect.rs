//! HTTP→HTTPS redirect for a service's domains on the StartOS UI's port.
//!
//! Port 80 belongs to the StartOS UI, whose listener answers on every address
//! the server holds. A browser given a bare domain name tries port 80 first, so
//! typing a service's private domain lands on the dashboard: a plaintext
//! request carries no SNI, which is what tells the service apart from every
//! other name this server answers to.

use std::collections::BTreeSet;
use std::net::IpAddr;

use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use http::uri::{Authority, Scheme};
use http::{HeaderValue, StatusCode, Uri};
use imbl_value::InternedString;

use crate::context::RpcContext;
use crate::net::host::binding::BindInfo;
use crate::net::service_interface::HostnameMetadata;
use crate::prelude::*;

const HTTPS_PORT: u16 = 443;
const HTTPS_SCHEME: &str = "https";

/// Bounce a request whose `Host` names a domain an installed service is served
/// on.
pub fn layer(ctx: RpcContext, router: Router) -> Router {
    router.layer(axum::middleware::from_fn(
        move |req: Request, next: Next| {
            let ctx = ctx.clone();
            async move {
                if let Some(name) = host(&req) {
                    let uri = req.uri().clone();
                    match redirect(&ctx, &name, &uri).await {
                        Ok(Some(res)) => return res,
                        Ok(None) => (),
                        Err(e) => {
                            tracing::warn!(
                                "failed to check whether {name} is a service domain: {e}"
                            );
                            tracing::debug!("{e:?}");
                        }
                    }
                }
                next.run(req).await
            }
        },
    ))
}

/// The host the request named, lowercased and stripped of any port and of the
/// root's trailing dot. `None` when the request names no host or names one by
/// IP address, neither of which can be a configured domain.
fn host(req: &Request) -> Option<String> {
    let host = match req.headers().get(http::header::HOST) {
        Some(host) => host.to_str().ok()?,
        // An HTTP/2 request carries `:authority` instead, which hyper puts in
        // the URI rather than in a header.
        None => req.uri().host()?,
    };
    // A bracketed host is an IPv6 literal, and it carries the only other colon
    // a host can hold.
    if host.starts_with('[') {
        return None;
    }
    let name = host.split_once(':').map_or(host, |(name, _)| name);
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    if name.is_empty() || name.parse::<IpAddr>().is_ok() {
        return None;
    }
    Some(name)
}

async fn redirect(ctx: &RpcContext, name: &str, uri: &Uri) -> Result<Option<Response>, Error> {
    // Only a request in origin form carries a path to send across. `OPTIONS *`
    // and the authority form of `CONNECT` do not.
    if uri
        .path_and_query()
        .map_or(true, |p| !p.path().starts_with('/'))
    {
        return Ok(None);
    }
    let Some(port) = service_tls_port(ctx, name).await? else {
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

/// The TLS port an installed service answers `name` on.
///
/// The server's own host is not searched: its TLS listener proxies back to this
/// very port, so redirecting a name it answers to would loop.
async fn service_tls_port(ctx: &RpcContext, name: &str) -> Result<Option<u16>, Error> {
    let db = ctx.db.peek().await;
    for (_, package) in db.as_public().as_package_data().as_entries()? {
        for (_, host) in package.as_hosts().as_entries()? {
            let configured =
                |domains: BTreeSet<InternedString>| domains.iter().any(|d| **d == *name);
            if !configured(host.as_private_domains().keys()?)
                && !configured(host.as_public_domains().keys()?)
            {
                continue;
            }
            if let Some(port) = tls_port(host.as_bindings().de()?.values(), name) {
                return Ok(Some(port));
            }
        }
    }
    Ok(None)
}

/// The port to send a browser that asked for `name` in plaintext.
///
/// Only a domain qualifies. The server's IP addresses and its `.local` name are
/// how the dashboard itself is reached, and every host carries them.
fn tls_port<'a>(bindings: impl IntoIterator<Item = &'a BindInfo>, name: &str) -> Option<u16> {
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
        })
        .filter_map(|addr| addr.port)
        // A browser that reached this port named no port of its own, so it
        // wants 443; any other port is a guess.
        .min_by_key(|port| (*port != HTTPS_PORT, *port))
}

/// Whether a browser can open this binding's TLS port. A binding that carries
/// another protocol over TLS — an Electrum server, a TURN server — has a domain
/// and a TLS port like any other.
fn serves_https(bind: &BindInfo) -> bool {
    bind.interfaces
        .values()
        .any(|iface| iface.address_info.ssl_scheme.as_deref() == Some(HTTPS_SCHEME))
}

#[cfg(test)]
mod test {
    use std::collections::BTreeMap;

    use super::*;
    use crate::net::host::binding::{BindOptions, DerivedAddressInfo, NetInfo};
    use crate::net::service_interface::{
        AddressInfo, HostnameInfo, ServiceInterface, ServiceInterfaceType,
    };
    use crate::{GatewayId, HostId, Id, ServiceInterfaceId};

    fn gateways() -> BTreeSet<GatewayId> {
        [GatewayId::from(InternedString::from_static("eth0"))]
            .into_iter()
            .collect()
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
                gateway: GatewayId::from(InternedString::from_static("eth0")),
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
    fn interface(ssl_scheme: Option<&str>) -> ServiceInterface {
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
            interface_type: ServiceInterfaceType::Ui,
        }
    }

    fn binding_serving(
        ssl_scheme: Option<&str>,
        available: impl IntoIterator<Item = HostnameInfo>,
    ) -> BindInfo {
        let iface = interface(ssl_scheme);
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
        binding_serving(Some(HTTPS_SCHEME), available)
    }

    #[test]
    fn sends_a_service_domain_to_its_tls_port() {
        let binds = [binding([
            private("cloud.mydomain.com", false, 8080),
            private("cloud.mydomain.com", true, HTTPS_PORT),
        ])];
        assert_eq!(
            tls_port(binds.iter(), "cloud.mydomain.com"),
            Some(HTTPS_PORT)
        );
    }

    #[test]
    fn sends_a_public_domain_to_its_tls_port() {
        let binds = [binding([public("cloud.mydomain.com", HTTPS_PORT)])];
        assert_eq!(
            tls_port(binds.iter(), "cloud.mydomain.com"),
            Some(HTTPS_PORT)
        );
    }

    // The dashboard is reached by the server's `.local` name over plaintext, and
    // every host carries that name.
    #[test]
    fn leaves_the_mdns_name_alone() {
        let binds = [binding([mdns("myserver.local", HTTPS_PORT)])];
        assert_eq!(tls_port(binds.iter(), "myserver.local"), None);
    }

    #[test]
    fn ignores_a_domain_served_only_in_plaintext() {
        let binds = [binding([private("cloud.mydomain.com", false, 8080)])];
        assert_eq!(tls_port(binds.iter(), "cloud.mydomain.com"), None);
    }

    // An Electrum server's TLS port speaks its own protocol, and a browser sent
    // there would speak HTTP at it.
    #[test]
    fn ignores_a_tls_port_that_is_not_https() {
        let binds = [binding_serving(
            Some("ssl"),
            [private("electrum.mydomain.com", true, 50002)],
        )];
        assert_eq!(tls_port(binds.iter(), "electrum.mydomain.com"), None);
    }

    #[test]
    fn ignores_a_tls_port_with_no_declared_scheme() {
        let binds = [binding_serving(
            None,
            [private("p2p.mydomain.com", true, 8443)],
        )];
        assert_eq!(tls_port(binds.iter(), "p2p.mydomain.com"), None);
    }

    // A binding a service has retired keeps its domains in the database, but
    // nothing serves them.
    #[test]
    fn ignores_a_disabled_binding() {
        let mut bind = binding([private("cloud.mydomain.com", true, HTTPS_PORT)]);
        bind.enabled = false;
        assert_eq!(tls_port([&bind], "cloud.mydomain.com"), None);
    }

    #[test]
    fn ignores_an_address_the_operator_switched_off() {
        let mut bind = binding([private("cloud.mydomain.com", true, HTTPS_PORT)]);
        bind.addresses.disabled.insert((
            InternedString::from_static("cloud.mydomain.com"),
            HTTPS_PORT,
        ));
        assert_eq!(tls_port([&bind], "cloud.mydomain.com"), None);
    }

    #[test]
    fn prefers_443_to_another_bindings_tls_port() {
        let binds = [
            binding([private("cloud.mydomain.com", true, 8443)]),
            binding([private("cloud.mydomain.com", true, HTTPS_PORT)]),
        ];
        assert_eq!(
            tls_port(binds.iter(), "cloud.mydomain.com"),
            Some(HTTPS_PORT)
        );
    }

    #[test]
    fn falls_back_to_the_lowest_tls_port() {
        let binds = [
            binding([private("cloud.mydomain.com", true, 9443)]),
            binding([private("cloud.mydomain.com", true, 8443)]),
        ];
        assert_eq!(tls_port(binds.iter(), "cloud.mydomain.com"), Some(8443));
    }

    fn request(host: &str) -> Request {
        Request::builder()
            .uri("/photos?share=1")
            .header(http::header::HOST, host)
            .body(Body::empty())
            .unwrap()
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

    // An HTTP/2 request has no `Host` header; hyper puts `:authority` in the URI.
    #[test]
    fn reads_the_authority_of_a_request_with_no_host_header() {
        let req = Request::builder()
            .uri("https://cloud.mydomain.com/photos")
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
