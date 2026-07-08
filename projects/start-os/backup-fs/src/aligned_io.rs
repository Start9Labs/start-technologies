use std::alloc::{self, Layout};
use std::cell::RefCell;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::FileExt;
use std::ptr;
use std::sync::OnceLock;
use std::time::Duration;

use crate::atomic_file::AtomicFile;
use crate::error::BkfsResult;

const BLOCK_SIZE: usize = 4096;
const ALIGN_MASK: usize = BLOCK_SIZE - 1;
const BUF_CAP: usize = 1 << 20; // 1 MiB

/// Chunk size for flush on network filesystems (CIFS/NFS). Keeps per-write
/// work small enough that CIFS client doesn't stall NIC TX completions.
const NETFS_FLUSH_CHUNK: usize = 64 * 1024;

/// Chunk size for flush on local filesystems. A single SQE per flush
/// minimizes per-command overhead on UAS/BOT USB and local block devices.
const LOCAL_FLUSH_CHUNK: usize = BUF_CAP;

/// Ring is sized for the worst case (most chunks per flush).
const RING_ENTRIES: u32 = (BUF_CAP / NETFS_FLUSH_CHUNK) as u32;

/// Maximum wall-clock time a single flush (all chunks) is allowed to take
/// before we give up and bubble a TimedOut error. Prevents indefinite hangs
/// when the backing store (CIFS, stuck USB, etc.) stops producing
/// completions. Can be overridden via BACKUPFS_FLUSH_TIMEOUT_SECS.
const DEFAULT_FLUSH_TIMEOUT: Duration = Duration::from_secs(120);

fn flush_timeout() -> Duration {
    static TIMEOUT: OnceLock<Duration> = OnceLock::new();
    *TIMEOUT.get_or_init(|| {
        std::env::var("BACKUPFS_FLUSH_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_FLUSH_TIMEOUT)
    })
}

/// If set, skip io_uring entirely and use a plain pwrite loop. Useful when
/// io_uring interacts badly with a specific backing store (early CIFS
/// kernels punt non-blocking writes to a workqueue that can wedge).
fn uring_disabled() -> bool {
    static DISABLED: OnceLock<bool> = OnceLock::new();
    *DISABLED.get_or_init(|| {
        std::env::var("BACKUPFS_NO_URING")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
    })
}

/// Detect whether an fd lives on a network filesystem that needs small chunks.
fn flush_chunk_for(fd: RawFd) -> usize {
    use std::mem::MaybeUninit;
    let mut sb = MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: fstatfs writes to the provided buffer
    let rc = unsafe { libc::fstatfs(fd, sb.as_mut_ptr()) };
    if rc != 0 {
        return NETFS_FLUSH_CHUNK; // conservative fallback
    }
    let sb = unsafe { sb.assume_init() };
    // CIFS: 0xFF534D42, SMB2: 0xFE534D42, NFS: 0x6969
    match sb.f_type as u64 {
        0xFF534D42 | 0xFE534D42 | 0x6969 => NETFS_FLUSH_CHUNK,
        _ => LOCAL_FLUSH_CHUNK,
    }
}

// ── Aligned buffer ────────────────────────────────────

/// A buffer with guaranteed 4096-byte alignment for O_DIRECT I/O.
struct AlignedBuf {
    ptr: ptr::NonNull<u8>,
    len: usize,
}

unsafe impl Send for AlignedBuf {}

impl AlignedBuf {
    fn new(len: usize) -> Self {
        let len = (len + ALIGN_MASK) & !ALIGN_MASK;
        let len = len.max(BLOCK_SIZE);
        let layout = Layout::from_size_align(len, BLOCK_SIZE).unwrap();
        let ptr = unsafe { alloc::alloc_zeroed(layout) };
        let ptr = ptr::NonNull::new(ptr).unwrap_or_else(|| alloc::handle_alloc_error(layout));
        Self { ptr, len }
    }

    fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }

    fn len(&self) -> usize {
        self.len
    }
}

impl Drop for AlignedBuf {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.len, BLOCK_SIZE).unwrap();
        unsafe { alloc::dealloc(self.ptr.as_ptr(), layout) };
    }
}

// ── Low-level aligned I/O ─────────────────────────────

fn align_down(v: u64) -> u64 {
    v & !(BLOCK_SIZE as u64 - 1)
}

