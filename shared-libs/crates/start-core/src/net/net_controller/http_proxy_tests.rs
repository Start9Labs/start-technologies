use super::*;
use crate::db::model::public::IpInfo;
use crate::net::host::binding::{ProxyOptions, Security};

fn options() -> BindOptions {
    BindOptions {
        preferred_external_port: 8080,
        add_ssl: Some(AddSslOptions {
            preferred_external_port: 8443,
            alpn: None,
            upstream_cert_validation: None,
        }),
        secure: None,
        proxy: None,
    }
}

#[test]
fn unproxied_service_tls_preserves_source_for_termination_rewrap_and_passthrough() {
    let address = "10.0.3.2:8080".parse().unwrap();
    let address_v6 = Some("[fd00::2]:8080".parse().unwrap());
    for (add_ssl, secure) in [(true, None), (true, Some(true)), (false, Some(true))] {
        let mut options = options();
        if !add_ssl {
            options.add_ssl = None;
        }
        options.secure = secure.map(|ssl| Security { ssl });
        let target = binding_proxy_target(&options, address, address_v6, None);
        assert!(target.preserve_source_ip);
        assert_eq!(target.passthrough, !add_ssl);
        assert!(
            !binding_proxy_target(&options, "127.0.0.1:80".parse().unwrap(), None, None)
                .preserve_source_ip
        );
        if add_ssl {
            options.proxy = Some(ProxyOptions::Http {
                add_x_forwarded_headers: false,
                auth: None,
            });
            assert!(!binding_proxy_target(&options, address, address_v6, None).preserve_source_ip);
        }
    }
}

fn row(host: &str, port: u16, ssl: bool, public: bool, metadata: HostnameMetadata) -> HostnameInfo {
    HostnameInfo {
        hostname: host.into(),
        port: Some(port),
        ssl,
        public,
        metadata,
    }
}

#[test]
fn plaintext_http_exposure_keeps_internal_addresses_and_gateway_security() {
    let gateway: GatewayId = "eth0".parse().unwrap();
    let ip: IpAddr = "192.0.2.10".parse().unwrap();
    let bridge = row(
        "10.0.3.1",
        8080,
        false,
        false,
        HostnameMetadata::Ipv4 {
            gateway: "lxcbr0".parse().unwrap(),
        },
    );
    let localhost = row(
        "127.0.0.1",
        8080,
        false,
        false,
        HostnameMetadata::Ipv4 {
            gateway: "lo".parse().unwrap(),
        },
    );
    let lan = row(
        "192.0.2.10",
        8080,
        false,
        false,
        HostnameMetadata::Ipv4 {
            gateway: gateway.clone(),
        },
    );
    let ssl = row(
        "192.0.2.11",
        8443,
        true,
        false,
        HostnameMetadata::Ipv4 {
            gateway: gateway.clone(),
        },
    );
    let other_port = row(
        "192.0.2.12",
        9090,
        false,
        false,
        HostnameMetadata::Ipv4 {
            gateway: gateway.clone(),
        },
    );
    let requirements = ForwardRequirements {
        public_gateways: BTreeSet::new(),
        private_ips: BTreeSet::from([
            ip,
            "192.0.2.11".parse().unwrap(),
            "192.0.2.12".parse().unwrap(),
        ]),
        secure: false,
    };
    for trusted in [false, true] {
        let interfaces = [(
            gateway.clone(),
            NetworkInterfaceInfo {
                secure: Some(trusted),
                ip_info: Some(Arc::new(IpInfo {
                    subnets: ["192.0.2.10/24".parse::<IpNet>().unwrap()]
                        .into_iter()
                        .collect(),
                    ..Default::default()
                })),
                ..Default::default()
            },
        )]
        .into_iter()
        .collect();
        let target = binding_proxy_target(&options(), "10.0.3.2:8080".parse().unwrap(), None, None);
        let target = plaintext_http_target(
            target,
            8080,
            &BTreeSet::from([&bridge, &localhost, &lan, &ssl, &other_port]),
            &interfaces,
            &requirements,
        );
        let mut expected = BTreeSet::from([
            "10.0.3.1".parse::<IpAddr>().unwrap(),
            "127.0.0.1".parse().unwrap(),
        ]);
        if trusted {
            expected.insert(ip);
        }
        assert_eq!(target.private, expected);
        assert!(target.public_v4.is_empty());
        assert!(target.public_v6.is_empty());
    }
}

#[test]
fn plaintext_http_public_scope_is_per_family_and_private_domains_keep_their_lan_ips() {
    let gateway: GatewayId = "eth0".parse().unwrap();
    let interfaces = [(
        gateway.clone(),
        NetworkInterfaceInfo {
            secure: Some(false),
            ip_info: Some(Arc::new(IpInfo {
                subnets: ["192.0.2.10/24", "2001:db8::10/64"]
                    .into_iter()
                    .map(|s| s.parse::<IpNet>().unwrap())
                    .collect(),
                ..Default::default()
            })),
            ..Default::default()
        },
    )]
    .into_iter()
    .collect();
    let requirements = ForwardRequirements {
        public_gateways: BTreeSet::from([gateway.clone()]),
        private_ips: BTreeSet::from([
            "192.0.2.10".parse().unwrap(),
            "2001:db8::10".parse().unwrap(),
        ]),
        secure: true,
    };
    let gua = row(
        "2001:db8::10",
        8080,
        false,
        true,
        HostnameMetadata::Ipv6 {
            gateway: gateway.clone(),
            scope_id: 0,
        },
    );
    let ipv4 = row(
        "192.0.2.10",
        8080,
        false,
        true,
        HostnameMetadata::Ipv4 {
            gateway: gateway.clone(),
        },
    );
    let private = row(
        "service.internal",
        8080,
        false,
        false,
        HostnameMetadata::PrivateDomain {
            gateways: BTreeSet::from([gateway.clone()]),
        },
    );
    for addresses in [
        BTreeSet::from([&gua]),
        BTreeSet::from([&ipv4]),
        BTreeSet::from([&private]),
    ] {
        let target = binding_proxy_target(&options(), "10.0.3.2:8080".parse().unwrap(), None, None);
        let target = plaintext_http_target(target, 8080, &addresses, &interfaces, &requirements);
        if addresses.contains(&gua) {
            assert_eq!(
                target.public_v6,
                BTreeSet::from(["2001:db8::10".parse().unwrap()])
            );
            assert!(target.public_v4.is_empty());
        } else if addresses.contains(&ipv4) {
            assert_eq!(target.public_v4, BTreeSet::from([gateway.clone()]));
            assert!(target.public_v6.is_empty());
        } else {
            assert_eq!(target.private, requirements.private_ips);
            assert!(target.public_v4.is_empty());
            assert!(target.public_v6.is_empty());
        }
    }
}
