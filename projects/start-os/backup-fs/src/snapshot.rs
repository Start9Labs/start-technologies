use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::path::Path;

use chacha20::Key;
use serde::{Deserialize, Serialize};

use crate::atomic_file::AtomicFile;
use crate::error::{BkfsError, BkfsResult};
use crate::objects::ObjectId;
use crate::seglog::Roots;

pub(crate) const COW_FORMAT_VERSION: u32 = 3;

/// A manually retained, named read-only recovery point.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub name: String,
    pub created_unix: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Snapshot {
    pub info: SnapshotInfo,
    pub roots: Roots,
    pub legacy: HashSet<ObjectId>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Catalog {
    version: u32,
    pub entries: BTreeMap<String, Snapshot>,
}

impl Catalog {
    pub fn load(data: &Path, key: &Key) -> BkfsResult<Self> {
        let bytes = match std::fs::read(data.join("snapshots")) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    version: 1,
                    entries: BTreeMap::new(),
                })
            }
            Err(e) => return Err(e.into()),
        };
        let catalog: Self = crate::serde::deserialize_sealed(&bytes, key)?;
        if catalog.version != 1 {
            return Err(BkfsError::unsupported("unsupported snapshot catalog"));
        }
        Ok(catalog)
    }

    pub fn persist(&self, data: &Path, key: &Key, ecc: crate::vault::EccParams) -> BkfsResult<()> {
        let bytes = crate::serde::serialize_sealed(self, key, ecc)?;
        let _: Self = crate::serde::deserialize_sealed(&bytes, key)?;
        let mut file = AtomicFile::create_buffered(data.join("snapshots"))?;
        file.write_all(&bytes)?;
        file.save()?;
        File::open(data)?.sync_all()?;
        Ok(())
    }

    pub fn pins(&self) -> HashSet<u64> {
        self.entries
            .values()
            .flat_map(|entry| entry.roots.segments())
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
