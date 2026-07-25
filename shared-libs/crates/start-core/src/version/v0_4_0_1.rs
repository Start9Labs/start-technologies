use exver::VersionRange;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_0};
use crate::prelude::*;

lazy_static::lazy_static! {
    static ref V0_4_0_1: exver::Version = exver::Version::new([0, 4, 0, 1], []);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_0::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_0_1.clone()
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    #[instrument(skip_all)]
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        let mut ssl_ports = Vec::new();
        for_each_host(db, |host| {
            move_port(host, "assignedPort", "assignedSslPort");
            collect_self_tls_ports(host, &mut ssl_ports);
        });
        set_port_ssl_flags(db, &ssl_ports, true);
        Ok(Value::Null)
    }
    fn down(self, db: &mut Value) -> Result<(), Error> {
        let mut ssl_ports = Vec::new();
        for_each_host(db, |host| {
            collect_self_tls_ports(host, &mut ssl_ports);
            move_port(host, "assignedSslPort", "assignedPort");
        });
        set_port_ssl_flags(db, &ssl_ports, false);
        Ok(())
    }
}

fn for_each_host(db: &mut Value, mut f: impl FnMut(&mut Value)) {
    if let Some(host) = db
        .get_mut("public")
        .and_then(|p| p.get_mut("serverInfo"))
        .and_then(|s| s.get_mut("network"))
        .and_then(|n| n.get_mut("host"))
    {
        f(host);
    }
    if let Some(packages) = db
        .get_mut("public")
        .and_then(|p| p.get_mut("packageData"))
        .and_then(|p| p.as_object_mut())
    {
        for (_, package) in packages.iter_mut() {
            if let Some(hosts) = package.get_mut("hosts").and_then(|h| h.as_object_mut()) {
                for (_, host) in hosts.iter_mut() {
                    f(host);
                }
            }
        }
    }
}

/// A binding whose container serves its own TLS now holds its external port in
/// `assignedSslPort`, like every other TLS-carrying binding. The port number
/// itself is preserved, so addresses the user has bookmarked and per-address
/// enable/disable overrides (keyed by port) survive the move.
fn move_port(host: &mut Value, from: &str, to: &str) {
    let Some(bindings) = host.get_mut("bindings").and_then(|b| b.as_object_mut()) else {
        return;
    };
    for (_, binding) in bindings.iter_mut() {
        let serves_own_tls = binding.get("options").map_or(false, |o| {
            o["secure"]["ssl"].as_bool().unwrap_or(false) && o["addSsl"].is_null()
        });
        if !serves_own_tls {
            continue;
        }
        let Some(net) = binding.get_mut("net").and_then(|n| n.as_object_mut()) else {
            continue;
        };
        if !net.get(to).map_or(true, |v| v.is_null()) {
            continue;
        }
        let Some(port) = net.get(from).and_then(|p| p.as_u64()) else {
            continue;
        };
        net.insert(to.into(), Value::from(port));
        net.insert(from.into(), Value::Null);
    }
}

/// A self-TLS binding's port is now answered by our SNI-passthrough listener,
/// so its `availablePorts` ssl flag flips with the move.
fn collect_self_tls_ports(host: &Value, ports: &mut Vec<u64>) {
    let Some(bindings) = host.get("bindings").and_then(|b| b.as_object()) else {
        return;
    };
    for (_, binding) in bindings.iter() {
        let serves_own_tls = binding.get("options").map_or(false, |o| {
            o["secure"]["ssl"].as_bool().unwrap_or(false) && o["addSsl"].is_null()
        });
        if !serves_own_tls {
            continue;
        }
        if let Some(port) = binding["net"]["assignedSslPort"].as_u64() {
            ports.push(port);
        }
    }
}

fn set_port_ssl_flags(db: &mut Value, ports: &[u64], ssl: bool) {
    let Some(available) = db
        .get_mut("private")
        .and_then(|p| p.get_mut("availablePorts"))
        .and_then(|a| a.as_object_mut())
    else {
        return;
    };
    for port in ports {
        available.insert(InternedString::from_display(port), Value::Bool(ssl));
    }
}

#[cfg(test)]
mod test {
    use imbl_value::json;

    use super::*;

    fn db(options: Value, net: Value) -> Value {
        json!({
            "public": {
                "serverInfo": { "network": { "host": { "bindings": {} } } },
                "packageData": {
                    "pkg": { "hosts": { "main": { "bindings": { "8080": {
                        "options": options,
                        "net": net,
                    } } } } }
                }
            },
            "private": { "availablePorts": { "8080": false } }
        })
    }

    fn net_of(db: &Value) -> &Value {
        &db["public"]["packageData"]["pkg"]["hosts"]["main"]["bindings"]["8080"]["net"]
    }

    fn port_ssl(db: &Value, port: &str) -> Option<bool> {
        db["private"]["availablePorts"][port].as_bool()
    }

    #[test]
    fn moves_only_self_tls_bindings() {
        let self_tls =
            json!({ "preferredExternalPort": 8080, "addSsl": null, "secure": { "ssl": true } });
        let mut d = db(
            self_tls.clone(),
            json!({ "assignedPort": 8080, "assignedSslPort": null }),
        );
        Version.up(&mut d, ()).unwrap();
        assert_eq!(net_of(&d)["assignedSslPort"].as_u64(), Some(8080));
        assert!(net_of(&d)["assignedPort"].is_null());
        // the port is now one of our SNI-passthrough listeners' ports
        assert_eq!(port_ssl(&d, "8080"), Some(true));

        // idempotent
        Version.up(&mut d, ()).unwrap();
        assert_eq!(net_of(&d)["assignedSslPort"].as_u64(), Some(8080));
        assert_eq!(port_ssl(&d, "8080"), Some(true));

        Version.down(&mut d).unwrap();
        assert_eq!(net_of(&d)["assignedPort"].as_u64(), Some(8080));
        assert!(net_of(&d)["assignedSslPort"].is_null());
        assert_eq!(port_ssl(&d, "8080"), Some(false));

        // the OS terminates TLS here: both ports already mean what they say
        let add_ssl = json!({
            "preferredExternalPort": 80,
            "addSsl": { "preferredExternalPort": 443 },
            "secure": null,
        });
        let mut d = db(
            add_ssl,
            json!({ "assignedPort": 8080, "assignedSslPort": 8443 }),
        );
        Version.up(&mut d, ()).unwrap();
        assert_eq!(net_of(&d)["assignedPort"].as_u64(), Some(8080));
        assert_eq!(net_of(&d)["assignedSslPort"].as_u64(), Some(8443));
        assert_eq!(port_ssl(&d, "8080"), Some(false));

        // plaintext binding is untouched
        let plain =
            json!({ "preferredExternalPort": 8080, "addSsl": null, "secure": { "ssl": false } });
        let mut d = db(
            plain,
            json!({ "assignedPort": 8080, "assignedSslPort": null }),
        );
        Version.up(&mut d, ()).unwrap();
        assert_eq!(net_of(&d)["assignedPort"].as_u64(), Some(8080));
        assert!(net_of(&d)["assignedSslPort"].is_null());
        assert_eq!(port_ssl(&d, "8080"), Some(false));
    }
}
