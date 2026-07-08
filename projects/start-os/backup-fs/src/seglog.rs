//! Log-structured object store for inode records.
//!
//! Inodes (and, later, the small-file content inlined in them) are stored as
//! append-only **records** in shared **segment** files under `segments/`,
//! rather than one file per inode. For a backup of many small files this
//! turns "one CREATE + one RENAME per inode" into "append to a shared
//! segment", amortizing the per-object backing-store cost that dominates the
//! many-small-files workload.
//!
//! ## Frame format (self-locating, resync-able)
//!
//! ```text
//!   magic        "BKL1"  (4)            // resync anchor
//!   header_crc   u32     (4)            // CRC-32 of the 16 header bytes below
//!   seq          u64     (8)            // global monotonic sequence number
//!   payload_len  u32     (4)            // length of the sealed payload
//!   reserved     u32     (4)            // 0
//!   payload      [u8; payload_len]      // vault::seal(bincode(Record))
//!   (zero-pad to 8-byte alignment)
//! ```
//!
//! The `magic` + `header_crc` let replay **scan forward to the next valid
//! header** when a frame is torn/corrupt, instead of truncating the log at
//! the first bad byte. `seq` (not physical position) is the authority for
//! "latest record for an inode wins" — so a later compaction stage can
//! relocate a frame to a new segment/offset while preserving its `seq`.
//!
//! Each payload is independently `vault`-sealed (ChaCha20 + SHA-256 tag +
//! Reed-Solomon ECC), so corruption within one frame is self-healed by its
//! own parity and never spreads.
//!
//! ## Durability & caching
//!
//! Segment writes go through the page cache (so a read right after a write
//! sees the data — "read your writes" — without an O_DIRECT tail buffer),
//! and [`SegmentLog::sync`] issues `fdatasync` + `posix_fadvise(DONTNEED)` to
//! make the tail durable and then drop the now-clean pages. Frames are small
//! and flushed on the batched-syncfs cadence, so dirty metadata stays bounded
//! — avoiding the CIFS "need-memory-to-flush / need-flush-to-free" trap that
//! large content writes (still O_DIRECT, see `blockstore`) are the real risk
//! for.
//!
//! ## Crash recovery
//!
//! [`SegmentLog::open`] replays every segment, applying records by `seq`
//! (later wins; `Tombstone` deletes), rebuilding the in-RAM index and the
//! per-segment live-byte accounting from scratch — never trusting counts
//! across a crash. A torn tail frame fails its header CRC or `vault::open`
//! and is skipped via forward-resync.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::fs::FileExt;
use std::path::PathBuf;

use chacha20::Key;
use serde::{Deserialize, Serialize};

use crate::error::{BkfsError, BkfsResult};
use crate::inode::{Attributes, Inode};
use crate::serde::{data_config, decode, encode};
use crate::vault::{self, EccParams};

const MAGIC: [u8; 4] = *b"BKL1";
const HEADER_LEN: usize = 24; // magic(4) + crc(4) + seq(8) + payload_len(4) + reserved(4)
const ALIGN: u64 = 8;

/// Default rolled-segment size, used only by the test-facing [`SegmentLog::open`]
/// convenience. Production opens via [`SegmentLog::open_sized`] with the size
/// pinned by the superblock (see `Constants::segment_size`). Small enough that
/// the active segment stays a modest object for rsync/rclone to re-scan;
/// large enough to amortize per-segment overhead.
#[cfg(test)]
fn segment_size() -> u64 {
    use std::sync::OnceLock;
    static N: OnceLock<u64> = OnceLock::new();
    *N.get_or_init(|| {
        std::env::var("BACKUPFS_SEGMENT_SIZE")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|&n| n >= 4096)
            .unwrap_or(8 * 1024 * 1024)
    })
}

/// One persisted record. Single-key by design: directory entries live in
/// their own bucket files (see `directory`) and large content in block files
/// (see `blockstore`), so the log never needs multi-key atomic records.
///
/// **Append-only: never reorder or remove variants.** The serialized form is a
/// bincode variant index (declaration order), so a reorder would silently
/// misread every existing log frame. Any unavoidable reorder requires a
/// `superblock::FORMAT_VERSION` bump.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Record {
    Inode {
        inode: u64,
        attrs: Attributes,
    },
    Tombstone {
        inode: u64,
    },
    /// Packed content for a small/medium file (≤ one chunk). Shares the
    /// segments with inode records but lives in a separate `content` index,
    /// so a content id may equal its file's inode number without colliding.
    Content {
        id: u64,
        bytes: Vec<u8>,
    },
    ContentTombstone {
        id: u64,
    },
}

