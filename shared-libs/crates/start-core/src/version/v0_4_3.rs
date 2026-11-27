use std::collections::{BTreeSet, HashMap, HashSet};

use exver::VersionRange;
use imbl_value::json;
use tokio::process::Command;
use zbus::Connection;
use zbus::zvariant::OwnedValue;

use super::v0_3_5::V0_3_0_COMPAT;
use super::v0_4_0_2::for_each_host;
use super::{VersionT, v0_4_2_2};
use crate::context::RpcContext;
use crate::net::gateway::active_connection::ActiveConnectionProxy;
use crate::net::gateway::{
    ConnectionSettingsProxy, NetworkManagerProxy, NmSettingsProxy, WIREGUARD_DEFAULT_MTU,
    settings_uuid, settings_wireguard_mtu,
};
use crate::net::host::binding::{DerivedAddressInfo, set_nonssl_wan_group};
use crate::net::service_interface::HostnameMetadata;
use crate::prelude::*;
use crate::util::Invoke;

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
        4
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        move_ssh_pubkeys(db, "private", "public")?;
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
        suffix_duplicate_shared_volume_names(db);
        Ok(Value::Null)
    }
    async fn post_up(self, _ctx: &RpcContext, _input: Value) -> Result<(), Error> {
        if let Err(error) = pin_automatic_wireguard_mtu().await {
            tracing::warn!(?error, "{}", t!("migration.wireguard-mtu-failed"));
        }
        Ok(())
    }
    fn down(self, db: &mut Value) -> Result<(), Error> {
        move_ssh_pubkeys(db, "public", "private")
    }
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

/// The uuid of a WireGuard profile on NetworkManager's automatic MTU.
fn automatic_wireguard_mtu_uuid(
    settings: &HashMap<String, HashMap<String, OwnedValue>>,
) -> Result<Option<String>, Error> {
    let is_wireguard = settings
        .get("connection")
        .and_then(|c| c.get("type"))
        .and_then(|t| t.downcast_ref::<zbus::zvariant::Str>().ok())
        .is_some_and(|t| t.as_str() == "wireguard");
    if !is_wireguard || settings_wireguard_mtu(settings)?.is_some() {
        return Ok(None);
    }
    Ok(settings_uuid(settings))
}

/// Sets every saved WireGuard profile on the automatic MTU to `WIREGUARD_DEFAULT_MTU`.
/// Active profiles reconnect. Returns the first failure after attempting every profile.
async fn pin_automatic_wireguard_mtu() -> Result<(), Error> {
    let connection = Connection::system().await?;
    let mut active = HashSet::new();
    for path in NetworkManagerProxy::new(&connection)
        .await?
        .active_connections()
        .await?
    {
        active.insert(
            ActiveConnectionProxy::new(&connection, path)
                .await?
                .connection()
                .await?,
        );
    }
    let mut result = Ok(());
    for path in NmSettingsProxy::new(&connection)
        .await?
        .list_connections()
        .await?
    {
        let pinned = async {
            let settings = ConnectionSettingsProxy::new(&connection, path.clone())
                .await?
                .get_settings()
                .await?;
            let Some(uuid) = automatic_wireguard_mtu_uuid(&settings)? else {
                return Ok(());
            };
            Command::new("nmcli")
                .args(["connection", "modify", "uuid", &uuid, "wireguard.mtu"])
                .arg(WIREGUARD_DEFAULT_MTU.to_string())
                .invoke(ErrorKind::Network)
                .await?;
            if active.contains(&path) {
                Command::new("nmcli")
                    .args(["connection", "up", "uuid", &uuid])
                    .invoke(ErrorKind::Network)
                    .await?;
            }
            Ok::<_, Error>(())
        }
        .await;
        if result.is_ok() {
            result = pinned;
        }
    }
    result
}

/// Switches on the public IPs grouped with each enabled plain public domain of a range.
/// The domain's forward already exposes them; earlier versions left them off after a renumber.
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
    if let Ok(value) = to_value(&info) {
        *addresses = value;
    }
}

/// Gives every shared volume a valid folder name, suffixing repeats to keep names unique.
fn suffix_duplicate_shared_volume_names(db: &mut Value) {
    let Some(volumes) = db
        .get_mut("public")
        .and_then(|public| public.get_mut("sharedVolumes"))
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let mut taken: BTreeSet<String> = volumes
        .values()
        .filter_map(|volume| volume["name"].as_str().map(str::to_owned))
        .collect();
    let mut seen = BTreeSet::new();
    for (_, volume) in volumes.iter_mut() {
        let Some(name) = volume["name"].as_str().map(str::to_owned) else {
            continue;
        };
        let valid = crate::shared_volumes::folder_name(&name);
        if valid == name && seen.insert(name.clone()) {
            continue;
        }
        let unused = crate::shared_volumes::unused_name(&valid, |name| taken.contains(name));
        taken.insert(unused.clone());
        seen.insert(unused.clone());
        volume["name"] = json!(unused);
    }
}

