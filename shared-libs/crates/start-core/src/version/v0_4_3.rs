use exver::VersionRange;
use imbl_value::json;

use super::v0_3_5::V0_3_0_COMPAT;
use super::v0_4_0_2::for_each_host;
use super::{VersionT, v0_4_2_2};
use crate::net::host::binding::{DerivedAddressInfo, set_nonssl_wan_group};
use crate::net::service_interface::HostnameMetadata;
use crate::prelude::*;

lazy_static::lazy_static! {
    static ref V0_4_3: exver::Version = exver::Version::new([0, 4, 3], []);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_2_2::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_3.clone()
    }
    fn migration_revision(self) -> usize {
        2
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        move_ssh_pubkeys(db, "private", "public")?;
        let info = server_info(db)?;
        let seeding = info.remove("registrySeeding").map(from_value).transpose()?;
        let preferences: crate::registry::bittorrent::TorrentPreferences =
            match info.get("bittorrent") {
                Some(preferences) => from_value(preferences.clone())?,
                None => crate::registry::bittorrent::TorrentPreferences::default()
                    .update(seeding, seeding),
            };
        info.insert("bittorrent".into(), to_value(&preferences)?);
        for_each_host(db, |host| {
            let Some(ranges) = host.get_mut("bindingRanges").and_then(Value::as_object_mut) else {
                return;
            };
            for (_, range) in ranges.iter_mut() {
                if let Some(addresses) = range.get_mut("addresses") {
                    group_enabled_plain_domains(addresses);
                }
            }
        });
        Ok(Value::Null)
    }
    fn down(self, db: &mut Value) -> Result<(), Error> {
        let info = server_info(db)?;
        info.remove("bittorrent");
        info.remove("registrySeeding");
        move_ssh_pubkeys(db, "public", "private")
    }
}

fn server_info(
    db: &mut Value,
) -> Result<&mut imbl_value::InOMap<imbl_value::InternedString, Value>, Error> {
    db.get_mut("public")
        .and_then(|public| public.get_mut("serverInfo"))
        .and_then(Value::as_object_mut)
        .or_not_found("serverInfo")
}

fn move_ssh_pubkeys(db: &mut Value, from: &str, to: &str) -> Result<(), Error> {
    let keys = db
        .get_mut(from)
        .and_then(Value::as_object_mut)
        .and_then(|source| source.remove("sshPubkeys"));
    let destination = db
        .as_object_mut()
        .or_not_found("database")?
        .entry(to.into())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .or_not_found("SSH key destination")?;
    destination
        .entry("sshPubkeys".into())
        .or_insert_with(|| keys.unwrap_or_else(|| json!({})));
    Ok(())
}

/// Restores public IPs already exposed by enabled plain public domains of ranges.
fn group_enabled_plain_domains(addresses: &mut Value) {
    let Ok(mut info) = from_value::<DerivedAddressInfo>(addresses.clone()) else {
        return;
    };
    let groups: Vec<_> = info
        .enabled()
        .into_iter()
        .filter(|a| !a.ssl)
        .filter_map(|a| match (&a.metadata, a.port) {
            (HostnameMetadata::PublicDomain { gateway }, Some(port)) => {
                Some((gateway.clone(), port))
            }
            _ => None,
        })
        .collect();
    if groups.is_empty() {
        return;
    }
    for (gateway, port) in groups {
        set_nonssl_wan_group(&mut info, &gateway, port, true);
    }
    info.refresh_effective();
    if let Ok(value) = to_value(&info) {
        *addresses = value;
    }
}

#[cfg(test)]
mod tests {
    use patch_db::json_ptr::ROOT;

    use super::*;

    #[test]
    fn migration_preserves_previous_sharing_intent() {
        for enabled in [false, true] {
            let mut db =
                imbl_value::json!({"public": {"serverInfo": {"registrySeeding": enabled}}});
            Version.up(&mut db, ()).unwrap();
            assert_eq!(
                db["public"]["serverInfo"]["bittorrent"],
                imbl_value::json!({"downloads": enabled, "seeding": enabled})
            );
            assert!(db["public"]["serverInfo"].get("registrySeeding").is_none());
        }
    }

    #[test]
    fn rerunning_migration_preserves_current_preferences() {
        for preferences in [
            imbl_value::json!({"downloads": false, "seeding": false}),
            imbl_value::json!({"downloads": true, "seeding": false}),
            imbl_value::json!({"downloads": true, "seeding": true}),
        ] {
            let mut db = imbl_value::json!({"public": {
                "sshPubkeys": {},
                "serverInfo": {"bittorrent": preferences}
            }});
            let original = db.clone();
            Version.up(&mut db, ()).unwrap();
            assert_eq!(db, original);
        }
    }

    #[test]
    fn obsolete_incoming_switch_is_not_retained_as_an_ingress_opt_in() {
        let mut db = imbl_value::json!({"public": {"serverInfo": {
            "bittorrent": {"downloads": true, "seeding": true, "incoming": true}
        }}});
        Version.up(&mut db, ()).unwrap();
        assert_eq!(
            db["public"]["serverInfo"]["bittorrent"],
            imbl_value::json!({"downloads": true, "seeding": true})
        );
    }

    #[test]
    fn downloads_start_disabled_and_downgrade_removes_the_preferences() {
        let mut db = imbl_value::json!({
            "public": {"serverInfo": {"hostname": "server"}},
            "private": {"sshPubkeys": {}}
        });
        let original = db.clone();
        Version.up(&mut db, ()).unwrap();
        assert_eq!(
            db["public"]["serverInfo"]["bittorrent"],
            imbl_value::json!({"downloads": false, "seeding": false})
        );
        Version.down(&mut db).unwrap();
        assert_eq!(db, original);
    }

