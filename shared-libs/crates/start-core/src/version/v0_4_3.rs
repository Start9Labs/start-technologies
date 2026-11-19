use exver::VersionRange;
use imbl_value::InternedString;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_2_2};
use crate::lxc::slot::ContainerSlot;
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
        1
    }
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        assign_container_slots(db)?;
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

#[cfg(test)]
mod tests {
    use super::*;

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