/// Perform a full pread, retrying on EINTR.
fn pread_full<F: FileExt>(file: &F, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    let mut pos = 0;
    while pos < buf.len() {
        match file.read_at(&mut buf[pos..], offset + pos as u64) {
            Ok(0) => break,
            Ok(n) => pos += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(pos)
}

// ── Flush implementations ─────────────────────────────

/// Plain synchronous pwrite loop. Used as a fallback when io_uring is
/// disabled or has been poisoned by a timeout.
fn pwrite_all<F: FileExt>(file: &F, buf: &[u8], offset: u64, chunk_size: usize) -> io::Result<()> {
    for (i, chunk) in buf.chunks(chunk_size).enumerate() {
        let base = offset + (i * chunk_size) as u64;
        let mut pos = 0;
        while pos < chunk.len() {
            match file.write_at(&chunk[pos..], base + pos as u64) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "pwrite: short write",
                    ))
                }
                Ok(n) => pos += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
    }
    Ok(())
}

/// Result of a uring flush attempt. The ring is considered poisoned if we
/// could not reap all submitted completions — callers must rebuild it
/// before reusing.
enum UringFlushOutcome {
    Ok,
    Poisoned(io::Error),
}

/// Submit all chunks as pwrite SQEs on a reused ring, then reap completions
/// with a wall-clock timeout. If the timeout expires before all CQEs
/// arrive, the ring is returned as Poisoned and the caller must recreate
/// it — leftover pending SQEs would produce stale CQEs on the next flush
/// and confuse user_data tracking.
fn uring_pwrite_all(
    ring: &mut io_uring::IoUring,
    fd: RawFd,
    buf: &[u8],
    offset: u64,
    chunk_size: usize,
) -> UringFlushOutcome {
    use io_uring::{opcode, types};

    if buf.is_empty() {
        return UringFlushOutcome::Ok;
    }

    let n_chunks = (buf.len() + chunk_size - 1) / chunk_size;

    {
        let mut sq = ring.submission();
        for (i, chunk) in buf.chunks(chunk_size).enumerate() {
            let off = offset + (i * chunk_size) as u64;
            let entry = opcode::Write::new(types::Fd(fd), chunk.as_ptr(), chunk.len() as u32)
                .offset(off)
                .build()
                .user_data(i as u64);
            // SAFETY: the buffer is alive for the duration of this function,
            // and we wait for all completions before returning. On timeout
            // the ring is rebuilt so in-flight SQEs can't reference a
            // dropped buffer.
            if unsafe { sq.push(&entry) }.is_err() {
                return UringFlushOutcome::Poisoned(io::Error::new(
                    io::ErrorKind::Other,
                    "io_uring submission queue full",
                ));
            }
        }
    }

    let ts = types::Timespec::from(flush_timeout());
    let args = types::SubmitArgs::new().timespec(&ts);
    match ring.submitter().submit_with_args(n_chunks, &args) {
        Ok(_) => {}
        Err(e)
            if e.raw_os_error() == Some(libc::ETIME) || e.raw_os_error() == Some(libc::EINTR) =>
        {
            return UringFlushOutcome::Poisoned(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "io_uring flush exceeded {}s timeout",
                    flush_timeout().as_secs()
                ),
            ));
        }
        Err(e) => return UringFlushOutcome::Poisoned(e),
    }

    let mut errors: Option<io::Error> = None;
    let mut completed = 0;
    for cqe in ring.completion() {
        completed += 1;
        let ret = cqe.result();
        if ret < 0 {
            errors.get_or_insert(io::Error::from_raw_os_error(-ret));
        } else {
            let idx = cqe.user_data() as usize;
            let expected = buf.chunks(chunk_size).nth(idx).map_or(0, |c| c.len());
            if (ret as usize) < expected {
                errors.get_or_insert(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "io_uring: short write",
                ));
            }
        }
    }
    if completed < n_chunks {
        return UringFlushOutcome::Poisoned(io::Error::new(
            io::ErrorKind::Other,
            format!("io_uring: {completed}/{n_chunks} writes completed"),
        ));
    }
    if let Some(e) = errors {
        return UringFlushOutcome::Poisoned(e);
    }
    UringFlushOutcome::Ok
}

