use std::sync::Arc;

use imbl::OrdMap;
use patch_db::ModelExt;

use super::*;
use crate::db::model::DatabaseModel;
use crate::db::model::public::{GatewayType, IpInfo, NetworkInterfaceInfo, ServerStatus};
use crate::net::forward::AvailablePorts;
use crate::net::host::Host;

fn database(preferences: TorrentPreferences) -> DatabaseModel {
    let gateway = NetworkInterfaceInfo {
        gateway_type: GatewayType::InboundOutbound,
        ip_info: Some(Arc::new(IpInfo {
            subnets: ["192.168.1.2/24", "2001:db8::1234/64", "fd00::1234/64"]
                .into_iter()
                .map(|subnet| subnet.parse::<ipnet::IpNet>().unwrap())
                .collect(),
            wan_ip: Some("203.0.113.45".parse().unwrap()),
            ..Default::default()
        })),
        ..Default::default()
    };
    Model::from_value(imbl_value::json!({
        "public": {"serverInfo": {
            "hostname": "server", "statusInfo": to_value(&ServerStatus::default()).unwrap(),
            "bittorrent": to_value(&preferences).unwrap(),
            "network": {"host": to_value(&Host::default()).unwrap(), "gateways": {"eth0": to_value(&gateway).unwrap()}}
        }},
        "private": {"availablePorts": to_value(&AvailablePorts::new()).unwrap()}
    }))
}

fn enabled() -> TorrentPreferences {
    TorrentPreferences {
        downloads: true,
        seeding: true,
    }
}

fn candidate(db: &DatabaseModel, hostname: &str) -> HostnameInfo {
    let info = db.as_public().as_server_info();
    let addresses = match info.as_network().as_host().as_bindings().as_idx(&6881) {
        Some(binding) => binding.as_addresses().de().unwrap(),
        None => incoming::preview(db).unwrap().unwrap().addresses,
    };
    addresses
        .available
        .into_iter()
        .find(|address| &*address.hostname == hostname)
        .unwrap()
}

