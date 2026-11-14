use super::*;
use crate::PackageId;

#[test]
fn absent_named_consent_remains_subject_to_explicit_negatives() {
    let key = (InternedString::intern("peers.example.com"), 50000);
    let mut addresses = DerivedAddressInfo {
        default_off: true,
        enabled_names: [key.clone()].into(),
        ..Default::default()
    };
    assert!(addresses.has_enabled_intent());
    addresses.disabled.insert(key);
    assert!(!addresses.has_enabled_intent());
}

fn candidates(ssl: bool) -> BTreeSet<HostnameInfo> {
    let gateway = GatewayId::from(InternedString::intern("eth0"));
    [
        (
            "192.168.1.2",
            HostnameMetadata::Ipv4 {
                gateway: gateway.clone(),
            },
        ),
        (
            "2001:db8::2",
            HostnameMetadata::Ipv6 {
                gateway: gateway.clone(),
                scope_id: 0,
            },
        ),
        (
            "server.local",
            HostnameMetadata::Mdns {
                gateways: BTreeSet::from([gateway.clone()]),
            },
        ),
        (
            "private.example",
            HostnameMetadata::PrivateDomain {
                gateways: BTreeSet::from([gateway.clone()]),
            },
        ),
        ("public.example", HostnameMetadata::PublicDomain { gateway }),
        (
            "plugin.example",
            HostnameMetadata::Plugin {
                package_id: PackageId::start_os(),
                remove_action: None,
                overflow_actions: Vec::new(),
                info: Value::Null,
            },
        ),
    ]
    .into_iter()
    .map(|(hostname, metadata)| HostnameInfo {
        hostname: InternedString::intern(hostname),
        port: Some(4242),
        ssl,
        public: matches!(metadata, HostnameMetadata::PublicDomain { .. }),
        metadata,
    })
    .collect()
}

fn named(info: &DerivedAddressInfo, hostname: &str) -> HostnameInfo {
    info.available
        .iter()
        .find(|address| address.hostname == InternedString::intern(hostname))
        .unwrap()
        .clone()
}

#[test]
fn default_off_candidates_require_consent_without_fabricating_opt_outs() {
    let mut info = DerivedAddressInfo::default();
    info.set_available(candidates(true), true);
    assert_eq!(info.available.len(), 6);
    assert!(info.enabled().is_empty());
    assert!(info.disabled.is_empty());
}

#[test]
fn ordinary_bindings_keep_lan_and_named_defaults() {
    let mut info = DerivedAddressInfo::default();
    info.set_available(candidates(true), false);
    for hostname in [
        "192.168.1.2",
        "2001:db8::2",
        "server.local",
        "private.example",
        "plugin.example",
    ] {
        assert!(info.enabled().contains(&named(&info, hostname)));
    }
}

#[test]
fn named_consent_survives_discovery_flaps_but_not_parent_clearing() {
    let mut info = DerivedAddressInfo::default();
    let available = candidates(true);
    info.set_available(available.clone(), true);
    for hostname in ["private.example", "public.example", "plugin.example"] {
        let address = named(&info, hostname);
        set_address_enabled_on(&mut info, &address, true).unwrap();
    }
    info.set_available(BTreeSet::new(), true);
    let previous = info.clone();
    info.set_available(available.clone(), true);
    info.resolve_address_groups(&previous);
    assert_eq!(info.enabled().len(), 3);
    info.disable_all();
    info.set_available(BTreeSet::new(), true);
    info.set_available(available, true);
    assert!(info.enabled().is_empty());
    assert!(info.enabled_names.is_empty());
}

#[test]
fn mdns_consent_controls_unselected_lan_ips_under_default_off() {
    let mut info = DerivedAddressInfo::default();
    info.set_available(candidates(false), true);
    let mdns = named(&info, "server.local");
    set_address_enabled_on(&mut info, &mdns, true).unwrap();
    for hostname in ["192.168.1.2", "2001:db8::2", "server.local"] {
        assert!(info.enabled().contains(&named(&info, hostname)));
    }
    let ip = named(&info, "192.168.1.2");
    set_address_enabled_on(&mut info, &ip, false).unwrap();
    assert!(!info.enabled().contains(&ip));
    set_address_enabled_on(&mut info, &mdns, false).unwrap();
    assert!(info.enabled().is_empty());
}

