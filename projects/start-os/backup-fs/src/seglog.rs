use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

use chacha20::Key;
use serde::{Deserialize, Serialize};

use crate::error::{BkfsError, BkfsResult, BkfsResultExt};
use crate::inode::{Attributes, Inode};
use crate::metadata::Reference;
use crate::serde::{data_config, decode, encode};
use crate::state::{Counters, State, View};
use crate::vault::{self, AuthenticationTag, Binding, EccParams};

fn decode_frame(key: &Key, loc: Location, bytes: &[u8], binding: Binding) -> BkfsResult<Record> {
    let (seq, payload_len) =
        parse_header(bytes).ok_or_else(|| BkfsError::unsupported("invalid log header"))?;
    if seq != loc.seq
        || payload_len != loc.payload_len as usize
        || frame_size(payload_len) != loc.len as u64
    {
        return Err(BkfsError::unsupported("log reference mismatch"));
    }
    let payload = bytes
        .get(HEADER_LEN..HEADER_LEN + payload_len)
        .ok_or_else(|| BkfsError::unsupported("truncated log frame"))?;
    let plain = vault::open_bound(payload, key, binding)?.verify_reference(&loc.tag)?;
    decode(&plain, data_config())
}

const MAGIC: [u8; 4] = *b"BKL1";
const HEADER_LEN: usize = 24;
const ALIGN: u64 = 8;
pub(crate) const INDEX_CACHE_NAME: &str = "index.cache";

pub(crate) fn next_sequence(seq: u64) -> BkfsResult<u64> {
    seq.checked_add(1)
        .ok_or_else(|| BkfsError::unsupported("record sequence exhausted"))
}

/// Variant order is part of every filesystem format's record encoding.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Record {
    Inode {
        inode: u64,
        attrs: Attributes,
    },
    Tombstone {
        inode: u64,
    },
    Content {
        id: u64,
        bytes: Vec<u8>,
    },
    ContentTombstone {
        id: u64,
    },
    Block {
        content: u64,
        index: u64,
        bytes: Vec<u8>,
    },
    Bucket {
        directory: u64,
        generation: u64,
        index: u32,
        bytes: Vec<u8>,
    },
}

impl Record {
    pub(crate) fn key(&self) -> RecordKey {
        match self {
            Self::Inode { inode, .. } | Self::Tombstone { inode } => RecordKey::Inode(*inode),
            Self::Content { id, .. } | Self::ContentTombstone { id } => RecordKey::Content(*id),
            Self::Block { content, index, .. } => RecordKey::Block(*content, *index),
            Self::Bucket {
                directory,
                generation,
                index,
                ..
            } => RecordKey::Bucket(*directory, *generation, *index),
        }
    }

    fn verify_key(self, key: RecordKey) -> BkfsResult<Self> {
        if self.key() != key {
            return Err(BkfsError::unsupported("log record identity mismatch"));
        }
        Ok(self)
    }

