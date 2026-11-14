use exver::VersionRange;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_2_2};
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
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        let info = server_info(db)?;
        let seeding = info.remove("registrySeeding").map(from_value).transpose()?;
        let preferences: crate::registry::bittorrent::TorrentPreferences =
            match info.get("bittorrent") {
                Some(preferences) => from_value(preferences.clone())?,
                None => crate::registry::bittorrent::TorrentPreferences::default()
                    .update(seeding, seeding),
            };
        info.insert("bittorrent".into(), to_value(&preferences)?);
        Ok(Value::Null)
    }
    fn down(self, db: &mut Value) -> Result<(), Error> {
        let info = server_info(db)?;
        info.remove("bittorrent");
        info.remove("registrySeeding");
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
    fn migration_preserves_previous_sharing_intent() {
        for enabled in [false, true] {
            let mut db =
                imbl_value::json!({"public": {"serverInfo": {"registrySeeding": enabled}}});
            Version.up(&mut db, ()).unwrap();
            assert_eq!(
                db["public"]["serverInfo"]["bittorrent"],
                imbl_value::json!({"downloads": enabled, "seeding": enabled})
            );
            assert!(db["public"]["serverInfo"].get("registrySeeding").is_none());
        }
    }

    #[test]
    fn rerunning_migration_preserves_current_preferences() {
        for preferences in [
            imbl_value::json!({"downloads": false, "seeding": false}),
            imbl_value::json!({"downloads": true, "seeding": false}),
            imbl_value::json!({"downloads": true, "seeding": true}),
        ] {
            let mut db = imbl_value::json!({"public": {"serverInfo": {
                "bittorrent": preferences
            }}});
            let original = db.clone();
            Version.up(&mut db, ()).unwrap();
            assert_eq!(db, original);
        }
    }

    #[test]
    fn obsolete_incoming_switch_is_not_retained_as_an_ingress_opt_in() {
        let mut db = imbl_value::json!({"public": {"serverInfo": {
            "bittorrent": {"downloads": true, "seeding": true, "incoming": true}
        }}});
        Version.up(&mut db, ()).unwrap();
        assert_eq!(
            db["public"]["serverInfo"]["bittorrent"],
            imbl_value::json!({"downloads": true, "seeding": true})
        );
    }

    #[test]
    fn downloads_start_disabled_and_downgrade_removes_the_preferences() {
        let mut db = imbl_value::json!({"public": {"serverInfo": {"hostname": "server"}}});
        let original = db.clone();
        Version.up(&mut db, ()).unwrap();
        assert_eq!(
            db["public"]["serverInfo"]["bittorrent"],
            imbl_value::json!({"downloads": false, "seeding": false})
        );
        Version.down(&mut db).unwrap();
        assert_eq!(db, original);
    }
}
