use exver::VersionRange;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_1_2};
use crate::prelude::*;

lazy_static::lazy_static! {
    static ref V0_4_2: exver::Version = exver::Version::new([0, 4, 2], []);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_1_2::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_2.clone()
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        server_info(db)?.insert(
            "registrySeeding".into(),
            Value::Bool(crate::db::model::public::default_registry_seeding()),
        );
        Ok(Value::Null)
    }
    fn down(self, db: &mut Value) -> Result<(), Error> {
        server_info(db)?.remove("registrySeeding");
        Ok(())
    }
}

fn server_info(
    db: &mut Value,
) -> Result<&mut imbl_value::InOMap<imbl_value::InternedString, Value>, Error> {
    db.get_mut("public")
        .and_then(|public| public.get_mut("serverInfo"))
        .and_then(Value::as_object_mut)
        .or_not_found("serverInfo")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_seeding_starts_disabled_and_downgrade_removes_the_preference() {
        let mut db = imbl_value::json!({"public": {"serverInfo": {"hostname": "server"}}});
        let original = db.clone();
        Version.up(&mut db, ()).unwrap();
        assert_eq!(
            db["public"]["serverInfo"]["registrySeeding"],
            Value::Bool(false)
        );
        Version.down(&mut db).unwrap();
        assert_eq!(db, original);
    }
}
