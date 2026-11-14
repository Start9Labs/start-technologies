use patch_db::json_ptr::ROOT;

use super::*;
use crate::account::AccountInfo;
use crate::db::model::Database;
use crate::net::forward::AvailablePorts;
use crate::net::host::Host;
use crate::net::host::binding::{BindInfo, BindOptions, ProxyOptions};
use crate::net::http_proxy::{HttpProxyGateway, HttpProxyStage};
use crate::util::serde::Der64;

fn stage(package: &str, profile: &str) -> HttpProxyStage {
    HttpProxyStage {
        package_id: package.parse().unwrap(),
        profile_id: profile.into(),
    }
}

fn host(ports: &mut AvailablePorts, port: u16) -> Host {
    let mut host = Host::new();
    let mut binding = BindInfo::new(
        ports,
        BindOptions {
            preferred_external_port: port,
            add_ssl: None,
            secure: None,
            proxy: Some(ProxyOptions::Http {
                add_x_forwarded_headers: true,
                auth: None,
            }),
        },
        true,
    )
    .unwrap();
    binding.proxy_chains = vec![
        HttpProxyChain {
            gateway: HttpProxyGateway::Network {
                gateway_id: "eth0".parse().unwrap(),
            },
            chain: vec![
                stage("waf", "edge"),
                stage("keeper", "first"),
                stage("waf", "strict"),
                stage("keeper", "last"),
            ],
            public_only: true,
        },
        HttpProxyChain {
            gateway: HttpProxyGateway::UrlPlugin {
                package_id: "tor".parse().unwrap(),
            },
            chain: vec![stage("waf", "edge")],
            public_only: false,
        },
        HttpProxyChain {
            gateway: HttpProxyGateway::Network {
                gateway_id: "retired".parse().unwrap(),
            },
            chain: vec![],
            public_only: true,
        },
    ];
    for id in ["waf", "keeper"] {
        binding.addresses.available.insert(HostnameInfo {
            ssl: false,
            public: false,
            hostname: format!("{id}.example").into(),
            port: binding.net.assigned_port,
            metadata: HostnameMetadata::Plugin {
                package_id: id.parse().unwrap(),
                remove_action: None,
                overflow_actions: vec![],
                info: Value::Null,
            },
        });
    }
    host.bindings.0.insert(port, binding);
    host
}

fn package(account: &AccountInfo, id: &str) -> PackageDataEntry {
    let manifest = serde_json::from_value(serde_json::json!({
        "id": id, "version": "1.0.0:0", "canMigrateTo": "*", "canMigrateFrom": "*",
        "title": id, "description": { "short": "Test service", "long": "Test service" },
        "releaseNotes": "", "license": "MIT", "upstreamRepo": "https://example.com",
        "images": {}, "volumes": [], "plugins": if id == "waf" || id == "keeper" { vec!["http-proxy-v0"] } else { vec![] }
    })).unwrap();
    PackageDataEntry {
        state_info: PackageState::Installed(InstalledState { manifest }),
        s9pk: format!("/tmp/{id}.s9pk").into(),
        status_info: Default::default(),
        registry: None,
        developer_key: Der64(account.server_identity_key.verifying_key()),
        last_backup: None,
        current_dependencies: Default::default(),
        actions: Default::default(),
        tasks: Default::default(),
        hosts: Default::default(),
        store_exposed_dependents: vec![],
        outbound_gateway: None,
        outbound_dns: None,
        plugin: Default::default(),
    }
}

