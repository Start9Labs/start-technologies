use exver::VersionRange;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_0_2};
use crate::prelude::*;

lazy_static::lazy_static! {
    static ref V0_4_0_3: exver::Version = exver::Version::new([0, 4, 0, 3], []);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_0_2::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_0_3.clone()
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        drop_package_icons(db);
        Ok(Value::Null)
    }
    fn down(self, _db: &mut Value) -> Result<(), Error> {
        Ok(())
    }
}

fn drop_package_icons(db: &mut Value) {
    let Some(packages) = db
        .get_mut("public")
        .and_then(|p| p.get_mut("packageData"))
        .and_then(|p| p.as_object_mut())
    else {
        return;
    };
    for (_, package) in packages.iter_mut() {
        let Some(package) = package.as_object_mut() else {
            continue;
        };
        package.remove("icon");
        if let Some(deps) = package
            .get_mut("currentDependencies")
            .and_then(|d| d.as_object_mut())
        {
            for (_, dep) in deps.iter_mut() {
                if let Some(dep) = dep.as_object_mut() {
                    dep.remove("icon");
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use imbl_value::json;

    use super::*;

    #[test]
    fn up_drops_package_and_dependency_icons() {
        let mut db = json!({
            "public": { "packageData": { "pkg": {
                "icon": "data:image/png;base64,aWNvbg==",
                "currentDependencies": { "dep": {
                    "title": "Dep",
                    "icon": "data:image/png;base64,aWNvbg==",
                    "kind": "exists",
                    "versionRange": "*",
                } },
            } } }
        });
        Version.up(&mut db, ()).unwrap();
        assert_eq!(
            db,
            json!({
                "public": { "packageData": { "pkg": {
                    "currentDependencies": { "dep": {
                        "title": "Dep",
                        "kind": "exists",
                        "versionRange": "*",
                    } },
                } } }
            })
        );
    }
}
