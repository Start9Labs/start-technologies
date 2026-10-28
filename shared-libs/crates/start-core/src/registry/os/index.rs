use std::collections::{BTreeMap, BTreeSet};

use exver::{Version, VersionRange};
use imbl_value::InternedString;
use serde::{Deserialize, Serialize};

use crate::prelude::*;
use crate::registry::asset::RegistryAsset;
use crate::registry::context::RegistryContext;
use crate::rpc_continuations::Guid;
use crate::s9pk::git_hash::GitHash;
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
    pub source_version: VersionRange,
    /// Commit that produced the OS image, if recorded at publication.
    #[serde(default)]
    pub git_hash: Option<GitHash>,
    /// Whether the release includes security fixes.
    #[serde(default)]
    pub security: bool,
    pub authorized: BTreeSet<Guid>,
    pub iso: BTreeMap<InternedString, RegistryAsset<Blake3Commitment>>,
    pub squashfs: BTreeMap<InternedString, RegistryAsset<Blake3Commitment>>,
    pub img: BTreeMap<InternedString, RegistryAsset<Blake3Commitment>>,
}

impl OsVersionInfo {
    pub(crate) fn require_git_hash(&self, expected: &GitHash) -> Result<(), Error> {
        ensure_code!(
            self.git_hash.as_ref() == Some(expected),
            ErrorKind::InvalidRequest,
            "{}",
            t!("registry.os.commit-changed")
        );
        Ok(())
    }
}

rpc_toolkit::reflect_ts!(OsVersionInfo);
rpc_toolkit::ts_export!(OsVersionInfo, namespaces = [""]);

pub async fn get_os_index(ctx: RegistryContext) -> Result<OsIndex, Error> {
    ctx.db.peek().await.into_index().into_os().de()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_metadata_updates_require_the_expected_image_commit() {
        let expected = "expected-build".parse().unwrap();
        let mut info = OsVersionInfo::default();
        assert!(info.require_git_hash(&expected).is_err());
        info.git_hash = Some("other-build".parse().unwrap());
        assert!(info.require_git_hash(&expected).is_err());
        info.git_hash = Some(expected.clone());
        info.require_git_hash(&expected).unwrap();
    }
}
