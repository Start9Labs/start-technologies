use std::collections::HashSet;
use std::fs::File;
use std::path::{Path, PathBuf};

use chacha20::Key;
use rand::rand_core::UnwrapErr;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{BkfsError, BkfsResult};

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ObjectId {
    directory: bool,
    hash: [u8; 16],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ObjectVersion {
    Deleted,
    Version([u8; 16]),
}

impl ObjectVersion {
    pub fn new() -> Self {
        let mut bytes = [0; 16];
        UnwrapErr(rand::rng()).fill_bytes(&mut bytes);
        Self::Version(bytes)
    }
}

impl ObjectId {
    fn hash(key: &Key, directory: bool, domain: &[u8], parts: &[&[u8]]) -> Self {
        let mut h = Sha256::new();
        h.update(key.as_slice());
        h.update(domain);
        for part in parts {
            h.update(part);
        }
        Self {
            directory,
            hash: h.finalize()[..16].try_into().unwrap(),
        }
    }

    pub fn block(key: &Key, content: u64, idx: u64) -> Self {
        Self::hash(
            key,
            false,
            b"block",
            &[&content.to_le_bytes(), &idx.to_le_bytes()],
        )
    }

    pub fn bucket(key: &Key, dir: u64, gen: u64, idx: u32) -> Self {
        Self::hash(
            key,
            true,
            b"dirbucket",
            &[&dir.to_le_bytes(), &gen.to_le_bytes(), &idx.to_le_bytes()],
        )
    }

    pub fn path(self, data: &Path, version: Option<ObjectVersion>) -> Option<PathBuf> {
        let mut name = hex(&self.hash[2..]);
        match version {
            Some(ObjectVersion::Deleted) => return None,
            Some(ObjectVersion::Version(bytes)) => {
                name.push('.');
                name.push_str(&hex(&bytes));
            }
            None => {}
        }
        Some(
            data.join(if self.directory {
                "dirents"
            } else {
                "contents"
            })
            .join(hex(&self.hash[..2]))
            .join(name),
        )
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn sync_object_path(path: &Path) -> BkfsResult<()> {
    let parent = path.parent().unwrap();
    File::open(parent)?.sync_all()?;
    File::open(parent.parent().unwrap())?.sync_all()?;
    File::open(parent.parent().unwrap().parent().unwrap())?.sync_all()?;
    Ok(())
}

pub(crate) fn collect_garbage(data: &Path, keep: &HashSet<PathBuf>) -> BkfsResult<usize> {
    let mut removed = 0;
    for name in ["contents", "dirents"] {
        let root = data.join(name);
        let shards = match std::fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        for shard in shards {
            let shard = shard?;
            if !shard.file_type()?.is_dir() {
                return Err(BkfsError::unsupported("unexpected object shard"));
            }
            for entry in std::fs::read_dir(shard.path())? {
                let entry = entry?;
                if !entry.file_type()?.is_file() {
                    return Err(BkfsError::unsupported("unexpected object file"));
                }
                if !keep.contains(&entry.path()) {
                    std::fs::remove_file(entry.path())?;
                    removed += 1;
                }
            }
            File::open(shard.path())?.sync_all()?;
        }
        File::open(root)?.sync_all()?;
    }
    Ok(removed)
}