    pub(crate) fn is_tombstone(&self) -> bool {
        matches!(self, Self::Tombstone { .. } | Self::ContentTombstone { .. })
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum RecordKey {
    Inode(u64),
    Content(u64),
    Block(u64, u64),
    Bucket(u64, u64, u32),
}

impl RecordKey {
    pub(crate) fn stream(self) -> Stream {
        match self {
            Self::Content(_) | Self::Block(..) => Stream::Data,
            Self::Inode(_) | Self::Bucket(..) => Stream::Metadata,
        }
    }
}

pub struct SealedRecord {
    key: RecordKey,
    sealed: vault::Sealed,
    tombstone: bool,
}

impl SealedRecord {
    pub(crate) fn len(&self) -> u64 {
        self.sealed.bytes.len() as u64
    }
}

pub(crate) fn seal_record(
    key: &Key,
    ecc: EccParams,
    record: Record,
    binding: Binding,
) -> BkfsResult<SealedRecord> {
    let sealed = vault::seal_bound(&encode(&record, data_config())?, key, ecc, binding);
    Ok(SealedRecord {
        key: record.key(),
        sealed,
        tombstone: record.is_tombstone(),
    })
}

#[cfg(test)]
pub fn seal_inode(
    key: &Key,
    ecc: EccParams,
    inode: Inode,
    attrs: &Attributes,
) -> BkfsResult<SealedRecord> {
    seal_record(
        key,
        ecc,
        Record::Inode {
            inode: inode.0,
            attrs: attrs.clone(),
        },
        Binding::Record,
    )
}
#[cfg(test)]
pub fn seal_tombstone(key: &Key, ecc: EccParams, inode: Inode) -> BkfsResult<SealedRecord> {
    seal_record(
        key,
        ecc,
        Record::Tombstone { inode: inode.0 },
        Binding::Record,
    )
}
pub fn seal_block(
    key: &Key,
    ecc: EccParams,
    content: u64,
    index: u64,
    bytes: Vec<u8>,
) -> BkfsResult<SealedRecord> {
    seal_record(
        key,
        ecc,
        Record::Block {
            content,
            index,
            bytes,
        },
        Binding::Record,
    )
}
pub fn seal_bucket(
    key: &Key,
    ecc: EccParams,
    directory: u64,
    generation: u64,
    index: u32,
    bytes: Vec<u8>,
) -> BkfsResult<SealedRecord> {
    seal_record(
        key,
        ecc,
        Record::Bucket {
            directory,
            generation,
            index,
            bytes,
        },
        Binding::Record,
    )
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Roots {
    pub index: HashMap<u64, Location>,
    pub content: HashMap<u64, Location>,
    pub blocks: HashMap<(u64, u64), Location>,
    pub buckets: HashMap<(u64, u64, u32), Location>,
    /// Committed segment lengths.
    pub sizes: HashMap<u64, u64>,
}

impl Roots {
    pub(crate) fn get(&self, key: RecordKey) -> Option<&Location> {
        match key {
            RecordKey::Inode(id) => self.index.get(&id),
            RecordKey::Content(id) => self.content.get(&id),
            RecordKey::Block(content, index) => self.blocks.get(&(content, index)),
            RecordKey::Bucket(directory, generation, index) => {
                self.buckets.get(&(directory, generation, index))
            }
        }
    }

    pub(crate) fn update(
        &mut self,
        key: RecordKey,
        location: Option<Location>,
    ) -> Option<Location> {
        fn replace<K: std::hash::Hash + Eq>(
            index: &mut HashMap<K, Location>,
            key: K,
            location: Option<Location>,
        ) -> Option<Location> {
            match location {
                Some(location) => index.insert(key, location),
                None => index.remove(&key),
            }
        }
        match key {
            RecordKey::Inode(id) => replace(&mut self.index, id, location),
            RecordKey::Content(id) => replace(&mut self.content, id, location),
            RecordKey::Block(content, index) => {
                replace(&mut self.blocks, (content, index), location)
            }
            RecordKey::Bucket(directory, generation, index) => {
                replace(&mut self.buckets, (directory, generation, index), location)
            }
        }
    }

    pub(crate) fn entries(&self) -> impl Iterator<Item = (RecordKey, Location)> + '_ {
        self.index
            .iter()
            .map(|(&id, &loc)| (RecordKey::Inode(id), loc))
            .chain(
                self.content
                    .iter()
                    .map(|(&id, &loc)| (RecordKey::Content(id), loc)),
            )
            .chain(
                self.blocks
                    .iter()
                    .map(|(&(content, index), &loc)| (RecordKey::Block(content, index), loc)),
            )
            .chain(
                self.buckets
                    .iter()
                    .map(|(&(directory, generation, index), &loc)| {
                        (RecordKey::Bucket(directory, generation, index), loc)
                    }),
            )
    }

    pub fn segments(&self) -> impl Iterator<Item = u64> + '_ {
        self.entries().map(|(_, loc)| loc.segment)
    }

    pub(crate) fn entry_count(&self) -> u64 {
        (self.index.len()
            + self.content.len()
            + self.blocks.len()
            + self.buckets.len()
            + self.sizes.len()) as u64
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub segment: u64,
    pub offset: u64,
    pub len: u32,
    pub seq: u64,
    pub payload_len: u32,
    pub tag: AuthenticationTag,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct SegMeta {
    pub total: u64,
    pub live: u64,
}

pub(crate) fn segment_path(dir: &Path, id: u64) -> PathBuf {
    dir.join(format!("{id:016x}.seg"))
}

pub(crate) fn list_segments(dir: &Path) -> BkfsResult<Vec<(u64, u64)>> {
    let mut segments = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if let Some(id) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.strip_suffix(".seg"))
            .and_then(|hex| u64::from_str_radix(hex, 16).ok())
        {
            segments.push((id, entry.metadata()?.len()));
        }
    }
    segments.sort_unstable();
    Ok(segments)
}

fn frame_size(payload_len: usize) -> u64 {
    (HEADER_LEN as u64 + payload_len as u64).div_ceil(ALIGN) * ALIGN
}

pub(crate) fn encode_frame(seq: u64, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; frame_size(payload.len()) as usize];
    bytes[..4].copy_from_slice(&MAGIC);
    bytes[8..16].copy_from_slice(&seq.to_le_bytes());
    bytes[16..20].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    let crc = crc32fast::hash(&bytes[8..24]);
    bytes[4..8].copy_from_slice(&crc.to_le_bytes());
    bytes[HEADER_LEN..HEADER_LEN + payload.len()].copy_from_slice(payload);
    bytes
}