fn new_ring() -> io::Result<io_uring::IoUring> {
    io_uring::IoUring::new(RING_ENTRIES)
}

// ── Buffered direct-I/O file ──────────────────────────

struct BufState {
    buf: AlignedBuf,
    /// File offset that buf[0] maps to (block-aligned). u64::MAX = no window loaded.
    base: u64,
    /// Bytes in the buffer that contain valid data (from disk or writes).
    valid: usize,
    /// Prefix length of the buffer loaded from disk (via load_window).
    /// Anything outside [0, loaded) needs a RMW read before a partial-block flush.
    loaded: usize,
    /// First dirty byte in the buffer (usize::MAX when clean).
    dirty_start: usize,
    /// One-past-last dirty byte.
    dirty_end: usize,
    /// Sequential read/write cursor (absolute file offset).
    pos: u64,
    /// Reusable io_uring for flush_dirty. None means fall back to a plain
    /// pwrite loop — either because BACKUPFS_NO_URING is set or because
    /// the ring was poisoned by a timeout and we haven't rebuilt it yet.
    ring: Option<io_uring::IoUring>,
    /// Flush chunk size (selected per-filesystem at construction).
    flush_chunk: usize,
}

/// Wraps an O_DIRECT file descriptor with a 1 MiB write-back buffer.
///
/// All actual disk I/O goes through the aligned internal buffer, satisfying
/// O_DIRECT's alignment requirements for buffer address, file offset, and
/// I/O size transparently. Flushes are pipelined via io_uring.
pub struct BufferedDirectFile<F: FileExt + AsRawFd> {
    file: Option<F>,
    state: RefCell<BufState>,
}

impl<F: FileExt + AsRawFd> BufferedDirectFile<F> {
    pub fn new(file: F) -> io::Result<Self> {
        let flush_chunk = flush_chunk_for(file.as_raw_fd());
        let ring = if uring_disabled() {
            None
        } else {
            Some(new_ring()?)
        };
        Ok(Self {
            file: Some(file),
            state: RefCell::new(BufState {
                buf: AlignedBuf::new(BUF_CAP),
                base: u64::MAX, // sentinel: no window loaded
                valid: 0,
                loaded: 0,
                dirty_start: usize::MAX,
                dirty_end: 0,
                pos: 0,
                ring,
                flush_chunk,
            }),
        })
    }

    fn file(&self) -> &F {
        self.file.as_ref().unwrap()
    }

