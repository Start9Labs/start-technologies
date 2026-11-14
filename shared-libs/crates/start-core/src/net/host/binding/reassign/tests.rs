use std::sync::Arc;

use rpc_toolkit::Empty;

use super::*;
use crate::db::model::public::{IpInfo, NetworkInterfaceInfo};
use crate::net::host::address::PublicDomainConfig;
use crate::net::host::binding::{AddSslOptions, BindOptions, RangeBindInfo, Security};
use crate::net::host::{ForPackage, ForServer, Host};
use crate::net::service_interface::{
    AddressInfo, HostnameInfo, ServiceInterface, ServiceInterfaceType,
};
use crate::{HostId, PackageId};

fn options(port: u16, ssl: Option<u16>, own_tls: bool) -> BindOptions {
    BindOptions {
        preferred_external_port: port,
        add_ssl: ssl.map(|preferred_external_port| AddSslOptions {
            preferred_external_port,
            alpn: None,
            upstream_cert_validation: None,
        }),
        secure: Some(Security { ssl: own_tls }),
        proxy: None,
    }
}

fn binding(ports: &mut AvailablePorts, port: u16, ssl: Option<u16>) -> BindInfo {
    let mut binding = BindInfo::new(ports, options(port, ssl, false), true).unwrap();
    binding.interfaces.insert(
        "api".parse().unwrap(),
        ServiceInterface {
            id: "api".parse().unwrap(),
            name: "API".into(),
            description: String::new(),
            masked: false,
            address_info: AddressInfo {
                username: None,
                host_id: "main".parse().unwrap(),
                internal_port: 8000,
                scheme: None,
                ssl_scheme: None,
                suffix: String::new(),
            },
            interface_type: ServiceInterfaceType::Api,
            preferred_launcher_address: None,
        },
    );
    binding
}

fn inheritance() -> (PackageId, HostId) {
    ("example".parse().unwrap(), "main".parse().unwrap())
}

fn database(host: Host, server: Host, ports: AvailablePorts) -> DatabaseModel {
    let gateway = NetworkInterfaceInfo {
        ip_info: Some(Arc::new(IpInfo {
            subnets: [
                "192.0.2.2/24".parse::<ipnet::IpNet>().unwrap(),
                "2001:db8::1/64".parse().unwrap(),
            ]
            .into_iter()
            .collect(),
            wan_ip: Some("198.51.100.2".parse().unwrap()),
            ..Default::default()
        })),
        ..Default::default()
    };
    let mut db = DatabaseModel::from(imbl_value::json!({
        "public": {
            "registryCache": {},
            "osUpdateCache": null,
            "ui": {},
            "disks": [],
            "sharedVolumes": {},
            "backupTargets": {},
            "serverInfo": {
                "hostname": "server",
                "network": {
                    "host": to_value(&server).unwrap(),
                    "gateways": { "eth0": to_value(&gateway).unwrap() },
                },
            },
            "packageData": { "example": { "hosts": { "main": to_value(&host).unwrap() } } },
        },
        "private": { "availablePorts": to_value(&ports).unwrap() },
    }));
    let hostname = ServerHostname::load(db.as_public().as_server_info()).unwrap();
    let gateways = db
        .as_public()
        .as_server_info()
        .as_network()
        .as_gateways()
        .de()
        .unwrap();
    for host in all_hosts(&mut db) {
        host.unwrap()
            .update_addresses(&hostname, &gateways, &ports)
            .unwrap();
    }
    db
}

fn package_host(db: &mut DatabaseModel) -> &mut Model<Host> {
    ForPackage::host_for(&inheritance(), db).unwrap()
}

fn params(internal_port: u16, external_port: u16, ssl: bool) -> ReassignPortParams {
    ReassignPortParams {
        internal_port,
        external_port,
        ssl,
    }
}

fn host_with(binding: BindInfo) -> Host {
    let mut host = Host::default();
    host.bindings.insert(8000, binding);
    host
}