fn parse_header(bytes: &[u8]) -> Option<(u64, usize)> {
    if bytes.len() < HEADER_LEN || bytes[..4] != MAGIC || bytes[20..24] != [0; 4] {
        return None;
    }
    let crc = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    if crc32fast::hash(&bytes[8..24]) != crc {
        return None;
    }
    Some((
        u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
        u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize,
    ))
}

pub(crate) struct Frame<'a> {
    pub offset: usize,
    pub seq: u64,
    pub payload: &'a [u8],
    pub len: u32,
}
pub(crate) struct Frames<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Frames<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
}
impl<'a> Iterator for Frames<'a> {
    type Item = Frame<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        while self.position + HEADER_LEN <= self.bytes.len() {
            let offset = self.position;
            let Some((seq, payload_len)) = parse_header(&self.bytes[offset..]) else {
                self.position += self.bytes[offset + 1..]
                    .windows(MAGIC.len())
                    .position(|window| window == MAGIC)?
                    + 1;
                continue;
            };
            let end = offset + HEADER_LEN + payload_len;
            if end > self.bytes.len() {
                self.position = self.bytes.len();
                return None;
            }
            let len = u32::try_from(frame_size(payload_len)).ok()?;
            self.position = (offset + len as usize).min(self.bytes.len());
            return Some(Frame {
                offset,
                seq,
                payload: &self.bytes[offset + HEADER_LEN..end],
                len,
            });
        }
        None
    }
}

use crate::segwriter::SegmentWriter;
pub(crate) use crate::segwriter::Stream;

const OPEN_READERS: usize = 64;
const READ_WINDOW: u64 = 1 << 20;
const CACHED_WINDOWS: usize = 16;
/// Frames up to this length are read through cached windows.
const WINDOWED_FRAME: u32 = 64 << 10;

/// Segment ids present in the directory, ignoring their lengths.
pub(crate) fn segment_ids(dir: &Path) -> BkfsResult<Vec<u64>> {
    let mut ids = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        if let Some(id) = entry?
            .file_name()
            .to_str()
            .and_then(|name| name.strip_suffix(".seg"))
            .and_then(|hex| u64::from_str_radix(hex, 16).ok())
        {
            ids.push(id);
        }
    }
    ids.sort_unstable();
    Ok(ids)
}

enum Persistence {
    Current(State),
    Legacy(crate::legacy::Log),
}

impl Persistence {
    fn directory(&self) -> &Path {
        match self {
            Self::Current(state) => state.directory_path(),
            Self::Legacy(log) => &log.path,
        }
    }
    fn binding(&self) -> Binding {
        match self {
            Self::Current(_) => Binding::Record,
            Self::Legacy(_) => Binding::Legacy,
        }
    }
    fn check_rw(&self) -> BkfsResult<()> {
        match self {
            Self::Current(state) => state.check_rw(),
            Self::Legacy(log) => log.check_rw(),
        }
    }
    fn catalog(&self) -> Option<Reference> {
        match self {
            Self::Current(state) => state.catalog(),
            Self::Legacy(_) => None,
        }
    }
    fn has_pending(&self) -> bool {
        match self {
            Self::Current(state) => state.has_pending(),
            Self::Legacy(log) => log.dirty,
        }
    }
    fn record(&mut self, key: RecordKey, location: Option<Location>) -> BkfsResult<()> {
        match self {
            Self::Current(state) => state.record(key, location),
            Self::Legacy(log) => {
                log.dirty = true;
                Ok(())
            }
        }
    }
    fn record_size(&mut self, segment: u64, len: u64) -> BkfsResult<()> {
        match self {
            Self::Current(state) => state.record_size(segment, len),
            Self::Legacy(_) => Ok(()),
        }
    }
}

