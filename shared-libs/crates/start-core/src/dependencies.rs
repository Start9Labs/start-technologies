use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

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

#[derive(Clone, Debug, Deserialize, Serialize, HasModel)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
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
impl TS for DepInfo {
    type WithoutGenerics = Self;
    fn decl() -> String {
        format!("type {} = {}", Self::name(), Self::inline())
    }
    fn decl_concrete() -> String {
        Self::decl()
    }
    fn name() -> String {
        "DepInfo".into()
    }
    fn inline() -> String {
        "{ description: LocaleString | null, optional: boolean, versionRange?: string | null, kind?: 'exists' | 'running' | null, healthChecks?: string[] } & MetadataSrc".into()
    }
    fn inline_flattened() -> String {
        Self::inline()
    }
    fn visit_dependencies(v: &mut impl ts_rs::TypeVisitor)
    where
        Self: 'static,
    {
        v.visit::<MetadataSrc>();
        v.visit::<LocaleString>();
    }
    fn output_path() -> Option<&'static std::path::Path> {
        Some(Path::new("DepInfo.ts"))
    }
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
pub struct DependencyMetadata {
    #[ts(type = "string")]
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