fn assert_unallocated(db: &DatabaseModel) {
    assert!(
        !db.as_public()
            .as_server_info()
            .as_network()
            .as_host()
            .as_bindings()
            .contains_key(&6881)
            .unwrap(),
        "{:?}",
        db.as_public()
            .as_server_info()
            .as_network()
            .as_host()
            .de()
            .unwrap()
    );
    assert_eq!(
        db.as_private().as_available_ports().as_value(),
        &to_value(&AvailablePorts::new()).unwrap()
    );
    assert!(
        db.as_public()
            .as_server_info()
            .as_network()
            .as_host()
            .as_port_forwards()
            .de()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn disabled_and_outgoing_only_have_no_resources_or_reconciliation_writes() {
    for preferences in [
        TorrentPreferences::default(),
        TorrentPreferences {
            downloads: true,
            seeding: false,
        },
        enabled(),
    ] {
        for available in [false, true] {
            let mut db = database(preferences);
            let before = db.as_value().clone();
            for _ in 0..3 {
                assert!(
                    incoming::configure_in(&mut db, available)
                        .unwrap()
                        .is_none()
                );
                let preview = incoming::preview(&db).unwrap().unwrap();
                assert!(!preview.addresses.available.is_empty());
                assert!(
                    preview
                        .addresses
                        .available
                        .iter()
                        .all(|address| address.port.is_none() && !address.is_internal())
                );
                assert!(preview.addresses.enabled().is_empty());
                assert_eq!(db.as_value(), &before);
                assert_unallocated(&db);
            }
        }
    }
}

#[test]
fn first_opt_in_allocates_and_last_opt_out_releases_without_restoring_consent() {
    let mut db = database(enabled());
    let preview = candidate(&db, "192.168.1.2");
    assert_eq!(preview.port, None);
    let selected = incoming::select_address_in(&mut db, true, preview, true).unwrap();
    let port = selected.port.unwrap();
    assert_ne!(port, 0);
    assert_ne!(port, 6881);
    assert_eq!(
        incoming::configure_in(&mut db, true)
            .unwrap()
            .unwrap()
            .announce_port,
        Some(port)
    );
    let binding = db
        .as_public()
        .as_server_info()
        .as_network()
        .as_host()
        .as_bindings()
        .as_idx(&6881)
        .unwrap()
        .de()
        .unwrap();
    assert_eq!(binding.interfaces.len(), 1);
    assert_eq!(binding.enabled_addresses().len(), 1);
    let public = candidate(&db, "203.0.113.45");
    incoming::select_address_in(&mut db, true, public.clone(), true).unwrap();
    incoming::select_address_in(&mut db, true, selected, false).unwrap();
    assert_eq!(
        incoming::configure_in(&mut db, true)
            .unwrap()
            .unwrap()
            .announce_port,
        Some(port)
    );
    incoming::select_address_in(&mut db, true, public, false).unwrap();
    assert_unallocated(&db);
    assert!(incoming::configure_in(&mut db, true).unwrap().is_none());
    assert!(
        incoming::preview(&db)
            .unwrap()
            .unwrap()
            .addresses
            .enabled()
            .is_empty()
    );
}

#[test]
fn consent_and_assignment_survive_discovery_loss_and_explicit_opt_out_still_retires() {
    for hostname in ["peers.example.com", "192.168.1.2"] {
        let mut db = database(enabled());
        if hostname == "peers.example.com" {
            db.as_public_mut()
                .as_server_info_mut()
                .as_network_mut()
                .as_host_mut()
                .as_public_domains_mut()
                .insert(
                    &hostname.into(),
                    &crate::net::host::address::PublicDomainConfig {
                        gateway: "eth0".parse().unwrap(),
                        acme: None,
                    },
                )
                .unwrap();
        }
        let selected = candidate(&db, hostname);
        let selected = incoming::select_address_in(&mut db, true, selected, true).unwrap();
        let gateways = db
            .as_public()
            .as_server_info()
            .as_network()
            .as_gateways()
            .de()
            .unwrap();
        let server_hostname =
            crate::hostname::ServerHostname::load(db.as_public().as_server_info()).unwrap();
        let ports = db.as_private().as_available_ports().de().unwrap();
        for discovered in [OrdMap::new(), gateways] {
            let network = db.as_public_mut().as_server_info_mut().as_network_mut();
            network.as_gateways_mut().ser(&discovered).unwrap();
            network
                .as_host_mut()
                .update_addresses(&server_hostname, &discovered, &ports)
                .unwrap();
            if discovered.is_empty() && hostname == "192.168.1.2" {
                let binding = db
                    .as_public()
                    .as_server_info()
                    .as_network()
                    .as_host()
                    .as_bindings()
                    .as_idx(&6881)
                    .unwrap()
                    .de()
                    .unwrap();
                assert!(binding.enabled_addresses().is_empty());
            }
            let before = db.as_value().clone();
            assert_eq!(incoming::retained_bindings(&db, true).unwrap().len(), 1);
            assert_eq!(
                incoming::configure_in(&mut db, true)
                    .unwrap()
                    .unwrap()
                    .announce_port,
                selected.port
            );
            assert_eq!(db.as_value(), &before);
        }
        let rediscovered = candidate(&db, hostname);
        assert_eq!(rediscovered.port, selected.port);
        let binding = db
            .as_public()
            .as_server_info()
            .as_network()
            .as_host()
            .as_bindings()
            .as_idx(&6881)
            .unwrap()
            .de()
            .unwrap();
        assert!(binding.enabled_addresses().contains(&rediscovered));
        incoming::select_address_in(&mut db, true, rediscovered, false).unwrap();
        assert_unallocated(&db);
    }
}

#[test]
fn rejected_first_requests_and_local_gua_preview_do_not_allocate() {
    let mut db = database(enabled());
    let mut stale = candidate(&db, "192.168.1.2");
    stale.port = Some(50000);
    assert!(incoming::select_address_in(&mut db, true, stale, true).is_err());
    let lan = candidate(&db, "192.168.1.2");
    assert!(incoming::select_address_in(&mut db, false, lan.clone(), true).is_err());
    assert!(incoming::select_gua_in(&mut db, true, lan.clone(), true).is_err());
    incoming::select_address_in(&mut db, true, lan, false).unwrap();
    let gua = candidate(&db, "2001:db8::1234");
    incoming::select_gua_in(&mut db, true, gua, false).unwrap();
    assert_unallocated(&db);
    for preferences in [
        TorrentPreferences::default(),
        TorrentPreferences {
            downloads: true,
            seeding: false,
        },
    ] {
        let mut db = database(preferences);
        let lan = candidate(&db, "192.168.1.2");
        assert!(incoming::select_address_in(&mut db, true, lan, true).is_err());
        assert_unallocated(&db);
    }
}

#[test]
fn explicit_gua_public_selection_allocates_through_native_policy() {
    let mut db = database(enabled());
    let gua = candidate(&db, "2001:db8::1234");
    incoming::select_gua_in(&mut db, true, gua, true).unwrap();
    assert!(incoming::configure_in(&mut db, true).unwrap().is_some());
    let binding = db
        .as_public()
        .as_server_info()
        .as_network()
        .as_host()
        .as_bindings()
        .as_idx(&6881)
        .unwrap()
        .de()
        .unwrap();
    assert!(
        binding
            .enabled_addresses()
            .iter()
            .any(|address| address.public && &*address.hostname == "2001:db8::1234")
    );
}

#[tokio::test]
async fn address_rows_are_the_ingress_opt_in_and_either_parent_clears_them() -> Result<(), Error> {
    for downloads_off in [false, true] {
        let db = TypedPatchDb::<Database>::load_unchecked(PatchDb::in_memory());
        db.put(&patch_db::json_ptr::ROOT, database(enabled()).as_value())
            .await?;
        db.mutate(|db| {
            let lan = candidate(db, "192.168.1.2");
            incoming::select_address_in(db, true, lan, true)
        })
        .await
        .result?;
        update_preferences(
            &db,
            SetPreferencesParams {
                downloads: downloads_off.then_some(false),
                seeding: (!downloads_off).then_some(false),
            },
        )
        .await?;
        assert_unallocated(&db.peek().await);
        update_preferences(
            &db,
            SetPreferencesParams {
                downloads: Some(true),
                seeding: Some(true),
            },
        )
        .await?;
        assert_unallocated(&db.peek().await);
    }
    Ok(())
}

#[tokio::test]
async fn watcher_failure_stops_peer_work_and_ingress_without_erasing_intent() -> Result<(), Error> {
    let db = TypedPatchDb::<Database>::load_unchecked(PatchDb::in_memory());
    db.put(&patch_db::json_ptr::ROOT, database(enabled()).as_value())
        .await?;
    db.mutate(|db| {
        let lan = candidate(db, "192.168.1.2");
        incoming::select_address_in(db, true, lan, true)
    })
    .await
    .result?;
    let torrents = TorrentClient::for_server(Default::default(), None);
    torrents.set_downloads_enabled(true);
    watcher_failed(&db, &torrents).await?;
    torrents.set_downloads_enabled(true);
    assert!(!torrents.downloads_enabled());
    let snapshot = db.peek().await;
    let info = snapshot.as_public().as_server_info();
    assert!(info.as_status_info().as_bittorrent_watcher_failed().de()?);
    assert_eq!(info.as_bittorrent().de()?, enabled());
    assert_unallocated(&snapshot);
    Ok(())
}

#[tokio::test]
async fn preferences_cascade_atomically_in_the_authoritative_database() -> Result<(), Error> {
    let db = TypedPatchDb::<Database>::load_unchecked(PatchDb::in_memory());
    db.put(
        &patch_db::json_ptr::ROOT,
        database(TorrentPreferences::default()).as_value(),
    )
    .await?;
    assert_eq!(
        update_preferences(
            &db,
            SetPreferencesParams {
                seeding: Some(true),
                ..Default::default()
            }
        )
        .await?,
        TorrentPreferences::default()
    );
    assert_eq!(
        update_preferences(
            &db,
            SetPreferencesParams {
                downloads: Some(true),
                seeding: Some(true)
            }
        )
        .await?,
        enabled()
    );
    assert_eq!(
        update_preferences(
            &db,
            SetPreferencesParams {
                downloads: Some(false),
                ..Default::default()
            }
        )
        .await?,
        TorrentPreferences::default()
    );
    assert_eq!(
        update_preferences(
            &db,
            SetPreferencesParams {
                downloads: Some(true),
                ..Default::default()
            }
        )
        .await?,
        TorrentPreferences {
            downloads: true,
            seeding: false
        }
    );
    assert_unallocated(&db.peek().await);
    Ok(())
}

#[test]
fn startup_retains_only_live_consented_bindings_and_respects_the_cap() {
    let mut db = database(enabled());
    assert!(incoming::retained_bindings(&db, true).unwrap().is_empty());
    let address = candidate(&db, "192.168.1.2");
    incoming::select_address_in(&mut db, true, address, true).unwrap();
    let retained = incoming::retained_bindings(&db, true).unwrap();
    assert_eq!(retained.len(), 1);
    assert!(retained.contains(&crate::net::host::binding::BindId {
        id: crate::HostId::admin(),
        internal_port: 6881,
    }));
    assert!(incoming::retained_bindings(&db, false).unwrap().is_empty());
    db.as_public_mut()
        .as_server_info_mut()
        .as_bittorrent_mut()
        .ser(&TorrentPreferences::default())
        .unwrap();
    assert!(incoming::retained_bindings(&db, true).unwrap().is_empty());
    assert!(incoming::configure_in(&mut db, false).unwrap().is_none());
    assert_unallocated(&db);
}

#[test]
fn startup_without_a_binding_does_not_require_unmaterialized_preferences() {
    let db = Model::from_value(
        imbl_value::json!({"public":{"serverInfo":{"network":{"host":{"bindings":{}}}}}}),
    );
    assert!(incoming::retained_bindings(&db, true).unwrap().is_empty());
}

#[test]
fn preference_flags_require_explicit_booleans() {
    for flag in ["--downloads", "--seeding"] {
        assert!(SetPreferencesParams::try_parse_from(["set-preferences", flag]).is_err());
        assert!(SetPreferencesParams::try_parse_from(["set-preferences", flag, "false"]).is_ok());
    }
}
