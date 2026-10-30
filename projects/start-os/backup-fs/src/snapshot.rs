use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use chacha20::Key;
use serde::{Deserialize, Serialize};

use crate::error::{BkfsError, BkfsResult};
use crate::metadata::Reference;
use crate::seglog::Roots;
use crate::state::View;
use crate::vault::{Binding, EccParams};

/// A manually retained, named read-only recovery point.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub name: String,
    pub created_unix: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Snapshot {
    pub info: SnapshotInfo,
    pub index: View,
    segments: HashSet<u64>,
}

impl Snapshot {
    pub fn new(info: SnapshotInfo, index: View, roots: &Roots) -> Self {
        Self {
            info,
            index,
            segments: roots.segments().collect(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Catalog {
    version: u32,
    pub entries: BTreeMap<String, Snapshot>,
}

impl Default for Catalog {
    fn default() -> Self {
        Self {
            version: 1,
            entries: BTreeMap::new(),
        }
    }
}

impl Catalog {
    pub fn load(segments: &Path, key: &Key, reference: Option<Reference>) -> BkfsResult<Self> {
        let Some(reference) = reference else {
            return Ok(Self::default());
        };
        let catalog: Self = reference.load(segments, key, Binding::Catalog)?;
        if catalog.version != 1 {
            return Err(BkfsError::unsupported("unsupported snapshot catalog"));
        }
        Ok(catalog)
    }
    pub fn write(&self, segments: &Path, key: &Key, ecc: EccParams) -> BkfsResult<Reference> {
        Reference::write(segments, key, ecc, Binding::Catalog, self)
    }
    pub fn views(&self) -> Vec<View> {
        self.entries
            .values()
            .map(|snapshot| snapshot.index)
            .collect()
    }
    pub fn pins(&self) -> HashSet<u64> {
        self.entries
            .values()
            .flat_map(|snapshot| snapshot.segments.iter().copied())
            .collect()
    }
}

pub(crate) fn validate_name(name: &str) -> BkfsResult<()> {
    if name.is_empty()
        || name.len() > 128
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        || name == "."
        || name == ".."
    {
        return Err(std::io::Error::from_raw_os_error(libc::EINVAL).into());
    }
    Ok(())
}
