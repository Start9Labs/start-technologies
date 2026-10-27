use exver::VersionRange;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_0_5};
use crate::prelude::*;
use crate::update::trusted_keys::{migrate_signers, os_signers};

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_0_5::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        exver::Version::new([0, 4, 1], [])
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        let server = db
            .get_mut("public")
            .and_then(|public| public.get_mut("serverInfo"))
            .and_then(Value::as_object_mut)
            .or_not_found("serverInfo")?;
        if !server.contains_key("trustedKeys") {
            let legacy = server
                .get("trustedOsUpdateSigners")
                .cloned()
                .map(from_value)
                .transpose()?
                .unwrap_or_default();
            server.insert("trustedKeys".into(), to_value(&migrate_signers(legacy)?)?);
        }
        server.remove("trustedOsUpdateSigners");
        Ok(Value::Null)
    }
    fn down(self, db: &mut Value) -> Result<(), Error> {
        let server = db
            .get_mut("public")
            .and_then(|public| public.get_mut("serverInfo"))
            .and_then(Value::as_object_mut)
            .or_not_found("serverInfo")?;
        if let Some(keys) = server.remove("trustedKeys") {
            server.insert(
                "trustedOsUpdateSigners".into(),
                to_value(&os_signers(&from_value(keys)?))?,
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update::signers::initial_signers;

    #[test]
    fn malformed_legacy_keys_fail_without_changing_saved_trust() {
        let mut db = imbl_value::json!({"public": {"serverInfo": {"trustedOsUpdateSigners": {"not a public key": {"label": "custom"}}}}});
        let saved = db.clone();
        assert!(Version.up(&mut db, ()).is_err());
        assert_eq!(db, saved);
    }

    #[test]
    fn migration_is_idempotent_and_downgrade_preserves_os_policy() {
        let legacy = initial_signers();
        let mut db =
            imbl_value::json!({"public": {"serverInfo": {"trustedOsUpdateSigners": legacy}}});
        Version.up(&mut db, ()).unwrap();
        let migrated = db.clone();
        Version.up(&mut db, ()).unwrap();
        assert_eq!(db, migrated);
        assert!(
            db["public"]["serverInfo"]
                .get("trustedOsUpdateSigners")
                .is_none()
        );
        Version.down(&mut db).unwrap();
        let restored: crate::update::signers::TrustedOsUpdateSigners =
            from_value(db["public"]["serverInfo"]["trustedOsUpdateSigners"].clone()).unwrap();
        assert_eq!(
            restored.into_keys().collect::<Vec<_>>(),
            legacy.into_keys().collect::<Vec<_>>()
        );
    }
}
