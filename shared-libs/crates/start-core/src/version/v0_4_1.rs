use exver::VersionRange;
use imbl_value::json;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_0_3, v0_4_0_5};
use crate::prelude::*;

lazy_static::lazy_static! {
    static ref V0_4_1: exver::Version = exver::Version::new([0, 4, 1], []);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_0_5::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_1.clone()
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        let private = db["private"].as_object_mut().or_not_found("private")?;
        let pubkeys = private
            .remove("sessionPubkeys")
            .unwrap_or_else(|| json!({}));
        private.insert(
            "sessions".into(),
            json!({ "pubkeys": pubkeys, "tokens": {} }),
        );
        Ok(Value::Null)
    }
    fn down(self, db: &mut Value) -> Result<(), Error> {
        let private = db["private"].as_object_mut().or_not_found("private")?;
        let pubkeys = private
            .remove("sessions")
            .map(|sessions| sessions["pubkeys"].clone())
            .unwrap_or_else(|| json!({}));
        private.insert("sessionPubkeys".into(), pubkeys);
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use imbl_value::json;

    use super::*;
    use crate::update::signers::initial_signers;
    use crate::version::Upgrade;

    #[tokio::test]
    async fn upgrade_from_0_4_0_3_reruns_its_stale_migration() {
        let initial = to_value(&initial_signers()).unwrap();
        let mut custom = initial_signers();
        custom.pop_first();
        custom.values_mut().next().unwrap().label = "Owner-managed".to_owned();
        let custom = to_value(&custom).unwrap();
        for (revision, saved, expected) in [
            (2, None, initial),
            (3, Some(json!({})), json!({})),
            (3, Some(custom.clone()), custom),
        ] {
            let previous = v0_4_0_3::Version;
            let mut db = json!({
                "private": { "sessionPubkeys": {} },
                "public": { "serverInfo": {
                    "version": previous.semver(),
                    "packageVersionCompat": "*",
                    "latestMigrationRevision": revision,
                    "postInitMigrationTodos": {},
                } },
            });
            if let Some(saved) = saved {
                db["public"]["serverInfo"]["trustedOsUpdateSigners"] = saved;
            }
            Upgrade::load(&previous, &Version, revision)
                .await
                .unwrap()
                .apply(&previous, &Version, &mut db)
                .unwrap();
            assert_eq!(
                db["public"]["serverInfo"]["trustedOsUpdateSigners"],
                expected,
            );
            assert_eq!(db["public"]["serverInfo"]["version"], json!("0.4.1"));
        }
    }

    #[test]
    fn sessions_split_by_credential_and_roll_back() {
        let pubkeys = json!({ "-----BEGIN PUBLIC KEY-----\nkey\n-----END PUBLIC KEY-----\n": {
            "name": null,
            "loggedIn": "2026-01-01T00:00:00Z",
            "lastActive": "2026-01-02T00:00:00Z",
            "userAgent": "agent",
        } });
        let original =
            json!({ "private": { "password": "hash", "sessionPubkeys": pubkeys.clone() } });
        let mut db = original.clone();
        Version.up(&mut db, ()).unwrap();
        assert_eq!(
            db,
            json!({ "private": {
                "password": "hash",
                "sessions": { "pubkeys": pubkeys, "tokens": {} },
            } })
        );
        db["private"]["sessions"]["tokens"] = json!({ "hash": {} });
        Version.down(&mut db).unwrap();
        assert_eq!(db, original);
    }
}
