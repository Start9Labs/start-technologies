use std::net::SocketAddr;

use exver::VersionRange;

use super::v0_3_5::V0_3_0_COMPAT;
use super::v0_4_0_2::for_each_host;
use super::{VersionT, v0_4_2_2};
use crate::net::host::binding::PublicIpKey;
use crate::net::service_interface::HostnameInfo;
use crate::prelude::*;

lazy_static::lazy_static! {
    static ref V0_4_3: exver::Version = exver::Version::new([0, 4, 3], []);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_2_2::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_3.clone()
    }
    fn migration_revision(self) -> usize {
        1
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        for_each_host(db, |host| {
            for table in ["bindings", "bindingRanges"] {
                let Some(bindings) = host.get_mut(table).and_then(Value::as_object_mut) else {
                    continue;
                };
                for (_, binding) in bindings.iter_mut() {
                    if let Some(addresses) = binding.get_mut("addresses") {
                        key_public_ip_opt_ins(addresses);
                    }
                }
            }
        });
        Ok(Value::Null)
    }
    fn down(self, _db: &mut Value) -> Result<(), Error> {
        Ok(())
    }
}

/// Rewrites socket-address opt-ins as `PublicIpKey`s of the rows they select.
/// An opt-in matching no current row is dropped; an already-keyed one is kept.
fn key_public_ip_opt_ins(addresses: &mut Value) {
    let available: Vec<HostnameInfo> = addresses
        .get("available")
        .and_then(|available| from_value(available.clone()).ok())
        .unwrap_or_default();
    for field in ["enabled", "guaWan"] {
        let Some(opt_ins) = addresses.get(field).and_then(Value::as_array) else {
            continue;
        };
        let keys: Vec<Value> = opt_ins
            .iter()
            .filter_map(|opt_in| {
                if opt_in.is_object() {
                    return Some(opt_in.clone());
                }
                let socket: SocketAddr = opt_in.as_str()?.parse().ok()?;
                let row = available
                    .iter()
                    .find(|row| row.to_socket_addr() == Some(socket))?;
                to_value(&PublicIpKey::of(row)?).ok()
            })
            .collect();
        addresses[field] = Value::Array(keys.into_iter().collect());
    }
}

#[cfg(test)]
mod tests {
    use imbl_value::json;

    use super::*;

    fn row(hostname: &str, port: u16, public: bool, metadata: Value) -> Value {
        json!({
            "ssl": false,
            "public": public,
            "hostname": hostname,
            "port": port,
            "metadata": metadata,
        })
    }

    fn database(addresses: Value) -> Value {
        json!({
            "public": {
                "serverInfo": { "network": { "host": { "bindings": {} } } },
                "packageData": { "service": { "hosts": { "web": {
                    "bindings": { "8080": { "addresses": addresses.clone() } },
                    "bindingRanges": { "21115": { "addresses": addresses } }
                } } } }
            }
        })
    }

    fn migrated(addresses: Value) -> Vec<Value> {
        let mut db = database(addresses);
        Version.up(&mut db, ()).unwrap();
        let host = &db["public"]["packageData"]["service"]["hosts"]["web"];
        vec![
            host["bindings"]["8080"]["addresses"].clone(),
            host["bindingRanges"]["21115"]["addresses"].clone(),
        ]
    }

    #[test]
    fn opt_ins_become_keys_of_the_rows_they_select() {
        let addresses = json!({
            "enabled": ["203.0.113.7:42000", "[2001:db8:1::a3b:2]:42000", "198.51.100.9:42000"],
            "guaWan": ["[2001:db8:1::a3b:2]:42000", "[2001:db8:9::1]:42000"],
            "available": [
                row("203.0.113.7", 42000, true, json!({ "kind": "ipv4", "gateway": "eth0" })),
                row("192.168.1.5", 42000, false, json!({ "kind": "ipv4", "gateway": "eth0" })),
                row(
                    "2001:db8:1::a3b:2",
                    42000,
                    true,
                    json!({ "kind": "ipv6", "gateway": "eth0", "scopeId": 2 }),
                ),
            ],
        });
        let wan = to_value(&PublicIpKey::at("eth0", "203.0.113.7:42000")).unwrap();
        let gua = to_value(&PublicIpKey::at("eth0", "[2001:db8:1::a3b:2]:42000")).unwrap();
        for addresses in migrated(addresses) {
            assert_eq!(addresses["enabled"], json!([wan.clone(), gua.clone()]));
            assert_eq!(addresses["guaWan"], json!([gua.clone()]));
        }
    }

    #[test]
    fn rerunning_the_migration_keeps_keyed_opt_ins() {
        let addresses = json!({
            "enabled": ["203.0.113.7:42000"],
            "guaWan": [],
            "available": [
                row("203.0.113.7", 42000, true, json!({ "kind": "ipv4", "gateway": "eth0" })),
            ],
        });
        let once = migrated(addresses);
        let mut db = database(once[0].clone());
        Version.up(&mut db, ()).unwrap();
        assert_eq!(
            db["public"]["packageData"]["service"]["hosts"]["web"]["bindings"]["8080"]["addresses"],
            once[0]
        );
    }
}
