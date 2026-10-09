use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::db::model::package::CurrentDependencyKind;
use crate::prelude::*;
use crate::s9pk::manifest::LocaleString;
use crate::util::PathOrUrl;
use crate::{Error, PackageId};

#[derive(Clone, Debug, Default, Deserialize, Serialize, HasModel, VisitFields)]
#[model = "Model<Self>"]
pub struct Dependencies(pub BTreeMap<PackageId, DepInfo>);

rpc_toolkit::reflect_ts!(Dependencies);
rpc_toolkit::ts_export!(Dependencies, namespaces = [""]);
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

#[derive(Clone, Debug, Deserialize, Serialize, HasModel, VisitFields)]
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

rpc_toolkit::reflect_ts!(DepInfo);
rpc_toolkit::ts_export!(DepInfo, namespaces = [""]);

#[derive(Clone, Debug, Deserialize, Serialize, VisitVariants)]
#[serde(rename_all = "camelCase")]
pub enum MetadataSrc {
    Metadata(Metadata),
    S9pk(Option<PathOrUrl>), // backwards compatibility
}

rpc_toolkit::reflect_ts!(MetadataSrc);
rpc_toolkit::ts_export!(MetadataSrc, namespaces = [""]);

#[derive(Clone, Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub title: LocaleString,
    pub icon: PathOrUrl,
}

rpc_toolkit::reflect_ts!(Metadata);
rpc_toolkit::ts_export!(Metadata, namespaces = [""]);

#[derive(Clone, Debug, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
#[visit(ts(rename = "ServiceDependencyMetadata"))]
pub struct DependencyMetadata {
    pub title: LocaleString,
}

rpc_toolkit::reflect_ts!(DependencyMetadata);
rpc_toolkit::ts_export!(DependencyMetadata, namespaces = [""]);

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