/// Which index a record belongs to. Inode numbers and content ids occupy
/// independent key spaces (a file's content id equals its inode number).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Space {
    Inode,
    Content,
}

/// A record already serialized + vault-sealed, ready to append. Produced
/// outside the log lock so the global append section never serializes crypto.
pub struct SealedRecord {
    key: u64,
    bytes: Vec<u8>,
    space: Space,
    tombstone: bool,
}

/// Seal an inode record (encryption + ECC) without touching the log.
pub fn seal_inode(
    key: &Key,
    ecc: EccParams,
    inode: Inode,
    attrs: &Attributes,
) -> BkfsResult<SealedRecord> {
    let plain = encode(
        &Record::Inode {
            inode: inode.0,
            attrs: attrs.clone(),
        },
        data_config(),
    )?;
    let bytes = vault::seal(&plain, key, ecc);
    Ok(SealedRecord {
        key: inode.0,
        bytes,
        space: Space::Inode,
        tombstone: false,
    })
}

/// Seal an inode tombstone record without touching the log.
pub fn seal_tombstone(key: &Key, ecc: EccParams, inode: Inode) -> BkfsResult<SealedRecord> {
    let plain = encode(&Record::Tombstone { inode: inode.0 }, data_config())?;
    let bytes = vault::seal(&plain, key, ecc);
    Ok(SealedRecord {
        key: inode.0,
        bytes,
        space: Space::Inode,
        tombstone: true,
    })
}

/// Seal a packed-content record without touching the log.
pub fn seal_content(key: &Key, ecc: EccParams, id: u64, bytes: &[u8]) -> BkfsResult<SealedRecord> {
    let plain = encode(
        &Record::Content {
            id,
            bytes: bytes.to_vec(),
        },
        data_config(),
    )?;
    let sealed = vault::seal(&plain, key, ecc);
    Ok(SealedRecord {
        key: id,
        bytes: sealed,
        space: Space::Content,
        tombstone: false,
    })
}

/// Seal a content tombstone record without touching the log.
pub fn seal_content_tombstone(key: &Key, ecc: EccParams, id: u64) -> BkfsResult<SealedRecord> {
    let plain = encode(&Record::ContentTombstone { id }, data_config())?;
    let bytes = vault::seal(&plain, key, ecc);
    Ok(SealedRecord {
        key: id,
        bytes,
        space: Space::Content,
        tombstone: true,
    })
}

/// Physical location of a record's frame within the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Location {
    pub segment: u64,
    pub offset: u64,
    pub len: u32,
}

#[derive(Default, Clone, Copy)]
struct SegMeta {
    total: u64,
    live: u64,
}

/// Padded on-disk size of a frame carrying `payload_len` sealed bytes.
fn frame_size(payload_len: usize) -> u64 {
    let raw = HEADER_LEN as u64 + payload_len as u64;
    raw.div_ceil(ALIGN) * ALIGN
}

fn encode_frame(seq: u64, payload: &[u8]) -> Vec<u8> {
    let total = frame_size(payload.len()) as usize;
    let mut buf = vec![0u8; total];
    buf[0..4].copy_from_slice(&MAGIC);
    // header_crc covers seq + payload_len + reserved (the 16 bytes at [8..24])
    buf[8..16].copy_from_slice(&seq.to_le_bytes());
    buf[16..20].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    // reserved [20..24] stays zero
    let crc = crc32fast::hash(&buf[8..24]);
    buf[4..8].copy_from_slice(&crc.to_le_bytes());
    buf[HEADER_LEN..HEADER_LEN + payload.len()].copy_from_slice(payload);
    buf
}

