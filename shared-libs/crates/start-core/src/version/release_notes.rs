use super::{Current, VersionT};
use crate::context::RpcContext;
use crate::notifications::{NotificationLevel, notify};
use crate::prelude::*;

const NOTES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../projects/start-os/release-notes/",
    env!("STARTOS_VERSION"),
    ".md"
));

pub async fn welcome(ctx: &RpcContext) -> Result<(), Error> {
    let version = Current::default().semver();
    let body = format!(
        "{NOTES}\n\n**[Full changelog for v{version}](https://github.com/Start9Labs/start-technologies/blob/start-os/v{version}/projects/start-os/CHANGELOG.md)** — every change in this release."
    );
    ctx.db
        .mutate(|db| {
            notify(
                db,
                None,
                NotificationLevel::Success,
                t!("release-notes.welcome-title", version = version.to_string()).to_string(),
                t!("release-notes.welcome-message").to_string(),
                body,
            )?;
            Ok(())
        })
        .await
        .result
}