pub struct SegmentLog {
    key: Key,
    ecc: EccParams,
    state: Persistence,
    roots: Roots,
    writer: Option<SegmentWriter>,
    next_seq: u64,
    max_inode: u64,
    pinned: HashSet<u64>,
    seg_meta: HashMap<u64, SegMeta>,
    readers: std::cell::RefCell<Vec<(u64, std::sync::Arc<File>)>>,
    /// Committed segment bytes, most recently used last.
    windows: std::cell::RefCell<Vec<(u64, u64, std::sync::Arc<Vec<u8>>)>>,
    /// Directory buckets written since the last commit, appended by it.
    staged_buckets: HashMap<(u64, u64, u32), Vec<u8>>,
    appended: u64,
}

#[cfg(test)]
#[path = "seglog_tests.rs"]
mod tests;

impl Drop for SegmentLog {
    fn drop(&mut self) {
        if self.writer.is_some()
            && (self.state.has_pending() || self.has_unsynced() || !self.staged_buckets.is_empty())
        {
            if let Err(error) = self.sync() {
                log::error!("failed to commit backup index on close: {error}");
            }
        }
    }
}

impl SegmentLog {
    #[cfg(test)]
    pub fn open(dir: PathBuf, key: Key) -> BkfsResult<Self> {
        Self::open_sized(dir, key, 8 * 1024 * 1024, EccParams::default(), false)
    }

    pub(crate) fn open_sized(
        dir: PathBuf,
        key: Key,
        segment_size: u64,
        ecc: EccParams,
        readonly: bool,
    ) -> BkfsResult<Self> {
        Self::open_format(
            dir,
            key,
            segment_size,
            ecc,
            readonly,
            crate::superblock::FORMAT_VERSION,
        )
    }

    pub(crate) fn open_format(
        dir: PathBuf,
        key: Key,
        segment_size: u64,
        ecc: EccParams,
        readonly: bool,
        format: u32,
    ) -> BkfsResult<Self> {
        let (state, roots, counters) = if format == 1 {
            let (log, mut roots, counters) = crate::legacy::Log::open(&dir, &key)?;
            roots.sizes = list_segments(&log.path)?.into_iter().collect();
            (Persistence::Legacy(log), roots, counters)
        } else {
            let (state, roots) = State::open(&dir, &key)?;
            let counters = state.counters();
            (Persistence::Current(state), roots, counters)
        };
        let writer = if readonly {
            None
        } else {
            let directory = state.directory();
            let mut next = roots.sizes.keys().max().map_or(0, |&id| id + 1);
            for id in segment_ids(directory)? {
                if !roots.sizes.contains_key(&id) {
                    std::fs::remove_file(segment_path(directory, id))?;
                }
                next = next.max(id + 1);
            }
            Some(SegmentWriter::new(directory, segment_size, next))
        };
        let mut log = Self {
            key,
            ecc,
            state,
            roots,
            writer,
            next_seq: counters.next_seq,
            max_inode: counters.max_inode,
            pinned: HashSet::new(),
            seg_meta: HashMap::new(),
            readers: Default::default(),
            windows: Default::default(),
            staged_buckets: HashMap::new(),
            appended: 0,
        };
        for (&id, &total) in &log.roots.sizes {
            log.seg_meta.insert(id, SegMeta { total, live: 0 });
        }
        log.recompute_live();
        Ok(log)
    }
    pub(crate) fn directory(&self) -> &Path {
        self.state.directory()
    }
    pub(crate) fn check_rw(&self) -> BkfsResult<()> {
        let Some(writer) = &self.writer else {
            return BkfsResult::errno_notrace(libc::EROFS);
        };
        writer.check()?;
        self.state.check_rw()
    }
    pub(crate) fn note_data_write(&mut self) {
        if let Persistence::Legacy(log) = &mut self.state {
            log.dirty = true;
        }
    }
    pub(crate) fn catalog_reference(&self) -> Option<Reference> {
        self.state.catalog()
    }
    pub(crate) fn load_view(&self, view: View) -> BkfsResult<Roots> {
        view.load(self.directory(), &self.key)
    }
    pub(crate) fn adopt_view(&mut self, view: View) -> BkfsResult<()> {
        self.roots = self.load_view(view)?;
        Ok(())
    }
    pub(crate) fn roots(&self) -> &Roots {
        &self.roots
    }
    pub(crate) fn pin(&mut self, segments: HashSet<u64>) {
        self.pinned = segments;
    }
    pub fn contains(&self, inode: Inode) -> bool {
        self.roots.index.contains_key(&inode.0)
    }
    pub fn max_inode(&self) -> u64 {
        self.max_inode
    }
    pub fn live_count(&self) -> usize {
        self.roots.index.len()
    }
    #[cfg(test)]
    pub fn content_count(&self) -> usize {
        self.roots.content.len()
    }

