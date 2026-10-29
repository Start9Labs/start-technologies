use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use chacha20::Key;
use serde::{Deserialize, Serialize};

use crate::atomic_file::AtomicFile;
use crate::error::{BkfsError, BkfsResult, BkfsResultExt};
use crate::metadata::Reference;
use crate::seglog::{Location, RecordKey, Roots};
use crate::serde::{data_config, decode, encode};
use crate::vault::{self, Binding, EccParams};

const VERSION: u32 = 1;
const ENTRIES_PER_TRANSACTION: usize = 16_384;
const ANCHOR: &str = "state";

type Change = (RecordKey, Option<Location>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Counters {
    pub next_seq: u64,
    pub max_inode: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct Anchor {
    version: u32,
    tip: Option<Reference>,
    counters: Counters,
    catalog: Option<Reference>,
}

#[derive(Serialize, Deserialize)]
struct Transaction {
    previous: Option<Reference>,
    changes: Vec<Change>,
}

pub(crate) struct State {
    directory: File,
    path: PathBuf,
    anchor: Anchor,
    pending: HashMap<RecordKey, Option<Location>>,
    uncertain: bool,
}

impl State {
    fn directory(dir: &Path, anchor: Anchor) -> BkfsResult<Self> {
        let directory = File::open(dir)?;
        // Lazy unmount detaches the backing path before Filesystem::destroy flushes it.
        let path = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()));
        Ok(Self {
            directory,
            path,
            anchor,
            pending: HashMap::new(),
            uncertain: false,
        })
    }

    pub fn create(
        dir: &Path,
        key: &Key,
        ecc: EccParams,
        roots: &Roots,
        counters: Counters,
        catalog: Option<Reference>,
    ) -> BkfsResult<Self> {
        let mut state = Self::directory(
            dir,
            Anchor {
                version: VERSION,
                tip: None,
                counters,
                catalog,
            },
        )?;
        state.checkpoint(key, ecc, roots, counters, catalog)?;
        Ok(state)
    }

    pub fn open(dir: &Path, key: &Key) -> BkfsResult<(Self, Roots)> {
        let bytes = std::fs::read(dir.join(ANCHOR))?;
        let plain = vault::open_bound(&bytes, key, Binding::State)?;
        let anchor: Anchor = decode(&plain.plaintext, data_config())?;
        if anchor.version != VERSION {
            return Err(BkfsError::unsupported(
                "unsupported authenticated index state",
            ));
        }
        let state = Self::directory(dir, anchor)?;
        let mut roots = Roots::default();
        let mut seen = HashSet::new();
        let mut tip = state.anchor.tip;
        while let Some(reference) = tip {
            let transaction: Transaction =
                reference.load(&state.path, key, Binding::Transaction)?;
            for (key, location) in transaction.changes {
                if seen.insert(key) {
                    roots.update(key, location);
                }
            }
            tip = transaction.previous;
        }
        Ok((state, roots))
    }

    pub fn counters(&self) -> Counters {
        self.anchor.counters
    }

    pub fn catalog(&self) -> Option<Reference> {
        self.anchor.catalog
    }

    pub fn check_rw(&self) -> BkfsResult<()> {
        if self.uncertain {
            return BkfsResult::errno_notrace(libc::EIO);
        }
        Ok(())
    }

    pub fn record(&mut self, key: RecordKey, location: Option<Location>) {
        self.pending.insert(key, location);
    }

    fn write_transactions(
        &self,
        key: &Key,
        ecc: EccParams,
        mut previous: Option<Reference>,
        changes: impl Iterator<Item = Change>,
    ) -> BkfsResult<(Option<Reference>, Vec<Reference>)> {
        let mut references = Vec::new();
        let mut changes = changes.peekable();
        while changes.peek().is_some() {
            let transaction = Transaction {
                previous,
                changes: changes.by_ref().take(ENTRIES_PER_TRANSACTION).collect(),
            };
            let reference =
                Reference::write(&self.path, key, ecc, Binding::Transaction, &transaction)?;
            references.push(reference);
            previous = Some(reference);
        }
        Ok((previous, references))
    }

    fn publish(&mut self, key: &Key, ecc: EccParams, anchor: Anchor) -> BkfsResult<()> {
        let plain = encode(&anchor, data_config())?;
        let _: Anchor = decode(&plain, data_config())?;
        let sealed = vault::seal_bound(&plain, key, ecc, Binding::State);
        let mut file = AtomicFile::create_buffered(self.path.join(ANCHOR))?;
        file.write_all(&sealed.bytes)?;
        let publication = file.save().and_then(|()| Ok(self.directory.sync_all()?));
        if let Err(error) = publication {
            self.uncertain = true;
            return Err(error);
        }
        self.anchor = anchor;
        self.pending.clear();
        Ok(())
    }

    fn sync_backing(&self) -> BkfsResult<()> {
        if unsafe { libc::syncfs(self.directory.as_raw_fd()) } < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }

    pub fn commit(
        &mut self,
        key: &Key,
        ecc: EccParams,
        counters: Counters,
        catalog: Option<Reference>,
    ) -> BkfsResult<()> {
        self.check_rw()?;
        let (tip, _) = self.write_transactions(
            key,
            ecc,
            self.anchor.tip,
            self.pending.iter().map(|(&key, &loc)| (key, loc)),
        )?;
        // Objects, log frames and index transactions must precede their new root.
        self.sync_backing()?;
        self.publish(
            key,
            ecc,
            Anchor {
                version: VERSION,
                tip,
                counters,
                catalog,
            },
        )
    }

    pub fn checkpoint(
        &mut self,
        key: &Key,
        ecc: EccParams,
        roots: &Roots,
        counters: Counters,
        catalog: Option<Reference>,
    ) -> BkfsResult<()> {
        self.check_rw()?;
        let (tip, references) = self.write_transactions(
            key,
            ecc,
            None,
            roots.entries().map(|(key, loc)| (key, Some(loc))),
        )?;
        self.sync_backing()?;
        self.publish(
            key,
            ecc,
            Anchor {
                version: VERSION,
                tip,
                counters,
                catalog,
            },
        )?;
        self.collect_metadata(references)
    }

    fn collect_metadata(&self, references: Vec<Reference>) -> BkfsResult<()> {
        let mut keep: HashSet<_> = references
            .into_iter()
            .map(|reference| reference.path(&self.path))
            .collect();
        if let Some(reference) = self.anchor.catalog {
            keep.insert(reference.path(&self.path));
        }
        let pool = Reference::directory(&self.path);
        let entries = match std::fs::read_dir(&pool) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let entry = entry?;
            if entry.file_type()?.is_file() && !keep.contains(&entry.path()) {
                std::fs::remove_file(entry.path())?;
            }
        }
        File::open(pool)?.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::ObjectId;

    fn initial(dir: &Path, key: &Key) -> State {
        State::create(
            dir,
            key,
            EccParams::default(),
            &Roots::default(),
            Counters {
                next_seq: 0,
                max_inode: 1,
            },
            None,
        )
        .unwrap()
    }

    fn location(offset: u64) -> Location {
        Location {
            segment: 1,
            offset,
            len: 128,
        }
    }

    #[test]
    fn incremental_updates_and_tombstones_survive_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x81; 32]);
        let mut state = initial(dir.path(), &key);
        let object = ObjectId::block(&key, 9, 0);
        state.record(RecordKey::Inode(7), Some(location(0)));
        state.record(RecordKey::Content(7), Some(location(128)));
        state.record(RecordKey::Object(object), Some(location(256)));
        state
            .commit(
                &key,
                EccParams::default(),
                Counters {
                    next_seq: 3,
                    max_inode: 7,
                },
                None,
            )
            .unwrap();
        state.record(RecordKey::Inode(7), Some(location(384)));
        state.record(RecordKey::Content(7), None);
        state
            .commit(
                &key,
                EccParams::default(),
                Counters {
                    next_seq: 5,
                    max_inode: 7,
                },
                None,
            )
            .unwrap();
        let (opened, roots) = State::open(dir.path(), &key).unwrap();
        assert_eq!(
            opened.counters(),
            Counters {
                next_seq: 5,
                max_inode: 7
            }
        );
        assert_eq!(roots.index.get(&7), Some(&location(384)));
        assert!(!roots.content.contains_key(&7));
        assert_eq!(roots.objects.get(&object), Some(&location(256)));
    }

    #[test]
    fn referenced_transaction_cannot_be_replaced_by_an_older_valid_one() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x82; 32]);
        let mut state = initial(dir.path(), &key);
        state.record(RecordKey::Inode(7), Some(location(0)));
        state
            .commit(
                &key,
                EccParams::default(),
                Counters {
                    next_seq: 1,
                    max_inode: 7,
                },
                None,
            )
            .unwrap();
        let old = state.anchor.tip.unwrap();
        state.record(RecordKey::Inode(7), Some(location(128)));
        state
            .commit(
                &key,
                EccParams::default(),
                Counters {
                    next_seq: 2,
                    max_inode: 7,
                },
                None,
            )
            .unwrap();
        std::fs::copy(
            old.path(dir.path()),
            state.anchor.tip.unwrap().path(dir.path()),
        )
        .unwrap();
        assert!(State::open(dir.path(), &key).is_err());
    }

    #[test]
    fn absent_anchor_or_referenced_transaction_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x83; 32]);
        assert!(State::open(dir.path(), &key).is_err());
        let mut state = initial(dir.path(), &key);
        state.record(RecordKey::Inode(7), Some(location(0)));
        state
            .commit(
                &key,
                EccParams::default(),
                Counters {
                    next_seq: 1,
                    max_inode: 7,
                },
                None,
            )
            .unwrap();
        std::fs::remove_file(state.anchor.tip.unwrap().path(dir.path())).unwrap();
        assert!(State::open(dir.path(), &key).is_err());
    }

    #[test]
    fn interrupted_publication_keeps_prior_index_and_can_retry() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x84; 32]);
        let mut state = initial(dir.path(), &key);
        state.record(RecordKey::Inode(7), Some(location(0)));
        let blocker = dir.path().join("state.tmp");
        std::fs::create_dir(&blocker).unwrap();
        assert!(state
            .commit(
                &key,
                EccParams::default(),
                Counters {
                    next_seq: 1,
                    max_inode: 7
                },
                None
            )
            .is_err());
        assert!(State::open(dir.path(), &key).unwrap().1.index.is_empty());
        std::fs::remove_dir(blocker).unwrap();
        state
            .commit(
                &key,
                EccParams::default(),
                Counters {
                    next_seq: 1,
                    max_inode: 7,
                },
                None,
            )
            .unwrap();
        assert_eq!(
            State::open(dir.path(), &key).unwrap().1.index.get(&7),
            Some(&location(0))
        );
    }

    #[test]
    fn checkpoint_splits_entries_and_reclaims_only_unreferenced_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x85; 32]);
        let mut state = initial(dir.path(), &key);
        state.record(RecordKey::Inode(7), Some(location(0)));
        state
            .commit(
                &key,
                EccParams::default(),
                Counters {
                    next_seq: 1,
                    max_inode: 7,
                },
                None,
            )
            .unwrap();
        let old = state.anchor.tip.unwrap().path(dir.path());
        let catalog = Reference::write(
            dir.path(),
            &key,
            EccParams::default(),
            Binding::Catalog,
            &"catalog".to_owned(),
        )
        .unwrap();
        let mut roots = Roots::default();
        for id in 1..=ENTRIES_PER_TRANSACTION as u64 + 1 {
            roots.index.insert(id, location(id * 128));
        }
        state
            .checkpoint(
                &key,
                EccParams::default(),
                &roots,
                Counters {
                    next_seq: 20_000,
                    max_inode: ENTRIES_PER_TRANSACTION as u64 + 1,
                },
                Some(catalog),
            )
            .unwrap();
        assert!(!old.exists());
        assert!(catalog.path(dir.path()).exists());
        assert_eq!(
            std::fs::read_dir(dir.path().join("state-objects"))
                .unwrap()
                .count(),
            3
        );
        let (opened, restored) = State::open(dir.path(), &key).unwrap();
        assert_eq!(restored, roots);
        assert_eq!(opened.catalog(), Some(catalog));
        assert_eq!(
            catalog
                .load::<String>(dir.path(), &key, Binding::Catalog)
                .unwrap(),
            "catalog"
        );
    }

    #[test]
    fn checkpoint_does_not_read_its_new_transactions_back() {
        use std::io::Read;
        use std::os::fd::FromRawFd;
        use std::os::unix::ffi::OsStrExt;

        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x8a; 32]);
        let mut state = initial(dir.path(), &key);
        let pool = Reference::directory(dir.path());
        std::fs::create_dir(&pool).unwrap();
        let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        assert!(fd >= 0);
        let mut events = unsafe { File::from_raw_fd(fd) };
        let pool = std::ffi::CString::new(pool.as_os_str().as_bytes()).unwrap();
        assert!(unsafe { libc::inotify_add_watch(fd, pool.as_ptr(), libc::IN_ACCESS) } >= 0);
        let mut roots = Roots::default();
        roots.index.insert(7, location(0));
        state
            .checkpoint(
                &key,
                EccParams::default(),
                &roots,
                Counters {
                    next_seq: 1,
                    max_inode: 7,
                },
                None,
            )
            .unwrap();
        let mut bytes = [0u8; 8192];
        let len = match events.read(&mut bytes) {
            Ok(len) => len,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return,
            Err(error) => panic!("{error}"),
        };
        let mut offset = 0;
        while offset < len {
            let event = unsafe {
                std::ptr::read_unaligned(bytes[offset..].as_ptr().cast::<libc::inotify_event>())
            };
            assert!(
                event.len == 0 || event.mask & libc::IN_ISDIR != 0,
                "checkpoint read a metadata blob back"
            );
            offset += std::mem::size_of::<libc::inotify_event>() + event.len as usize;
        }
    }

    #[test]
    fn held_directory_can_publish_after_the_backing_path_disappears() {
        let parent = tempfile::tempdir().unwrap();
        let original = parent.path().join("original");
        let moved = parent.path().join("moved");
        std::fs::create_dir(&original).unwrap();
        let key = Key::from([0x86; 32]);
        let mut state = initial(&original, &key);
        std::fs::rename(&original, &moved).unwrap();
        state.record(RecordKey::Inode(7), Some(location(0)));
        state
            .commit(
                &key,
                EccParams::default(),
                Counters {
                    next_seq: 1,
                    max_inode: 7,
                },
                None,
            )
            .unwrap();
        assert!(!original.exists());
        assert_eq!(
            State::open(&moved, &key).unwrap().1.index.get(&7),
            Some(&location(0))
        );
    }

    #[test]
    fn failed_root_replacement_blocks_publication_until_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x88; 32]);
        let mut state = initial(dir.path(), &key);
        state.record(RecordKey::Inode(7), Some(location(0)));
        let anchor = dir.path().join(ANCHOR);
        let saved = dir.path().join("saved");
        std::fs::rename(&anchor, &saved).unwrap();
        std::fs::create_dir(&anchor).unwrap();
        let counters = Counters {
            next_seq: 1,
            max_inode: 7,
        };
        assert!(state
            .commit(&key, EccParams::default(), counters, None)
            .is_err());
        std::fs::remove_dir(&anchor).unwrap();
        std::fs::rename(saved, anchor).unwrap();
        assert!(state.check_rw().is_err());
        assert!(state
            .commit(&key, EccParams::default(), counters, None)
            .is_err());
        let (reopened, roots) = State::open(dir.path(), &key).unwrap();
        reopened.check_rw().unwrap();
        assert!(roots.index.is_empty());
    }

    #[test]
    fn corrected_root_and_transaction_retain_current_references() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x89; 32]);
        let mut state = initial(dir.path(), &key);
        state.record(RecordKey::Inode(7), Some(location(0)));
        state
            .commit(
                &key,
                EccParams::default(),
                Counters {
                    next_seq: 1,
                    max_inode: 7,
                },
                None,
            )
            .unwrap();
        for path in [
            dir.path().join(ANCHOR),
            state.anchor.tip.unwrap().path(dir.path()),
        ] {
            let mut bytes = std::fs::read(&path).unwrap();
            let middle = bytes.len() / 2;
            bytes[middle] ^= 1;
            std::fs::write(path, bytes).unwrap();
        }
        assert_eq!(
            State::open(dir.path(), &key).unwrap().1.index.get(&7),
            Some(&location(0))
        );
    }

    #[test]
    fn replacing_the_whole_anchor_selects_its_genuine_older_history() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x87; 32]);
        let mut state = initial(dir.path(), &key);
        let old_anchor = std::fs::read(dir.path().join(ANCHOR)).unwrap();
        state.record(RecordKey::Inode(7), Some(location(0)));
        state
            .commit(
                &key,
                EccParams::default(),
                Counters {
                    next_seq: 1,
                    max_inode: 7,
                },
                None,
            )
            .unwrap();
        std::fs::write(dir.path().join(ANCHOR), old_anchor).unwrap();
        let (opened, roots) = State::open(dir.path(), &key).unwrap();
        assert_eq!(opened.counters().next_seq, 0);
        assert!(roots.index.is_empty());
    }
}
