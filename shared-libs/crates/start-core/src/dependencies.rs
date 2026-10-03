use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use visit_rs::ts::TS;

use crate::db::model::package::CurrentDependencyKind;
use crate::prelude::*;
use crate::s9pk::manifest::LocaleString;
use crate::util::PathOrUrl;
use crate::{Error, PackageId};

#[derive(Clone, Debug, Default, Deserialize, Serialize, HasModel, TS)]
#[model = "Model<Self>"]
#[ts(export)]
pub struct Dependencies(pub BTreeMap<PackageId, DepInfo>);
impl Map for Dependencies {
    type Key = PackageId;
    type Value = DepInfo;
    fn key_str(key: &Self::Key) -> Result<impl AsRef<str>, Error> {
        Ok(key)
    }
    fn key_string(key: &Self::Key) -> Result<imbl_value::InternedString, Error> {
        Ok(key.clone().into())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export)]
pub struct DepInfo {
    pub description: Option<LocaleString>,
    pub optional: bool,
    #[serde(default)]
    pub version_range: Option<exver::VersionRange>,
    #[serde(flatten)]
    pub kind: Option<CurrentDependencyKind>,
    #[serde(flatten)]
    pub metadata: Option<MetadataSrc>,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MetadataSrc {
    Metadata(Metadata),
    S9pk(Option<PathOrUrl>), // backwards compatibility
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Metadata {
    pub title: LocaleString,
    pub icon: PathOrUrl,
}

#[derive(Clone, Debug, Deserialize, Serialize, HasModel, TS)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[ts(export, rename = "ServiceDependencyMetadata")]
pub struct DependencyMetadata {
    pub title: LocaleString,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_dependency_has_unknown_range() {
        let dep: DepInfo = serde_json::from_str(
            r#"{"description":null,"optional":false,"metadata":{"title":"Bitcoin","icon":"https://example.com/icon.png"}}"#,
        )
        .unwrap();
        assert!(dep.version_range.is_none());
        assert!(dep.kind.is_none());
    }

    #[test]
    fn published_dependency_range_round_trips() {
        let dep: DepInfo = serde_json::from_str(
            r#"{"description":null,"optional":false,"versionRange":">=31.1:17","kind":"running","healthChecks":["bitcoind"],"metadata":{"title":"Bitcoin","icon":"https://example.com/icon.png"}}"#,
        )
        .unwrap();
        assert_eq!(dep.version_range.unwrap().to_string(), ">=31.1:17");
        assert!(
            matches!(dep.kind, Some(CurrentDependencyKind::Running { health_checks }) if health_checks.contains(&"bitcoind".parse().unwrap()))
        );
    }
}
