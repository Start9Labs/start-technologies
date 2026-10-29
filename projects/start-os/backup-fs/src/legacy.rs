use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use chacha20::Key;
use serde::{Deserialize, Serialize};

use crate::error::BkfsResult;
use crate::objects::ObjectId;
use crate::seglog::{self, Record, RecordKey, SegMeta};
use crate::snapshot::SnapshotInfo;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Location {
    pub segment: u64,
    pub offset: u64,
    pub len: u32,
}
pub(crate) type Roots = seglog::Roots<Location>;

#[derive(Serialize, Deserialize)]
struct DurableRoots {
    roots: Roots,
    next_seq: u64,
    max_inode: u64,
}
#[derive(Deserialize)]
struct Checkpoint {
    version: u32,
    state: DurableRoots,
    _seg_meta: HashMap<u64, SegMeta>,
    segments: Vec<(u64, u64)>,
}

#[derive(Deserialize)]
pub(crate) struct Snapshot {
    pub info: SnapshotInfo,
    pub roots: Roots,
    pub legacy: HashSet<ObjectId>,
}
#[derive(Deserialize)]
struct Catalog {
    version: u32,
    entries: BTreeMap<String, Snapshot>,
}

pub(crate) struct Source {
    pub roots: Roots,
    pub snapshots: BTreeMap<String, Snapshot>,
    pub next_seq: u64,
    pub max_inode: u64,
    floor: u64,
    winning: HashMap<RecordKey, u64>,
}

fn optional(path: &Path) -> BkfsResult<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
pub(crate) fn fixture(destination: &Path, version: u32) {
    let archive = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/legacy-795d8a265.tar.gz");
    std::fs::create_dir_all(destination).unwrap();
    assert!(std::process::Command::new("tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(destination)
        .arg("--strip-components=2")
        .arg(format!("./v{version}"))
        .status()
        .unwrap()
        .success());
}

impl Source {
    pub fn load(data: &Path, key: &Key) -> BkfsResult<Self> {
        let dir = data.join("segments");
        let mut durable = optional(&dir.join("roots"))?
            .map(|bytes| crate::serde::deserialize_legacy::<DurableRoots>(&bytes, key))
            .transpose()?;
        if durable.is_none() {
            if let Some(bytes) = optional(&dir.join(seglog::INDEX_CACHE_NAME))? {
                if let Ok(checkpoint) = crate::serde::deserialize_legacy::<Checkpoint>(&bytes, key)
                {
                    if checkpoint.version == 2
                        && checkpoint.segments == seglog::list_segments(&dir)?
                    {
                        durable = Some(checkpoint.state);
                    }
                }
            }
        }
        let snapshots = match optional(&data.join("snapshots"))? {
            Some(bytes) => {
                let catalog: Catalog = crate::serde::deserialize_legacy(&bytes, key)?;
                if catalog.version != 1 {
                    return Err(crate::error::BkfsError::unsupported(
                        "unsupported legacy snapshot catalog",
                    ));
                }
                catalog.entries
            }
            None => BTreeMap::new(),
        };
        let durable = durable.unwrap_or(DurableRoots {
            roots: Roots::default(),
            next_seq: 1,
            max_inode: 0,
        });
        Ok(Self {
            roots: durable.roots,
            snapshots,
            next_seq: durable.next_seq.max(1),
            max_inode: durable.max_inode,
            floor: durable.next_seq,
            winning: HashMap::new(),
        })
    }

    pub fn advance_sequence(&mut self, seq: u64) -> BkfsResult<()> {
        self.next_seq = self.next_seq.max(seglog::next_sequence(seq)?);
        Ok(())
    }

    pub fn observe(&mut self, record: &Record, seq: u64, location: Location) -> BkfsResult<()> {
        if seq < self.floor {
            return Ok(());
        }
        self.advance_sequence(seq)?;
        let key = record.key();
        if let RecordKey::Inode(id) = key {
            self.max_inode = self.max_inode.max(id);
        }
        if self.winning.get(&key).is_some_and(|&winning| winning > seq) {
            return Ok(());
        }
        self.winning.insert(key, seq);
        self.roots
            .update(key, (!record.is_tombstone()).then_some(location));
        Ok(())
    }
}