/// Parse a frame header at the start of `buf`. Returns `(seq, payload_len)`
/// if the header is intact (magic + CRC), else None (caller resyncs).
fn parse_header(buf: &[u8]) -> Option<(u64, usize)> {
    if buf.len() < HEADER_LEN || buf[0..4] != MAGIC {
        return None;
    }
    let crc = u32::from_le_bytes(buf[4..8].try_into().unwrap());
    if crc32fast::hash(&buf[8..24]) != crc {
        return None;
    }
    let seq = u64::from_le_bytes(buf[8..16].try_into().unwrap());
    let payload_len = u32::from_le_bytes(buf[16..20].try_into().unwrap()) as usize;
    Some((seq, payload_len))
}

pub struct SegmentLog {
    dir: PathBuf,
    key: Key,
    /// Open append handle for the active (highest-id) segment.
    active: File,
    active_id: u64,
    active_offset: u64,
    next_seq: u64,
    /// inode number → latest inode-record location.
    index: HashMap<u64, Location>,
    /// content id → latest content-record location.
    content: HashMap<u64, Location>,
    seg_meta: HashMap<u64, SegMeta>,
    /// Highest inode number ever observed in any frame (incl. tombstoned),
    /// so a recovered allocator never re-hands a number that was used.
    max_inode: u64,
    /// Roll to a new segment once the active one would exceed this. Captured
    /// at open (from `segment_size()`) so it can't shift under us — and so a
    /// test can pin it without racing the process-global env cache.
    segment_size: u64,
}

fn segment_path(dir: &std::path::Path, id: u64) -> PathBuf {
    dir.join(format!("{id:016x}.seg"))
}

impl SegmentLog {
    /// Open (creating if absent) the log under `dir` with the default segment
    /// size, replaying existing segments to rebuild the in-RAM index. Test
    /// convenience; production uses [`SegmentLog::open_sized`] with the
    /// superblock-pinned size.
    #[cfg(test)]
    pub fn open(dir: PathBuf, key: Key) -> BkfsResult<Self> {
        Self::open_sized(dir, key, segment_size())
    }

    /// As [`open`], with an explicit segment size (from the superblock at
    /// mount; pinned per-test elsewhere) instead of the env-cached default.
    pub(crate) fn open_sized(dir: PathBuf, key: Key, segment_size: u64) -> BkfsResult<Self> {
        std::fs::create_dir_all(&dir)?;
        let mut segment_ids = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(hex) = name.strip_suffix(".seg") {
                if let Ok(id) = u64::from_str_radix(hex, 16) {
                    segment_ids.push(id);
                }
            }
        }
        segment_ids.sort_unstable();

        let mut index: HashMap<u64, Location> = HashMap::new();
        let mut content: HashMap<u64, Location> = HashMap::new();
        // Winning seq per key per space, so a later-seq record wins even if
        // physically earlier (after a compaction relocation).
        let mut winning_inode: HashMap<u64, u64> = HashMap::new();
        let mut winning_content: HashMap<u64, u64> = HashMap::new();
        let mut seg_meta: HashMap<u64, SegMeta> = HashMap::new();
        let mut next_seq = 1u64;
        let mut max_inode = 0u64;

        for &id in &segment_ids {
            let mut file = File::open(segment_path(&dir, id))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            let mut pos = 0usize;
            let mut live = 0u64;
            while pos + HEADER_LEN <= bytes.len() {
                let Some((seq, payload_len)) = parse_header(&bytes[pos..]) else {
                    // Torn/garbage frame: resync to the next magic.
                    pos += match find_magic(&bytes[pos + 1..]) {
                        Some(off) => off + 1,
                        None => break,
                    };
                    continue;
                };
                let frame_len = frame_size(payload_len);
                let end = pos + HEADER_LEN + payload_len;
                if end > bytes.len() {
                    break; // truncated tail
                }
                let payload = &bytes[pos + HEADER_LEN..end];
                match vault::open(payload, &key).and_then(|p| decode::<Record>(&p, data_config())) {
                    Ok(record) => {
                        next_seq = next_seq.max(seq + 1);
                        let key = record.key();
                        let loc = Location {
                            segment: id,
                            offset: pos as u64,
                            len: frame_len as u32,
                        };
                        match record {
                            Record::Inode { .. } | Record::Tombstone { .. } => {
                                max_inode = max_inode.max(key);
                                if winning_inode.get(&key).map_or(true, |&w| seq >= w) {
                                    winning_inode.insert(key, seq);
                                    if matches!(record, Record::Inode { .. }) {
                                        index.insert(key, loc);
                                    } else {
                                        index.remove(&key);
                                    }
                                }
                            }
                            Record::Content { .. } | Record::ContentTombstone { .. } => {
                                if winning_content.get(&key).map_or(true, |&w| seq >= w) {
                                    winning_content.insert(key, seq);
                                    if matches!(record, Record::Content { .. }) {
                                        content.insert(key, loc);
                                    } else {
                                        content.remove(&key);
                                    }
                                }
                            }
                        }
                        live += frame_len;
                    }
                    Err(_) => {
                        // Frame failed ECC/integrity: skip it (a later good
                        // frame for the same inode, if any, still applies).
                        pos += match find_magic(&bytes[pos + 1..]) {
                            Some(off) => off + 1,
                            None => break,
                        };
                        continue;
                    }
                }
                pos += frame_len as usize;
            }
            seg_meta.insert(
                id,
                SegMeta {
                    total: bytes.len() as u64,
                    live,
                },
            );
        }

