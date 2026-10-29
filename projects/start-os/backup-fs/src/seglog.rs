use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use chacha20::Key;
use serde::{Deserialize, Serialize};

use crate::error::{BkfsError, BkfsResult, BkfsResultExt};
use crate::inode::{Attributes, Inode};
use crate::metadata::Reference;
use crate::objects::{ObjectId, ObjectVersion};
use crate::serde::{data_config, decode, encode};
use crate::state::{Counters, State, View};
use crate::vault::{self, AuthenticationTag, Binding, EccParams};

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
    Object {
        id: ObjectId,
        version: ObjectVersion,
    },
}

impl Record {
    pub(crate) fn key(&self) -> RecordKey {
        match self {
            Self::Inode { inode, .. } | Self::Tombstone { inode } => RecordKey::Inode(*inode),
            Self::Content { id, .. } | Self::ContentTombstone { id } => RecordKey::Content(*id),
            Self::Object { id, .. } => RecordKey::Object(*id),
        }
    }

    pub(crate) fn is_tombstone(&self) -> bool {
        matches!(self, Self::Tombstone { .. } | Self::ContentTombstone { .. })
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum RecordKey {
    Inode(u64),
    Content(u64),
    Object(ObjectId),
}

pub struct SealedRecord {
    key: RecordKey,
    sealed: vault::Sealed,
    tombstone: bool,
}

fn seal_record(key: &Key, ecc: EccParams, record: Record) -> BkfsResult<SealedRecord> {
    let sealed = vault::seal_bound(&encode(&record, data_config())?, key, ecc, Binding::Record);
    Ok(SealedRecord {
        key: record.key(),
        sealed,
        tombstone: record.is_tombstone(),
    })
}

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
    )
}
pub fn seal_tombstone(key: &Key, ecc: EccParams, inode: Inode) -> BkfsResult<SealedRecord> {
    seal_record(key, ecc, Record::Tombstone { inode: inode.0 })
}
pub fn seal_content(key: &Key, ecc: EccParams, id: u64, bytes: &[u8]) -> BkfsResult<SealedRecord> {
    seal_record(
        key,
        ecc,
        Record::Content {
            id,
            bytes: bytes.to_vec(),
        },
    )
}
pub fn seal_content_tombstone(key: &Key, ecc: EccParams, id: u64) -> BkfsResult<SealedRecord> {
    seal_record(key, ecc, Record::ContentTombstone { id })
}
pub fn seal_object(
    key: &Key,
    ecc: EccParams,
    id: ObjectId,
    version: ObjectVersion,
) -> BkfsResult<SealedRecord> {
    seal_record(key, ecc, Record::Object { id, version })
}

/// Field order is shared with legacy format-2/3 indexes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Roots<L = Location> {
    pub index: HashMap<u64, L>,
    pub content: HashMap<u64, L>,
    pub objects: HashMap<ObjectId, L>,
}

impl<L> Default for Roots<L> {
    fn default() -> Self {
        Self {
            index: HashMap::new(),
            content: HashMap::new(),
            objects: HashMap::new(),
        }
    }
}

impl<L: Copy> Roots<L> {
    pub(crate) fn update(&mut self, key: RecordKey, location: Option<L>) -> Option<L> {
        fn replace<K: std::hash::Hash + Eq, L>(
            index: &mut HashMap<K, L>,
            key: K,
            location: Option<L>,
        ) -> Option<L> {
            match location {
                Some(location) => index.insert(key, location),
                None => index.remove(&key),
            }
        }
        match key {
            RecordKey::Inode(id) => replace(&mut self.index, id, location),
            RecordKey::Content(id) => replace(&mut self.content, id, location),
            RecordKey::Object(id) => replace(&mut self.objects, id, location),
        }
    }
    pub(crate) fn entries(&self) -> impl Iterator<Item = (RecordKey, L)> + '_ {
        self.index
            .iter()
            .map(|(&id, &loc)| (RecordKey::Inode(id), loc))
            .chain(
                self.content
                    .iter()
                    .map(|(&id, &loc)| (RecordKey::Content(id), loc)),
            )
            .chain(
                self.objects
                    .iter()
                    .map(|(&id, &loc)| (RecordKey::Object(id), loc)),
            )
    }
}

