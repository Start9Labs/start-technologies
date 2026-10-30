use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chacha20::Key;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::BkfsResult;
use crate::references::StorageReferences;
use crate::seglog::{self, Record, RecordKey, SegMeta};
use crate::util::hex;

const OBJECT_ROOTS: [&str; 2] = ["contents", "dirents"];

/// The format-1 file holding a block or directory bucket.
pub(crate) fn object_path(data: &Path, key: &Key, object: RecordKey) -> Option<PathBuf> {
    let mut hash = Sha256::new();
    hash.update(key.as_slice());
    let root = match object {
        RecordKey::Block(content, index) => {
            hash.update(b"block");
            hash.update(content.to_le_bytes());
            hash.update(index.to_le_bytes());
            "contents"
        }
        RecordKey::Bucket(directory, generation, index) => {
            hash.update(b"dirbucket");
            hash.update(directory.to_le_bytes());
            hash.update(generation.to_le_bytes());
            hash.update(index.to_le_bytes());
            "dirents"
        }
        RecordKey::Inode(_) | RecordKey::Content(_) => return None,
    };
    let hash = hash.finalize();
    Some(
        data.join(root)
            .join(hex(&hash[..2]))
            .join(hex(&hash[2..16])),
    )
}

/// Removes a format-1 shard directory and any objects left in it.
fn remove_shard(shard: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir(shard) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => std::fs::remove_dir_all(shard),
        Ok(()) => Ok(()),
    }
}

/// Best-effort removal of the format-1 object directories.
pub(crate) fn remove_object_trees(data: &Path) {
    let shards = Mutex::new(Vec::new());
    for root in OBJECT_ROOTS {
        match std::fs::read_dir(data.join(root)) {
            Ok(entries) => shards
                .lock()
                .unwrap()
                .extend(entries.flatten().map(|entry| entry.path())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => ::log::warn!("cannot list retired {root} objects: {error}"),
        }
    }
    std::thread::scope(|scope| {
        for _ in 0..16 {
            scope.spawn(|| loop {
                let Some(shard) = shards.lock().unwrap().pop() else {
                    break;
                };
                if let Err(error) = remove_shard(&shard) {
                    ::log::warn!("cannot remove retired objects in {shard:?}: {error}");
                }
            });
        }
    });
    for root in OBJECT_ROOTS {
        match std::fs::remove_dir(data.join(root)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                ::log::warn!("cannot remove retired {root} directory: {error}")
            }
            _ => {}
        }
    }
}

mod log;
pub(crate) use log::Log;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct Location {
    pub segment: u64,
    pub offset: u64,
    pub len: u32,
}

#[derive(Deserialize, Serialize)]
struct Checkpoint {
    version: u32,
    next_seq: u64,
    max_inode: u64,
    index: HashMap<u64, Location>,
    content: HashMap<u64, Location>,
    _seg_meta: HashMap<u64, SegMeta>,
    segments: Vec<(u64, u64)>,
}

pub(crate) struct Source {
    pub references: HashMap<RecordKey, Location>,
    pub next_seq: u64,
    pub max_inode: u64,
    pub storage: HashMap<u64, StorageReferences>,
    floor: u64,
    winning: HashMap<RecordKey, u64>,
}

#[cfg(test)]
pub(crate) fn fixture(destination: &Path) {
    let archive = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/legacy-format1.tar.gz");
    std::fs::create_dir_all(destination).unwrap();
    assert!(std::process::Command::new("tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(destination)
        .status()
        .unwrap()
        .success());
}

impl Source {
    pub fn load(data: &Path, key: &Key) -> BkfsResult<Self> {
        let dir = data.join("segments");
        let checkpoint = match std::fs::read(dir.join(seglog::INDEX_CACHE_NAME)) {
            Ok(bytes) => crate::vault::open_legacy(&bytes, key)
                .and_then(|plain| {
                    crate::serde::decode::<Checkpoint>(&plain, crate::serde::data_config())
                })
                .ok(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let mut source = Self {
            references: HashMap::new(),
            next_seq: 1,
            max_inode: 0,
            storage: HashMap::new(),
            floor: 1,
            winning: HashMap::new(),
        };
        if let Some(checkpoint) = checkpoint {
            if checkpoint.version == 1 && checkpoint.segments == seglog::list_segments(&dir)? {
                source.references.extend(
                    checkpoint
                        .index
                        .into_iter()
                        .map(|(id, loc)| (RecordKey::Inode(id), loc)),
                );
                source.references.extend(
                    checkpoint
                        .content
                        .into_iter()
                        .map(|(id, loc)| (RecordKey::Content(id), loc)),
                );
                source.next_seq = checkpoint.next_seq.max(1);
                source.floor = source.next_seq;
                source.max_inode = checkpoint.max_inode;
            }
        }
        Ok(source)
    }

    pub fn roots(&self, locations: &HashMap<(u64, u64), seglog::Location>) -> seglog::Roots {
        let mut roots = seglog::Roots::default();
        for (&key, old) in &self.references {
            let location = locations
                .get(&(old.segment, old.offset))
                .copied()
                .unwrap_or(seglog::Location {
                    segment: old.segment,
                    offset: old.offset,
                    len: old.len,
                    ..Default::default()
                });
            roots.update(key, Some(location));
        }
        roots
    }

    pub fn advance_sequence(&mut self, seq: u64) -> BkfsResult<()> {
        self.next_seq = self.next_seq.max(seglog::next_sequence(seq)?);
        Ok(())
    }

    pub fn observe(&mut self, record: &Record, seq: u64, location: Location) -> BkfsResult<()> {
        if seq >= self.floor {
            self.advance_sequence(seq)?;
            let key = record.key();
            if let RecordKey::Inode(id) = key {
                self.max_inode = self.max_inode.max(id);
            }
            if !self.winning.get(&key).is_some_and(|&winning| winning > seq) {
                self.winning.insert(key, seq);
                if record.is_tombstone() {
                    self.references.remove(&key);
                } else {
                    self.references.insert(key, location);
                }
            }
        }
        match record {
            Record::Inode { inode, attrs }
                if self
                    .references
                    .get(&RecordKey::Inode(*inode))
                    .is_some_and(|selected| {
                        selected.segment == location.segment && selected.offset == location.offset
                    }) =>
            {
                self.storage
                    .insert(*inode, StorageReferences::for_inode(*inode, attrs));
            }
            Record::Tombstone { inode }
                if !self.references.contains_key(&RecordKey::Inode(*inode)) =>
            {
                self.storage.remove(inode);
            }
            _ => {}
        }
        Ok(())
    }
}