#[cfg(test)]
mod tests {
    use patch_db::json_ptr::ROOT;

    use super::*;

    fn profile(kind: &str, mtu: Option<u32>) -> HashMap<String, HashMap<String, OwnedValue>> {
        let mut settings = HashMap::from([(
            "connection".to_string(),
            HashMap::from([
                (
                    "type".to_string(),
                    OwnedValue::try_from(zbus::zvariant::Value::from(kind)).unwrap(),
                ),
                (
                    "uuid".to_string(),
                    OwnedValue::try_from(zbus::zvariant::Value::from("the-uuid")).unwrap(),
                ),
            ]),
        )]);
        let mut wg = HashMap::new();
        if let Some(mtu) = mtu {
            wg.insert("mtu".to_string(), OwnedValue::from(mtu));
        }
        settings.insert("wireguard".to_string(), wg);
        settings
    }

    #[tokio::test]
    async fn an_existing_0_4_3_server_queues_the_mtu_migration_once() {
        let version = super::super::Version::from_exver_version(Version.semver())
            .as_version_t()
            .unwrap();
        for applied in [0, 1, 2] {
            let mut db = json!({ "public": { "serverInfo": {
                "version": "0.4.3",
                "packageVersionCompat": ">=0.3.0 <0.5.0",
                "postInitMigrationTodos": {},
                "latestMigrationRevision": applied,
            } } });
            for queued in [true, false] {
                db["public"]["serverInfo"]["postInitMigrationTodos"] = json!({});
                let revision = super::super::applied_migration_revision(&mut db).unwrap();
                super::super::Upgrade::load(&version, &version, revision)
                    .await
                    .unwrap()
                    .apply(&version, &version, &mut db)
                    .unwrap();
                assert_eq!(
                    db["public"]["serverInfo"]["postInitMigrationTodos"]
                        .as_object()
                        .unwrap()
                        .contains_key("0.4.3"),
                    queued
                );
            }
        }
    }

    #[test]
    fn automatic_wireguard_profiles_are_selected() {
        assert_eq!(
            automatic_wireguard_mtu_uuid(&profile("wireguard", None)).unwrap(),
            Some("the-uuid".to_string())
        );
        assert_eq!(
            automatic_wireguard_mtu_uuid(&profile("wireguard", Some(0))).unwrap(),
            Some("the-uuid".to_string())
        );
    }

    #[test]
    fn explicit_mtus_and_other_profiles_are_kept() {
        assert_eq!(
            automatic_wireguard_mtu_uuid(&profile("wireguard", Some(1420))).unwrap(),
            None
        );
        assert_eq!(
            automatic_wireguard_mtu_uuid(&profile("802-3-ethernet", None)).unwrap(),
            None
        );
    }

    #[test]
    fn ssh_keys_move_without_changing_metadata_and_round_trip() {
        let mut db = json!({
            "public": { "ui": { "snakeHighScore": 42 } },
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
        fixture["private"]["sshPubkeys"] = json!({});
        let db = PatchDb::in_memory();
        db.put(&ROOT, &fixture).await.unwrap();
        Version.pre_init(&db).await.unwrap();
        let migrated = db.dump(&ROOT).await.value;
        assert_eq!(migrated["public"]["sshPubkeys"], json!({}));
        assert!(migrated["private"].get("sshPubkeys").is_none());
        assert_eq!(
            migrated["public"]["serverInfo"]["latestMigrationRevision"],
            json!(Version.migration_revision())
        );
        Version.pre_init(&db).await.unwrap();
        assert_eq!(db.dump(&ROOT).await.value, migrated);
    }

    #[test]
    fn repeated_shared_volume_names_gain_unused_suffixes() {
        let volume = |name: &str| json!({"name": name, "source": {"kind": "local"}, "grants": {}});
        let mut db = json!({"public": {"sharedVolumes": {
            "A": volume("Photos"),
            "B": volume("Photos"),
            "C": volume("Photos (2)"),
            "D": volume("Music"),
            "E": volume("Photos"),
            "F": volume("media/photos"),
            "G": volume(".."),
        }}});
        suffix_duplicate_shared_volume_names(&mut db);
        let names: Vec<_> = db["public"]["sharedVolumes"]
            .as_object()
            .unwrap()
            .values()
            .map(|volume| volume["name"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            names,
            [
                "Photos",
                "Photos (3)",
                "Photos (2)",
                "Music",
                "Photos (4)",
                "media-photos",
                "Volume"
            ]
        );
        let migrated = db.clone();
        suffix_duplicate_shared_volume_names(&mut db);
        assert_eq!(db, migrated);
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
        Version.up(&mut db, ()).unwrap();
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
        assert_eq!(migrated(addresses.clone()), addresses);
    }

    #[test]
    fn a_range_whose_plain_domain_is_off_keeps_its_ip_off() {
        assert!(range_on(migrated(renumbered(true))).is_empty());
    }
}
