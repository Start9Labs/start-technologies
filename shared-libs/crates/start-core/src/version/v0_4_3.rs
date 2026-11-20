use exver::VersionRange;
use imbl_value::InternedString;

use super::v0_3_5::V0_3_0_COMPAT;
use super::v0_4_0_2::for_each_host;
use super::{VersionT, v0_4_2_2};
use crate::lxc::slot::ContainerSlot;
use crate::net::host::binding::{DerivedAddressInfo, set_nonssl_wan_group};
use crate::net::service_interface::HostnameMetadata;
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
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn migration_revision(self) -> usize {
        2
    }
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        assign_container_slots(db)?;
        for_each_host(db, |host| {
            let Some(ranges) = host.get_mut("bindingRanges").and_then(Value::as_object_mut) else {
                return;
            };
            for (_, range) in ranges.iter_mut() {
                if let Some(addresses) = range.get_mut("addresses") {
                    group_enabled_plain_domains(addresses);
                }
            }
        });
        Ok(Value::Null)
    }
    fn down(self, db: &mut Value) -> Result<(), Error> {
        if let Some(packages) = package_data(db) {
            for (_, package) in packages.iter_mut() {
                if let Some(package) = package.as_object_mut() {
                    package.remove("containerSlot");
                }
            }
        }
        Ok(())
    }
}

fn package_data(db: &mut Value) -> Option<&mut imbl_value::InOMap<InternedString, Value>> {
    db.get_mut("public")?
        .get_mut("packageData")?
        .as_object_mut()
}

/// Gives each package without a slot the lowest free one, in package-id order.
fn assign_container_slots(db: &mut Value) -> Result<(), Error> {
    let Some(packages) = package_data(db) else {
        return Ok(());
    };
    let mut held = Vec::new();
    let mut unassigned = Vec::new();
    for (id, package) in packages.iter() {
        match package.get("containerSlot") {
            Some(slot) => held.push(from_value::<ContainerSlot>(slot.clone())?),
            None => unassigned.push(id.clone()),
        }
    }
    unassigned.sort();
    for id in unassigned {
        let slot = ContainerSlot::lowest_free(held.iter().copied())?;
        held.push(slot);
        packages
            .get_mut(&id)
            .and_then(Value::as_object_mut)
            .or_not_found(&id)?
            .insert("containerSlot".into(), to_value(&slot)?);
    }
    Ok(())
}

/// Switches on the public IPs grouped with each enabled plain public domain of a range.
/// The domain's forward already exposes them; earlier versions left them off after a renumber.
fn group_enabled_plain_domains(addresses: &mut Value) {
    let Ok(mut info) = from_value::<DerivedAddressInfo>(addresses.clone()) else {
        return;
    };
    let groups: Vec<_> = info
        .enabled()
        .into_iter()
        .filter(|a| !a.ssl)
        .filter_map(|a| match (&a.metadata, a.port) {
            (HostnameMetadata::PublicDomain { gateway }, Some(port)) => {
                Some((gateway.clone(), port))
            }
            _ => None,
        })
        .collect();
    if groups.is_empty() {
        return;
    }
    for (gateway, port) in groups {
        set_nonssl_wan_group(&mut info, &gateway, port, true);
    }
    if let Ok(value) = to_value(&info) {
        *addresses = value;
    }
}

#[cfg(test)]
mod tests {
    use imbl_value::json;

    use super::*;

    fn row(hostname: &str, public: bool, metadata: Value) -> Value {
        json!({
            "ssl": false,
            "public": public,
            "hostname": hostname,
            "port": 40000,
            "metadata": metadata,
        })
    }

    fn migrated(addresses: Value) -> Value {
        let mut db = json!({
            "public": {
                "serverInfo": { "network": { "host": { "bindings": {} } } },
                "packageData": { "service": { "hosts": { "p2p": {
                    "bindings": {},
                    "bindingRanges": { "28332": { "addresses": addresses } }
                } } } }
            }
        });
        Version.up(&mut db, ()).unwrap();
        db["public"]["packageData"]["service"]["hosts"]["p2p"]["bindingRanges"]["28332"]
            ["addresses"]
            .clone()
    }

    fn range_on(addresses: Value) -> Vec<String> {
        let info: DerivedAddressInfo = from_value(addresses).unwrap();
        info.enabled()
            .into_iter()
            .filter(|a| a.public)
            .map(|a| match a.to_socket_addr() {
                Some(sa) => sa.to_string(),
                None => a.hostname.to_string(),
            })
            .collect()
    }

    fn renumbered(domain_disabled: bool) -> Value {
        let disabled = if domain_disabled {
            json!([["smp.example.com", 40000]])
        } else {
            json!([])
        };
        json!({
            "enabled": ["198.51.100.2:40000"],
            "disabled": disabled,
            "available": [
                row("203.0.113.9", true, json!({ "kind": "ipv4", "gateway": "wg-in" })),
                row(
                    "smp.example.com",
                    true,
                    json!({ "kind": "public-domain", "gateway": "wg-in" }),
                ),
            ],
        })
    }

    #[test]
    fn a_renumbered_range_keeps_its_enabled_plain_domain_and_turns_its_ip_on() {
        let addresses = migrated(renumbered(false));
        assert_eq!(
            range_on(addresses.clone()),
            ["203.0.113.9:40000", "smp.example.com"]
        );
        assert_eq!(migrated(addresses.clone()), addresses);
    }

    #[test]
    fn a_range_whose_plain_domain_is_off_keeps_its_ip_off() {
        assert!(range_on(migrated(renumbered(true))).is_empty());
    }

    #[test]
    fn installed_packages_receive_distinct_slots_in_id_order() {
        let mut db = imbl_value::json!({
            "public": { "packageData": { "lnd": {}, "bitcoind": {}, "electrs": {} } }
        });
        Version.up(&mut db, ()).unwrap();
        let slot = |id: &str| db["public"]["packageData"][id]["containerSlot"].clone();
        assert_eq!(slot("bitcoind"), imbl_value::json!(0));
        assert_eq!(slot("electrs"), imbl_value::json!(1));
        assert_eq!(slot("lnd"), imbl_value::json!(2));
        Version.down(&mut db).unwrap();
        assert_eq!(db["public"]["packageData"]["lnd"], imbl_value::json!({}));
    }

    #[test]
    fn a_rerun_keeps_assigned_slots_and_fills_only_the_missing() {
        let mut db = imbl_value::json!({
            "public": { "packageData": {
                "bitcoind": { "containerSlot": 1 },
                "electrs": {},
                "lnd": { "containerSlot": 0 },
                "mempool": {}
            } }
        });
        Version.up(&mut db, ()).unwrap();
        Version.up(&mut db, ()).unwrap();
        let slot = |id: &str| db["public"]["packageData"][id]["containerSlot"].clone();
        assert_eq!(slot("lnd"), imbl_value::json!(0));
        assert_eq!(slot("bitcoind"), imbl_value::json!(1));
        assert_eq!(slot("electrs"), imbl_value::json!(2));
        assert_eq!(slot("mempool"), imbl_value::json!(3));
    }
}
