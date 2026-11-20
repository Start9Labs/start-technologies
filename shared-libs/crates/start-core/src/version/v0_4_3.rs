use exver::VersionRange;
use imbl_value::json;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_2_2};
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
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn migration_revision(self) -> usize {
        1
    }
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        move_ssh_pubkeys(db, "private", "public")?;
        Ok(Value::Null)
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

#[cfg(test)]
mod tests {
    use patch_db::json_ptr::ROOT;

    use super::*;

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
        assert_eq!(db["public"]["sshPubkeys"], original["private"]["sshPubkeys"]);
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

        let account = crate::account::AccountInfo::new(
            "password",
            std::time::SystemTime::now(),
            None,
        )
        .unwrap();
        let mut fixture = to_value(&Database::init(&account, false, None, None).unwrap()).unwrap();
        fixture["public"].as_object_mut().unwrap().remove("sshPubkeys");
        fixture["public"]["serverInfo"]["latestMigrationRevision"] = json!(0);
        fixture["private"]["sshPubkeys"] = json!({});
        let db = PatchDb::in_memory();
        db.put(&ROOT, &fixture).await.unwrap();
        Version.pre_init(&db).await.unwrap();
        let migrated = db.dump(&ROOT).await.value;
        assert_eq!(migrated["public"]["sshPubkeys"], json!({}));
        assert!(migrated["private"].get("sshPubkeys").is_none());
        assert_eq!(migrated["public"]["serverInfo"]["latestMigrationRevision"], json!(1));
        Version.pre_init(&db).await.unwrap();
        assert_eq!(db.dump(&ROOT).await.value, migrated);
    }
}