    /// Flush dirty bytes to disk via io_uring pipelined writes.
    ///
    /// O_DIRECT requires block-aligned writes, so the flush range is rounded
    /// out to block boundaries. If the dirty range doesn't start/end on a
    /// block boundary, the partial blocks at the edges are read from disk
    /// first to preserve their non-dirty bytes (read-modify-write).
    fn flush_dirty(state: &mut BufState, file: &F) -> io::Result<()> {
        if state.dirty_start >= state.dirty_end {
            return Ok(());
        }
        let start = state.dirty_start & !ALIGN_MASK;
        let end = ((state.dirty_end + ALIGN_MASK) & !ALIGN_MASK).min(state.buf.len());

        // Fill partial-block edges from disk so the upcoming aligned write
        // doesn't clobber on-disk bytes outside the dirty range.
        //
        // O_DIRECT requires the buffer address, file offset, AND length to
        // each be a multiple of the device's logical block size. Reading a
        // sub-block range (e.g. [dirty_end..end)) directly into the window
        // buffer violates all three on strict backends like ext4/vfat/exfat
        // and fails with EINVAL — silently breaking the flush. We side-step
        // that by reading the entire edge BLOCK into an aligned scratch and
        // copying only the non-dirty bytes back into the window buffer; the
        // pread is fully aligned and the dirty bytes already in the window
        // are preserved.
        //
        // `state.loaded` tracks the prefix that's already current with disk,
        // so a previously-loaded edge can skip the pread.
        let head_needs_read = start < state.dirty_start && state.loaded < state.dirty_start;
        let tail_needs_read = end > state.dirty_end && state.loaded < end;
        if head_needs_read || tail_needs_read {
            let head_block_start = start;
            let tail_block_start = end - BLOCK_SIZE;
            let mut scratch = AlignedBuf::new(BLOCK_SIZE);
            // Tracks which block, if any, is currently cached in scratch.
            // Re-used across head+tail when both edges land in the same
            // block (dirty range fits in a single block).
            let mut scratch_block: Option<usize> = None;

            if head_needs_read {
                let n = pread_full(
                    file,
                    scratch.as_mut_slice(),
                    state.base + head_block_start as u64,
                )?;
                if n < BLOCK_SIZE {
                    scratch.as_mut_slice()[n..].fill(0);
                }
                scratch_block = Some(head_block_start);
                let copy_start = state.loaded.max(start);
                let copy_end = state.dirty_start;
                if copy_start < copy_end {
                    let off = copy_start - head_block_start;
                    let len = copy_end - copy_start;
                    state.buf.as_mut_slice()[copy_start..copy_end]
                        .copy_from_slice(&scratch.as_slice()[off..off + len]);
                }
                state.loaded = state.loaded.max(head_block_start + BLOCK_SIZE);
            }

            if tail_needs_read {
                if scratch_block != Some(tail_block_start) {
                    let n = pread_full(
                        file,
                        scratch.as_mut_slice(),
                        state.base + tail_block_start as u64,
                    )?;
                    if n < BLOCK_SIZE {
                        scratch.as_mut_slice()[n..].fill(0);
                    }
                    scratch_block = Some(tail_block_start);
                }
                debug_assert_eq!(scratch_block, Some(tail_block_start));
                let copy_start = state.dirty_end.max(tail_block_start);
                let copy_end = end;
                if copy_start < copy_end {
                    let off = copy_start - tail_block_start;
                    let len = copy_end - copy_start;
                    state.buf.as_mut_slice()[copy_start..copy_end]
                        .copy_from_slice(&scratch.as_slice()[off..off + len]);
                }
                state.loaded = state.loaded.max(end);
            }
        }

        let buf = &state.buf.as_slice()[start..end];
        let chunk = state.flush_chunk;
        let file_offset = state.base + start as u64;

        if let Some(ring) = state.ring.as_mut() {
            match uring_pwrite_all(ring, file.as_raw_fd(), buf, file_offset, chunk) {
                UringFlushOutcome::Ok => {}
                UringFlushOutcome::Poisoned(err) => {
                    // Ring may have pending SQEs referencing our buffer or
                    // stale CQEs that would misalign user_data on the next
                    // flush. Drop it — a fresh ring will be built lazily.
                    state.ring = None;
                    log::warn!("io_uring flush failed ({err}); falling back to pwrite");
                    pwrite_all(file, buf, file_offset, chunk)?;
                    // Try to restore the ring for subsequent flushes. If
                    // the kernel is in bad shape, stay on pwrite until
                    // we're reset.
                    state.ring = new_ring().ok();
                }
            }
        } else {
            pwrite_all(file, buf, file_offset, chunk)?;
        }

        state.dirty_start = usize::MAX;
        state.dirty_end = 0;
        Ok(())
    }

    /// Load a new window starting at the block containing `offset`.
    fn load_window(state: &mut BufState, file: &F, offset: u64) -> io::Result<()> {
        state.base = align_down(offset);
        let n = pread_full(file, state.buf.as_mut_slice(), state.base)?;
        state.valid = n;
        state.loaded = n;
        state.dirty_start = usize::MAX;
        state.dirty_end = 0;
        Ok(())
    }

    /// Reset window without reading from disk. Used on the write path to
    /// avoid an O_DIRECT read (which can deadlock CIFS for regions beyond EOF).
    fn reset_window(state: &mut BufState, offset: u64) {
        state.base = align_down(offset);
        state.valid = 0;
        state.loaded = 0;
        state.dirty_start = usize::MAX;
        state.dirty_end = 0;
    }

    /// Ensure the buffer window covers `offset` for reading. Flushes and reloads.
    fn ensure_window_read(state: &mut BufState, file: &F, offset: u64) -> io::Result<()> {
        if offset >= state.base && offset - state.base < BUF_CAP as u64 {
            return Ok(());
        }
        Self::flush_dirty(state, file)?;
        Self::load_window(state, file, offset)
    }

