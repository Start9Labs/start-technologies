use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use chacha20::Key;
use serde::{Deserialize, Serialize};

use crate::aligned_io::RangeReader;
use crate::atomic_file::AtomicFile;
use crate::error::{BkfsError, BkfsResult, BkfsResultExt};
use crate::metadata::Reference;
use crate::seglog::{Location, RecordKey, Roots};
use crate::serde::{data_config, decode, encode};
use crate::vault::{self, AuthenticationTag, Binding, EccParams};

const VERSION: u32 = 1;
const ENTRIES_PER_TRANSACTION: usize = 16_384;
/// Journal entries tolerated before an unmount checkpoint, beyond a quarter
/// of the checkpoint's own entries.
const JOURNAL_ENTRIES_FLOOR: u64 = 65_536;
const JOURNAL_SEGMENT_SIZE: u64 = 8 * 1024 * 1024;
const JOURNAL_SUFFIX: &str = ".idx";
const JOURNAL_ALIGN: u64 = 4096;
const ANCHOR: &str = "state";
type Change = (RecordKey, Option<Location>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Counters {
    pub next_seq: u64,
    pub max_inode: u64,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
enum IndexReference {
    Checkpoint(Reference),
    Journal {
        segment: u64,
        offset: u64,
        len: u32,
        tag: AuthenticationTag,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct View {
    tip: Option<IndexReference>,
    pub counters: Counters,
}

#[derive(Clone, Serialize, Deserialize)]
struct Anchor {
    version: u32,
    index: View,
    catalog: Option<Reference>,
}

#[derive(Serialize, Deserialize)]
struct Transaction {
    previous: Option<IndexReference>,
    changes: Vec<Change>,
    /// Segment lengths; zero marks a removed segment.
    segments: Vec<(u64, u64)>,
}

fn journal_path(dir: &Path, id: u64) -> PathBuf {
    dir.join(format!("{id:016x}{JOURNAL_SUFFIX}"))
}

fn journal_id(name: &std::ffi::OsStr) -> Option<u64> {
    u64::from_str_radix(name.to_str()?.strip_suffix(JOURNAL_SUFFIX)?, 16).ok()
}

struct Transactions<'a> {
    dir: &'a Path,
    key: &'a Key,
    pending: Option<IndexReference>,
    reader: RangeReader,
}

impl Iterator for Transactions<'_> {
    type Item = BkfsResult<(IndexReference, Transaction)>;

    fn next(&mut self) -> Option<Self::Item> {
        let reference = self.pending.take()?;
        Some((|| {
            let transaction: Transaction = match reference {
                IndexReference::Checkpoint(reference) => {
                    reference.load(self.dir, self.key, Binding::Transaction)?
                }
                IndexReference::Journal {
                    segment,
                    offset,
                    len,
                    tag,
                } => {
                    let mut blob = vec![0; len as usize];
                    self.reader
                        .read_exact(&journal_path(self.dir, segment), offset, &mut blob)
                        .map_err(BkfsError::referenced_io)?;
                    let plain = vault::open_bound(&blob, self.key, Binding::Transaction)?
                        .verify_reference(&tag)?;
                    decode(&plain, data_config())?
                }
            };
            self.pending = transaction.previous;
            Ok((reference, transaction))
        })())
    }
}

struct LoadedIndex {
    roots: Roots,
    checkpoint_refs: Option<Vec<Reference>>,
    entries: Entries,
}

/// Index entries stored in each kind of transaction of a view.
#[derive(Clone, Copy, Default)]
struct Entries {
    checkpoint: u64,
    journal: u64,
}

impl View {
    fn transactions<'a>(self, dir: &'a Path, key: &'a Key) -> Transactions<'a> {
        Transactions {
            dir,
            key,
            pending: self.tip,
            reader: RangeReader::default(),
        }
    }

    pub fn load(self, dir: &Path, key: &Key) -> BkfsResult<Roots> {
        Ok(self.materialize(dir, key)?.roots)
    }

    fn materialize(self, dir: &Path, key: &Key) -> BkfsResult<LoadedIndex> {
        let mut roots = Roots::default();
        let mut seen = HashSet::new();
        let mut sized = HashSet::new();
        let mut checkpoint_refs = Some(Vec::new());
        let mut entries = Entries::default();
        for node in self.transactions(dir, key) {
            let (reference, transaction) = node?;
            let count = (transaction.changes.len() + transaction.segments.len()) as u64;
            match reference {
                IndexReference::Checkpoint(reference) => {
                    entries.checkpoint += count;
                    if let Some(references) = &mut checkpoint_refs {
                        references.push(reference);
                    }
                }
                IndexReference::Journal { .. } => {
                    entries.journal += count;
                    checkpoint_refs = None;
                }
            }
            for (key, location) in transaction.changes {
                if seen.insert(key) {
                    roots.update(key, location);
                }
            }
            for (segment, len) in transaction.segments {
                if sized.insert(segment) && len > 0 {
                    roots.sizes.insert(segment, len);
                }
            }
        }
        Ok(LoadedIndex {
            roots,
            checkpoint_refs,
            entries,
        })
    }

    /// The root publisher must sync these chunks before making the view authoritative.
    pub fn write_checkpoint(
        dir: &Path,
        key: &Key,
        ecc: EccParams,
        roots: &Roots,
        counters: Counters,
    ) -> BkfsResult<(Self, Vec<Reference>)> {
        let mut changes = roots.entries().peekable();
        let mut segments = roots.sizes.iter().map(|(&id, &len)| (id, len)).peekable();
        let mut tip = None;
        let mut references = Vec::new();
        while changes.peek().is_some() || segments.peek().is_some() {
            let transaction = Transaction {
                previous: tip,
                changes: changes
                    .by_ref()
                    .take(ENTRIES_PER_TRANSACTION)
                    .map(|(key, loc)| (key, Some(loc)))
                    .collect(),
                segments: segments.by_ref().take(ENTRIES_PER_TRANSACTION).collect(),
            };
            let reference = Reference::write(dir, key, ecc, Binding::Transaction, &transaction)?;
            references.push(reference);
            tip = Some(IndexReference::Checkpoint(reference));
        }
        Ok((Self { tip, counters }, references))
    }
}

struct Journal {
    file: File,
    id: u64,
    offset: u64,
    dirty: bool,
}

impl Journal {
    fn create(dir: &Path) -> BkfsResult<Self> {
        let mut maximum = None;
        for entry in std::fs::read_dir(dir)? {
            if let Some(id) = journal_id(&entry?.file_name()) {
                maximum = Some(maximum.map_or(id, |old: u64| old.max(id)));
            }
        }
        let id = match maximum {
            Some(id) => id
                .checked_add(1)
                .ok_or_else(|| BkfsError::unsupported("index journal id exhausted"))?,
            None => 0,
        };
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(journal_path(dir, id))?;
        Ok(Self {
            file,
            id,
            offset: 0,
            dirty: false,
        })
    }

    /// Starts the next append on a block no committed transaction occupies.
    fn align(&mut self) {
        self.offset = self.offset.next_multiple_of(JOURNAL_ALIGN);
    }

    fn append(&mut self, blob: &vault::Sealed) -> BkfsResult<IndexReference> {
        let len = u32::try_from(blob.bytes.len())
            .map_err(|_| BkfsError::unsupported("index transaction too large"))?;
        self.file.write_all_at(&blob.bytes, self.offset)?;
        self.dirty = true;
        let reference = IndexReference::Journal {
            segment: self.id,
            offset: self.offset,
            len,
            tag: blob.tag,
        };
        self.offset += len as u64;
        Ok(reference)
    }
}

pub(crate) struct State {
    directory: File,
    path: PathBuf,
    anchor: Anchor,
    pending: HashMap<RecordKey, Option<Location>>,
    pending_sizes: HashMap<u64, u64>,
    journal: Option<Journal>,
    checkpoint_refs: Option<Vec<Reference>>,
    entries: Entries,
    pool: Option<File>,
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
            pending_sizes: HashMap::new(),
            journal: None,
            checkpoint_refs: None,
            entries: Entries::default(),
            pool: None,
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
        let (index, references) = View::write_checkpoint(dir, key, ecc, roots, counters)?;
        let mut state = Self::directory(
            dir,
            Anchor {
                version: VERSION,
                index,
                catalog,
            },
        )?;
        state.sync_backing()?;
        state.publish(key, ecc, state.anchor.clone())?;
        state.checkpoint_refs = Some(references);
        state.entries.checkpoint = roots.entry_count();
        Ok(state)
    }

    /// Whether the journal has grown enough to replace with a checkpoint.
    pub fn checkpoint_due(&self) -> bool {
        self.entries.journal > 0
            && self.entries.journal >= JOURNAL_ENTRIES_FLOOR.max(self.entries.checkpoint / 4)
    }

    pub fn open(dir: &Path, key: &Key) -> BkfsResult<(Self, Roots)> {
        let directory = File::open(dir)?;
        let path = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()));
        let bytes = std::fs::read(path.join(ANCHOR)).map_err(BkfsError::referenced_io)?;
        let plain = vault::open_bound(&bytes, key, Binding::State)?;
        let anchor: Anchor = decode(&plain.plaintext, data_config())?;
        if anchor.version != VERSION {
            return Err(BkfsError::unsupported(
                "unsupported authenticated index state",
            ));
        }
        let loaded = anchor.index.materialize(&path, key)?;
        Ok((
            Self {
                directory,
                path,
                anchor,
                pending: HashMap::new(),
                pending_sizes: HashMap::new(),
                journal: None,
                checkpoint_refs: loaded.checkpoint_refs,
                entries: loaded.entries,
                pool: None,
                uncertain: false,
            },
            loaded.roots,
        ))
    }

    pub fn directory_path(&self) -> &Path {
        &self.path
    }

    #[cfg(test)]
    pub fn view(&self) -> View {
        self.anchor.index
    }
    pub fn counters(&self) -> Counters {
        self.anchor.index.counters
    }
    pub fn catalog(&self) -> Option<Reference> {
        self.anchor.catalog
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty() || !self.pending_sizes.is_empty()
    }

    pub fn check_rw(&self) -> BkfsResult<()> {
        if self.uncertain {
            return BkfsResult::errno_notrace(libc::EIO);
        }
        Ok(())
    }

    pub fn record(&mut self, key: RecordKey, location: Option<Location>) -> BkfsResult<()> {
        self.check_rw()?;
        self.pending.insert(key, location);
        self.checkpoint_refs = None;
        Ok(())
    }

    /// Records a segment's length; zero records its removal.
    pub fn record_size(&mut self, segment: u64, len: u64) -> BkfsResult<()> {
        self.check_rw()?;
        self.pending_sizes.insert(segment, len);
        self.checkpoint_refs = None;
        Ok(())
    }

    fn publish(&mut self, key: &Key, ecc: EccParams, anchor: Anchor) -> BkfsResult<()> {
        let plain = encode(&anchor, data_config())?;
        let _: Anchor = decode(&plain, data_config())?;
        let sealed = vault::seal_bound(&plain, key, ecc, Binding::State);
        let mut file = AtomicFile::create_buffered(self.path.join(ANCHOR))?;
        file.write_all(&sealed.bytes)?;
        let publication = file.save_with_directory_sync(&self.directory);
        if let Err(error) = publication {
            self.uncertain = true;
            return Err(error);
        }
        self.anchor = anchor;
        self.pending.clear();
        self.pending_sizes.clear();
        Ok(())
    }

    /// Makes index transactions and new directory entries durable.
    fn sync_backing(&mut self) -> BkfsResult<()> {
        let mut result = match &self.journal {
            Some(journal) if journal.dirty => journal.file.sync_data(),
            _ => Ok(()),
        };
        if result.is_ok() && self.pool.is_none() {
            match File::open(Reference::directory(&self.path)) {
                Ok(pool) => self.pool = Some(pool),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => result = Err(error),
            }
        }
        if let (Ok(()), Some(pool)) = (&result, &self.pool) {
            result = pool.sync_all();
        }
        if result.is_ok() {
            result = self.directory.sync_all();
        }
        match result {
            Ok(()) => {
                if let Some(journal) = &mut self.journal {
                    journal.dirty = false;
                }
                Ok(())
            }
            Err(error) => {
                self.uncertain = true;
                Err(error.into())
            }
        }
    }

    pub fn commit(
        &mut self,
        key: &Key,
        ecc: EccParams,
        counters: Counters,
        catalog: Option<Reference>,
    ) -> BkfsResult<()> {
        self.check_rw()?;
        let mut tip = self.anchor.index.tip;
        let changes: Vec<_> = self.pending.iter().map(|(&key, &loc)| (key, loc)).collect();
        let segments: Vec<_> = self
            .pending_sizes
            .iter()
            .map(|(&id, &len)| (id, len))
            .collect();
        let mut changes = changes.into_iter().peekable();
        let mut segments = segments.into_iter().peekable();
        if let Some(journal) = &mut self.journal {
            journal.align();
        }
        while changes.peek().is_some() || segments.peek().is_some() {
            let transaction = Transaction {
                previous: tip,
                changes: changes.by_ref().take(ENTRIES_PER_TRANSACTION).collect(),
                segments: segments.by_ref().take(ENTRIES_PER_TRANSACTION).collect(),
            };
            let plain = encode(&transaction, data_config())?;
            let _: Transaction = decode(&plain, data_config())?;
            let blob = vault::seal_bound(&plain, key, ecc, Binding::Transaction);
            if self.journal.as_ref().is_some_and(|journal| {
                journal.offset > 0
                    && journal.offset + blob.bytes.len() as u64 > JOURNAL_SEGMENT_SIZE
            }) {
                self.sync_backing()?;
                self.journal = None;
            }
            if self.journal.is_none() {
                self.journal = Some(Journal::create(&self.path)?);
            }
            tip = Some(self.journal.as_mut().unwrap().append(&blob)?);
            self.entries.journal += (transaction.changes.len() + transaction.segments.len()) as u64;
        }
        // Objects, log frames and index transactions must precede their new root.
        self.sync_backing()?;
        let index = View { tip, counters };
        if index == self.anchor.index && catalog == self.anchor.catalog {
            return Ok(());
        }
        self.publish(
            key,
            ecc,
            Anchor {
                version: VERSION,
                index,
                catalog,
            },
        )
    }

    /// The caller must record every change to the supplied roots.
    pub fn checkpoint(
        &mut self,
        key: &Key,
        ecc: EccParams,
        roots: &Roots,
        counters: Counters,
        catalog: Option<Reference>,
        retained: &[View],
    ) -> BkfsResult<View> {
        self.check_rw()?;
        if self.checkpoint_refs.is_some() {
            self.commit(key, ecc, counters, catalog)?;
            self.collect_metadata(key, self.checkpoint_refs.as_deref().unwrap(), retained)?;
            return Ok(self.anchor.index);
        }
        let (index, references) = View::write_checkpoint(&self.path, key, ecc, roots, counters)?;
        self.sync_backing()?;
        self.publish(
            key,
            ecc,
            Anchor {
                version: VERSION,
                index,
                catalog,
            },
        )?;
        self.journal = None;
        self.checkpoint_refs = Some(references);
        self.entries = Entries {
            checkpoint: roots.entry_count(),
            journal: 0,
        };
        self.collect_metadata(key, self.checkpoint_refs.as_deref().unwrap(), retained)?;
        Ok(index)
    }

    fn collect_metadata(
        &self,
        key: &Key,
        references: &[Reference],
        retained: &[View],
    ) -> BkfsResult<()> {
        let mut seen: HashSet<_> = references
            .iter()
            .copied()
            .map(IndexReference::Checkpoint)
            .collect();
        for view in retained {
            let mut walker = view.transactions(&self.path, key);
            while let Some(reference) = walker.pending {
                if !seen.insert(reference) {
                    break;
                }
                walker.next().unwrap()?;
            }
        }
        let mut keep = HashSet::new();
        let mut journals = HashSet::new();
        for reference in seen {
            match reference {
                IndexReference::Checkpoint(reference) => {
                    keep.insert(reference.path(&self.path));
                }
                IndexReference::Journal { segment, .. } => {
                    journals.insert(segment);
                }
            }
        }
        if let Some(reference) = self.anchor.catalog {
            keep.insert(reference.path(&self.path));
        }
        let pool = Reference::directory(&self.path);
        match std::fs::read_dir(&pool) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry?;
                    if entry.file_type()?.is_file() && !keep.contains(&entry.path()) {
                        std::fs::remove_file(entry.path())?;
                    }
                }
                File::open(pool)?.sync_all()?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        for entry in std::fs::read_dir(&self.path)? {
            let entry = entry?;
            if journal_id(&entry.file_name()).is_some_and(|id| !journals.contains(&id)) {
                std::fs::remove_file(entry.path())?;
            }
        }
        self.directory.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            ..Default::default()
        }
    }

    fn commit(state: &mut State, key: &Key, seq: u64) {
        state
            .commit(
                key,
                EccParams::default(),
                Counters {
                    next_seq: seq,
                    max_inode: 7,
                },
                None,
            )
            .unwrap();
    }

    fn journal(reference: IndexReference) -> (u64, u64, u32) {
        let IndexReference::Journal {
            segment,
            offset,
            len,
            ..
        } = reference
        else {
            panic!("not a packed transaction")
        };
        (segment, offset, len)
    }

    // The filter remains installed until this thread exits.
    fn inject_fsync_eio(fd: std::os::fd::RawFd) {
        let mut filter = [
            libc::sock_filter {
                code: (libc::BPF_LD | libc::BPF_W | libc::BPF_ABS) as u16,
                jt: 0,
                jf: 0,
                k: 0,
            },
            libc::sock_filter {
                code: (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16,
                jt: 0,
                jf: 3,
                k: libc::SYS_fsync as u32,
            },
            libc::sock_filter {
                code: (libc::BPF_LD | libc::BPF_W | libc::BPF_ABS) as u16,
                jt: 0,
                jf: 0,
                k: 16,
            },
            libc::sock_filter {
                code: (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16,
                jt: 0,
                jf: 1,
                k: fd as u32,
            },
            libc::sock_filter {
                code: (libc::BPF_RET | libc::BPF_K) as u16,
                jt: 0,
                jf: 0,
                k: libc::SECCOMP_RET_ERRNO | libc::EIO as u32,
            },
            libc::sock_filter {
                code: (libc::BPF_RET | libc::BPF_K) as u16,
                jt: 0,
                jf: 0,
                k: libc::SECCOMP_RET_ALLOW,
            },
        ];
        let program = libc::sock_fprog {
            len: filter.len() as u16,
            filter: filter.as_mut_ptr(),
        };
        assert_eq!(
            unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) },
            0
        );
        assert_eq!(
            unsafe { libc::prctl(libc::PR_SET_SECCOMP, libc::SECCOMP_MODE_FILTER, &program) },
            0
        );
    }

    #[test]
    fn backing_sync_failure_blocks_publication_even_after_a_successful_barrier() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x81; 32]);
        let mut state = initial(dir.path(), &key);
        let before = std::fs::read(dir.path().join(ANCHOR)).unwrap();
        state
            .record(RecordKey::Inode(7), Some(location(0)))
            .unwrap();
        let recovered = state.directory.try_clone().unwrap();
        inject_fsync_eio(state.directory.as_raw_fd());
        let result = state.commit(
            &key,
            EccParams::default(),
            Counters {
                next_seq: 1,
                max_inode: 7,
            },
            None,
        );
        let _failed_handle = std::mem::replace(&mut state.directory, recovered);
        assert_eq!(result.unwrap_err().to_errno(), libc::EIO);
        state.sync_backing().unwrap();
        assert_eq!(state.check_rw().unwrap_err().to_errno(), libc::EIO);
        assert!(state
            .record(RecordKey::Inode(8), Some(location(128)))
            .is_err());
        assert!(state
            .commit(
                &key,
                EccParams::default(),
                Counters {
                    next_seq: 2,
                    max_inode: 8
                },
                None
            )
            .is_err());
        assert!(state
            .checkpoint(
                &key,
                EccParams::default(),
                &Roots::default(),
                state.counters(),
                None,
                &[]
            )
            .is_err());
        assert_eq!(std::fs::read(dir.path().join(ANCHOR)).unwrap(), before);
        let (reopened, roots) = State::open(dir.path(), &key).unwrap();
        assert!(reopened.check_rw().is_ok());
        assert!(roots.index.is_empty());
    }

    #[test]
    fn incremental_updates_and_tombstones_survive_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x81; 32]);
        let mut state = initial(dir.path(), &key);
        let object = RecordKey::Block(9, 0);
        state
            .record(RecordKey::Inode(7), Some(location(0)))
            .unwrap();
        state
            .record(RecordKey::Content(7), Some(location(128)))
            .unwrap();
        state.record(object, Some(location(256))).unwrap();
        commit(&mut state, &key, 3);
        state
            .record(RecordKey::Inode(7), Some(location(384)))
            .unwrap();
        state.record(RecordKey::Content(7), None).unwrap();
        commit(&mut state, &key, 5);
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
        assert_eq!(roots.get(object), Some(&location(256)));
    }

    #[test]
    fn referenced_transaction_cannot_be_replaced_by_an_older_valid_one() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x82; 32]);
        let mut state = initial(dir.path(), &key);
        state
            .record(RecordKey::Inode(7), Some(location(0)))
            .unwrap();
        commit(&mut state, &key, 1);
        let (old_segment, old_offset, old_len) = journal(state.view().tip.unwrap());
        state
            .record(RecordKey::Inode(7), Some(location(128)))
            .unwrap();
        commit(&mut state, &key, 2);
        let (segment, offset, len) = journal(state.view().tip.unwrap());
        assert!(old_len <= len);
        let mut old = vec![0; old_len as usize];
        File::open(journal_path(dir.path(), old_segment))
            .unwrap()
            .read_exact_at(&mut old, old_offset)
            .unwrap();
        OpenOptions::new()
            .write(true)
            .open(journal_path(dir.path(), segment))
            .unwrap()
            .write_all_at(&old, offset)
            .unwrap();
        assert!(State::open(dir.path(), &key).is_err());
    }

    #[test]
    fn absent_anchor_or_referenced_transaction_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x83; 32]);
        assert!(State::open(dir.path(), &key).is_err());
        let mut state = initial(dir.path(), &key);
        state
            .record(RecordKey::Inode(7), Some(location(0)))
            .unwrap();
        commit(&mut state, &key, 1);
        let (id, _, _) = journal(state.view().tip.unwrap());
        std::fs::remove_file(journal_path(dir.path(), id)).unwrap();
        assert!(State::open(dir.path(), &key).is_err());
    }

    #[test]
    fn interrupted_publication_keeps_prior_index_and_can_retry() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x84; 32]);
        let mut state = initial(dir.path(), &key);
        state
            .record(RecordKey::Inode(7), Some(location(0)))
            .unwrap();
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
        commit(&mut state, &key, 1);
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
        state
            .record(RecordKey::Inode(7), Some(location(0)))
            .unwrap();
        commit(&mut state, &key, 1);
        let old = journal_path(dir.path(), journal(state.view().tip.unwrap()).0);
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
            state
                .record(RecordKey::Inode(id), Some(location(id * 128)))
                .unwrap();
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
                &[],
            )
            .unwrap();
        assert!(!old.exists());
        assert!(catalog.path(dir.path()).exists());
        assert_eq!(
            std::fs::read_dir(Reference::directory(dir.path()))
                .unwrap()
                .count(),
            3
        );
        let (opened, restored) = State::open(dir.path(), &key).unwrap();
        assert_eq!(restored, roots);
        assert_eq!(opened.catalog(), Some(catalog));
        state
            .record(RecordKey::Inode(7), Some(location(999)))
            .unwrap();
        commit(&mut state, &key, 20_001);
        assert_eq!(
            State::open(dir.path(), &key).unwrap().1.index.get(&7),
            Some(&location(999))
        );
    }

    #[test]
    fn retained_views_keep_their_metadata_and_versions_separate() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x86; 32]);
        let mut state = initial(dir.path(), &key);
        let mut original = Roots::default();
        original.index.insert(7, location(0));
        state
            .record(RecordKey::Inode(7), Some(location(0)))
            .unwrap();
        let snapshot = state
            .checkpoint(
                &key,
                EccParams::default(),
                &original,
                Counters {
                    next_seq: 1,
                    max_inode: 7,
                },
                None,
                &[],
            )
            .unwrap();
        state
            .record(RecordKey::Inode(7), Some(location(128)))
            .unwrap();
        commit(&mut state, &key, 2);
        let mut current = original.clone();
        current.index.insert(7, location(128));
        state
            .checkpoint(
                &key,
                EccParams::default(),
                &current,
                Counters {
                    next_seq: 2,
                    max_inode: 7,
                },
                None,
                &[snapshot],
            )
            .unwrap();
        assert_eq!(snapshot.load(dir.path(), &key).unwrap(), original);
        assert_eq!(State::open(dir.path(), &key).unwrap().1, current);
        state
            .checkpoint(
                &key,
                EccParams::default(),
                &current,
                Counters {
                    next_seq: 2,
                    max_inode: 7,
                },
                None,
                &[],
            )
            .unwrap();
        assert!(snapshot.load(dir.path(), &key).is_err());
    }

    #[test]
    fn durable_updates_share_packed_files_and_restart_ignores_uncommitted_tail() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x87; 32]);
        let mut state = initial(dir.path(), &key);
        for step in 1..=40 {
            state
                .record(RecordKey::Inode(7), Some(location(step * 128)))
                .unwrap();
            commit(&mut state, &key, step);
        }
        assert_eq!(
            std::fs::read_dir(dir.path())
                .unwrap()
                .filter(|entry| journal_id(&entry.as_ref().unwrap().file_name()).is_some())
                .count(),
            1
        );
        let original = state.view();
        drop(state);
        let (mut state, _) = State::open(dir.path(), &key).unwrap();
        state
            .record(RecordKey::Inode(7), Some(location(9000)))
            .unwrap();
        commit(&mut state, &key, 41);
        let (id, _, _) = journal(state.view().tip.unwrap());
        assert_ne!(id, journal(original.tip.unwrap()).0);
        let mut file = OpenOptions::new()
            .append(true)
            .open(journal_path(dir.path(), id))
            .unwrap();
        file.write_all(b"uncommitted torn data").unwrap();
        assert_eq!(
            State::open(dir.path(), &key).unwrap().1.index.get(&7),
            Some(&location(9000))
        );
    }

    #[test]
    fn a_torn_append_leaves_the_previous_commit_readable() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x88; 32]);
        let mut state = initial(dir.path(), &key);
        state
            .record(RecordKey::Inode(7), Some(location(128)))
            .unwrap();
        commit(&mut state, &key, 1);
        let anchor = std::fs::read(dir.path().join(ANCHOR)).unwrap();
        state
            .record(RecordKey::Inode(8), Some(location(256)))
            .unwrap();
        commit(&mut state, &key, 2);
        let (id, offset, _) = journal(state.view().tip.unwrap());
        drop(state);
        std::fs::write(dir.path().join(ANCHOR), anchor).unwrap();
        let block = offset / JOURNAL_ALIGN * JOURNAL_ALIGN;
        OpenOptions::new()
            .write(true)
            .open(journal_path(dir.path(), id))
            .unwrap()
            .write_all_at(&[0; JOURNAL_ALIGN as usize], block)
            .unwrap();
        let (_, roots) = State::open(dir.path(), &key).unwrap();
        assert_eq!(roots.index.get(&7), Some(&location(128)));
        assert_eq!(roots.index.get(&8), None);
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
            .record(RecordKey::Inode(7), Some(location(0)))
            .unwrap();
        let view = state
            .checkpoint(
                &key,
                EccParams::default(),
                &roots,
                Counters {
                    next_seq: 1,
                    max_inode: 7,
                },
                None,
                &[],
            )
            .unwrap();
        assert_eq!(
            state
                .checkpoint(
                    &key,
                    EccParams::default(),
                    &roots,
                    view.counters,
                    None,
                    &[view]
                )
                .unwrap(),
            view
        );
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
        let key = Key::from([0x8b; 32]);
        let mut state = initial(&original, &key);
        std::fs::rename(&original, &moved).unwrap();
        state
            .record(RecordKey::Inode(7), Some(location(0)))
            .unwrap();
        commit(&mut state, &key, 1);
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
        state
            .record(RecordKey::Inode(7), Some(location(0)))
            .unwrap();
        let anchor = dir.path().join(ANCHOR);
        let saved = dir.path().join("saved");
        std::fs::rename(&anchor, &saved).unwrap();
        std::fs::create_dir(&anchor).unwrap();
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
        std::fs::remove_dir(&anchor).unwrap();
        std::fs::rename(saved, anchor).unwrap();
        assert!(state
            .record(RecordKey::Inode(8), Some(location(0)))
            .is_err());
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
        let (reopened, roots) = State::open(dir.path(), &key).unwrap();
        reopened.check_rw().unwrap();
        assert!(roots.index.is_empty());
    }

    #[test]
    fn corrected_root_and_transaction_retain_current_references() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x89; 32]);
        let mut state = initial(dir.path(), &key);
        state
            .record(RecordKey::Inode(7), Some(location(0)))
            .unwrap();
        commit(&mut state, &key, 1);
        let anchor = dir.path().join(ANCHOR);
        let mut bytes = std::fs::read(&anchor).unwrap();
        let middle = bytes.len() / 2;
        bytes[middle] ^= 1;
        std::fs::write(anchor, bytes).unwrap();
        let (id, offset, len) = journal(state.view().tip.unwrap());
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(journal_path(dir.path(), id))
            .unwrap();
        let position = offset + len as u64 / 2;
        let mut byte = [0];
        file.read_exact_at(&mut byte, position).unwrap();
        byte[0] ^= 1;
        file.write_all_at(&byte, position).unwrap();
        assert_eq!(
            State::open(dir.path(), &key).unwrap().1.index.get(&7),
            Some(&location(0))
        );
    }

    #[test]
    fn replacing_the_whole_anchor_selects_its_genuine_older_history() {
        let dir = tempfile::tempdir().unwrap();
        let key = Key::from([0x8c; 32]);
        let mut state = initial(dir.path(), &key);
        let old_anchor = std::fs::read(dir.path().join(ANCHOR)).unwrap();
        state
            .record(RecordKey::Inode(7), Some(location(0)))
            .unwrap();
        commit(&mut state, &key, 1);
        std::fs::write(dir.path().join(ANCHOR), old_anchor).unwrap();
        let (opened, roots) = State::open(dir.path(), &key).unwrap();
        assert_eq!(opened.counters().next_seq, 0);
        assert!(roots.index.is_empty());
    }
}
