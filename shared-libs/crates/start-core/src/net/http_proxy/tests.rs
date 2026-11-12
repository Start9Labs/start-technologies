use super::*;

pub(super) fn gateway() -> HttpProxyGateway {
    HttpProxyGateway::UrlPlugin {
        package_id: "urlgateway".parse().unwrap(),
    }
}

fn profile(id: &str) -> HttpProxyProfile {
    HttpProxyProfile {
        id: id.into(),
        name: LocaleString::Translated(id.to_owned()),
        protocols: BTreeSet::from([HttpProxyProtocol::Http1]),
    }
}

fn network(id: &str) -> HttpProxyGateway {
    HttpProxyGateway::Network {
        gateway_id: id.parse().unwrap(),
    }
}

fn http_binding() -> (
    crate::net::host::binding::BindInfo,
    crate::net::forward::AvailablePorts,
) {
    use crate::net::host::binding::{BindInfo, BindOptions, ProxyOptions};
    let mut ports = crate::net::forward::AvailablePorts::new();
    let binding = BindInfo::new(
        &mut ports,
        BindOptions {
            preferred_external_port: 8080,
            add_ssl: None,
            secure: None,
            proxy: Some(ProxyOptions::Http {
                add_x_forwarded_headers: false,
                auth: None,
            }),
        },
        false,
    )
    .unwrap();
    (binding, ports)
}

#[test]
fn gateway_replacement_preserves_other_intent_and_empty_public_only_preferences() {
    let (mut binding, mut ports) = http_binding();
    let packages = AllPackageData::default();
    for id in ["eth0", "eth1"] {
        replace_chain(
            &mut binding,
            &packages,
            HttpProxyChain {
                gateway: network(id),
                chain: vec![],
                public_only: true,
            },
        )
        .unwrap();
    }
    replace_chain(
        &mut binding,
        &packages,
        HttpProxyChain {
            gateway: network("eth0"),
            chain: vec![],
            public_only: false,
        },
    )
    .unwrap();
    assert_eq!(binding.proxy_chains.len(), 2);
    assert!(!binding.proxy_chains[0].public_only);
    assert!(binding.proxy_chains[1].public_only);
    assert!(!binding.has_proxy_chains());
    let mut options = binding.options.clone();
    options.proxy = None;
    assert!(binding.update(&mut ports, options, false).is_ok());
}

#[test]
fn url_gateway_public_only_input_is_rejected_without_mutation() {
    let (mut binding, _ports) = http_binding();
    let params: SetHttpProxyChainParams = serde_json::from_value(serde_json::json!({
        "internalPort":8080,"policy":{"gateway":{"kind":"url-plugin","packageId":"tor"},"chain":[],"publicOnly":true}
    })).unwrap();
    assert!(replace_chain(&mut binding, &AllPackageData::default(), params.policy).is_err());
    assert!(binding.proxy_chains.is_empty());
}

#[test]
fn attachment_gateway_identity_separates_equal_names_and_survives_policy_changes() {
    let mut host = crate::net::host::Host::new();
    let (mut binding, _ports) = http_binding();
    let stage = HttpProxyStage {
        package_id: "waf".parse().unwrap(),
        profile_id: "standard".into(),
    };
    binding.proxy_chains = vec![
        HttpProxyChain {
            gateway: network("tor"),
            chain: vec![stage.clone()],
            public_only: true,
        },
        HttpProxyChain {
            gateway: HttpProxyGateway::UrlPlugin {
                package_id: "tor".parse().unwrap(),
            },
            chain: vec![stage],
            public_only: false,
        },
    ];
    host.bindings.0.insert(8080, binding);
    let packages = AllPackageData::default();
    let provider = "waf".parse().unwrap();
    let attachments = host_attachments(
        &packages,
        &provider,
        &PackageId::start_os(),
        &HostId::admin(),
        &host,
    );
    assert_eq!(attachments.len(), 2);
    assert_eq!(attachments[0].binding.package_id, PackageId::start_os());
    assert_ne!(attachments[0].ingress, attachments[1].ingress);
    assert_ne!(attachments[0].next, attachments[1].next);
    host.bindings.0.get_mut(&8080).unwrap().proxy_chains[0].public_only = false;
    assert_eq!(
        attachments,
        host_attachments(
            &packages,
            &provider,
            &PackageId::start_os(),
            &HostId::admin(),
            &host
        )
    );
}

#[test]
fn profile_publication_replaces_an_authoritative_valid_list() {
    assert!(profiles_are_valid(&[]));
    assert!(profiles_are_valid(&[
        profile("standard"),
        profile("strict")
    ]));
    assert!(!profiles_are_valid(&[
        profile("standard"),
        profile("standard")
    ]));
    assert!(!profiles_are_valid(&[profile("")]));
    let mut unsupported = profile("standard");
    unsupported.protocols.clear();
    assert!(!profiles_are_valid(&[unsupported]));
}

#[test]
fn a_provider_can_appear_again_with_a_different_profile() {
    let edge = HttpProxyStage {
        package_id: "waf".parse().unwrap(),
        profile_id: "edge".into(),
    };
    let app = HttpProxyStage {
        package_id: edge.package_id.clone(),
        profile_id: "app".into(),
    };
    assert!(chain_is_unique(&[edge.clone(), app]));
    assert!(!chain_is_unique(&[edge.clone(), edge]));
}

#[test]
fn attachment_socket_identity_is_binding_and_profile_not_chain_position() {
    let binding = HttpProxyBinding {
        package_id: "app".parse().unwrap(),
        host_id: "web".parse().unwrap(),
        internal_port: 8080,
    };
    let socket = binding.socket_name(&gateway(), "standard");
    assert_eq!(socket, binding.socket_name(&gateway(), "standard"));
    assert_ne!(socket, binding.socket_name(&gateway(), "strict"));
    for other in [
        HttpProxyBinding {
            package_id: "other".parse().unwrap(),
            ..binding.clone()
        },
        HttpProxyBinding {
            host_id: "api".parse().unwrap(),
            ..binding.clone()
        },
        HttpProxyBinding {
            internal_port: 8081,
            ..binding.clone()
        },
    ] {
        assert_ne!(socket, other.socket_name(&gateway(), "standard"));
    }
    let path = binding.socket_path(
        Path::new(CONTAINER_SOCKET_DIR),
        "next",
        &gateway(),
        "../../elsewhere",
    );
    assert_eq!(
        path.parent().unwrap(),
        Path::new(CONTAINER_SOCKET_DIR).join("next")
    );
}

#[test]
fn profile_and_attachment_schemas_keep_transport_separate_from_protocol() {
    let profile: HttpProxyProfile = serde_json::from_value(serde_json::json!({
        "id": "strict", "name": "Strict", "protocols": ["http1", "http2"]
    }))
    .unwrap();
    assert_eq!(profile.protocols.len(), 2);
    let endpoint = HttpProxyEndpoint::Unix {
        path: "/media/startos/http-proxy/next/test.sock".into(),
    };
    assert_eq!(
        serde_json::to_value(endpoint).unwrap(),
        serde_json::json!({
            "transport": "unix", "path": "/media/startos/http-proxy/next/test.sock"
        })
    );
}