#[tokio::test]
async fn removing_a_package_prunes_every_chain_in_the_same_database_transaction() {
    let account = AccountInfo::new("test", std::time::SystemTime::now(), None).unwrap();
    let mut data = Database::init(&account, false, None, None).unwrap();
    data.public.server_info.network.host = host(&mut data.private.available_ports, 80);
    for (id, ports) in [("app", [8080, 8081]), ("other", [9090, 9091])] {
        let mut entry = package(&account, id);
        for port in ports {
            let mut value = host(&mut data.private.available_ports, port);
            value.bindings.0.insert(
                port + 10,
                BindInfo::new(
                    &mut data.private.available_ports,
                    BindOptions {
                        preferred_external_port: port + 10,
                        add_ssl: None,
                        secure: None,
                        proxy: None,
                    },
                    false,
                )
                .unwrap(),
            );
            entry
                .hosts
                .0
                .insert(format!("host-{port}").parse().unwrap(), value);
        }
        data.public
            .package_data
            .0
            .insert(id.parse().unwrap(), entry);
    }
    data.public
        .package_data
        .0
        .insert("keeper".parse().unwrap(), package(&account, "keeper"));
    let mut removed = package(&account, "waf");
    let PackageState::Installed(installed) = removed.state_info else {
        unreachable!()
    };
    removed.state_info = PackageState::Removing(installed);
    data.public
        .package_data
        .0
        .insert("waf".parse().unwrap(), removed);
    let db: TypedPatchDb<Database> = TypedPatchDb::load_unchecked(PatchDb::in_memory());
    db.put(&ROOT, &data).await.unwrap();
    let provider = "waf".parse().unwrap();
    let before = db.peek().await;
    let result = db.mutate(|db| remove_package_data(db, &provider)).await;
    assert!(result.result.unwrap().is_some());
    let mut snapshot = db.peek().await;
    assert!(
        !snapshot
            .as_public()
            .as_package_data()
            .contains_key(&provider)
            .unwrap()
    );
    let mut bindings = 0;
    for host in all_hosts(&mut snapshot) {
        for (_, binding) in host.unwrap().as_bindings_mut().as_entries_mut().unwrap() {
            let binding = binding.de().unwrap();
            if binding.options.proxy.is_none() {
                assert!(binding.proxy_chains.is_empty());
                continue;
            }
            bindings += 1;
            assert_eq!(
                binding.proxy_chains[0].chain,
                [stage("keeper", "first"), stage("keeper", "last")]
            );
            assert!(binding.proxy_chains[0].public_only);
            assert!(binding.proxy_chains[1].chain.is_empty());
            assert!(!binding.proxy_chains[1].public_only);
            assert!(binding.proxy_chains[2].chain.is_empty());
            assert!(binding.proxy_chains[2].public_only);
            assert_eq!(binding.addresses.available.len(), 1);
            assert_eq!(
                &*binding.addresses.available.first().unwrap().hostname,
                "keeper.example"
            );
        }
    }
    assert_eq!(bindings, 5);
    let after = db.dump(&ROOT).await.value;
    assert!(
        db.mutate(|db| remove_package_data(db, &provider))
            .await
            .result
            .unwrap()
            .is_none()
    );
    assert_eq!(db.dump(&ROOT).await.value, after);
    assert!(
        before
            .as_public()
            .as_package_data()
            .contains_key(&provider)
            .unwrap()
    );
    let keeper: PackageId = "keeper".parse().unwrap();
    db.mutate(|db| {
        let entry = db
            .as_public_mut()
            .as_package_data_mut()
            .as_idx_mut(&keeper)
            .unwrap();
        let PackageState::Installed(installed) = entry.as_state_info().de()? else {
            unreachable!()
        };
        entry
            .as_state_info_mut()
            .ser(&PackageState::Removing(installed))?;
        remove_package_data(db, &keeper)
    })
    .await
    .result
    .unwrap();
    let mut final_state = db.peek().await;
    for host in all_hosts(&mut final_state) {
        for (_, binding) in host.unwrap().as_bindings_mut().as_entries_mut().unwrap() {
            let binding = binding.de().unwrap();
            assert!(!binding.has_proxy_chains());
            if binding.options.proxy.is_some() {
                assert_eq!(binding.proxy_chains.len(), 3);
                assert!(binding.proxy_chains[0].public_only);
                assert!(binding.proxy_chains[2].public_only);
                assert!(binding.addresses.available.is_empty());
            }
        }
    }
}
