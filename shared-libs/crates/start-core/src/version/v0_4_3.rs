use std::collections::{HashMap, HashSet};

use exver::VersionRange;
use tokio::process::Command;
use zbus::Connection;
use zbus::zvariant::OwnedValue;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_2_2};
use crate::context::RpcContext;
use crate::net::gateway::active_connection::ActiveConnectionProxy;
use crate::net::gateway::{
    ConnectionSettingsProxy, NetworkManagerProxy, NmSettingsProxy, WIREGUARD_DEFAULT_MTU,
    settings_uuid, settings_wireguard_mtu,
};
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
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn migration_revision(self) -> usize {
        1
    }
    fn up(self, _db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        Ok(Value::Null)
    }
    async fn post_up(self, _ctx: &RpcContext, _input: Value) -> Result<(), Error> {
        if let Err(error) = pin_automatic_wireguard_mtu().await {
            tracing::warn!(?error, "{}", t!("migration.wireguard-mtu-failed"));
        }
        Ok(())
    }
    fn down(self, _db: &mut Value) -> Result<(), Error> {
        Ok(())
    }
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

#[cfg(test)]
mod tests {
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
        let mut db = imbl_value::json!({ "public": { "serverInfo": {
            "version": "0.4.3",
            "packageVersionCompat": ">=0.3.0 <0.5.0",
            "postInitMigrationTodos": {},
            "latestMigrationRevision": 0,
        } } });
        for queued in [true, false] {
            db["public"]["serverInfo"]["postInitMigrationTodos"] = imbl_value::json!({});
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
}