#[test]
fn reassignment_preserves_binding_intent_and_rekeys_ip_overrides() {
    let mut ports = AvailablePorts::new();
    let mut bind = binding(&mut ports, 8080, Some(8443));
    let options = to_value(&bind.options).unwrap();
    let interfaces = to_value(&bind.interfaces).unwrap();
    bind.addresses
        .enabled
        .insert("198.51.100.2:8080".parse().unwrap());
    bind.addresses
        .disabled
        .insert(("server.local".into(), 8080));
    bind.addresses
        .lan_enabled
        .insert(("192.0.2.2".into(), 8080));
    bind.addresses
        .gua_wan
        .insert("[2001:db8::1]:8080".parse().unwrap());
    bind.addresses
        .enabled
        .insert("[2001:db8::1]:8080".parse().unwrap());
    bind.addresses
        .disabled
        .insert(("server.local".into(), 8443));
    let mut db = database(host_with(bind), Host::default(), ports);

    reassign::<ForPackage>(&mut db, &params(8000, 9000, false), &inheritance()).unwrap();

    let host = package_host(&mut db).de().unwrap();
    let bind = &host.bindings[&8000];
    assert!(bind.enabled);
    assert_eq!(bind.net.assigned_port, Some(9000));
    assert_eq!(bind.net.assigned_ssl_port, Some(8443));
    assert_eq!(to_value(&bind.options).unwrap(), options);
    assert_eq!(to_value(&bind.interfaces).unwrap(), interfaces);
    assert!(
        bind.addresses
            .enabled
            .contains(&"198.51.100.2:9000".parse().unwrap())
    );
    assert!(
        !bind
            .addresses
            .enabled
            .contains(&"198.51.100.2:8080".parse().unwrap())
    );
    assert!(
        bind.addresses
            .disabled
            .contains(&("server.local".into(), 9000))
    );
    assert!(
        bind.addresses
            .disabled
            .contains(&("server.local".into(), 8443))
    );
    assert!(
        bind.addresses
            .lan_enabled
            .contains(&("192.0.2.2".into(), 9000))
    );
    assert!(
        bind.addresses
            .gua_wan
            .contains(&"[2001:db8::1]:9000".parse().unwrap())
    );
    assert!(
        host.port_forwards
            .iter()
            .any(|forward| forward.src.port() == 9000)
    );
    assert!(
        !host
            .port_forwards
            .iter()
            .any(|forward| forward.src.port() == 8080)
    );
    let mut ports = db.as_private().as_available_ports().de().unwrap();
    assert_eq!(ports.try_alloc(8080, false, true), Some(8080));
    assert_eq!(ports.try_alloc(9000, false, true), None);
    assert!(ports.is_ssl(8443));
}

#[test]
fn ssl_reassignment_keeps_shared_domain_ports_and_preferences() {
    let mut ports = AvailablePorts::new();
    let server = host_with(binding(&mut ports, 80, Some(443)));
    let mut bind = binding(&mut ports, 8080, Some(8443));
    bind.options
        .add_ssl
        .as_mut()
        .unwrap()
        .preferred_external_port = 443;
    let old = bind.net.assigned_ssl_port.unwrap();
    bind.addresses
        .disabled
        .insert(("shared.example".into(), 443));
    bind.addresses
        .enabled
        .insert(format!("198.51.100.2:{old}").parse().unwrap());
    let mut host = host_with(bind);
    host.private_domains.insert(
        "shared.example".into(),
        ["eth0".parse().unwrap()].into_iter().collect(),
    );
    let mut db = database(host, server, ports);

    reassign::<ForPackage>(&mut db, &params(8000, 9443, true), &inheritance()).unwrap();

    let host = package_host(&mut db).de().unwrap();
    let bind = &host.bindings[&8000];
    assert_eq!(bind.net.assigned_port, Some(8080));
    assert_eq!(bind.net.assigned_ssl_port, Some(9443));
    assert_eq!(bind.options.preferred_ssl_port(), Some(443));
    assert!(
        bind.addresses
            .available
            .iter()
            .any(|address| address.hostname == "shared.example"
                && address.ssl
                && address.port == Some(443))
    );
    assert!(
        bind.addresses
            .disabled
            .contains(&("shared.example".into(), 443))
    );
    assert!(
        bind.addresses
            .enabled
            .contains(&"198.51.100.2:9443".parse().unwrap())
    );
    let mut ports = db.as_private().as_available_ports().de().unwrap();
    assert!(ports.is_ssl(443));
    assert!(ports.is_ssl(9443));
    assert!(!ports.is_ssl(old));
    assert_eq!(ports.try_alloc(old, false, true), Some(old));
}

#[test]
fn moving_a_shared_listener_refreshes_sibling_domains_without_enabling_them() {
    let mut ports = AvailablePorts::new();
    let listener = binding(&mut ports, 8081, Some(8444));
    let mut bind = binding(&mut ports, 8080, Some(8443));
    bind.options
        .add_ssl
        .as_mut()
        .unwrap()
        .preferred_external_port = 8444;
    bind.addresses.disabled.extend([
        ("private.example".into(), 8444),
        ("public.example".into(), 8444),
    ]);
    let mut host = host_with(bind);
    host.bindings.insert(8001, listener);
    host.private_domains.insert(
        "private.example".into(),
        ["eth0".parse().unwrap()].into_iter().collect(),
    );
    host.public_domains.insert(
        "public.example".into(),
        PublicDomainConfig {
            gateway: "eth0".parse().unwrap(),
            acme: None,
        },
    );
    let mut db = database(host, Host::default(), ports);

    reassign::<ForPackage>(&mut db, &params(8001, 9443, true), &inheritance()).unwrap();

    let host = package_host(&mut db).de().unwrap();
    let addresses = &host.bindings[&8000].addresses;
    for domain in ["private.example", "public.example"] {
        let row = addresses
            .available
            .iter()
            .find(|row| row.hostname == domain && row.ssl)
            .unwrap();
        assert_eq!(row.port, Some(8443));
        assert!(!addresses.enabled().contains(row));
    }
    assert!(
        !host
            .port_forwards
            .iter()
            .any(|forward| forward.src.port() == 8444)
    );
}