        // Recompute live bytes precisely: a frame is live iff it is the
        // current index Location for its inode. The pass above counted every
        // parseable frame; correct it by subtracting superseded frames.
        recompute_live(&index, &content, &mut seg_meta);

        // Open (or create) the active segment = the highest id, appended to.
        let active_id = segment_ids.last().copied().unwrap_or(0);
        let active = OpenOptions::new()
            .create(true)
            .read(true)
            .append(false)
            .write(true)
            .open(segment_path(&dir, active_id))?;
        let active_offset = active.metadata()?.len();

        Ok(Self {
            dir,
            key,
            active,
            active_id,
            active_offset,
            next_seq,
            index,
            content,
            seg_meta,
            max_inode,
            segment_size,
        })
    }

    pub fn contains(&self, inode: Inode) -> bool {
        self.index.contains_key(&inode.0)
    }

    pub fn max_inode(&self) -> u64 {
        self.max_inode
    }

    /// Read and decode the record at `loc`.
    pub fn read_at(&self, loc: Location) -> BkfsResult<Record> {
        let mut frame = vec![0u8; loc.len as usize];
        let file;
        let f: &File = if loc.segment == self.active_id {
            &self.active
        } else {
            file = File::open(segment_path(&self.dir, loc.segment))?;
            &file
        };
        f.read_exact_at(&mut frame, loc.offset)?;
        let (_, payload_len) = parse_header(&frame)
            .ok_or_else(|| BkfsError::wrap(std::io::Error::other("corrupt log frame header")))?;
        let payload = &frame[HEADER_LEN..HEADER_LEN + payload_len];
        let plain = vault::open(payload, &self.key)?;
        decode(&plain, data_config())
    }

    /// Load an inode's attributes, or None if it isn't in the index.
    pub fn load(&self, inode: Inode) -> BkfsResult<Option<Attributes>> {
        let Some(&loc) = self.index.get(&inode.0) else {
            return Ok(None);
        };
        match self.read_at(loc)? {
            Record::Inode { attrs, .. } => Ok(Some(attrs)),
            _ => Ok(None),
        }
    }

    /// Load a packed content extent by id, or None if absent.
    pub fn load_content(&self, id: u64) -> BkfsResult<Option<Vec<u8>>> {
        let Some(&loc) = self.content.get(&id) else {
            return Ok(None);
        };
        match self.read_at(loc)? {
            Record::Content { bytes, .. } => Ok(Some(bytes)),
            _ => Ok(None),
        }
    }

    /// Number of live packed-content extents (content index entries).
    #[cfg(test)]
    pub fn content_count(&self) -> usize {
        self.content.len()
    }

    /// Append a pre-sealed record (see [`seal_inode`]/[`seal_tombstone`]).
    /// Sealing is done by the caller *outside* the log lock so the global
    /// append critical section never serializes ChaCha20/Reed-Solomon work.
    pub fn append(&mut self, rec: &SealedRecord) -> BkfsResult<()> {
        let seq = self.next_seq;
        self.next_seq += 1;
        let frame = encode_frame(seq, &rec.bytes);
        // Roll to a new segment if the active one is full (but never leave a
        // segment empty: always write at least one frame per segment).
        if self.active_offset > 0 && self.active_offset + frame.len() as u64 > self.segment_size {
            self.roll()?;
        }
        let offset = self.active_offset;
        self.active.write_all_at(&frame, offset)?;
        self.active_offset += frame.len() as u64;
        let loc = Location {
            segment: self.active_id,
            offset,
            len: frame.len() as u32,
        };

        let seg = self.seg_meta.entry(self.active_id).or_default();
        seg.total += frame.len() as u64;
        seg.live += frame.len() as u64;

        let dst = match rec.space {
            Space::Inode => {
                self.max_inode = self.max_inode.max(rec.key);
                &mut self.index
            }
            Space::Content => &mut self.content,
        };
        // Supersede any prior live frame for this key in its space.
        if let Some(prev) = dst.get(&rec.key).copied() {
            if let Some(m) = self.seg_meta.get_mut(&prev.segment) {
                m.live = m.live.saturating_sub(prev.len as u64);
            }
        }
        if rec.tombstone {
            // The tombstone frame is itself dead weight once written.
            dst.remove(&rec.key);
            if let Some(m) = self.seg_meta.get_mut(&loc.segment) {
                m.live = m.live.saturating_sub(loc.len as u64);
            }
        } else {
            dst.insert(rec.key, loc);
        }
        Ok(())
    }

    /// Append/replace an inode record (seals inline; convenience for tests).
    #[cfg(test)]
    pub fn put(&mut self, inode: Inode, attrs: &Attributes) -> BkfsResult<()> {
        let rec = seal_inode(&self.key, EccParams::default(), inode, attrs)?;
        self.append(&rec)
    }

    /// Append a tombstone (seals inline; convenience for tests).
    #[cfg(test)]
    pub fn tombstone(&mut self, inode: Inode) -> BkfsResult<()> {
        let rec = seal_tombstone(&self.key, EccParams::default(), inode)?;
        self.append(&rec)
    }

    fn roll(&mut self) -> BkfsResult<()> {
        // Make the segment we're leaving durable before starting a new one.
        self.sync()?;
        self.active_id += 1;
        self.active = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(segment_path(&self.dir, self.active_id))?;
        self.active_offset = 0;
        // Make the new segment's directory entry durable.
        self.fsync_dir()?;
        Ok(())
    }

    /// Flush the active segment to stable storage and drop its now-clean
    /// pages from the cache to keep metadata cache bounded.
    pub fn sync(&self) -> BkfsResult<()> {
        self.active.sync_data()?;
        // Best-effort: drop clean pages we just synced.
        let fd = self.active.as_raw_fd();
        // SAFETY: fd is a valid owned fd; DONTNEED on a synced range is advisory.
        unsafe {
            libc::posix_fadvise(
                fd,
                0,
                self.active_offset as libc::off_t,
                libc::POSIX_FADV_DONTNEED,
            );
        }
        Ok(())
    }

    /// fsync the segments directory so a create/rename/unlink of a segment
    /// file is itself durable (a lost segment dir-entry is catastrophic,
    /// unlike a per-inode file the heal could prune).
    fn fsync_dir(&self) -> BkfsResult<()> {
        File::open(&self.dir)?.sync_all()?;
        Ok(())
    }

    /// Read a frame's raw on-disk bytes (header + sealed payload + padding).
    fn read_raw_frame(&self, loc: Location) -> BkfsResult<Vec<u8>> {
        let mut frame = vec![0u8; loc.len as usize];
        let file;
        let f: &File = if loc.segment == self.active_id {
            &self.active
        } else {
            file = File::open(segment_path(&self.dir, loc.segment))?;
            &file
        };
        f.read_exact_at(&mut frame, loc.offset)?;
        Ok(frame)
    }

    /// Append an existing frame VERBATIM (preserving its seq), repointing the
    /// index. Used by compaction to relocate a live frame without re-sealing
    /// — re-sealing would change every byte (fresh nonce) and break rsync
    /// delta-matching of the relocated bytes.
    fn append_verbatim(&mut self, frame: &[u8], space: Space, key: u64) -> BkfsResult<()> {
        if self.active_offset > 0 && self.active_offset + frame.len() as u64 > self.segment_size {
            self.roll()?;
        }
        let offset = self.active_offset;
        self.active.write_all_at(frame, offset)?;
        self.active_offset += frame.len() as u64;
        let loc = Location {
            segment: self.active_id,
            offset,
            len: frame.len() as u32,
        };
        let seg = self.seg_meta.entry(self.active_id).or_default();
        seg.total += frame.len() as u64;
        seg.live += frame.len() as u64;
        match space {
            Space::Inode => self.index.insert(key, loc),
            Space::Content => self.content.insert(key, loc),
        };
        Ok(())
    }

    /// Reclaim dead space: rewrite every sealed segment whose dead-byte ratio
    /// exceeds `dead_threshold` by copying its still-live frames verbatim into
    /// the active segment, then deleting it. Crash-safe: the relocated copies
    /// are made durable (and the dir fsynced) BEFORE the old segment is
    /// removed, so a crash can only leave reclaimable dead duplicates, never
    /// lose a live frame. Returns the number of segments compacted.
    pub fn compact(&mut self, dead_threshold: f64) -> BkfsResult<usize> {
        // The active segment is never a compaction candidate, so its own dead
        // frames would strand for the mount's lifetime. If it's grown dead
        // enough, seal it (roll) first so it becomes compactable below.
        if let Some(m) = self.seg_meta.get(&self.active_id) {
            if self.active_offset > 0
                && m.total > 0
                && (m.total - m.live) as f64 / m.total as f64 > dead_threshold
            {
                self.roll()?;
            }
        }
        let candidates: Vec<u64> = self
            .seg_meta
            .iter()
            .filter(|(&id, m)| {
                id != self.active_id
                    && m.total > 0
                    && (m.total - m.live) as f64 / m.total as f64 > dead_threshold
            })
            .map(|(&id, _)| id)
            .collect();
        for seg_id in &candidates {
            self.compact_segment(*seg_id)?;
        }
        Ok(candidates.len())
    }

    fn compact_segment(&mut self, seg_id: u64) -> BkfsResult<()> {
        // Live frames in this segment = index entries pointing into it.
        let mut live: Vec<(Space, u64, Location)> = Vec::new();
        for (&k, &l) in &self.index {
            if l.segment == seg_id {
                live.push((Space::Inode, k, l));
            }
        }
        for (&k, &l) in &self.content {
            if l.segment == seg_id {
                live.push((Space::Content, k, l));
            }
        }
        for (space, key, loc) in live {
            let frame = self.read_raw_frame(loc)?;
            self.append_verbatim(&frame, space, key)?;
        }
        // Relocated copies durable before the source is deleted.
        self.sync()?;
        self.fsync_dir()?;
        std::fs::remove_file(segment_path(&self.dir, seg_id))?;
        self.fsync_dir()?;
        self.seg_meta.remove(&seg_id);
        Ok(())
    }

    /// Number of live inodes (index entries).
    pub fn live_count(&self) -> usize {
        self.index.len()
    }

    #[cfg(test)]
    pub fn index_len(&self) -> usize {
        self.index.len()
    }
}

