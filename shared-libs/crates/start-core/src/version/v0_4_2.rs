use exver::VersionRange;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_1};
use crate::prelude::*;
use crate::trusted_keys::{TrustedKeys, individual_keys};

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_1::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        exver::Version::new([0, 4, 2], [])
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
        let stored = server
            .get("trustedKeys")
            .cloned()
            .or_not_found("trustedKeys")?;
        let keys: TrustedKeys = from_value(stored)?;
        server.insert("trustedKeys".into(), to_value(&individual_keys(&keys)?)?);
        Ok(Value::Null)
    }
    fn down(self, _: &mut Value) -> Result<(), Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trusted_keys::{TrustContext, enabled_keys, initial_trusted_keys};

    #[test]
    fn trust_migration_preserves_both_policies_and_is_idempotent() {
        let keys = initial_trusted_keys();
        let mut db = imbl_value::json!({ "public": { "serverInfo": { "trustedKeys": keys } } });
        Version.up(&mut db, ()).unwrap();
        let migrated: TrustedKeys =
            from_value(db["public"]["serverInfo"]["trustedKeys"].clone()).unwrap();
        assert!(migrated.values().all(|key| key.public_keys.len() == 1));
        for context in [TrustContext::Marketplace, TrustContext::Os] {
            assert_eq!(
                enabled_keys(&migrated, context)
                    .into_iter()
                    .collect::<std::collections::BTreeSet<_>>(),
                enabled_keys(&keys, context)
                    .into_iter()
                    .collect::<std::collections::BTreeSet<_>>(),
            );
        }
        let first = db.clone();
        Version.up(&mut db, ()).unwrap();
        assert_eq!(db, first);
        Version.down(&mut db).unwrap();
        assert_eq!(db, first);
    }
}
