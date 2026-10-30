use std::collections::HashMap;
use std::path::Path;

use chacha20::Key;
use serde::Deserialize;

use crate::error::BkfsResult;
use crate::objects::StorageReferences;
use crate::seglog::{self, Record, RecordKey, SegMeta};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub(crate) struct Location {
    pub segment: u64,
    pub offset: u64,
    pub len: u32,
}

#[derive(Deserialize)]
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
