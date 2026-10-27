use exver::VersionRange;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_0_4};
use crate::prelude::*;

lazy_static::lazy_static! {
    static ref V0_4_0_5: exver::Version = exver::Version::new([0, 4, 0, 5], []);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_0_4::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_0_5.clone()
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn up(self, _db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        Ok(Value::Null)
    }
    fn down(self, _db: &mut Value) -> Result<(), Error> {
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use imbl_value::json;

    use super::*;
    use crate::version::{DynVersionT, PreUps, migrate_from_unchecked, rollback_to_unchecked};

    #[tokio::test]
    async fn revision_five_migrates_and_rolls_back_without_changing_service_data() {
        let previous = v0_4_0_4::Version;
        let current = Version;
        let registered = crate::version::Version::from_exver_version(current.semver())
            .as_version_t()
            .unwrap();
        assert_eq!(registered.previous().semver(), previous.semver());
        let mut db = json!({ "public": { "serverInfo": {
            "version": "0.4.0.4",
            "packageVersionCompat": ">=0.3.0 <0.5.0",
            "postInitMigrationTodos": {},
            "latestMigrationRevision": 0,
        }, "packageData": { "service": { "setting": "preserved" } } } });
        let packages = db["public"]["packageData"].clone();
        let pre_ups = PreUps::load(&previous, &registered).await.unwrap();
        migrate_from_unchecked(&previous, &registered, pre_ups, &mut db).unwrap();
        assert_eq!(db["public"]["serverInfo"]["version"], json!("0.4.0.5"));
        assert_eq!(
            db["public"]["serverInfo"]["postInitMigrationTodos"],
            json!({ "0.4.0.5": null })
        );
        assert_eq!(db["public"]["packageData"], packages);
        rollback_to_unchecked(&registered, &previous, &mut db).unwrap();
        assert_eq!(db["public"]["serverInfo"]["version"], json!("0.4.0.4"));
        assert_eq!(db["public"]["packageData"], packages);
    }
}
