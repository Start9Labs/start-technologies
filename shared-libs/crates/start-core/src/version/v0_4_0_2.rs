use std::collections::BTreeSet;

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
        rehome_displaced_ssl_legs(db);
        Ok(Value::Null)
    }
    fn down(self, _db: &mut Value) -> Result<(), Error> {
        // Every earlier version keeps the ports it finds: 80 is what it wants
        // for the UI, and a rehomed binding holds both legs, the shape the
        // since-removed `carried` left alone.
        Ok(())
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

/// The ports a displaced binding should be holding: `(ssl leg's preferred port,
/// plaintext leg's preferred port, the port the plaintext leg was pushed onto)`.
/// `None` when the binding's ssl leg is not sitting on the plaintext leg's port.
fn displaced_ssl_leg(binding: &Value) -> Option<(u64, u64, Option<u64>)> {
    let options = binding.get("options")?;
    // Only a binding we terminate TLS for has an ssl port of its own to be
    // displaced from; one that serves its own TLS has no plaintext leg at all.
    let ssl_to = options
        .get("addSsl")?
        .get("preferredExternalPort")?
        .as_u64()?;
    if options["secure"]["ssl"].as_bool().unwrap_or(false) {
        return None;
    }
    let plain_to = options.get("preferredExternalPort")?.as_u64()?;
    // A package asking for one number on both legs gets what it asked for.
    if plain_to == ssl_to {
        return None;
    }
    let net = binding.get("net")?;
    if net.get("assignedSslPort")?.as_u64()? != plain_to {
        return None;
    }
    Some((
        ssl_to,
        plain_to,
        net.get("assignedPort").and_then(|p| p.as_u64()),
    ))
}

/// Put each leg of a package binding back on the port its options ask for.
///
/// A 0.3.x service whose manifest carried no `lan-config` was migrated as a
/// single plaintext binding by `SystemForEmbassy::exportNetwork` — electrs,
/// whose `lan-config` was commented out, is the reported case. Installing the
/// 0.4 package over it reached `BindInfo::update` on the same host and internal
/// port, where the since-removed `carried` handed that lone number to the ssl
/// leg (reclaimed first) and left the plaintext leg on a fresh ephemeral port.
/// The address wallets connect to then served TLS on the port the package asked
/// to serve *plaintext* on — electrs answering TLS on 50001 rather than 50002 —
/// and `update` prefers the port it already holds, so it never healed.
///
/// A binding is left alone when its ssl leg's preferred port is held elsewhere,
/// rather than moved onto a port another binding is serving.
fn rehome_displaced_ssl_legs(db: &mut Value) {
    let mut taken: BTreeSet<u64> = db
        .get("private")
        .and_then(|p| p.get("availablePorts"))
        .and_then(|a| a.as_object())
        .map(|a| a.keys().filter_map(|k| k.parse().ok()).collect())
        .unwrap_or_default();

    // Deferred so the walk below can hold `packageData` mutably.
    let mut freed: Vec<u64> = Vec::new();
    let mut claimed: Vec<(u64, bool)> = Vec::new();

    if let Some(packages) = db
        .get_mut("public")
        .and_then(|p| p.get_mut("packageData"))
        .and_then(|p| p.as_object_mut())
    {
        for (_, package) in packages.iter_mut() {
            let Some(hosts) = package.get_mut("hosts").and_then(|h| h.as_object_mut()) else {
                continue;
            };
            for (_, host) in hosts.iter_mut() {
                let Some(bindings) = host.get_mut("bindings").and_then(|b| b.as_object_mut())
                else {
                    continue;
                };
                for (_, binding) in bindings.iter_mut() {
                    let Some((ssl_to, plain_to, plain_from)) = displaced_ssl_leg(binding) else {
                        continue;
                    };
                    // `plain_to` is this binding's own ssl hold, so only an outside
                    // claim on `ssl_to` blocks the move — including one made by a
                    // binding rehomed earlier in this walk.
                    if taken.contains(&ssl_to) && Some(ssl_to) != plain_from {
                        continue;
                    }
                    let Some(net) = binding.get_mut("net").and_then(|n| n.as_object_mut()) else {
                        continue;
                    };
                    net.insert("assignedSslPort".into(), Value::from(ssl_to));
                    net.insert("assignedPort".into(), Value::from(plain_to));

                    if let Some(plain_from) = plain_from {
                        taken.remove(&plain_from);
                        freed.push(plain_from);
                    }
                    taken.insert(ssl_to);
                    claimed.push((ssl_to, true));
                    claimed.push((plain_to, false));
                }
            }
        }
    }

    if freed.is_empty() && claimed.is_empty() {
        return;
    }
    let Some(available) = db
        .get_mut("private")
        .and_then(|p| p.get_mut("availablePorts"))
        .and_then(|a| a.as_object_mut())
    else {
        return;
    };
    for port in freed {
        available.remove(&InternedString::from_display(&port));
    }
    // After the frees, so a port one binding released and another claimed lands
    // claimed.
    for (port, ssl) in claimed {
        available.insert(InternedString::from_display(&port), Value::Bool(ssl));
    }
}

#[cfg(test)]
mod test {
    use imbl_value::json;

    use super::*;

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

    fn pkg_db(package_data: Value, available_ports: Value) -> Value {
        json!({
            "public": {
                "serverInfo": { "network": { "host": { "bindings": {} } } },
                "packageData": package_data,
            },
            "private": { "availablePorts": available_ports }
        })
    }

    /// One package, one host, one binding — the electrs shape: plaintext
    /// preferred on 50001, the OS terminating TLS on 50002.
    fn electrum_pkg(id: &str, plain: u64, net: Value) -> Value {
        json!({ id: { "hosts": { "electrum": { "bindings": { plain.to_string(): {
            "options": {
                "preferredExternalPort": plain,
                "addSsl": { "preferredExternalPort": 50002 },
                "secure": null,
            },
            "net": net,
        } } } } } })
    }

    fn net_at(db: &Value, id: &str, internal_port: &str) -> Value {
        db["public"]["packageData"][id]["hosts"]["electrum"]["bindings"][internal_port]["net"]
            .clone()
    }

    // The reported case: a 0.3.5.1 box whose electrs binding came through the
    // v1 compat shim as plaintext-only, then had the 0.4 package installed over
    // it — TLS answering on 50001, plaintext pushed to an ephemeral port.
    #[test]
    fn rehomes_an_ssl_leg_off_the_plaintext_legs_number() {
        let mut db = pkg_db(
            electrum_pkg(
                "electrs",
                50001,
                json!({ "assignedPort": 52981, "assignedSslPort": 50001 }),
            ),
            json!({ "50001": true, "52981": false }),
        );
        rehome_displaced_ssl_legs(&mut db);
        assert_eq!(
            net_at(&db, "electrs", "50001"),
            json!({ "assignedPort": 50001, "assignedSslPort": 50002 })
        );
        assert_eq!(
            db["private"]["availablePorts"],
            json!({ "50001": false, "50002": true })
        );
    }

    #[test]
    fn leaves_a_binding_on_its_preferred_ports_alone() {
        let mut db = pkg_db(
            electrum_pkg(
                "electrs",
                50001,
                json!({ "assignedPort": 50001, "assignedSslPort": 50002 }),
            ),
            json!({ "50001": false, "50002": true }),
        );
        let before = db.clone();
        rehome_displaced_ssl_legs(&mut db);
        assert_eq!(db, before);
    }

    // A binding we rewrap has no plaintext leg, so its ssl port sitting on
    // `preferredExternalPort` is the number it legitimately carried over from
    // serving its own TLS there — not a displaced leg.
    #[test]
    fn leaves_a_rewrapped_binding_that_kept_its_number() {
        let mut db = pkg_db(
            json!({ "svc": { "hosts": { "electrum": { "bindings": { "8080": {
                "options": {
                    "preferredExternalPort": 8080,
                    "addSsl": { "preferredExternalPort": 8443 },
                    "secure": { "ssl": true },
                },
                "net": { "assignedPort": null, "assignedSslPort": 8080 },
            } } } } } }),
            json!({ "8080": true }),
        );
        let before = db.clone();
        rehome_displaced_ssl_legs(&mut db);
        assert_eq!(db, before);
    }

    #[test]
    fn leaves_a_binding_alone_when_its_ssl_port_is_held_elsewhere() {
        let mut db = pkg_db(
            electrum_pkg(
                "electrs",
                50001,
                json!({ "assignedPort": 52981, "assignedSslPort": 50001 }),
            ),
            json!({ "50001": true, "50002": true, "52981": false }),
        );
        let before = db.clone();
        rehome_displaced_ssl_legs(&mut db);
        assert_eq!(db, before);
    }

    // Two Electrum servers both prefer 50002; the second keeps what it has
    // rather than being moved onto the port the first just claimed.
    #[test]
    fn only_one_of_two_displaced_bindings_takes_the_contested_port() {
        let mut package_data = electrum_pkg(
            "electrs",
            50001,
            json!({ "assignedPort": 52981, "assignedSslPort": 50001 }),
        );
        let fulcrum = electrum_pkg(
            "fulcrum",
            50011,
            json!({ "assignedPort": 52990, "assignedSslPort": 50011 }),
        );
        package_data["fulcrum"] = fulcrum["fulcrum"].clone();
        let mut db = pkg_db(
            package_data,
            json!({ "50001": true, "50011": true, "52981": false, "52990": false }),
        );
        rehome_displaced_ssl_legs(&mut db);
        assert_eq!(
            net_at(&db, "electrs", "50001"),
            json!({ "assignedPort": 50001, "assignedSslPort": 50002 })
        );
        assert_eq!(
            net_at(&db, "fulcrum", "50011"),
            json!({ "assignedPort": 52990, "assignedSslPort": 50011 })
        );
        assert_eq!(
            db["private"]["availablePorts"],
            json!({ "50001": false, "50002": true, "50011": true, "52990": false })
        );
    }

    // `alloc` draws from 49152 up, so the displaced plaintext leg can land on
    // the ssl leg's preferred port. Only this binding is then in the way.
    #[test]
    fn rehomes_when_the_plaintext_leg_drifted_onto_the_ssl_port() {
        let mut db = pkg_db(
            electrum_pkg(
                "electrs",
                50001,
                json!({ "assignedPort": 50002, "assignedSslPort": 50001 }),
            ),
            json!({ "50001": true, "50002": false }),
        );
        rehome_displaced_ssl_legs(&mut db);
        assert_eq!(
            net_at(&db, "electrs", "50001"),
            json!({ "assignedPort": 50001, "assignedSslPort": 50002 })
        );
        assert_eq!(
            db["private"]["availablePorts"],
            json!({ "50001": false, "50002": true })
        );
    }

    #[test]
    fn tolerates_a_db_without_package_data() {
        let mut db = json!({ "public": { "serverInfo": {} }, "private": {} });
        let before = db.clone();
        rehome_displaced_ssl_legs(&mut db);
        assert_eq!(db, before);
    }
}
