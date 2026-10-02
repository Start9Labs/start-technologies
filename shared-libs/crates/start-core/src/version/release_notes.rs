use super::{Current, VersionT};
use crate::context::RpcContext;
use crate::notifications::{NotificationLevel, notify};
use crate::prelude::*;

const RELEASE_NOTES_PATH: &str = "/usr/lib/startos/release-notes.md";
const PRE_UPDATE_HEADING: &str = "## ⚠️ Before You Update";

fn without_pre_update(notes: &str) -> String {
    let mut skip = false;
    notes
        .lines()
        .filter(|line| {
            if line.starts_with("## ") {
                skip = line.trim_end() == PRE_UPDATE_HEADING;
            }
            !skip
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub async fn welcome(ctx: &RpcContext) -> Result<(), Error> {
    let Some(notes) = crate::util::io::read_file_to_string(RELEASE_NOTES_PATH)
        .await
        .log_err()
    else {
        return Ok(());
    };
    let notes = without_pre_update(&notes);
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

#[cfg(test)]
mod test {
    use super::without_pre_update;

    #[test]
    fn drops_only_the_pre_update_section() {
        let notes = "Lede.\n\n## ⚠️ Before You Update\n\n> Warning.\n\n## Highlights\n\n- One\n\n## Important\n\nTwo";
        assert_eq!(
            without_pre_update(notes),
            "Lede.\n\n## Highlights\n\n- One\n\n## Important\n\nTwo"
        );
    }
}
