use std::path::Path;

use super::RegistryMigration;
use crate::prelude::*;
use crate::registry::package::icon::{icons_dir, store_icon};
use crate::util::DataUrl;

pub struct IconStore;
impl RegistryMigration for IconStore {
    fn action(&self, db: &mut Value, datadir: &Path) -> Result<(), Error> {
        let dir = icons_dir(datadir);
        let Some(packages) = db["index"]["package"]["packages"].as_object_mut() else {
            return Ok(());
        };
        for (_, package) in packages.iter_mut() {
            let Some(versions) = package["versions"].as_object_mut() else {
                continue;
            };
            for (_, info) in versions.iter_mut() {
                store_inline_icon(&dir, &mut info["icon"])?;
                if let Some(deps) = info["dependencyMetadata"].as_object_mut() {
                    for (_, dep) in deps.iter_mut() {
                        store_inline_icon(&dir, &mut dep["icon"])?;
                    }
                }
            }
        }
        Ok(())
    }
}

fn store_inline_icon(dir: &Path, icon: &mut Value) -> Result<(), Error> {
    let Some(data_url) = icon.as_str().filter(|s| s.starts_with("data:")) else {
        return Ok(());
    };
    let data_url = data_url
        .parse::<DataUrl<'static>>()
        .with_kind(ErrorKind::ParseUrl)?;
    *icon = to_value(&store_icon(dir, &data_url)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use imbl_value::json;

    use super::*;

    #[test]
    fn moves_package_and_dependency_icons_into_the_store() {
        let datadir = tempfile::tempdir().unwrap();
        let icon = "data:image/png;base64,aWNvbg==";
        let mut db = json!({ "index": { "package": { "packages": {
            "a": { "versions": {
                "1.0.0:0": { "icon": icon, "dependencyMetadata": { "b": { "icon": icon } } },
                "2.0.0:0": { "icon": icon, "dependencyMetadata": { "b": { "icon": null } } },
            } },
        } } } });

        IconStore.action(&mut db, datadir.path()).unwrap();

        let versions = &db["index"]["package"]["packages"]["a"]["versions"];
        let stored = &versions["1.0.0:0"]["icon"];
        let path = stored.as_str().unwrap().to_owned();
        assert!(path.starts_with("blake3/") && path.ends_with(".png"));
        assert_eq!(
            &versions["1.0.0:0"]["dependencyMetadata"]["b"]["icon"],
            stored
        );
        assert_eq!(&versions["2.0.0:0"]["icon"], stored);
        assert!(versions["2.0.0:0"]["dependencyMetadata"]["b"]["icon"].is_null());
        let stored = std::fs::read_dir(icons_dir(datadir.path()).join("blake3")).unwrap();
        assert_eq!(stored.count(), 1);
        assert_eq!(
            std::fs::read(icons_dir(datadir.path()).join(&path)).unwrap(),
            b"icon"
        );

        let before = db.clone();
        IconStore.action(&mut db, datadir.path()).unwrap();
        assert_eq!(db, before);
    }
}