#[test]
fn assigning_a_shared_preferred_port_does_not_move_a_siblings_ip_overrides() {
    let mut ports = AvailablePorts::new();
    let listener = binding(&mut ports, 8081, Some(8444));
    let mut bind = binding(&mut ports, 8080, Some(9443));
    bind.options
        .add_ssl
        .as_mut()
        .unwrap()
        .preferred_external_port = 8444;
    bind.addresses
        .disabled
        .insert(("private.example".into(), 8444));
    bind.addresses
        .enabled
        .insert("198.51.100.2:9443".parse().unwrap());
    let mut host = host_with(bind);
    host.bindings.insert(8001, listener);
    host.private_domains.insert(
        "private.example".into(),
        ["eth0".parse().unwrap()].into_iter().collect(),
    );
    let mut db = database(host, Host::default(), ports);
    reassign::<ForPackage>(&mut db, &params(8001, 10443, true), &inheritance()).unwrap();
    reassign::<ForPackage>(&mut db, &params(8001, 8444, true), &inheritance()).unwrap();

    let host = package_host(&mut db).de().unwrap();
    let addresses = &host.bindings[&8000].addresses;
    assert!(
        addresses
            .enabled
            .contains(&"198.51.100.2:9443".parse().unwrap())
    );
    assert!(
        !addresses
            .enabled
            .contains(&"198.51.100.2:8444".parse().unwrap())
    );
    let domain = addresses
        .available
        .iter()
        .find(|row| row.hostname == "private.example" && row.ssl)
        .unwrap();
    assert_eq!(domain.port, Some(8444));
    assert!(!addresses.enabled().contains(domain));
}

#[test]
fn server_ui_ssl_port_is_fixed_through_both_api_routes() {
    let mut ports = AvailablePorts::new();
    let mut server = Host::default();
    server
        .bindings
        .insert(80, binding(&mut ports, 80, Some(443)));
    let mut db = database(Host::default(), server, ports);
    let before = to_value(&db).unwrap();

    for port in [0, 80, 8443] {
        assert!(reassign::<ForServer>(&mut db, &params(80, port, true), &Empty {}).is_err());
        assert!(
            reassign::<ForPackage>(
                &mut db,
                &params(80, port, true),
                &(PackageId::start_os(), HostId::admin()),
            )
            .is_err()
        );
        assert_eq!(to_value(&db).unwrap(), before);
    }
    reassign::<ForServer>(&mut db, &params(80, 443, true), &Empty {}).unwrap();
    reassign::<ForPackage>(
        &mut db,
        &params(80, 443, true),
        &(PackageId::start_os(), HostId::admin()),
    )
    .unwrap();
    assert_eq!(to_value(&db).unwrap(), before);
}

#[test]
fn server_ui_ssl_port_can_be_restored_to_443() {
    let mut ports = AvailablePorts::new();
    let mut server = Host::default();
    server
        .bindings
        .insert(80, binding(&mut ports, 80, Some(8443)));
    let mut db = database(Host::default(), server, ports);
    reassign::<ForServer>(&mut db, &params(80, 443, true), &Empty {}).unwrap();
    let server = ForServer::host_for(&Empty {}, &mut db)
        .unwrap()
        .de()
        .unwrap();
    assert_eq!(server.bindings[&80].net.assigned_ssl_port, Some(443));
}

#[test]
fn unavailable_ports_and_missing_legs_leave_state_unchanged() {
    let mut ports = AvailablePorts::new();
    let host = host_with(binding(&mut ports, 8080, Some(8443)));
    ports.try_alloc_range(9000, 3, true).unwrap();
    let mut server = Host::default();
    server.binding_ranges.insert(
        9000,
        RangeBindInfo {
            enabled: true,
            external_start_port: 9000,
            number_of_ports: 3,
            addresses: Default::default(),
            interface: None,
        },
    );
    let mut db = database(host, server, ports);
    for params in [
        params(8000, 0, false),
        params(8000, 80, false),
        params(8000, 1024, true),
        params(8000, 5353, false),
        params(8000, 5355, true),
        params(8000, 5432, false),
        params(8000, 6010, true),
        params(8000, 8443, false),
        params(8000, 8080, true),
        params(8000, 9001, false),
        params(8000, 9002, true),
        params(9999, 9000, false),
    ] {
        let before = db.clone();
        assert!(reassign::<ForPackage>(&mut db, &params, &inheritance()).is_err());
        assert_eq!(to_value(&db).unwrap(), to_value(&before).unwrap());
    }
    let mut ports = AvailablePorts::new();
    let host = host_with(binding(&mut ports, 8080, None));
    let mut db = database(host, Host::default(), ports);
    let before = db.clone();
    assert!(reassign::<ForPackage>(&mut db, &params(8000, 9000, true), &inheritance()).is_err());
    assert_eq!(to_value(&db).unwrap(), to_value(&before).unwrap());
}

