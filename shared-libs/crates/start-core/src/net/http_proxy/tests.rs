use super::*;

fn profile(id: &str) -> HttpProxyProfile {
    HttpProxyProfile {
        id: id.into(),
        name: LocaleString::Translated(id.to_owned()),
        protocols: BTreeSet::from([HttpProxyProtocol::Http1]),
    }
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
    let socket = binding.socket_name("standard");
    assert_eq!(socket, binding.socket_name("standard"));
    assert_ne!(socket, binding.socket_name("strict"));
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
        assert_ne!(socket, other.socket_name("standard"));
    }
    let path = binding.socket_path(Path::new(CONTAINER_SOCKET_DIR), "next", "../../elsewhere");
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