    pub fn read_at(&self, loc: Location) -> BkfsResult<Record> {
        decode_frame(
            &self.key,
            loc,
            &self.read_raw_frame(loc)?,
            self.state.binding(),
        )
    }
    pub(crate) fn read_indexed(&self, key: RecordKey, loc: Location) -> BkfsResult<Record> {
        self.read_at(loc)?.verify_key(key)
    }

    pub(crate) fn visit_records<'a>(
        &self,
        references: impl Iterator<Item = (RecordKey, &'a Location)>,
        mut visit: impl FnMut(Record) -> BkfsResult<()>,
    ) -> BkfsResult<()> {
        self.visit_record_results(references, |record| visit(record?))
    }

    /// Backing I/O errors stop traversal without reaching the visitor.
    pub(crate) fn visit_record_results<'a>(
        &self,
        references: impl Iterator<Item = (RecordKey, &'a Location)>,
        mut visit: impl FnMut(BkfsResult<Record>) -> BkfsResult<()>,
    ) -> BkfsResult<()> {
        let mut references: Vec<_> = references.collect();
        references.sort_unstable_by_key(|(_, loc)| (loc.segment, loc.offset));
        let mut reader = crate::aligned_io::RangeReader::default();
        for (key, &loc) in references {
            let bytes = match self.pending_frame(loc) {
                Some(bytes) => bytes,
                None => {
                    let mut bytes = vec![0; loc.len as usize];
                    reader
                        .read_exact(
                            &segment_path(self.directory(), loc.segment),
                            loc.offset,
                            &mut bytes,
                        )
                        .map_err(BkfsError::referenced_io)?;
                    bytes
                }
            };
            visit(
                decode_frame(&self.key, loc, &bytes, self.state.binding())
                    .and_then(|record| record.verify_key(key)),
            )?;
        }
        Ok(())
    }

    fn pending_frame(&self, loc: Location) -> Option<Vec<u8>> {
        let frame = self.writer.as_ref()?.pending(loc.segment, loc.offset)?;
        Some(frame.get(..loc.len as usize).unwrap_or(&frame).to_vec())
    }

    fn reader(&self, segment: u64) -> BkfsResult<std::sync::Arc<File>> {
        let mut readers = self.readers.borrow_mut();
        if let Some(position) = readers.iter().position(|(id, _)| *id == segment) {
            let entry = readers.remove(position);
            let file = entry.1.clone();
            readers.push(entry);
            return Ok(file);
        }
        let file = std::sync::Arc::new(
            File::open(segment_path(self.directory(), segment))
                .map_err(BkfsError::referenced_io)?,
        );
        if readers.len() >= OPEN_READERS {
            readers.remove(0);
        }
        readers.push((segment, file.clone()));
        Ok(file)
    }

    fn read_raw_frame(&self, loc: Location) -> BkfsResult<Vec<u8>> {
        if let Some(bytes) = self.pending_frame(loc) {
            return Ok(bytes);
        }
        let end = loc.offset + loc.len as u64;
        let committed = self.roots.sizes.get(&loc.segment).copied().unwrap_or(0);
        if loc.len <= WINDOWED_FRAME && end <= committed {
            return self.read_window(loc.segment, loc.offset, loc.len as usize, committed);
        }
        let mut bytes = vec![0; loc.len as usize];
        self.reader(loc.segment)?
            .read_exact_at(&mut bytes, loc.offset)
            .map_err(BkfsError::referenced_io)?;
        Ok(bytes)
    }