impl Record {
    fn key(&self) -> u64 {
        match self {
            Record::Inode { inode, .. } | Record::Tombstone { inode } => *inode,
            Record::Content { id, .. } | Record::ContentTombstone { id } => *id,
        }
    }
}

/// Index of the first occurrence of MAGIC in `buf`, if any.
fn find_magic(buf: &[u8]) -> Option<usize> {
    buf.windows(MAGIC.len()).position(|w| w == MAGIC)
}

/// Reset every segment's live bytes to the sum of frame sizes still pointed
/// at by an index — BOTH the inode index and the content index. (Omitting
/// the content index scored every live packed extent as dead after a
/// remount, so compaction needlessly rewrote fully-live content segments.)
fn recompute_live(
    index: &HashMap<u64, Location>,
    content: &HashMap<u64, Location>,
    seg_meta: &mut HashMap<u64, SegMeta>,
) {
    for m in seg_meta.values_mut() {
        m.live = 0;
    }
    for loc in index.values().chain(content.values()) {
        if let Some(m) = seg_meta.get_mut(&loc.segment) {
            m.live += loc.len as u64;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inode::{FileData, InodeAttributes};

    fn key() -> Key {
        Key::from([5u8; 32])
    }

    fn attrs(size: u64) -> Attributes {
        let mut a =
            InodeAttributes::new(Inode(1), None, FileData::File(crate::inode::ContentId(1)));
        a.attrs.size = size;
        a.attrs
    }

    #[test]
    fn put_load_roundtrip_and_replay() {
        let tmp = tempdir::TempDir::new("seglog").unwrap();
        let dir = tmp.path().join("segments");
        {
            let mut log = SegmentLog::open(dir.clone(), key()).unwrap();
            for i in 1..=50u64 {
                log.put(Inode(i), &attrs(i * 100)).unwrap();
            }
            // overwrite some
            for i in 1..=10u64 {
                log.put(Inode(i), &attrs(999)).unwrap();
            }
            log.tombstone(Inode(25)).unwrap();
            log.sync().unwrap();
            assert!(log.load(Inode(25)).unwrap().is_none());
            assert_eq!(log.load(Inode(1)).unwrap().unwrap().size, 999);
            assert_eq!(log.load(Inode(40)).unwrap().unwrap().size, 4000);
        }
        // Reopen → replay rebuilds the index, latest-seq-wins, tombstone gone.
        let log = SegmentLog::open(dir, key()).unwrap();
        assert_eq!(log.index_len(), 49); // 50 created, 1 tombstoned
        assert!(log.load(Inode(25)).unwrap().is_none());
        assert_eq!(log.load(Inode(1)).unwrap().unwrap().size, 999);
        assert_eq!(log.load(Inode(50)).unwrap().unwrap().size, 5000);
        assert_eq!(log.max_inode(), 50);
    }

    #[test]
    fn rotates_across_segments() {
        // Pin the segment size directly — `segment_size()` is a process-global
        // OnceLock cached on first use, so setting the env var here would race
        // with any other test that touched the log first.
        const SEG: u64 = 8192;
        let tmp = tempdir::TempDir::new("seglog").unwrap();
        let dir = tmp.path().join("segments");
        {
            let mut log = SegmentLog::open_sized(dir.clone(), key(), SEG).unwrap();
            for i in 1..=200u64 {
                log.put(Inode(i), &attrs(i)).unwrap();
            }
            log.sync().unwrap();
        }
        let n_segs = std::fs::read_dir(&dir).unwrap().count();
        assert!(n_segs > 1, "expected multiple segments, got {n_segs}");
        let log = SegmentLog::open_sized(dir, key(), SEG).unwrap();
        assert_eq!(log.index_len(), 200);
        for i in 1..=200u64 {
            assert_eq!(log.load(Inode(i)).unwrap().unwrap().size, i);
        }
    }

    #[test]
    fn resync_past_corrupt_frame() {
        let tmp = tempdir::TempDir::new("seglog").unwrap();
        let dir = tmp.path().join("segments");
        {
            let mut log = SegmentLog::open(dir.clone(), key()).unwrap();
            for i in 1..=20u64 {
                log.put(Inode(i), &attrs(i)).unwrap();
            }
            log.sync().unwrap();
        }
        // Corrupt one frame's payload in the middle of the single segment so
        // its vault::open fails; replay must still recover the others.
        let seg = segment_path(&dir, 0);
        let mut bytes = std::fs::read(&seg).unwrap();
        // Find the 3rd frame and trash a payload byte.
        let mut pos = 0;
        for _ in 0..3 {
            let (_, plen) = parse_header(&bytes[pos..]).unwrap();
            if pos / 200 == 2 {
                break;
            }
            pos += frame_size(plen) as usize;
        }
        let target = pos + HEADER_LEN + 4;
        for b in &mut bytes[target..target + 64] {
            *b ^= 0xFF;
        }
        std::fs::write(&seg, &bytes).unwrap();

        let log = SegmentLog::open(dir, key()).unwrap();
        // At least all-but-one inode survive (the corrupted frame's inode is
        // beyond ECC repair and is dropped).
        assert!(
            log.index_len() >= 19,
            "expected ≥19 survivors, got {}",
            log.index_len()
        );
    }
}