#[test]
fn plugin_projection_tracks_metadata_replacement_and_removal() {
    let mut info = DerivedAddressInfo::default();
    info.set_available(candidates(true), true);
    let plugin = named(&info, "plugin.example");
    set_address_enabled_on(&mut info, &plugin, true).unwrap();
    info.refresh_selected();
    assert!(info.selected.as_ref().unwrap().contains(&plugin));
    let mut replacement = plugin.clone();
    if let HostnameMetadata::Plugin { info, .. } = &mut replacement.metadata {
        *info = serde_json::json!({ "revision": 2 }).into();
    }
    info.available.remove(&plugin);
    info.available.insert(replacement.clone());
    info.refresh_selected();
    assert!(info.selected.as_ref().unwrap().contains(&replacement));
    assert!(!info.selected.as_ref().unwrap().contains(&plugin));
    info.available.remove(&replacement);
    info.refresh_selected();
    assert!(info.selected.as_ref().unwrap().is_empty());
}

#[test]
fn default_off_lan_ip_without_mdns_requires_an_override() {
    let mut available = candidates(true);
    available.retain(|address| !matches!(address.metadata, HostnameMetadata::Mdns { .. }));
    let mut info = DerivedAddressInfo::default();
    info.set_available(available, true);
    let ip = named(&info, "192.168.1.2");
    assert!(!info.enabled().contains(&ip));
    set_address_enabled_on(&mut info, &ip, true).unwrap();
    assert!(info.enabled().contains(&ip));
}

#[test]
fn rekeying_moves_named_consent_but_not_plugin_endpoints() {
    let mut info = DerivedAddressInfo::default();
    info.set_available(candidates(true), true);
    let private = named(&info, "private.example");
    let plugin = named(&info, "plugin.example");
    set_address_enabled_on(&mut info, &private, true).unwrap();
    set_address_enabled_on(&mut info, &plugin, true).unwrap();
    info.rekey_port(4242, 4545);
    let available = info
        .available
        .iter()
        .cloned()
        .map(|mut address| {
            if !matches!(address.metadata, HostnameMetadata::Plugin { .. }) {
                address.port = Some(4545);
            }
            address
        })
        .collect();
    info.set_available(available, true);
    assert!(info.enabled().contains(&named(&info, "private.example")));
    assert!(info.enabled().contains(&plugin));
    assert!(!info.enabled_names.contains(&(plugin.hostname, 4545)));
}

#[test]
fn inactive_and_stale_rows_cannot_select_lan_or_wan_exposure() {
    let options: BindOptions = serde_json::from_value(serde_json::json!({
        "preferredExternalPort": 4242, "defaultOff": true,
    }))
    .unwrap();
    let mut ports = AvailablePorts::new();
    let mut binding = BindInfo::new(&mut ports, options, true).unwrap();
    binding.addresses.set_available(candidates(true), true);
    let gua = named(&binding.addresses, "2001:db8::2");
    let mut stale = gua.clone();
    stale.port = Some(4545);
    assert!(binding.set_address_enabled(&stale, true).is_err());
    assert!(binding.set_gua_wan(&stale, true).is_err());
    binding.disable();
    assert!(binding.set_address_enabled(&gua, true).is_err());
    assert!(binding.set_gua_wan(&gua, true).is_err());
    assert!(binding.addresses.enabled().is_empty());
    assert!(binding.addresses.gua_wan.is_empty());
}

#[test]
fn legacy_public_only_preserves_served_names_without_enabling_new_lan_rows() {
    let options: BindOptions = serde_json::from_value(serde_json::json!({
        "preferredExternalPort": 4242, "publicOnly": true,
    }))
    .unwrap();
    assert!(options.default_off);
    let encoded = serde_json::to_value(&options).unwrap();
    assert_eq!(encoded["defaultOff"], true);
    assert!(encoded.get("publicOnly").is_none());
    let mut info = DerivedAddressInfo::default();
    info.available = candidates(true)
        .into_iter()
        .filter(|address| address.public)
        .collect();
    info.set_available(candidates(true), options.default_off);
    assert_eq!(info.enabled().len(), 1);
    assert_eq!(
        info.enabled().first().unwrap().hostname,
        InternedString::intern("public.example")
    );
}
