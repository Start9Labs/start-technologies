use std::collections::BTreeMap;
use std::net::SocketAddr;

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
        unswap_carried_legs(db);
        Ok(Value::Null)
    }
    fn down(self, _db: &mut Value) -> Result<(), Error> {
        // Every earlier version wants 80 here and keeps the port it finds.
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

/// The lowest port [`crate::net::forward::AvailablePorts::alloc`] hands out.
const EPHEMERAL_START: u64 = 49152;

/// Give back the external ports a rebind swapped between a binding's two legs.
///
/// Before #3638 `BindInfo::update` carried the one number a binding held to
/// whichever leg reclaimed first, and the TLS leg reclaims first. A binding that
/// arrived holding only a plaintext port and then gained `addSsl` therefore
/// handed that number to TLS and dropped its own leg onto an ephemeral port, so
/// a client configured against the number before the rebind now speaks plaintext
/// to a TLS listener.
///
/// Servers migrated from 0.3.5.1 are the population. The v1 compat shim binds
/// each `tor-config` port with `addSsl: null` (it comes only from `lan-config`),
/// and the 0.4 package then rebinds the same host and internal port with one.
///
/// The fingerprint is not reachable any other way: `BindInfo::new` allocates the
/// TLS leg from `addSsl` first and the plaintext leg from `preferredExternalPort`
/// second, so a binding that lost either race lands on an ephemeral port for that
/// leg rather than on the other leg's number.
fn unswap_carried_legs(db: &mut Value) {
    let Some(mut ports) = db
        .get("private")
        .and_then(|p| p.get("availablePorts"))
        .and_then(|a| a.as_object())
        .map(|a| {
            a.iter()
                .filter_map(|(k, v)| Some((k.parse::<u64>().ok()?, v.as_bool()?)))
                .collect::<BTreeMap<_, _>>()
        })
    else {
        return;
    };

    let mut moved = false;
    for_each_binding(db, |bind| {
        let Some(options) = bind.get("options") else {
            return;
        };
        // A container that serves its own TLS has no plaintext leg to swap.
        if options
            .get("secure")
            .and_then(|s| s.get("ssl"))
            .and_then(|s| s.as_bool())
            == Some(true)
        {
            return;
        }
        let Some(plain) = options
            .get("preferredExternalPort")
            .and_then(|p| p.as_u64())
        else {
            return;
        };
        let Some(ssl) = options
            .get("addSsl")
            .and_then(|s| s.get("preferredExternalPort"))
            .and_then(|p| p.as_u64())
        else {
            return;
        };
        let net = bind.get("net");
        let held_ssl = net
            .and_then(|n| n.get("assignedSslPort"))
            .and_then(|p| p.as_u64());
        let held_plain = net
            .and_then(|n| n.get("assignedPort"))
            .and_then(|p| p.as_u64());

        // TLS sitting on the plaintext leg's number, with the plaintext leg
        // pushed into the range only `alloc` hands out.
        if held_ssl != Some(plain) {
            return;
        }
        let Some(held_plain) = held_plain.filter(|p| *p >= EPHEMERAL_START) else {
            return;
        };
        // Someone else took the number the TLS leg asked for; moving onto it
        // would break them instead.
        if ports.contains_key(&ssl) {
            return;
        }

        if let Some(net) = bind.get_mut("net").and_then(|n| n.as_object_mut()) {
            net.insert("assignedPort".into(), Value::from(plain));
            net.insert("assignedSslPort".into(), Value::from(ssl));
        }
        // The overrides are keyed by port, and both ports move at once — a
        // rekey per leg would carry the first leg's entries twice.
        if let Some(addresses) = bind.get_mut("addresses") {
            swap_override_ports(addresses, held_plain, plain, ssl);
        }
        ports.remove(&held_plain);
        ports.insert(plain, false);
        ports.insert(ssl, true);
        moved = true;
    });

    if !moved {
        return;
    }
    if let Some(private) = db.get_mut("private").and_then(|p| p.as_object_mut()) {
        private.insert(
            "availablePorts".into(),
            Value::Object(
                ports
                    .into_iter()
                    .map(|(k, v)| (InternedString::from_display(&k), Value::Bool(v)))
                    .collect(),
            ),
        );
    }
}

/// Move every `enabled` / `disabled` / `guaWan` override off the two ports the
/// legs just exchanged: the plaintext leg's from `old_plain` to `new_plain`, and
/// the TLS leg's from `new_plain` to `new_ssl`. Losing one would silently
/// re-enable an address the operator turned off.
fn swap_override_ports(addresses: &mut Value, old_plain: u64, new_plain: u64, new_ssl: u64) {
    let remap = |port: u64| {
        if port == old_plain {
            new_plain
        } else if port == new_plain {
            new_ssl
        } else {
            port
        }
    };
    for key in ["enabled", "guaWan"] {
        if let Some(list) = addresses.get_mut(key).and_then(|l| l.as_array_mut()) {
            for entry in list.iter_mut() {
                let Some(mut addr) = entry.as_str().and_then(|s| s.parse::<SocketAddr>().ok())
                else {
                    continue;
                };
                let port = remap(addr.port() as u64);
                addr.set_port(port as u16);
                *entry = Value::String(addr.to_string().into());
            }
        }
    }
    if let Some(list) = addresses.get_mut("disabled").and_then(|l| l.as_array_mut()) {
        for entry in list.iter_mut() {
            let Some(port) = entry.get(1).and_then(|p| p.as_u64()) else {
                continue;
            };
            if let Some(pair) = entry.as_array_mut() {
                pair.set(1, Value::from(remap(port)));
            }
        }
    }
}

/// Every binding on the server host and on every installed package.
fn for_each_binding(db: &mut Value, mut f: impl FnMut(&mut Value)) {
    let mut visit = |host: &mut Value| {
        if let Some(bindings) = host.get_mut("bindings").and_then(|b| b.as_object_mut()) {
            for (_, bind) in bindings.iter_mut() {
                f(bind);
            }
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

    fn pkg_db(net: Value, addresses: Value, available_ports: Value) -> Value {
        json!({
            "public": { "packageData": { "electrs": { "hosts": { "electrum": { "bindings": {
                "50001": {
                    "options": {
                        "preferredExternalPort": 50001,
                        "addSsl": { "preferredExternalPort": 50002 },
                        "secure": Value::Null,
                    },
                    "net": net,
                    "addresses": addresses,
                },
            } } } } } },
            "private": { "availablePorts": available_ports }
        })
    }

    fn pkg_bind(db: &Value) -> &Value {
        &db["public"]["packageData"]["electrs"]["hosts"]["electrum"]["bindings"]["50001"]
    }

    fn no_overrides() -> Value {
        json!({ "enabled": [], "disabled": [], "guaWan": [], "available": [] })
    }

    // A server migrated from 0.3.5.1: the v1 compat shim bound 50001 with no
    // `addSsl`, and the 0.4 package's rebind handed that number to the TLS leg.
    #[test]
    fn unswaps_a_binding_that_gained_add_ssl() {
        let mut db = pkg_db(
            json!({ "assignedPort": 51820, "assignedSslPort": 50001 }),
            no_overrides(),
            json!({ "50001": true, "51820": false }),
        );
        unswap_carried_legs(&mut db);
        assert_eq!(pkg_bind(&db)["net"]["assignedPort"], json!(50001));
        assert_eq!(pkg_bind(&db)["net"]["assignedSslPort"], json!(50002));
        assert_eq!(
            db["private"]["availablePorts"],
            json!({ "50001": false, "50002": true })
        );
    }

    // Both ports move at once, so the overrides have to cross rather than shift:
    // losing a `disabled` entry silently re-enables an address the operator
    // turned off.
    #[test]
    fn carries_the_overrides_across_the_swap() {
        let mut db = pkg_db(
            json!({ "assignedPort": 51820, "assignedSslPort": 50001 }),
            json!({
                "enabled": ["1.2.3.4:50001", "1.2.3.4:8443"],
                "disabled": [["electrs.local", 51820], ["other.local", 9735]],
                "guaWan": ["[2001:db8::1]:51820"],
                "available": [],
            }),
            json!({ "50001": true, "51820": false }),
        );
        unswap_carried_legs(&mut db);
        let addresses = &pkg_bind(&db)["addresses"];
        // The TLS leg's WAN opt-in follows TLS to 50002; the unrelated one stays.
        assert_eq!(
            addresses["enabled"],
            json!(["1.2.3.4:50002", "1.2.3.4:8443"])
        );
        // The plaintext leg's disable follows plaintext to 50001.
        assert_eq!(
            addresses["disabled"],
            json!([["electrs.local", 50001], ["other.local", 9735]])
        );
        assert_eq!(addresses["guaWan"], json!(["[2001:db8::1]:50001"]));
    }

    #[test]
    fn leaves_a_healthy_binding_alone() {
        let mut db = pkg_db(
            json!({ "assignedPort": 50001, "assignedSslPort": 50002 }),
            no_overrides(),
            json!({ "50001": false, "50002": true }),
        );
        let before = db.clone();
        unswap_carried_legs(&mut db);
        assert_eq!(db, before);
    }

    // Fulcrum already holds 50002. Moving onto it would break that binding
    // instead, so this one keeps its ports and stays broken.
    #[test]
    fn leaves_a_binding_alone_when_the_ssl_number_is_taken() {
        let mut db = pkg_db(
            json!({ "assignedPort": 51820, "assignedSslPort": 50001 }),
            no_overrides(),
            json!({ "50001": true, "50002": true, "51820": false }),
        );
        let before = db.clone();
        unswap_carried_legs(&mut db);
        assert_eq!(db, before);
    }

    // Losing the race for `addSsl`'s number puts the TLS leg on an ephemeral
    // port, not on the plaintext leg's — a shape `new` produces legitimately.
    #[test]
    fn leaves_a_binding_whose_ssl_leg_merely_lost_a_race_alone() {
        let mut db = pkg_db(
            json!({ "assignedPort": 50001, "assignedSslPort": 51820 }),
            no_overrides(),
            json!({ "50001": false, "51820": true }),
        );
        let before = db.clone();
        unswap_carried_legs(&mut db);
        assert_eq!(db, before);
    }

    #[test]
    fn leaves_a_native_tls_binding_alone() {
        let mut db = pkg_db(
            json!({ "assignedPort": 51820, "assignedSslPort": 50001 }),
            no_overrides(),
            json!({ "50001": true, "51820": false }),
        );
        db["public"]["packageData"]["electrs"]["hosts"]["electrum"]["bindings"]["50001"]["options"]
            ["secure"] = json!({ "ssl": true });
        let before = db.clone();
        unswap_carried_legs(&mut db);
        assert_eq!(db, before);
    }

    #[test]
    fn tolerates_a_db_with_no_packages() {
        let mut db = json!({ "public": { "serverInfo": {} }, "private": { "availablePorts": {} } });
        let before = db.clone();
        unswap_carried_legs(&mut db);
        assert_eq!(db, before);
    }

    #[test]
    fn tolerates_a_db_without_the_admin_binding() {
        let mut db = json!({ "public": { "serverInfo": {} }, "private": {} });
        let before = db.clone();
        rehome_admin_ui_port(&mut db);
        assert_eq!(db, before);
    }
}
