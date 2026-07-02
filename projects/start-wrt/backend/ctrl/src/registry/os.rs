use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::asset::RegistryAsset;
use crate::sign::commitment::Blake3Commitment;

/// Signature context for Start9 registry assets.
/// Same as start-os: all asset signatures use this domain separator.
pub const SIG_CONTEXT: &str = "startos";

/// Information about a single OS version from the registry.
/// Wire-compatible with start-os's `OsVersionInfo`.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OsVersionInfo {
    pub headline: String,
    pub release_notes: String,
    pub source_version: String,
    /// Platform → asset maps. The registry returns all three; we use whichever
    /// contains our platform's sysupgrade binary.
    #[serde(default)]
    pub iso: BTreeMap<String, RegistryAsset<Blake3Commitment>>,
    #[serde(default)]
    pub squashfs: BTreeMap<String, RegistryAsset<Blake3Commitment>>,
    #[serde(default)]
    pub img: BTreeMap<String, RegistryAsset<Blake3Commitment>>,
}

impl OsVersionInfo {
    /// Find the asset for a given platform, checking all asset type maps.
    /// Prefers squashfs, then img, then iso.
    pub fn asset_for_platform(
        &self,
        platform: &str,
    ) -> Option<&RegistryAsset<Blake3Commitment>> {
        self.squashfs
            .get(platform)
            .or_else(|| self.img.get(platform))
            .or_else(|| self.iso.get(platform))
    }
}
