use std::path::Path;

use super::{Current, VersionT};
use crate::context::RpcContext;
use crate::notifications::{NotificationLevel, notify};
use crate::prelude::*;

async fn read_notes(directory: &Path, locale: &str) -> Result<String, Error> {
    if let Some(notes) = crate::util::io::maybe_read_file_to_string(
        directory.join("release-notes").join(format!("{locale}.md")),
    )
    .await?
    {
        return Ok(notes);
    }
    crate::util::io::read_file_to_string(directory.join("release-notes.md")).await
}

fn notification_body(notes: &str, version: &exver::Version, locale: &str) -> String {
    let footer = t!(
        "release-notes.full-changelog",
        locale = locale,
        version = version.to_string(),
        url = format!(
            "https://github.com/Start9Labs/start-technologies/releases/tag/start-os/v{version}"
        )
    );
    format!("{notes}\n\n{footer}")
}

pub async fn welcome(ctx: &RpcContext) -> Result<(), Error> {
    let locale = rust_i18n::locale();
    let Some(notes) = read_notes(Path::new("/usr/lib/startos"), &locale)
        .await
        .log_err()
    else {
        return Ok(());
    };
    let version = Current::default().semver();
    let body = notification_body(&notes, &version, &locale);
    ctx.db
        .mutate(|db| {
            notify(
                db,
                None,
                NotificationLevel::Success,
                t!(
                    "release-notes.welcome-title",
                    locale = &locale,
                    version = version.to_string()
                )
                .to_string(),
                t!("release-notes.welcome-message", locale = &locale).to_string(),
                body,
            )?;
            Ok(())
        })
        .await
        .result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn localized_notes_and_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::write(root.join("release-notes.md"), "English highlights").unwrap();
        assert_eq!(
            read_notes(root, "pl_PL").await.unwrap(),
            "English highlights"
        );
        std::fs::create_dir(root.join("release-notes")).unwrap();
        std::fs::write(root.join("release-notes/pl_PL.md"), "Polskie nowości").unwrap();

        assert_eq!(read_notes(root, "pl_PL").await.unwrap(), "Polskie nowości");
        for locale in ["en_US", "de_DE", "es_ES", "fr_FR"] {
            assert_eq!(
                read_notes(root, locale).await.unwrap(),
                "English highlights"
            );
        }

        std::fs::remove_file(root.join("release-notes.md")).unwrap();
        assert_eq!(read_notes(root, "pl_PL").await.unwrap(), "Polskie nowości");
        assert!(read_notes(root, "de_DE").await.is_err());
    }

    #[tokio::test]
    async fn localized_read_failure_is_not_missing_translation() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::create_dir_all(root.join("release-notes/pl_PL.md")).unwrap();
        std::fs::write(root.join("release-notes.md"), "English highlights").unwrap();
        assert!(read_notes(root, "pl_PL").await.is_err());
    }

    #[test]
    fn localized_footer() {
        let version = exver::Version::new([0, 4, 0, 5], []);
        for (locale, label) in [
            ("en_US", "Full changelog for v0.4.0.5"),
            ("de_DE", "Vollständiges Änderungsprotokoll für v0.4.0.5"),
            ("es_ES", "Registro completo de cambios de v0.4.0.5"),
            ("fr_FR", "Journal complet des modifications de la v0.4.0.5"),
            ("pl_PL", "Pełna lista zmian w wersji 0.4.0.5"),
        ] {
            let body = notification_body("Highlights", &version, locale);
            assert!(body.starts_with("Highlights\n\n**["));
            assert!(body.contains(label), "{locale}: {body}");
            assert!(body.contains(
                "](https://github.com/Start9Labs/start-technologies/releases/tag/start-os/v0.4.0.5)**"
            ));
        }
        let next_version = exver::Version::new([0, 4, 1], []);
        assert!(
            notification_body("Highlights", &next_version, "en_US").contains(
                "](https://github.com/Start9Labs/start-technologies/releases/tag/start-os/v0.4.1)**"
            )
        );
    }
}