impl Roots {
    pub fn segments(&self) -> impl Iterator<Item = u64> + '_ {
        self.entries().map(|(_, loc)| loc.segment)
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

pub(crate) struct SegmentWriter {
    dir: PathBuf,
    file: File,
    id: u64,
    offset: u64,
    limit: u64,
}
impl SegmentWriter {
    pub fn new(dir: &Path, limit: u64) -> BkfsResult<Self> {
        let id = match list_segments(dir)?.last() {
            Some(&(id, _)) => id
                .checked_add(1)
                .ok_or_else(|| BkfsError::unsupported("data segment id exhausted"))?,
            None => 0,
        };
        let file = Self::create(dir, id)?;
        Ok(Self {
            dir: dir.to_owned(),
            file,
            id,
            offset: 0,
            limit,
        })
    }
    fn create(dir: &Path, id: u64) -> BkfsResult<File> {
        Ok(OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(segment_path(dir, id))?)
    }
    fn write_frame(&mut self, bytes: &[u8]) -> BkfsResult<(u64, u64)> {
        if self.offset > 0 && self.offset + bytes.len() as u64 > self.limit {
            self.id = self
                .id
                .checked_add(1)
                .ok_or_else(|| BkfsError::unsupported("data segment id exhausted"))?;
            self.file = Self::create(&self.dir, self.id)?;
            self.offset = 0;
        }
        let offset = self.offset;
        self.file.write_all_at(bytes, offset)?;
        self.offset += bytes.len() as u64;
        Ok((self.id, offset))
    }
    pub fn append(&mut self, seq: u64, record: &SealedRecord) -> BkfsResult<Location> {
        let bytes = encode_frame(seq, &record.sealed.bytes);
        let (segment, offset) = self.write_frame(&bytes)?;
        Ok(Location {
            segment,
            offset,
            len: bytes.len() as u32,
            seq,
            payload_len: record.sealed.bytes.len() as u32,
            tag: record.sealed.tag,
        })
    }
    fn relocate(&mut self, old: Location, bytes: &[u8]) -> BkfsResult<Location> {
        let (segment, offset) = self.write_frame(bytes)?;
        Ok(Location {
            segment,
            offset,
            ..old
        })
    }
}

pub struct SegmentLog {
    key: Key,
    ecc: EccParams,
    state: State,
    roots: Roots,
    writer: Option<SegmentWriter>,
    next_seq: u64,
    max_inode: u64,
    pinned: HashSet<u64>,
    seg_meta: HashMap<u64, SegMeta>,
}

#[cfg(test)]
#[path = "seglog_tests.rs"]
mod tests;

impl Drop for SegmentLog {
    fn drop(&mut self) {
        if self.writer.is_some() && self.state.has_pending() {
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
        let (state, roots) = State::open(&dir, &key)?;
        let counters = state.counters();
        let writer = if readonly {
            None
        } else {
            Some(SegmentWriter::new(state.directory_path(), segment_size)?)
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
        };
        for (id, total) in list_segments(log.directory())? {
            log.seg_meta.insert(id, SegMeta { total, live: 0 });
        }
        log.recompute_live();
        Ok(log)
    }
    pub(crate) fn directory(&self) -> &Path {
        self.state.directory_path()
    }
    pub(crate) fn check_rw(&self) -> BkfsResult<()> {
        if self.writer.is_none() {
            return BkfsResult::errno_notrace(libc::EROFS);
        }
        self.state.check_rw()
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
        let bytes = self.read_raw_frame(loc)?;
        let (seq, payload_len) =
            parse_header(&bytes).ok_or_else(|| BkfsError::unsupported("invalid log header"))?;
        if seq != loc.seq
            || payload_len != loc.payload_len as usize
            || frame_size(payload_len) != loc.len as u64
        {
            return Err(BkfsError::unsupported("log reference mismatch"));
        }
        let payload = bytes
            .get(HEADER_LEN..HEADER_LEN + payload_len)
            .ok_or_else(|| BkfsError::unsupported("truncated log frame"))?;
        let plain =
            vault::open_bound(payload, &self.key, Binding::Record)?.verify_reference(&loc.tag)?;
        decode(&plain, data_config())
    }
    pub(crate) fn read_indexed(&self, key: RecordKey, loc: Location) -> BkfsResult<Record> {
        let record = self.read_at(loc)?;
        if record.key() != key {
            return Err(BkfsError::unsupported("log record identity mismatch"));
        }
        Ok(record)
    }
    fn read_raw_frame(&self, loc: Location) -> BkfsResult<Vec<u8>> {
        let mut bytes = vec![0; loc.len as usize];
        File::open(segment_path(self.directory(), loc.segment))?
            .read_exact_at(&mut bytes, loc.offset)?;
        Ok(bytes)
    }
    pub fn load(&self, inode: Inode) -> BkfsResult<Option<Attributes>> {
        self.roots
            .index
            .get(&inode.0)
            .map(
                |&loc| match self.read_indexed(RecordKey::Inode(inode.0), loc)? {
                    Record::Inode { attrs, .. } => Ok(attrs),
                    _ => Err(BkfsError::unsupported("invalid inode reference")),
                },
            )
            .transpose()
    }
    pub fn load_content(&self, id: u64) -> BkfsResult<Option<Vec<u8>>> {
        self.roots
            .content
            .get(&id)
            .map(
                |&loc| match self.read_indexed(RecordKey::Content(id), loc)? {
                    Record::Content { bytes, .. } => Ok(bytes),
                    _ => Err(BkfsError::unsupported("invalid content reference")),
                },
            )
            .transpose()
    }
    pub(crate) fn object(&self, id: ObjectId) -> BkfsResult<Option<ObjectVersion>> {
        self.object_in(&self.roots, id)
    }
    pub(crate) fn object_in(
        &self,
        roots: &Roots,
        id: ObjectId,
    ) -> BkfsResult<Option<ObjectVersion>> {
        roots
            .objects
            .get(&id)
            .map(
                |&loc| match self.read_indexed(RecordKey::Object(id), loc)? {
                    Record::Object { version, .. } => Ok(version),
                    _ => Err(BkfsError::unsupported("invalid object reference")),
                },
            )
            .transpose()
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
    pub fn append(&mut self, record: &SealedRecord) -> BkfsResult<()> {
        self.check_rw()?;
        let next = next_sequence(self.next_seq)?;
        let loc = self
            .writer
            .as_mut()
            .unwrap()
            .append(self.next_seq, record)?;
        self.next_seq = next;
        self.seg_meta.entry(loc.segment).or_default().total += loc.len as u64;
        if let RecordKey::Inode(id) = record.key {
            self.max_inode = self.max_inode.max(id);
        }
        self.update(record.key, (!record.tombstone).then_some(loc))
    }
    pub fn sync(&mut self) -> BkfsResult<()> {
        if self.writer.is_none() {
            return Ok(());
        }
        self.state
            .commit(&self.key, self.ecc, self.counters(), self.state.catalog())
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
        self.state
            .commit(&self.key, self.ecc, self.counters(), Some(reference))?;
        self.pin(pins);
        Ok(())
    }
    pub fn save_checkpoint(&mut self, retained: &[View]) -> BkfsResult<View> {
        self.check_rw()?;
        self.state.checkpoint(
            &self.key,
            self.ecc,
            &self.roots,
            self.counters(),
            self.state.catalog(),
            retained,
        )
    }
    pub(crate) fn retain_storage(
        &mut self,
        ids: &HashSet<ObjectId>,
        packed: &HashSet<u64>,
    ) -> BkfsResult<()> {
        self.check_rw()?;
        let remove: Vec<_> = self
            .roots
            .objects
            .keys()
            .filter(|id| !ids.contains(id))
            .copied()
            .map(RecordKey::Object)
            .chain(
                self.roots
                    .content
                    .keys()
                    .filter(|id| !packed.contains(id))
                    .copied()
                    .map(RecordKey::Content),
            )
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
    pub fn compact(&mut self, threshold: f64, retained: &[View]) -> BkfsResult<usize> {
        self.check_rw()?;
        let active = self.writer.as_ref().unwrap().id;
        let candidates: HashSet<_> = self
            .seg_meta
            .iter()
            .filter(|(&id, meta)| {
                id != active
                    && !self.pinned.contains(&id)
                    && meta.total > 0
                    && meta.total.saturating_sub(meta.live) as f64 / meta.total as f64 > threshold
            })
            .map(|(&id, _)| id)
            .collect();
        let live: Vec<_> = self
            .roots
            .entries()
            .filter(|(_, loc)| candidates.contains(&loc.segment))
            .collect();
        for (key, old) in live {
            let bytes = self.read_raw_frame(old)?;
            let new = self.writer.as_mut().unwrap().relocate(old, &bytes)?;
            self.seg_meta.entry(new.segment).or_default().total += new.len as u64;
            self.update(key, Some(new))?;
        }
        self.save_checkpoint(retained)?;
        for id in &candidates {
            std::fs::remove_file(segment_path(self.directory(), *id))?;
            self.seg_meta.remove(id);
        }
        File::open(self.directory())?.sync_all()?;
        Ok(candidates.len())
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