    #[test]
    fn ssh_keys_move_without_changing_metadata_and_round_trip() {
        let mut db = json!({
            "public": { "serverInfo": {}, "ui": { "snakeHighScore": 42 } },
            "private": {
                "sshPrivkey": "private-key",
                "sshPubkeys": { "fingerprint": {
                    "created_at": "2020-01-01T00:00:00Z",
                    "updated_at": "2021-01-01T00:00:00Z",
                    "value": "public-key"
                } }
            }
        });
        let original = db.clone();
        Version.up(&mut db, ()).unwrap();
        assert_eq!(
            db["public"]["sshPubkeys"],
            original["private"]["sshPubkeys"]
        );
        assert!(db["private"].get("sshPubkeys").is_none());
        assert_eq!(db["private"]["sshPrivkey"], json!("private-key"));
        assert_eq!(db["public"]["ui"], original["public"]["ui"]);
        let migrated = db.clone();
        Version.up(&mut db, ()).unwrap();
        assert_eq!(db, migrated);
        Version.down(&mut db).unwrap();
        assert_eq!(db, original);
    }

    #[tokio::test]
    async fn same_version_upgrade_moves_legacy_keys_before_database_decode() {
        use crate::db::model::Database;

        let account =
            crate::account::AccountInfo::new("password", std::time::SystemTime::now(), None)
                .unwrap();
        let mut fixture = to_value(&Database::init(&account, false, None, None).unwrap()).unwrap();
        fixture["public"]
            .as_object_mut()
            .unwrap()
            .remove("sshPubkeys");
        fixture["public"]["serverInfo"]["latestMigrationRevision"] = json!(1);
        fixture["public"]["serverInfo"]["bittorrent"] =
            json!({"downloads": true, "seeding": false});
        fixture["private"]["sshPubkeys"] = json!({});
        let db = PatchDb::in_memory();
        db.put(&ROOT, &fixture).await.unwrap();
        Version.pre_init(&db).await.unwrap();
        let migrated = db.dump(&ROOT).await.value;
        assert_eq!(migrated["public"]["sshPubkeys"], json!({}));
        assert!(migrated["private"].get("sshPubkeys").is_none());
        assert_eq!(
            migrated["public"]["serverInfo"]["bittorrent"],
            fixture["public"]["serverInfo"]["bittorrent"]
        );
        assert_eq!(
            migrated["public"]["serverInfo"]["latestMigrationRevision"],
            json!(Version.migration_revision())
        );
        Version.pre_init(&db).await.unwrap();
        assert_eq!(db.dump(&ROOT).await.value, migrated);
    }

    fn row(hostname: &str, public: bool, metadata: Value) -> Value {
        json!({
            "ssl": false,
            "public": public,
            "hostname": hostname,
            "port": 40000,
            "metadata": metadata,
        })
    }

    fn migrated(addresses: Value) -> Value {
        let mut db = json!({
            "public": {
                "serverInfo": { "network": { "host": { "bindings": {} } } },
                "packageData": { "service": { "hosts": { "p2p": {
                    "bindings": {},
                    "bindingRanges": { "28332": { "addresses": addresses } }
                } } } }
            }
        });
        db["public"]["serverInfo"]["bittorrent"] = json!({"downloads": true, "seeding": false});
        Version.up(&mut db, ()).unwrap();
        assert_eq!(
            db["public"]["serverInfo"]["bittorrent"],
            json!({"downloads": true, "seeding": false})
        );
        db["public"]["packageData"]["service"]["hosts"]["p2p"]["bindingRanges"]["28332"]
            ["addresses"]
            .clone()
    }

    fn range_on(addresses: Value) -> Vec<String> {
        let info: DerivedAddressInfo = from_value(addresses).unwrap();
        info.enabled()
            .into_iter()
            .filter(|a| a.public)
            .map(|a| match a.to_socket_addr() {
                Some(sa) => sa.to_string(),
                None => a.hostname.to_string(),
            })
            .collect()
    }

    fn renumbered(domain_disabled: bool) -> Value {
        let disabled = if domain_disabled {
            json!([["smp.example.com", 40000]])
        } else {
            json!([])
        };
        json!({
            "enabled": ["198.51.100.2:40000"],
            "disabled": disabled,
            "available": [
                row("203.0.113.9", true, json!({ "kind": "ipv4", "gateway": "wg-in" })),
                row(
                    "smp.example.com",
                    true,
                    json!({ "kind": "public-domain", "gateway": "wg-in" }),
                ),
            ],
        })
    }

    #[test]
    fn a_renumbered_range_keeps_its_enabled_plain_domain_and_turns_its_ip_on() {
        let addresses = migrated(renumbered(false));
        assert_eq!(
            range_on(addresses.clone()),
            ["203.0.113.9:40000", "smp.example.com"]
        );
        let info: DerivedAddressInfo = from_value(addresses.clone()).unwrap();
        assert_eq!(
            info.effective,
            info.enabled().into_iter().cloned().collect()
        );
        assert_eq!(migrated(addresses.clone()), addresses);
    }

    #[test]
    fn a_range_whose_plain_domain_is_off_keeps_its_ip_off() {
        assert!(range_on(migrated(renumbered(true))).is_empty());
    }
}
