use exver::VersionRange;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_2};
use crate::prelude::*;
use crate::trusted_keys::ensure_builtin_registries;

lazy_static::lazy_static! {
    static ref V0_4_2_1: exver::Version = exver::Version::new([0, 4, 2, 1], []);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_2::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_2_1.clone()
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn migration_revision(self) -> usize {
        1
    }
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        ensure_builtin_registries(&mut db["public"]["ui"]);
        Ok(Value::Null)
    }
    fn down(self, _db: &mut Value) -> Result<(), Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upgraded(registries: Option<Value>) -> Value {
        let mut db = imbl_value::json!({ "public": { "ui": {} } });
        if let Some(registries) = registries {
            db["public"]["ui"]["registries"] = registries;
        }
        Version.up(&mut db, ()).unwrap();
        db["public"]["ui"]["registries"].clone()
    }

    #[test]
    fn a_0_3_5_server_gets_the_builtin_registries() {
        let mut db = imbl_value::json!({ "public": { "ui": {
            "marketplace": { "known-hosts": {
                "https://registry.start9.com/": {},
                "https://community-registry.start9.com/": {}
            } }
        } } });
        super::super::v0_4_0_alpha_1::Version
            .up(&mut db, ())
            .unwrap();
        assert_eq!(db["public"]["ui"]["registries"], imbl_value::json!({}));
        Version.up(&mut db, ()).unwrap();
        assert_eq!(
            db["public"]["ui"]["registries"],
            imbl_value::json!({
                "https://registry.start9.com/": "Start9 Registry",
                "https://community-registry.start9.com/": "Community Registry"
            })
        );
    }

    #[test]
    fn missing_or_null_registries_get_the_builtins() {
        for registries in [None, Some(Value::Null)] {
            assert_eq!(upgraded(registries).as_object().unwrap().len(), 2);
        }
    }

    #[test]
    fn a_missing_builtin_is_restored_beside_saved_registries() {
        assert_eq!(
            upgraded(Some(imbl_value::json!({
                "https://registry.start9.com": "Mine",
                "https://registry.example/": "Custom"
            }))),
            imbl_value::json!({
                "https://registry.start9.com": "Mine",
                "https://registry.example/": "Custom",
                "https://community-registry.start9.com/": "Community Registry"
            })
        );
    }

    #[test]
    fn restoring_is_idempotent() {
        let mut db = imbl_value::json!({ "public": { "ui": { "registries": {} } } });
        Version.up(&mut db, ()).unwrap();
        let first = db.clone();
        Version.up(&mut db, ()).unwrap();
        assert_eq!(db, first);
    }
}
