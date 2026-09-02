use exver::VersionRange;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_0_1};
use crate::hostname::repair_hostname;
use crate::prelude::*;

lazy_static::lazy_static! {
    static ref V0_4_0_2: exver::Version = exver::Version::new([0, 4, 0, 2], []);
}

const UI_PORT: u64 = 80;

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_0_1::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_0_2.clone()
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn migration_revision(self) -> usize {
        1
    }
    #[instrument(skip_all)]
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        rehome_admin_ui_port(db);
        drop_server_name(db);
        for_each_alpn(db, |alpn| {
            if alpn.as_array().is_some() {
                return;
            }
            *alpn = alpn
                .as_object()
                .and_then(|o| o.get("specified"))
                .cloned()
                // `reflect` said what an absent list says.
                .unwrap_or(Value::Null);
        });
        repair_unusable_hostname(db);
        Ok(Value::Null)
    }
    fn down(self, db: &mut Value) -> Result<(), Error> {
        restore_server_name(db);
        // Every earlier version wants 80 here and keeps the port it finds.
        for_each_alpn(db, |alpn| {
            if let Some(list) = alpn.as_array() {
                let mut wrapped = imbl_value::InOMap::new();
                wrapped.insert("specified".into(), Value::Array(list.clone()));
                *alpn = Value::Object(wrapped);
            }
        });
        Ok(())
    }
}

fn for_each_alpn(db: &mut Value, mut f: impl FnMut(&mut Value)) {
    let mut visit = |host: &mut Value| {
        let Some(bindings) = host.get_mut("bindings").and_then(|b| b.as_object_mut()) else {
            return;
        };
        for (_, binding) in bindings.iter_mut() {
            let Some(alpn) = binding
                .get_mut("options")
                .and_then(|o| o.get_mut("addSsl"))
                .and_then(|s| s.get_mut("alpn"))
                .filter(|a| !a.is_null())
            else {
                continue;
            };
            f(alpn);
        }
    };
    if let Some(host) = db
        .get_mut("public")
        .and_then(|p| p.get_mut("serverInfo"))
        .and_then(|s| s.get_mut("network"))
        .and_then(|n| n.get_mut("host"))
    {
        visit(host);
    }
    if let Some(packages) = db
        .get_mut("public")
        .and_then(|p| p.get_mut("packageData"))
        .and_then(|p| p.as_object_mut())
    {
        for (_, package) in packages.iter_mut() {
            if let Some(hosts) = package.get_mut("hosts").and_then(|h| h.as_object_mut()) {
                for (_, host) in hosts.iter_mut() {
                    visit(host);
                }
            }
        }
    }
}

fn server_info_mut(db: &mut Value) -> Option<&mut imbl_value::InOMap<InternedString, Value>> {
    db.get_mut("public")
        .and_then(|p| p.get_mut("serverInfo"))
        .and_then(|s| s.as_object_mut())
}

fn repair_unusable_hostname(db: &mut Value) {
    let Some(server_info) = server_info_mut(db) else {
        return;
    };
    let Some(stored) = server_info.get("hostname").and_then(|h| h.as_str()) else {
        return;
    };
    let repaired = repair_hostname(stored);
    if repaired.as_ref() == stored {
        return;
    }
    let repaired = repaired.to_string();
    server_info.insert(InternedString::intern("hostname"), Value::from(repaired));
}

fn drop_server_name(db: &mut Value) {
    if let Some(server_info) = server_info_mut(db) {
        server_info.remove(&InternedString::intern("name"));
    }
}

fn restore_server_name(db: &mut Value) {
    let Some(server_info) = server_info_mut(db) else {
        return;
    };
    if server_info.contains_key(&InternedString::intern("name")) {
        return;
    }
    let name = server_info
        .get("hostname")
        .and_then(Value::as_str)
        .map_or_else(|| "StartOS".to_owned(), title_case);
    server_info.insert(InternedString::intern("name"), Value::from(name));
}

fn title_case(hostname: &str) -> String {
    let mut capitalize = true;
    hostname
        .chars()
        .map(|c| {
            if c == '-' {
                capitalize = true;
                ' '
            } else if capitalize {
                capitalize = false;
                c.to_ascii_uppercase()
            } else {
                c
            }
        })
        .collect()
}