#[test]
fn no_op_reassignment_is_unchanged_and_the_server_can_choose_low_ports() {
    let mut ports = AvailablePorts::new();
    let host = host_with(binding(&mut ports, 8080, None));
    let mut db = database(host, Host::default(), ports);
    let before = db.clone();
    reassign::<ForPackage>(&mut db, &params(8000, 8080, false), &inheritance()).unwrap();
    assert_eq!(to_value(&db).unwrap(), to_value(&before).unwrap());
    let mut ports = AvailablePorts::new();
    let server = host_with(binding(&mut ports, 8080, None));
    let mut db = database(Host::default(), server, ports);
    assert!(reassign::<ForServer>(&mut db, &params(8000, 0, false), &Empty {}).is_err());
    reassign::<ForServer>(&mut db, &params(8000, 80, false), &Empty {}).unwrap();
    assert_eq!(
        ForServer::host_for(&Empty {}, &mut db)
            .unwrap()
            .de()
            .unwrap()
            .bindings[&8000]
            .net
            .assigned_port,
        Some(80)
    );
}

#[test]
fn reassignment_does_not_enable_a_dormant_binding_or_advertise_its_forwards() {
    let mut ports = AvailablePorts::new();
    let mut bind = binding(&mut ports, 8080, None);
    bind.enabled = false;
    bind.addresses
        .enabled
        .insert("198.51.100.2:8080".parse().unwrap());
    let mut db = database(host_with(bind), Host::default(), ports);
    assert!(package_host(&mut db).de().unwrap().port_forwards.is_empty());
    reassign::<ForPackage>(&mut db, &params(8000, 9000, false), &inheritance()).unwrap();
    let host = package_host(&mut db).de().unwrap();
    assert!(!host.bindings[&8000].enabled);
    assert!(host.port_forwards.is_empty());
}

#[test]
fn reassignment_leaves_plugin_addresses_and_their_disabled_state_alone() {
    let mut ports = AvailablePorts::new();
    let mut bind = binding(&mut ports, 8080, None);
    let address = HostnameInfo {
        hostname: "external.example".into(),
        port: Some(8080),
        ssl: false,
        public: true,
        metadata: HostnameMetadata::Plugin {
            package_id: "plugin".parse().unwrap(),
            remove_action: None,
            overflow_actions: vec![],
            info: Value::Null,
        },
    };
    bind.addresses.available.insert(address.clone());
    bind.addresses
        .disabled
        .insert((address.hostname.clone(), 8080));
    let mut db = database(host_with(bind), Host::default(), ports);
    reassign::<ForPackage>(&mut db, &params(8000, 9000, false), &inheritance()).unwrap();
    let host = package_host(&mut db).de().unwrap();
    let addresses = &host.bindings[&8000].addresses;
    assert!(addresses.available.contains(&address));
    assert!(!addresses.enabled().contains(&address));
}

#[test]
fn rpc_params_use_camel_case_and_default_to_the_non_ssl_leg() {
    let params: ReassignPortParams = serde_json::from_value(serde_json::json!({
        "internalPort": 8000,
        "externalPort": 9000,
    }))
    .unwrap();
    assert_eq!(params.internal_port, 8000);
    assert_eq!(params.external_port, 9000);
    assert!(!params.ssl);
}

#[test]
fn service_rebinding_keeps_the_reassigned_port_for_every_tls_mode() {
    for options in [
        options(8080, None, false),
        options(8080, Some(8443), false),
        options(8443, None, true),
        options(8443, Some(9443), true),
    ] {
        let ssl = !options.wants_plain_port();
        let mut ports = AvailablePorts::new();
        let mut bind = BindInfo::new(&mut ports, options.clone(), false).unwrap();
        let old = if ssl {
            bind.net.assigned_ssl_port.unwrap()
        } else {
            bind.net.assigned_port.unwrap()
        };
        move_assigned_port(&mut bind, &mut ports, 10443, ssl, false).unwrap();
        assert_eq!(ports.try_alloc(old, ssl, true), Some(old));
        let bind = bind.update(&mut ports, options, false).unwrap();
        assert_eq!(
            if ssl {
                bind.net.assigned_ssl_port
            } else {
                bind.net.assigned_port
            },
            Some(10443)
        );
    }
}
