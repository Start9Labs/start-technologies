use exver::VersionRange;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_1_3};
use crate::prelude::*;

lazy_static::lazy_static! {
    static ref V0_4_2: exver::Version = exver::Version::new([0, 4, 2], []);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_1_3::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_2.clone()
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn up(self, _db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        Ok(Value::Null)
    }
    fn down(self, _db: &mut Value) -> Result<(), Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use imbl_value::json;
    use patch_db::json_ptr::ROOT;

    use super::*;
    use crate::db::model::Database;
    use crate::trusted_keys::{initial_trusted_keys, legacy};
    use crate::version::{Current, rollback_to_unchecked, v0_4_1_1};

    #[tokio::test]
    async fn upgrade_preserves_nut_settings_sessions_and_trust() {
        let account =
            crate::account::AccountInfo::new("password", std::time::SystemTime::now(), None)
                .unwrap();
        let settings = json!({
            "mode": "client", "upsName": "ups", "host": "192.0.2.1", "port": 3493,
            "monitorUsername": "monitor", "monitorPassword": "test-password",
        });
        let mut trust = initial_trusted_keys();
        for key in trust.values_mut() {
            key.os = false;
        }
        for from in ["0.4.1", "0.4.1.1", "0.4.1.2", "0.4.1.3", "0.4.2", "0.4.2.1"] {
            for enabled in [false, true] {
                let mut fixture =
                    to_value(&Database::init(&account, false, None, None).unwrap()).unwrap();
                fixture["public"]["serverInfo"]["version"] = json!(from);
                fixture["public"]["serverInfo"]["postInitMigrationTodos"] = json!({});
                fixture["public"]["serverInfo"]["nut"] =
                    json!({ "enabled": enabled, "settings": settings.clone() });
                fixture["public"]["serverInfo"]["trustedKeys"] =
                    if matches!(from, "0.4.1" | "0.4.1.1") {
                        to_value(&legacy::grouped_keys(trust.clone())).unwrap()
                    } else {
                        to_value(&trust).unwrap()
                    };
                fixture["private"]["sessions"]["tokens"] =
                    json!({ "a".repeat(64): crate::auth::Session::default() });
                let saved_nut = fixture["public"]["serverInfo"]["nut"].clone();
                let saved_trust = to_value(&trust).unwrap();
                let saved_sessions = fixture["private"]["sessions"].clone();
                let db = PatchDb::in_memory();
                db.put(&ROOT, &fixture).await.unwrap();
                Current::default().pre_init(&db).await.unwrap();
                let mut migrated = db.dump(&ROOT).await.value;
                assert_eq!(
                    migrated["public"]["serverInfo"]["version"],
                    to_value(&Current::default().semver()).unwrap()
                );
                assert_eq!(migrated["public"]["serverInfo"]["nut"], saved_nut);
                assert_eq!(migrated["public"]["serverInfo"]["trustedKeys"], saved_trust);
                assert_eq!(migrated["private"]["sessions"], saved_sessions);
                Current::default().pre_init(&db).await.unwrap();
                assert_eq!(db.dump(&ROOT).await.value, migrated);
                rollback_to_unchecked(&Current::default(), &v0_4_1_1::Version, &mut migrated)
                    .unwrap();
                assert_eq!(
                    migrated["public"]["serverInfo"]["version"],
                    json!("0.4.1.1")
                );
                assert_eq!(migrated["public"]["serverInfo"]["nut"], saved_nut);
                assert_eq!(
                    legacy::decode_keys(
                        migrated["public"]["serverInfo"]["trustedKeys"].clone(),
                        chrono::Utc::now(),
                    )
                    .unwrap(),
                    trust
                );
                assert_eq!(migrated["private"]["sessions"], saved_sessions);
            }
        }
    }
}