    /// Ensure the buffer window covers `offset` for writing. Flushes dirty data
    /// but does NOT read the new window from disk.
    fn ensure_window_write(state: &mut BufState, file: &F, offset: u64) -> io::Result<()> {
        if offset >= state.base && offset - state.base < BUF_CAP as u64 {
            return Ok(());
        }
        Self::flush_dirty(state, file)?;
        Self::reset_window(state, offset);
        Ok(())
    }
}

impl<F: FileExt + AsRawFd> Drop for BufferedDirectFile<F> {
    fn drop(&mut self) {
        if let Some(file) = &self.file {
            let state = self.state.get_mut();
            // Surface the error so the cause isn't lost. The real
            // save paths flush explicitly via save/save_fast and
            // propagate errors; this is the last-ditch backstop for
            // an unwind path where there's no caller to return to.
            if let Err(e) = Self::flush_dirty(state, file) {
                log::error!("BufferedDirectFile::drop: flush_dirty failed: {e}");
            }
        }
    }
}

impl<F: FileExt + AsRawFd> FileExt for BufferedDirectFile<F> {
    fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut state = self.state.borrow_mut();
        Self::ensure_window_read(&mut state, self.file(), offset)?;
        let off = (offset - state.base) as usize;
        let available = state.valid.saturating_sub(off);
        let n = buf.len().min(available);
        buf[..n].copy_from_slice(&state.buf.as_slice()[off..off + n]);
        Ok(n)
    }

    fn write_at(&self, buf: &[u8], offset: u64) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut state = self.state.borrow_mut();
        Self::ensure_window_write(&mut state, self.file(), offset)?;
        let off = (offset - state.base) as usize;
        let space = BUF_CAP - off;
        let n = buf.len().min(space);
        state.buf.as_mut_slice()[off..off + n].copy_from_slice(&buf[..n]);
        state.dirty_start = state.dirty_start.min(off);
        state.dirty_end = state.dirty_end.max(off + n);
        state.valid = state.valid.max(off + n);
        Ok(n)
    }
}

impl<F: FileExt + AsRawFd> Read for BufferedDirectFile<F> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let file = self.file.as_ref().unwrap();
        let state = self.state.get_mut();
        Self::ensure_window_read(state, file, state.pos)?;
        let off = (state.pos - state.base) as usize;
        let available = state.valid.saturating_sub(off);
        let n = buf.len().min(available);
        buf[..n].copy_from_slice(&state.buf.as_slice()[off..off + n]);
        state.pos += n as u64;
        Ok(n)
    }
}

impl<F: FileExt + AsRawFd> Write for BufferedDirectFile<F> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let file = self.file.as_ref().unwrap();
        let state = self.state.get_mut();
        Self::ensure_window_write(state, file, state.pos)?;
        let off = (state.pos - state.base) as usize;
        let space = BUF_CAP - off;
        let n = buf.len().min(space);
        state.buf.as_mut_slice()[off..off + n].copy_from_slice(&buf[..n]);
        state.dirty_start = state.dirty_start.min(off);
        state.dirty_end = state.dirty_end.max(off + n);
        state.valid = state.valid.max(off + n);
        state.pos += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        let file = self.file.as_ref().unwrap();
        let state = self.state.get_mut();
        Self::flush_dirty(state, file)
    }
}

impl<F: FileExt + AsRawFd> Seek for BufferedDirectFile<F> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let state = self.state.get_mut();
        state.pos = match pos {
            SeekFrom::Start(n) => n,
            SeekFrom::Current(n) => if n >= 0 {
                state.pos.checked_add(n as u64)
            } else {
                state.pos.checked_sub(n.unsigned_abs())
            }
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid seek"))?,
            SeekFrom::End(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "SeekFrom::End not supported on BufferedDirectFile",
                ));
            }
        };
        Ok(state.pos)
    }
}

// ── Type-specific methods ─────────────────────────────

impl BufferedDirectFile<AtomicFile> {
    pub fn save(mut self) -> BkfsResult<()> {
        self.flush()?;
        // Take the file so Drop doesn't double-flush
        self.file.take().unwrap().save()
    }

    /// Flush the buffered window + rename, but skip sync_all. Paired
    /// with a batched syncfs at the dispatch layer — see
    /// `AtomicFile::save_fast` for the trade-off.
    pub fn save_fast(mut self) -> BkfsResult<()> {
        self.flush()?;
        self.file.take().unwrap().save_fast()
    }
}
