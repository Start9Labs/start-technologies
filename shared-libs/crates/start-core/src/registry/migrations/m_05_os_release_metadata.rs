use std::path::Path;

use super::RegistryMigration;
use crate::prelude::*;

pub struct OsReleaseMetadata;

impl RegistryMigration for OsReleaseMetadata {
    fn action(&self, db: &mut Value, _: &Path) -> Result<(), Error> {
        for (_, info) in db["index"]["os"]["versions"]
            .as_object_mut()
            .unwrap()
            .iter_mut()
        {
            *info = to_value(&from_value::<crate::registry::os::index::OsVersionInfo>(
                info.clone(),
            )?)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use imbl_value::json;

    use super::*;
    use crate::registry::os::index::OsVersionInfo;

    #[test]
    fn initializes_legacy_versions_and_preserves_release_metadata() {
        let mut legacy = to_value(&OsVersionInfo::default()).unwrap();
        legacy.as_object_mut().unwrap().remove("gitHash");
        legacy.as_object_mut().unwrap().remove("security");
        let mut published = legacy.clone();
        published["gitHash"] = "0123456789abcdef".into();
        published["security"] = true.into();
        let mut db = json!({ "index": { "os": { "versions": {
            "0.4.0:0": legacy,
            "0.4.0.1:0": published.clone(),
        } } } });
        OsReleaseMetadata.action(&mut db, Path::new(".")).unwrap();
        let versions = &db["index"]["os"]["versions"];
        assert_eq!(versions["0.4.0:0"]["security"].as_bool(), Some(false));
        assert!(versions["0.4.0:0"]["gitHash"].is_null());
        assert_eq!(versions["0.4.0.1:0"], published);
        let before = db.clone();
        OsReleaseMetadata.action(&mut db, Path::new(".")).unwrap();
        assert_eq!(db, before);
    }
}
