use exver::VersionRange;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{Current, VersionT, v0_4_0_alpha_0, v0_4_0_beta_10};
use crate::context::RpcContext;
use crate::notifications::{NotificationLevel, notify};
use crate::prelude::*;

lazy_static::lazy_static! {
    static ref V0_4_0: exver::Version = exver::Version::new([0, 4, 0], []);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_0_beta_10::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_0.clone()
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    #[instrument(skip_all)]
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        // Stabilization release: no migration. The flag — "came from a 0.4.0
        // pre-release below beta.10" — is stashed for `post_up`'s welcome
        // routing, which lacks the mid-migration db.
        Ok(Value::Bool(
            !migrated_from_pre_0_4_0(db) && migrated_through_beta_10(db),
        ))
    }
    async fn post_up(self, ctx: &RpcContext, input: Value) -> Result<(), Error> {
        // `input` is `up`'s came-from-a-0.4.0-beta flag.
        if should_welcome_to_release(self, input.as_bool().unwrap_or(false)) {
            let highlights = include_str!("update_details/v0_4_0_highlights.md").to_string();
            ctx.db
                .mutate(|db| {
                    notify(
                        db,
                        None,
                        NotificationLevel::Success,
                        "Welcome to stable StartOS 0.4.0!".to_string(),
                        "Click \"View Details\" for the highlights — including important changes to backups and sign-in.".to_string(),
                        highlights,
                    )?;
                    Ok(())
                })
                .await
                .result?;
        }
        Ok(())
    }
    fn down(self, _db: &mut Value) -> Result<(), Error> {
        Ok(())
    }
}

/// True when this run has migrated a version <= `0.4.0-alpha.0`, i.e. the server came
/// from a pre-0.4.0 release. Reads `postInitMigrationTodos`, which `commit` fills as the
/// run progresses, so it must be called from `up` (before `post_init` drains it).
fn migrated_from_pre_0_4_0(db: &Value) -> bool {
    let floor = v0_4_0_alpha_0::Version.semver();
    db["public"]["serverInfo"]["postInitMigrationTodos"]
        .as_object()
        .into_iter()
        .flat_map(|todos| todos.iter())
        .filter_map(|(k, _)| (&**k).parse::<exver::Version>().ok())
        .any(|v| v <= floor)
}

/// True when this run has migrated `0.4.0-beta.10` — its key is committed only when the
/// source version is below it, so a direct beta.10 -> 0.4.0 hop (todos still empty at
/// `up` time) stays false. Like `migrated_from_pre_0_4_0`, must be called from `up`.
fn migrated_through_beta_10(db: &Value) -> bool {
    let beta_10 = v0_4_0_beta_10::Version.semver();
    db["public"]["serverInfo"]["postInitMigrationTodos"]
        .as_object()
        .into_iter()
        .flat_map(|todos| todos.iter())
        .filter_map(|(k, _)| (&**k).parse::<exver::Version>().ok())
        .any(|v| v == beta_10)
}

/// 0.4.0's welcome fires only when 0.4.0 is the release being landed on (the current
/// head, so an intermediate hop in a multi-version jump stays silent) and the server
/// came from a 0.4.0 pre-release below beta.10: pre-0.4.0 arrivals get
/// `v0_4_0_alpha_0`'s welcome (`update_details/v0_4_0.md`) instead, and a beta.10
/// server already has everything `update_details/v0_4_0_highlights.md` describes.
/// `from_0_4_0_beta` is the flag threaded through `up`'s output.
fn should_welcome_to_release(version: impl VersionT, from_0_4_0_beta: bool) -> bool {
    version.semver() == Current::default().semver() && from_0_4_0_beta
}

#[cfg(test)]
mod test {
    use imbl_value::json;

    use super::*;

    #[test]
    fn welcome_routing() {
        let todos = |v| json!({ "public": { "serverInfo": { "postInitMigrationTodos": v } } });

        assert!(!migrated_from_pre_0_4_0(&todos(json!({})))); // empty at up() time
        assert!(!migrated_from_pre_0_4_0(
            &json!({ "public": { "serverInfo": {} } })
        ));
        assert!(!migrated_from_pre_0_4_0(&todos(
            json!({ "0.4.0-alpha.6": null, "0.4.0-beta.9": null })
        )));
        assert!(migrated_from_pre_0_4_0(&todos(
            json!({ "0.3.5.2": null, "0.4.0-alpha.0": null })
        )));
        // boundary: the last 0.3.x release still commits the alpha.0 key
        assert!(migrated_from_pre_0_4_0(&todos(
            json!({ "0.4.0-alpha.0": null, "0.4.0-alpha.1": null })
        )));

        // beta.10's key is committed only when the source is below it
        assert!(!migrated_through_beta_10(&todos(json!({})))); // direct beta.10 -> 0.4.0 hop
        assert!(!migrated_through_beta_10(
            &json!({ "public": { "serverInfo": {} } })
        ));
        assert!(migrated_through_beta_10(&todos(
            json!({ "0.4.0-beta.10": null })
        )));

        // from beta.9: welcome. from beta.10: silent. from 0.3.x: alpha_0's welcome instead.
        let from_beta_9 = todos(json!({ "0.4.0-beta.10": null }));
        assert!(!migrated_from_pre_0_4_0(&from_beta_9) && migrated_through_beta_10(&from_beta_9));
        let from_0_3_x = todos(
            json!({ "0.3.5.2": null, "0.4.0-alpha.0": null, "0.4.0-beta.9": null, "0.4.0-beta.10": null }),
        );
        assert!(migrated_from_pre_0_4_0(&from_0_3_x));

        // 0.4.0 is the landing release: welcome only for 0.4.0-beta arrivals.
        assert!(should_welcome_to_release(Version, true));
        assert!(!should_welcome_to_release(Version, false));
        // an intermediate (non-landing) release stays silent, even from the 0.4.0 line
        assert!(!should_welcome_to_release(v0_4_0_beta_10::Version, true));
    }
}
