use super::{Current, VersionT};
use crate::context::RpcContext;
use crate::notifications::{NotificationLevel, notify};
use crate::prelude::*;

/// This build's release notes, the same file the GitHub release and the registry
/// entry the update screen reads are composed from.
const NOTES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../projects/start-os/release-notes/",
    env!("STARTOS_VERSION"),
    ".md"
));

/// Welcome a server to the version it just landed on. The link is appended
/// rather than placed inside `## Highlights` as the GitHub body places it: a
/// notification renders as one document, so the bottom is the bottom.
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
                format!("Welcome to StartOS {version}!"),
                "Click \"View Details\" for what's new in this release.".to_string(),
                body,
            )?;
            Ok(())
        })
        .await
        .result
}