    /// Committed bytes served from a cached window of their segment.
    fn read_window(
        &self,
        segment: u64,
        offset: u64,
        len: usize,
        committed: u64,
    ) -> BkfsResult<Vec<u8>> {
        let mut windows = self.windows.borrow_mut();
        let hit = windows.iter().position(|(id, base, bytes)| {
            *id == segment && *base <= offset && offset + len as u64 <= base + bytes.len() as u64
        });
        let (base, bytes) = match hit {
            Some(position) => {
                let entry = windows.remove(position);
                let found = (entry.1, entry.2.clone());
                windows.push(entry);
                found
            }
            None => {
                let mut base = offset / READ_WINDOW * READ_WINDOW;
                if offset + WINDOWED_FRAME as u64 > base + READ_WINDOW {
                    base = offset;
                }
                let mut bytes = vec![0; (committed - base).min(READ_WINDOW) as usize];
                self.reader(segment)?
                    .read_exact_at(&mut bytes, base)
                    .map_err(BkfsError::referenced_io)?;
                let bytes = std::sync::Arc::new(bytes);
                if windows.len() >= CACHED_WINDOWS {
                    windows.remove(0);
                }
                windows.push((segment, base, bytes.clone()));
                (base, bytes)
            }
        };
        let start = (offset - base) as usize;
        Ok(bytes[start..start + len].to_vec())
    }

