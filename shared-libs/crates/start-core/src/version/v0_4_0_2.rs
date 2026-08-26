use exver::VersionRange;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_0_1};
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
    #[instrument(skip_all)]
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        rehome_admin_ui_port(db);
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
        Ok(Value::Null)
    }
    fn down(self, db: &mut Value) -> Result<(), Error> {
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

/// Every `addSsl.alpn` a host holds, on the server's own bindings and on every
/// package's.
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

/// Give the StartOS UI back its well-known plaintext port.
///
/// `Public::init` plants the admin binding already holding `assignedSslPort`
/// but not `assignedPort`, so `os_bindings` reaches it through `BindInfo::update`,
/// which before #3558 could only fall through to a port at or above 49152 —
/// and then kept it, since `update` prefers the port it already holds.
///
/// Nothing else can hold 80: it was unclaimable for everyone until #3558 and is
/// privileged-only after it. Writing it also clears the unheld 80 that installs
/// before #3558 were seeded with.
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

    /// A server binding and a package binding, each carrying `alpn` as given.
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

    /// A stored list is carried as the list itself.
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

    /// `reflect` named the client's own list, which is what no list names now.
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

    // The shape a box installed before #3558 upgrades from: the plaintext leg
    // drifted to an ephemeral port, and `availablePorts` still carries the 80
    // that `Database::init` seeded but no binding ever held.
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

    // A box that came up through v0_4_0_alpha_20 had `availablePorts` rebuilt
    // from its bindings, so it has no unheld 80 to clear.
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
}