fn rehome_admin_ui_port(db: &mut Value) {
    let Some(net) = db
        .get_mut("public")
        .and_then(|p| p.get_mut("serverInfo"))
        .and_then(|s| s.get_mut("network"))
        .and_then(|n| n.get_mut("host"))
        .and_then(|h| h.get_mut("bindings"))
        .and_then(|b| b.get_mut("80"))
        .and_then(|b| b.get_mut("net"))
        .and_then(|n| n.as_object_mut())
    else {
        return;
    };
    let held = net.get("assignedPort").and_then(|p| p.as_u64());
    if held == Some(UI_PORT) {
        return;
    }
    net.insert("assignedPort".into(), Value::from(UI_PORT));

    let Some(available) = db
        .get_mut("private")
        .and_then(|p| p.get_mut("availablePorts"))
        .and_then(|a| a.as_object_mut())
    else {
        return;
    };
    if let Some(held) = held {
        available.remove(&InternedString::from_display(&held));
    }
    available.insert(InternedString::from_display(&UI_PORT), Value::Bool(false));
}

#[cfg(test)]
mod test {
    use imbl_value::json;

    use super::*;

    fn db_with_alpn(server: Value, package: Value) -> Value {
        json!({
            "public": {
                "serverInfo": { "network": { "host": { "bindings": {
                    "443": { "net": {}, "options": { "addSsl": { "alpn": server } } },
                } } } },
                "packageData": { "pkg": { "hosts": { "main": { "bindings": {
                    "8443": { "net": {}, "options": { "addSsl": { "alpn": package } } },
                } } } } },
            },
            "private": { "availablePorts": {} }
        })
    }

    fn server_alpn(db: &Value) -> Value {
        db["public"]["serverInfo"]["network"]["host"]["bindings"]["443"]["options"]["addSsl"]
            ["alpn"]
            .clone()
    }

    fn package_alpn(db: &Value) -> Value {
        db["public"]["packageData"]["pkg"]["hosts"]["main"]["bindings"]["8443"]["options"]["addSsl"]
            ["alpn"]
            .clone()
    }

    #[test]
    fn a_stored_alpn_becomes_the_list_it_named() {
        let mut d = db_with_alpn(
            json!({ "specified": ["http/1.1", "h2"] }),
            json!({ "specified": [] }),
        );
        Version.up(&mut d, ()).unwrap();
        assert_eq!(server_alpn(&d), json!(["http/1.1", "h2"]));
        assert_eq!(package_alpn(&d), json!([]));

        // idempotent
        Version.up(&mut d, ()).unwrap();
        assert_eq!(server_alpn(&d), json!(["http/1.1", "h2"]));

        Version.down(&mut d).unwrap();
        assert_eq!(server_alpn(&d), json!({ "specified": ["http/1.1", "h2"] }));
        assert_eq!(package_alpn(&d), json!({ "specified": [] }));
    }

    #[test]
    fn a_stored_reflect_becomes_no_list() {
        let mut d = db_with_alpn(json!("reflect"), Value::Null);
        Version.up(&mut d, ()).unwrap();
        assert_eq!(server_alpn(&d), Value::Null);
        assert_eq!(package_alpn(&d), Value::Null);
    }

    fn db(net: Value, available_ports: Value) -> Value {
        json!({
            "public": { "serverInfo": { "network": { "host": { "bindings": {
                "80": { "net": net },
            } } } } },
            "private": { "availablePorts": available_ports }
        })
    }

    fn net_of(db: &Value) -> &Value {
        &db["public"]["serverInfo"]["network"]["host"]["bindings"]["80"]["net"]
    }

    #[test]
    fn rehomes_a_drifted_port_and_frees_it() {
        let mut db = db(
            json!({ "assignedPort": 55543, "assignedSslPort": 443 }),
            json!({ "80": false, "443": true, "55543": false }),
        );
        rehome_admin_ui_port(&mut db);
        assert_eq!(net_of(&db)["assignedPort"], json!(80));
        assert_eq!(net_of(&db)["assignedSslPort"], json!(443));
        assert_eq!(
            db["private"]["availablePorts"],
            json!({ "80": false, "443": true })
        );
    }

    #[test]
    fn claims_80_when_no_seed_is_present() {
        let mut db = db(
            json!({ "assignedPort": 49876, "assignedSslPort": 443 }),
            json!({ "443": true, "49876": false }),
        );
        rehome_admin_ui_port(&mut db);
        assert_eq!(net_of(&db)["assignedPort"], json!(80));
        assert_eq!(
            db["private"]["availablePorts"],
            json!({ "80": false, "443": true })
        );
    }

    #[test]
    fn leaves_a_healthy_install_alone() {
        let ports = json!({ "80": false, "443": true });
        let mut db = db(json!({ "assignedPort": 80, "assignedSslPort": 443 }), ports);
        let before = db.clone();
        rehome_admin_ui_port(&mut db);
        assert_eq!(db, before);
    }