    fn load_indexed(&self, key: RecordKey) -> BkfsResult<Option<Record>> {
        self.roots
            .get(key)
            .map(|&loc| self.read_indexed(key, loc))
            .transpose()
    }
    pub fn load(&self, inode: Inode) -> BkfsResult<Option<Attributes>> {
        match self.load_indexed(RecordKey::Inode(inode.0))? {
            Some(Record::Inode { attrs, .. }) => Ok(Some(attrs)),
            Some(_) => Err(BkfsError::unsupported("invalid inode reference")),
            None => Ok(None),
        }
    }
    pub fn load_content(&self, id: u64) -> BkfsResult<Option<Vec<u8>>> {
        match self.load_indexed(RecordKey::Content(id))? {
            Some(Record::Content { bytes, .. }) => Ok(Some(bytes)),
            Some(_) => Err(BkfsError::unsupported("invalid content reference")),
            None => Ok(None),
        }
    }
    pub(crate) fn load_block(&self, content: u64, index: u64) -> BkfsResult<Option<Vec<u8>>> {
        match self.load_indexed(RecordKey::Block(content, index))? {
            Some(Record::Block { bytes, .. }) => Ok(Some(bytes)),
            Some(_) => Err(BkfsError::unsupported("invalid block reference")),
            None => Ok(None),
        }
    }
    pub(crate) fn load_bucket(
        &self,
        directory: u64,
        generation: u64,
        index: u32,
    ) -> BkfsResult<Option<Vec<u8>>> {
        if let Some(plain) = self.staged_buckets.get(&(directory, generation, index)) {
            return Ok(Some(plain.clone()));
        }
        match self.load_indexed(RecordKey::Bucket(directory, generation, index))? {
            Some(Record::Bucket { bytes, .. }) => Ok(Some(bytes)),
            Some(_) => Err(BkfsError::unsupported("invalid directory bucket reference")),
            None => Ok(None),
        }
    }
    fn update(&mut self, key: RecordKey, location: Option<Location>) -> BkfsResult<()> {
        self.state.record(key, location)?;
        if let Some(old) = self.roots.update(key, location) {
            if let Some(meta) = self.seg_meta.get_mut(&old.segment) {
                meta.live = meta.live.saturating_sub(old.len as u64);
            }
        }
        if let Some(location) = location {
            self.seg_meta.entry(location.segment).or_default().live += location.len as u64;
        }
        Ok(())
    }
    fn write_frame(&mut self, stream: Stream, bytes: Vec<u8>) -> BkfsResult<(u64, u64)> {
        let len = bytes.len() as u64;
        let (segment, offset) = self.writer.as_mut().unwrap().append(stream, bytes)?;
        let meta = self.seg_meta.entry(segment).or_default();
        meta.total = meta.total.max(offset + len);
        self.appended += len;
        Ok((segment, offset))
    }
    pub fn append(&mut self, record: &SealedRecord) -> BkfsResult<()> {
        self.check_rw()?;
        let next = next_sequence(self.next_seq)?;
        let seq = self.next_seq;
        let bytes = encode_frame(seq, &record.sealed.bytes);
        let len = bytes.len() as u32;
        let (segment, offset) = self.write_frame(record.key.stream(), bytes)?;
        self.next_seq = next;
        if let RecordKey::Inode(id) = record.key {
            self.max_inode = self.max_inode.max(id);
        }
        let location = Location {
            segment,
            offset,
            len,
            seq,
            payload_len: record.sealed.bytes.len() as u32,
            tag: record.sealed.tag,
        };
        self.update(record.key, (!record.tombstone).then_some(location))
    }
    /// Indexes unauthenticated bytes; reads of the key fail.
    pub(crate) fn preserve_unreadable(&mut self, key: RecordKey, payload: &[u8]) -> BkfsResult<()> {
        self.check_rw()?;
        let next = next_sequence(self.next_seq)?;
        let seq = self.next_seq;
        let bytes = encode_frame(seq, payload);
        let len = bytes.len() as u32;
        let (segment, offset) = self.write_frame(key.stream(), bytes)?;
        self.next_seq = next;
        let location = Location {
            segment,
            offset,
            len,
            seq,
            payload_len: payload.len() as u32,
            tag: Default::default(),
        };
        self.update(key, Some(location))
    }
    /// Replaces a directory bucket at the next commit.
    pub(crate) fn stage_bucket(
        &mut self,
        directory: u64,
        generation: u64,
        index: u32,
        plain: Vec<u8>,
    ) -> BkfsResult<()> {
        self.check_rw()?;
        self.staged_buckets
            .insert((directory, generation, index), plain);
        Ok(())
    }
    fn append_staged_buckets(&mut self) -> BkfsResult<()> {
        let mut staged: Vec<_> = self.staged_buckets.drain().collect();
        staged.sort_unstable_by_key(|(key, _)| *key);
        for ((directory, generation, index), plain) in staged {
            let record = seal_bucket(&self.key, self.ecc, directory, generation, index, plain)?;
            self.append(&record)?;
        }
        Ok(())
    }
    /// Whether a removal changed the index.
    pub(crate) fn remove(&mut self, key: RecordKey) -> BkfsResult<bool> {
        self.check_rw()?;
        let staged = match key {
            RecordKey::Bucket(directory, generation, index) => self
                .staged_buckets
                .remove(&(directory, generation, index))
                .is_some(),
            _ => false,
        };
        if self.roots.get(key).is_none() {
            return Ok(staged);
        }
        self.update(key, None)?;
        Ok(true)
    }
    fn has_unsynced(&self) -> bool {
        self.writer
            .as_ref()
            .is_some_and(|writer| writer.has_unsynced())
    }
    /// Makes appended frames durable and records their segment lengths.
    fn flush_writes(&mut self) -> BkfsResult<()> {
        if self.writer.is_none() {
            return Ok(());
        }
        self.append_staged_buckets()?;
        let writer = self.writer.as_mut().unwrap();
        writer.sync()?;
        for (segment, len) in writer.take_lengths() {
            self.roots.sizes.insert(segment, len);
            self.seg_meta.entry(segment).or_default().total = len;
            self.state.record_size(segment, len)?;
        }
        Ok(())
    }
    pub fn sync(&mut self) -> BkfsResult<()> {
        if self.writer.is_none() {
            return Ok(());
        }
        self.flush_writes()?;
        let counters = self.counters();
        match &mut self.state {
            Persistence::Current(state) => {
                state.commit(&self.key, self.ecc, counters, state.catalog())
            }
            Persistence::Legacy(log) => log.sync(),
        }
    }
    fn counters(&self) -> Counters {
        Counters {
            next_seq: self.next_seq,
            max_inode: self.max_inode,
        }
    }
    pub(crate) fn publish_catalog(
        &mut self,
        reference: Reference,
        pins: HashSet<u64>,
    ) -> BkfsResult<()> {
        self.check_rw()?;
        self.flush_writes()?;
        let counters = self.counters();
        let Persistence::Current(state) = &mut self.state else {
            return Err(BkfsError::unsupported(
                "convert this backup before creating snapshots",
            ));
        };
        state.commit(&self.key, self.ecc, counters, Some(reference))?;
        self.pin(pins);
        Ok(())
    }
    /// Commits, then checkpoints once the journal outgrows its policy.
    pub fn close_session(&mut self, retained: &[View]) -> BkfsResult<()> {
        self.sync()?;
        let due = match &self.state {
            Persistence::Current(state) => state.checkpoint_due(),
            Persistence::Legacy(_) => true,
        };
        if due {
            self.save_checkpoint(retained)?;
        }
        Ok(())
    }
    /// The checkpoint view; `None` for a format-1 store.
    pub fn save_checkpoint(&mut self, retained: &[View]) -> BkfsResult<Option<View>> {
        self.check_rw()?;
        self.flush_writes()?;
        let counters = self.counters();
        match &mut self.state {
            Persistence::Current(state) => state
                .checkpoint(
                    &self.key,
                    self.ecc,
                    &self.roots,
                    counters,
                    state.catalog(),
                    retained,
                )
                .map(Some),
            Persistence::Legacy(log) => {
                log.checkpoint(&self.key, self.ecc, &self.roots, counters, &self.seg_meta)?;
                Ok(None)
            }
        }
    }
    pub(crate) fn retain_storage(
        &mut self,
        storage: &HashSet<RecordKey>,
        packed: &HashSet<u64>,
    ) -> BkfsResult<()> {
        self.check_rw()?;
        let remove: Vec<_> = self
            .roots
            .entries()
            .map(|(key, _)| key)
            .filter(|key| match key {
                RecordKey::Inode(_) => false,
                RecordKey::Content(id) => !packed.contains(id),
                RecordKey::Block(..) | RecordKey::Bucket(..) => !storage.contains(key),
            })
            .collect();
        for key in remove {
            self.update(key, None)?;
        }
        Ok(())
    }
    fn recompute_live(&mut self) {
        for meta in self.seg_meta.values_mut() {
            meta.live = 0;
        }
        for (_, loc) in self.roots.entries() {
            if let Some(meta) = self.seg_meta.get_mut(&loc.segment) {
                meta.live += loc.len as u64;
            }
        }
    }
    /// Relocates live frames out of mostly-dead segments, then deletes them.
    pub fn compact(&mut self, threshold: f64, budget: u64) -> BkfsResult<usize> {
        self.check_rw()?;
        let active: HashSet<u64> = self.writer.as_ref().unwrap().active_segments().collect();
        let mut ranked: Vec<_> = self
            .seg_meta
            .iter()
            .filter(|(&id, meta)| {
                !active.contains(&id)
                    && !self.pinned.contains(&id)
                    && meta.total > 0
                    && meta.total.saturating_sub(meta.live) as f64 / meta.total as f64 > threshold
            })
            .map(|(&id, meta)| (meta.live as f64 / meta.total as f64, id, meta.live))
            .collect();
        ranked.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let mut candidates = HashSet::new();
        let mut relocated = 0;
        for (_, id, live) in ranked {
            if relocated > 0 && relocated + live > budget {
                break;
            }
            relocated += live;
            candidates.insert(id);
        }
        if candidates.is_empty() {
            return Ok(0);
        }
        let mut live: Vec<_> = self
            .roots
            .entries()
            .filter(|(_, loc)| candidates.contains(&loc.segment))
            .collect();
        live.sort_unstable_by_key(|(_, loc)| (loc.segment, loc.offset));
        for (key, old) in live {
            let bytes = self.read_raw_frame(old)?;
            let (segment, offset) = self.write_frame(key.stream(), bytes)?;
            self.update(
                key,
                Some(Location {
                    segment,
                    offset,
                    ..old
                }),
            )?;
        }
        self.sync()?;
        for &id in &candidates {
            self.readers
                .borrow_mut()
                .retain(|(segment, _)| *segment != id);
            self.windows
                .borrow_mut()
                .retain(|(segment, ..)| *segment != id);
            match std::fs::remove_file(segment_path(self.directory(), id)) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    return Err(error.into())
                }
                _ => {}
            }
            self.seg_meta.remove(&id);
            self.roots.sizes.remove(&id);
            self.state.record_size(id, 0)?;
        }
        File::open(self.directory())?.sync_all()?;
        self.sync()?;
        Ok(candidates.len())
    }
    /// Bytes appended since the log was opened.
    pub(crate) fn appended(&self) -> u64 {
        self.appended
    }

    /// Forgets cached segment bytes after a test edits segment files.
    #[cfg(test)]
    pub(crate) fn drop_read_caches(&self) {
        self.readers.borrow_mut().clear();
        self.windows.borrow_mut().clear();
    }
    #[cfg(test)]
    pub fn put(&mut self, inode: Inode, attrs: &Attributes) -> BkfsResult<()> {
        let record = seal_inode(&self.key, self.ecc, inode, attrs)?;
        self.append(&record)
    }
    #[cfg(test)]
    pub fn tombstone(&mut self, inode: Inode) -> BkfsResult<()> {
        let record = seal_tombstone(&self.key, self.ecc, inode)?;
        self.append(&record)
    }
}
