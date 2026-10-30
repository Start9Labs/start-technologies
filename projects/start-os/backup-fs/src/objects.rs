use std::collections::HashSet;
use std::fs::File;
use std::path::{Path, PathBuf};

use chacha20::Key;
use rand::rand_core::UnwrapErr;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{BkfsError, BkfsResult};
use crate::inode::{Attributes, FileData};
use crate::util::hex;

const HASH_BYTES: usize = 16;
const SHARD_BYTES: usize = 2;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ObjectId {
    directory: bool,
    hash: [u8; HASH_BYTES],
}

#[derive(Clone, Copy)]
pub(crate) enum StorageReferences {
    None,
    Blocks {
        content: u64,
        count: u64,
    },
    Packed(u64),
    Buckets {
        inode: u64,
        generation: u64,
        count: u32,
    },
}

impl StorageReferences {
    pub fn for_inode(inode: u64, attrs: &Attributes) -> Self {
        match &attrs.contents {
            FileData::File(content) => Self::Blocks {
                content: content.0,
                count: crate::blockstore::block_count(attrs.size),
            },
            FileData::Packed(content) => Self::Packed(content.0),
            FileData::Directory(directory) => {
                let (generation, count) = directory.bucket_geometry();
                Self::Buckets {
                    inode,
                    generation,
                    count,
                }
            }
            _ => Self::None,
        }
    }

    pub fn packed(self) -> Option<u64> {
        match self {
            Self::Packed(id) => Some(id),
            _ => None,
        }
    }

    pub fn visit_objects(self, key: &Key, mut visit: impl FnMut(ObjectId)) {
        match self {
            Self::Blocks { content, count } => {
                for idx in 0..count {
                    visit(ObjectId::block(key, content, idx));
                }
            }
            Self::Buckets {
                inode,
                generation,
                count,
            } => {
                for idx in 0..count {
                    visit(ObjectId::bucket(key, inode, generation, idx));
                }
            }
            _ => {}
        }
    }
}

pub(crate) type VersionToken = [u8; 16];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ObjectVersion {
    Deleted,
    Version(VersionToken),
}

impl ObjectVersion {
    pub fn new_token() -> VersionToken {
        let mut bytes = VersionToken::default();
        UnwrapErr(rand::rng()).fill_bytes(&mut bytes);
        bytes
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
            hash: h.finalize()[..HASH_BYTES].try_into().unwrap(),
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
        let mut name = hex(&self.hash[SHARD_BYTES..]);
        match version {
            Some(ObjectVersion::Deleted) => return None,
            Some(ObjectVersion::Version(bytes)) => {
                name.push('.');
                name.push_str(&hex(&bytes));
            }
            None => {}
        }
        Some(
            Self::root(data, self.directory)
                .join(hex(&self.hash[..SHARD_BYTES]))
                .join(name),
        )
    }

    fn root(data: &Path, directory: bool) -> PathBuf {
        data.join(if directory { "dirents" } else { "contents" })
    }

    pub(crate) fn legacy(
        directory: bool,
        shard: &std::ffi::OsStr,
        file: &std::ffi::OsStr,
    ) -> Option<Self> {
        let shard = shard.to_str()?;
        let file = file.to_str()?;
        if shard.len() != SHARD_BYTES * 2 || file.len() != (HASH_BYTES - SHARD_BYTES) * 2 {
            return None;
        }
        let hex = format!("{shard}{file}");
        let mut hash = [0; HASH_BYTES];
        for (byte, pair) in hash.iter_mut().zip(hex.as_bytes().chunks_exact(2)) {
            *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
        }
        Some(Self { directory, hash })
    }
}

pub(crate) struct Inventory {
    objects: Vec<(PathBuf, Option<ObjectId>)>,
    directories: Vec<PathBuf>,
}

impl Inventory {
    pub fn load(data: &Path) -> BkfsResult<Self> {
        let mut inventory = Self {
            objects: Vec::new(),
            directories: Vec::new(),
        };
        for directory in [false, true] {
            let root = ObjectId::root(data, directory);
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
                    let legacy =
                        ObjectId::legacy(directory, &shard.file_name(), &entry.file_name());
                    inventory.objects.push((entry.path(), legacy));
                }
                inventory.directories.push(shard.path());
            }
            inventory.directories.push(root);
        }
        Ok(inventory)
    }

    pub fn legacy_ids(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.objects.iter().filter_map(|(_, id)| *id)
    }

    pub fn collect_garbage(self, keep: &HashSet<PathBuf>) -> BkfsResult<usize> {
        let mut removed = 0;
        for (path, _) in self.objects {
            if !keep.contains(&path) {
                std::fs::remove_file(path)?;
                removed += 1;
            }
        }
        for path in self.directories {
            File::open(path)?.sync_all()?;
        }
        Ok(removed)
    }
}