    #[test]
    fn claims_80_when_the_binding_holds_no_plaintext_port() {
        let mut db = db(
            json!({ "assignedPort": null, "assignedSslPort": 443 }),
            json!({ "443": true }),
        );
        rehome_admin_ui_port(&mut db);
        assert_eq!(net_of(&db)["assignedPort"], json!(80));
        assert_eq!(
            db["private"]["availablePorts"],
            json!({ "80": false, "443": true })
        );
    }

    #[test]
    fn tolerates_a_db_without_the_admin_binding() {
        let mut db = json!({ "public": { "serverInfo": {} }, "private": {} });
        let before = db.clone();
        rehome_admin_ui_port(&mut db);
        assert_eq!(db, before);
    }
    #[test]
    fn migrates_port_alpn_and_hostname_together() {
        let mut db = db_with_alpn(json!({ "specified": ["h2"] }), json!("reflect"));
        db["public"]["serverInfo"]["name"] = json!("Old Name");
        db["public"]["serverInfo"]["hostname"] = json!("a".repeat(70));
        db["public"]["serverInfo"]["network"]["host"]["bindings"]["80"] =
            json!({ "net": { "assignedPort": 55543, "assignedSslPort": 443 } });
        db["private"]["availablePorts"] = json!({ "80": false, "443": true, "55543": false });

        Version.up(&mut db, ()).unwrap();

        assert_eq!(db["public"]["serverInfo"].get("name"), None);
        assert_eq!(
            db["public"]["serverInfo"]["hostname"],
            json!("a".repeat(32))
        );
        assert_eq!(net_of(&db)["assignedPort"], json!(80));
        assert_eq!(server_alpn(&db), json!(["h2"]));
        assert_eq!(package_alpn(&db), Value::Null);

        Version.down(&mut db).unwrap();

        assert_eq!(
            db["public"]["serverInfo"]["name"],
            json!(format!("A{}", "a".repeat(31)))
        );
        assert_eq!(server_alpn(&db), json!({ "specified": ["h2"] }));
        assert_eq!(package_alpn(&db), Value::Null);
        assert_eq!(net_of(&db)["assignedPort"], json!(80));
    }

    #[test]
    fn drops_and_restores_the_display_name() {
        let mut db = json!({ "public": { "serverInfo": {
            "name": "My Cool Server", "hostname": "my-cool-server"
        } } });
        drop_server_name(&mut db);
        assert_eq!(db["public"]["serverInfo"].get("name"), None);
        restore_server_name(&mut db);
        assert_eq!(db["public"]["serverInfo"]["name"], json!("My Cool Server"));
    }

    #[test]
    fn restore_preserves_an_existing_display_name() {
        let mut db = json!({ "public": { "serverInfo": {
            "name": "Chosen Name", "hostname": "different-hostname"
        } } });
        let before = db.clone();
        restore_server_name(&mut db);
        assert_eq!(db, before);
    }

    #[test]
    fn leaves_a_usable_hostname_alone() {
        let mut db = json!({ "public": { "serverInfo": { "hostname": "my-cool-server" } } });
        let before = db.clone();
        repair_unusable_hostname(&mut db);
        assert_eq!(db, before);
    }

    #[test]
    fn brings_a_hostname_the_kernel_refuses_back_into_range() {
        let mut db = json!({ "public": { "serverInfo": { "hostname": "a".repeat(70) } } });
        repair_unusable_hostname(&mut db);
        assert_eq!(
            db["public"]["serverInfo"]["hostname"],
            json!("a".repeat(32))
        );
    }

    #[test]
    fn trims_a_hostname_over_the_limit() {
        let mut db = json!({ "public": { "serverInfo": { "hostname": "a".repeat(55) } } });
        repair_unusable_hostname(&mut db);
        assert_eq!(
            db["public"]["serverInfo"]["hostname"],
            json!("a".repeat(32))
        );
    }

    #[test]
    fn repairing_a_db_without_a_hostname_is_harmless() {
        let mut db = json!({ "public": { "serverInfo": {} } });
        let before = db.clone();
        repair_unusable_hostname(&mut db);
        assert_eq!(db, before);
    }

    #[test]
    fn rollback_names_a_server_whose_hostname_is_missing() {
        let mut db = json!({ "public": { "serverInfo": {} } });
        restore_server_name(&mut db);
        assert_eq!(db["public"]["serverInfo"]["name"], json!("StartOS"));
    }
}
