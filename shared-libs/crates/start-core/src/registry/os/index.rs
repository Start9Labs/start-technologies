use std::collections::{BTreeMap, BTreeSet};

use exver::{Version, VersionRange};
use imbl_value::InternedString;
use serde::{Deserialize, Serialize};

use crate::prelude::*;
use crate::registry::asset::RegistryAsset;
use crate::registry::context::RegistryContext;
use crate::rpc_continuations::Guid;
use crate::sign::commitment::blake3::Blake3Commitment;

#[derive(Debug, Default, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct OsIndex {
    pub versions: OsVersionInfoMap,
}

rpc_toolkit::reflect_ts!(OsIndex);
rpc_toolkit::ts_export!(OsIndex, namespaces = [""]);

#[derive(Debug, Default, Deserialize, Serialize, VisitFields)]
pub struct OsVersionInfoMap(pub BTreeMap<Version, OsVersionInfo>);

rpc_toolkit::reflect_ts!(OsVersionInfoMap);
impl Map for OsVersionInfoMap {
    type Key = Version;
    type Value = OsVersionInfo;
    fn key_str(key: &Self::Key) -> Result<impl AsRef<str>, Error> {
        Ok(InternedString::from_display(key))
    }
    fn key_string(key: &Self::Key) -> Result<InternedString, Error> {
        Ok(InternedString::from_display(key))
    }
}

#[derive(Debug, Default, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct OsVersionInfo {
    pub headline: String,
    pub release_notes: String,
    /// Locale Markdown excluding `en_US`; missing languages fall back to `release_notes`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub release_notes_translations: BTreeMap<InternedString, String>,
    pub source_version: VersionRange,
    pub authorized: BTreeSet<Guid>,
    pub iso: BTreeMap<InternedString, RegistryAsset<Blake3Commitment>>, // platform (i.e. x86_64-nonfree) -> asset
    pub squashfs: BTreeMap<InternedString, RegistryAsset<Blake3Commitment>>, // platform (i.e. x86_64-nonfree) -> asset
    pub img: BTreeMap<InternedString, RegistryAsset<Blake3Commitment>>, // platform (i.e. raspberrypi) -> asset
}

rpc_toolkit::reflect_ts!(OsVersionInfo);
rpc_toolkit::ts_export!(OsVersionInfo, namespaces = [""]);

impl OsVersionInfo {
    pub(super) fn set_release_notes(
        &mut self,
        english: String,
        translations: Option<BTreeMap<InternedString, String>>,
    ) -> Result<(), Error> {
        let translations = translations.unwrap_or_default();
        if translations.contains_key("en_US") {
            return Err(Error::new(
                eyre!(
                    "{}",
                    t!("registry.os.version.english-notes-in-translations")
                ),
                ErrorKind::InvalidRequest,
            ));
        }
        self.release_notes = english;
        self.release_notes_translations = translations;
        Ok(())
    }
}

pub async fn get_os_index(ctx: RegistryContext) -> Result<OsIndex, Error> {
    ctx.db.peek().await.into_index().into_os().de()
}

#[cfg(test)]
mod tests {
    use imbl_value::json;

    use super::*;
    use crate::registry::os::version::AddVersionParams;

    #[test]
    fn english_only_notes_round_trip_without_a_schema_rewrite() {
        let legacy = json!({
            "headline": "v0.4.0.5",
            "releaseNotes": "English notes",
            "sourceVersion": "*",
            "authorized": [],
            "iso": {},
            "squashfs": {},
            "img": {},
        });
        let info: OsVersionInfo = from_value(legacy.clone()).unwrap();
        assert!(info.release_notes_translations.is_empty());
        assert_eq!(to_value(&info).unwrap(), legacy);
        let promotion = AddVersionParams::from_info("0.4.0.5".parse().unwrap(), &info);
        assert_eq!(promotion.release_notes, "English notes");
        assert!(promotion.release_notes_translations.unwrap().is_empty());
    }

    #[test]
    fn release_notes_serve_all_translations_and_remain_readable_by_legacy_clients() {
        let mut info = OsVersionInfo::default();
        info.set_release_notes(
            "English notes".into(),
            Some(BTreeMap::from([
                ("de_DE".into(), "Deutsche Hinweise".into()),
                ("pl_PL".into(), "Polskie informacje".into()),
            ])),
        )
        .unwrap();
        let wire = to_value(&info).unwrap();
        assert_eq!(wire["releaseNotes"].as_str(), Some("English notes"));
        assert_eq!(
            wire["releaseNotesTranslations"],
            json!({ "de_DE": "Deutsche Hinweise", "pl_PL": "Polskie informacje" })
        );

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct LegacyVersionInfo {
            release_notes: String,
        }
        let legacy: LegacyVersionInfo = from_value(wire.clone()).unwrap();
        assert_eq!(legacy.release_notes, "English notes");
        let restored: OsVersionInfo = from_value(wire).unwrap();
        let promotion = AddVersionParams::from_info("0.4.0.5".parse().unwrap(), &restored);
        let wire = to_value(&promotion).unwrap();
        assert_eq!(wire["releaseNotes"].as_str(), Some("English notes"));
        assert_eq!(
            wire["releaseNotesTranslations"],
            json!({ "de_DE": "Deutsche Hinweise", "pl_PL": "Polskie informacje" })
        );
    }

    #[test]
    fn replacing_notes_replaces_translations_without_duplicating_english() {
        let mut info = OsVersionInfo::default();
        info.set_release_notes(
            "English".into(),
            Some(BTreeMap::from([("pl_PL".into(), "Polski".into())])),
        )
        .unwrap();
        let before = to_value(&info).unwrap();
        let error = info
            .set_release_notes(
                "Changed English".into(),
                Some(BTreeMap::from([(
                    "en_US".into(),
                    "Conflicting English".into(),
                )])),
            )
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidRequest);
        assert_eq!(to_value(&info).unwrap(), before);
        info.set_release_notes("Changed English".into(), None)
            .unwrap();
        assert_eq!(info.release_notes, "Changed English");
        assert!(info.release_notes_translations.is_empty());
    }
}
