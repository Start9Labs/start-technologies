use std::path::Path;

use super::RegistryMigration;
use crate::prelude::*;

/// Removes package-level access. A package that was private loses its versions.
pub struct S9pkAccess;
impl RegistryMigration for S9pkAccess {
    fn action(&self, db: &mut Value, _: &Path) -> Result<(), Error> {
        let Some(packages) = db["index"]["package"]["packages"].as_object_mut() else {
            return Ok(());
        };
        for (_, package) in packages.iter_mut() {
            let Some(package) = package.as_object_mut() else {
                continue;
            };
            let private = package.remove("private").and_then(|p| p.as_bool()) == Some(true);
            package.remove("readers");
            if private {
                package.insert("versions".into(), Value::Object(Default::default()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use imbl_value::json;

    use super::*;

    #[test]
    fn drops_package_access_and_the_versions_of_private_packages() {
        let version = json!({ "1.0.0:0": { "releaseNotes": "notes" } });
        let mut db = json!({ "index": { "package": { "packages": {
            "public": { "authorized": {}, "versions": version, "private": false, "readers": ["a"] },
            "legacy": { "authorized": {}, "versions": version },
            "secret": { "authorized": {}, "versions": version, "private": true, "readers": [] },
        } } } });

        S9pkAccess.action(&mut db, Path::new("")).unwrap();

        let packages = &db["index"]["package"]["packages"];
        for id in ["public", "legacy"] {
            assert_eq!(
                packages[id],
                json!({ "authorized": {}, "versions": version })
            );
        }
        assert_eq!(
            packages["secret"],
            json!({ "authorized": {}, "versions": {} })
        );

        let before = db.clone();
        S9pkAccess.action(&mut db, Path::new("")).unwrap();
        assert_eq!(db, before);
    }
}
