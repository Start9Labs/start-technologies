use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{Current, VersionT};
use crate::context::RpcContext;
use crate::notifications::{NotificationLevel, notify};
use crate::prelude::*;
use crate::s9pk::v2::manifest::LocaleString;
use crate::util::io::{delete_file, maybe_read_file_to_string, write_file_atomic};

const UPDATE_NOTES: &str = "/media/startos/images/release-notes.json";
const PRE_UPDATE_START: &str = "<!-- pre-update -->";
const PRE_UPDATE_END: &str = "<!-- /pre-update -->";

/// Registry release notes for the image an update installed.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNotes {
    pub version: exver::Version,
    pub release_notes: LocaleString,
}

pub async fn save_update_notes(notes: &UpdateNotes) -> Result<(), Error> {
    write_update_notes(Path::new(UPDATE_NOTES), notes).await
}

async fn write_update_notes(path: &Path, notes: &UpdateNotes) -> Result<(), Error> {
    write_file_atomic(
        path,
        serde_json::to_vec(notes).with_kind(ErrorKind::Serialization)?,
    )
    .await
}

async fn read_update_notes(
    path: &Path,
    version: &exver::Version,
    locale: &str,
) -> Result<Option<String>, Error> {
    let Some(data) = maybe_read_file_to_string(path).await? else {
        return Ok(None);
    };
    let notes: UpdateNotes = serde_json::from_str(&data).with_kind(ErrorKind::Deserialization)?;
    if &notes.version != version {
        return Ok(None);
    }
    let notes = without_pre_update(&notes.release_notes.localized_for(locale));
    Ok((!notes.trim().is_empty()).then_some(notes))
}

/// Leaves notes with an unterminated pre-update section unchanged.
fn without_pre_update(notes: &str) -> String {
    let Some(start) = notes.find(PRE_UPDATE_START) else {
        return notes.to_owned();
    };
    let Some(end) = notes[start..].find(PRE_UPDATE_END) else {
        return notes.to_owned();
    };
    let rest = &notes[start + end + PRE_UPDATE_END.len()..];
    format!("{}{}", &notes[..start], rest.trim_start())
}

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

async fn welcome_body(
    update_notes: &Path,
    packaged: &Path,
    version: &exver::Version,
    locale: &str,
) -> Option<String> {
    if let Some(notes) = read_update_notes(update_notes, version, locale)
        .await
        .log_err()
        .flatten()
    {
        return Some(notes);
    }
    let notes = read_notes(packaged, locale).await.log_err()?;
    Some(notification_body(&notes, version, locale))
}

pub async fn welcome(ctx: &RpcContext) -> Result<(), Error> {
    let locale = rust_i18n::locale();
    let version = Current::default().semver();
    let body = welcome_body(
        Path::new(UPDATE_NOTES),
        Path::new("/usr/lib/startos"),
        &version,
        &locale,
    )
    .await;
    let Some(body) = body else {
        delete_file(UPDATE_NOTES).await.log_err();
        return Ok(());
    };
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
        .result?;
    delete_file(UPDATE_NOTES).await.log_err();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_update_section_is_removed() {
        let notes = "<!-- pre-update -->\n\n## ⚠️ Before You Update\n\n> Back up.\n\n<!-- /pre-update -->\n\n## Highlights\n\n- New.\n";
        assert_eq!(without_pre_update(notes), "## Highlights\n\n- New.\n");
        assert_eq!(without_pre_update("## Highlights\n"), "## Highlights\n");
        let unterminated = "<!-- pre-update -->\n\n## ⚠️ Before You Update\n";
        assert_eq!(without_pre_update(unterminated), unterminated);
    }

    #[tokio::test]
    async fn saved_registry_notes_replace_packaged_notes() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let saved = root.join("saved.json");
        std::fs::write(root.join("release-notes.md"), "Packaged highlights").unwrap();
        let version = exver::Version::new([0, 4, 0, 5], []);
        let release_notes: LocaleString = serde_json::from_value(serde_json::json!({
            "en_US": "<!-- pre-update -->\n\nWarning.\n\n<!-- /pre-update -->\n\nEnglish notes.\n\n**[Full changelog](https://example.com)**\n",
            "pl_PL": "Polskie notatki.\n",
        }))
        .unwrap();
        write_update_notes(
            &saved,
            &UpdateNotes {
                version: version.clone(),
                release_notes,
            },
        )
        .await
        .unwrap();

        let english = welcome_body(&saved, root, &version, "en_US").await.unwrap();
        assert_eq!(
            english,
            "English notes.\n\n**[Full changelog](https://example.com)**\n"
        );
        assert_eq!(
            welcome_body(&saved, root, &version, "pl_PL").await.unwrap(),
            "Polskie notatki.\n"
        );

        let other = exver::Version::new([0, 4, 0, 6], []);
        let fallback = welcome_body(&saved, root, &other, "en_US").await.unwrap();
        assert!(fallback.starts_with("Packaged highlights\n\n**["));
        assert!(
            welcome_body(&root.join("missing.json"), root, &version, "en_US")
                .await
                .unwrap()
                .starts_with("Packaged highlights")
        );
    }

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
