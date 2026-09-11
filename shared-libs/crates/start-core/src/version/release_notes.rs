use super::{Current, VersionT};
use crate::context::RpcContext;
use crate::notifications::{NotificationLevel, notify};
use crate::prelude::*;

const RELEASE_NOTES_PATH: &str = "/usr/lib/startos/release-notes.md";

pub async fn welcome(ctx: &RpcContext) -> Result<(), Error> {
    let Some(notes) = crate::util::io::read_file_to_string(RELEASE_NOTES_PATH)
        .await
        .log_err()
    else {
        return Ok(());
    };
    let version = Current::default().semver();
    let body = format!(
        "{notes}\n\n**[Full changelog for v{version}](https://github.com/Start9Labs/start-technologies/blob/start-os/v{version}/projects/start-os/CHANGELOG.md)** — every change in this release."
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
