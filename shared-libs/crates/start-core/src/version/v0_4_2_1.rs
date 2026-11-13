use exver::VersionRange;
use url::Url;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_2};
use crate::prelude::*;
use crate::trusted_keys::{
    Registries, ensure_builtin_registries, initial_os_registry, ui_registries,
};

lazy_static::lazy_static! {
    static ref V0_4_2_1: exver::Version = exver::Version::new([0, 4, 2, 1], []);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

/// Moves registry intent to `serverInfo`, preserving removals after the revision-1 repair.
fn up(db: &mut Value) -> Result<(), Error> {
    let repair_missing_builtins = super::applied_migration_revision(&mut db.clone())? == 0;
    let Some(public) = db.get_mut("public").and_then(Value::as_object_mut) else {
        return Ok(());
    };
    let legacy_registries = public.get("ui").map(ui_registries);
    let legacy_os_registry = public
        .get("ui")
        .and_then(|ui| ui.get("startosRegistry"))
        .and_then(Value::as_str)
        .and_then(|url| url.parse::<Url>().ok());
    if let Some(ui) = public.get_mut("ui").and_then(Value::as_object_mut) {
        ui.remove("registries");
        ui.remove("startosRegistry");
    }
    let Some(server_info) = public.get_mut("serverInfo").and_then(Value::as_object_mut) else {
        return Ok(());
    };
    let mut registries: Registries = match server_info.get("registries") {
        Some(saved) if !saved.is_null() => from_value(saved.clone())?,
        _ => legacy_registries.unwrap_or_default(),
    };
    if repair_missing_builtins {
        ensure_builtin_registries(&mut registries);
    }
    server_info.insert("registries".into(), to_value(&registries)?);
    if matches!(server_info.get("osRegistry"), None | Some(Value::Null)) {
        let os_registry = legacy_os_registry.unwrap_or_else(initial_os_registry);
        server_info.insert("osRegistry".into(), to_value(&os_registry)?);
    }
    Ok(())
}

fn down(db: &mut Value) {
    let Some(public) = db.get_mut("public").and_then(Value::as_object_mut) else {
        return;
    };
    let Some(server_info) = public.get_mut("serverInfo").and_then(Value::as_object_mut) else {
        return;
    };
    let registries = server_info.remove("registries");
    let os_registry = server_info.remove("osRegistry");
    if let Some(ui) = public.get_mut("ui").and_then(Value::as_object_mut) {
        if let Some(registries) = registries {
            ui.insert("registries".into(), registries);
        }
        if let Some(os_registry) = os_registry {
            ui.insert("startosRegistry".into(), os_registry);
        }
    }
}

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
        2
    }
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        up(db)?;
        Ok(Value::Null)
    }
    fn down(self, db: &mut Value) -> Result<(), Error> {
        down(db);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUILTINS: [&str; 2] = [
        "https://registry.start9.com/",
        "https://community-registry.start9.com/",
    ];

    fn db_with_ui(ui: Value) -> Value {
        imbl_value::json!({ "public": { "serverInfo": {}, "ui": ui } })
    }

    fn upgraded(ui: Value) -> Value {
        let mut db = db_with_ui(ui);
        Version.up(&mut db, ()).unwrap();
        db
    }

    #[test]
    fn a_0_3_5_server_gets_the_builtin_registries() {
        let mut db = db_with_ui(imbl_value::json!({
            "marketplace": { "known-hosts": {
                "https://registry.start9.com/": {},
                "https://community-registry.start9.com/": {}
            } }
        }));
        super::super::v0_4_0_alpha_1::Version
            .up(&mut db, ())
            .unwrap();
        Version.up(&mut db, ()).unwrap();
        assert_eq!(
            db["public"]["serverInfo"]["registries"],
            imbl_value::json!({
                "https://registry.start9.com/": "Start9 Registry",
                "https://community-registry.start9.com/": "Community Registry"
            })
        );
        assert_eq!(
            db["public"]["serverInfo"]["osRegistry"],
            imbl_value::json!("https://registry.start9.com/")
        );
    }

    #[test]
    fn registries_and_the_os_registry_leave_the_ui_section() {
        let db = upgraded(imbl_value::json!({
            "registries": { "https://registry.example/": "Custom" },
            "startosRegistry": "https://registry.example/",
            "snakeHighScore": 42
        }));
        assert_eq!(
            db["public"]["ui"],
            imbl_value::json!({ "snakeHighScore": 42 })
        );
        let registries = &db["public"]["serverInfo"]["registries"];
        assert_eq!(
            registries["https://registry.example/"],
            imbl_value::json!("Custom")
        );
        assert_eq!(registries.as_object().unwrap().len(), 3);
        assert_eq!(
            db["public"]["serverInfo"]["osRegistry"],
            imbl_value::json!("https://registry.example/")
        );
    }

    #[test]
    fn missing_or_null_registries_get_the_builtins() {
        for ui in [
            imbl_value::json!({}),
            imbl_value::json!({ "registries": null }),
        ] {
            let db = upgraded(ui);
            let registries = db["public"]["serverInfo"]["registries"]
                .as_object()
                .unwrap();
            assert!(BUILTINS.iter().all(|url| registries.contains_key(*url)));
            assert_eq!(
                db["public"]["serverInfo"]["osRegistry"],
                to_value(&initial_os_registry()).unwrap()
            );
        }
    }

    #[test]
    fn a_missing_builtin_is_restored_beside_saved_registries() {
        assert_eq!(
            upgraded(imbl_value::json!({ "registries": {
                "https://registry.start9.com": "Mine",
                "https://registry.example/": "Custom"
            } }))["public"]["serverInfo"]["registries"],
            imbl_value::json!({
                "https://registry.start9.com": "Mine",
                "https://registry.example/": "Custom",
                "https://community-registry.start9.com/": "Community Registry"
            })
        );
    }

    #[test]
    fn a_db_without_server_info_is_left_unchanged() {
        let mut db = imbl_value::json!({ "public": {} });
        let before = db.clone();
        Version.up(&mut db, ()).unwrap();
        assert_eq!(db, before);
    }

    #[test]
    fn revision_2_after_revision_1_is_idempotent() {
        let mut revision_1 = db_with_ui(imbl_value::json!({
            "registries": { "https://registry.example/": null },
            "startosRegistry": "https://registry.start9.com/"
        }));
        ensure_builtin_registries_in_ui(&mut revision_1);
        let mut db = revision_1.clone();
        Version.up(&mut db, ()).unwrap();
        let first = db.clone();
        Version.up(&mut db, ()).unwrap();
        assert_eq!(db, first);
        assert_eq!(
            first["public"]["serverInfo"]["registries"]["https://registry.example/"],
            Value::Null
        );
    }

    #[test]
    fn rolling_back_restores_the_ui_section() {
        let before = db_with_ui(imbl_value::json!({
            "registries": {
                "https://registry.start9.com/": "Start9 Registry",
                "https://community-registry.start9.com/": "Community Registry"
            },
            "startosRegistry": "https://registry.start9.com/"
        }));
        let mut db = before.clone();
        Version.up(&mut db, ()).unwrap();
        Version.down(&mut db).unwrap();
        assert_eq!(db, before);
    }

    fn ensure_builtin_registries_in_ui(db: &mut Value) {
        let mut registries = ui_registries(&db["public"]["ui"]);
        ensure_builtin_registries(&mut registries);
        db["public"]["ui"]["registries"] = to_value(&registries).unwrap();
    }
}
