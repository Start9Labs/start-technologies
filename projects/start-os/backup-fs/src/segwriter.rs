//! Append-only segment writer.
//!
//! Frames fill aligned 1 MiB units that pool threads write concurrently.
//! [`SegmentWriter::sync`] pads open units to the alignment boundary, waits
//! for their writes and synchronizes the files; no byte range is written twice.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{unbounded, Receiver, Sender};

use crate::aligned_io::AlignedBuf;
use crate::error::{BkfsError, BkfsResult, BkfsResultExt};
use crate::seglog::segment_path;

pub(crate) const UNIT: usize = 1 << 20;
const ALIGN: u64 = 4096;
const DEFAULT_IN_FLIGHT: usize = 16 * UNIT;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Bytes the writer keeps in flight. `BACKUPFS_WRITE_IN_FLIGHT` is in MiB.
fn in_flight_limit() -> usize {
    static LIMIT: OnceLock<usize> = OnceLock::new();
    *LIMIT.get_or_init(|| {
        std::env::var("BACKUPFS_WRITE_IN_FLIGHT")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|&n| n > 0)
            .map(|n| n.saturating_mul(UNIT))
            .unwrap_or(DEFAULT_IN_FLIGHT)
    })
}

/// Longest wait for backing writes before the writer fails closed.
fn write_timeout() -> Duration {
    static TIMEOUT: OnceLock<Duration> = OnceLock::new();
    *TIMEOUT.get_or_init(|| {
        std::env::var("BACKUPFS_FLUSH_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_TIMEOUT)
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Stream {
    Metadata,
    Data,
}

impl Stream {
    fn slot(self) -> usize {
        match self {
            Self::Metadata => 0,
            Self::Data => 1,
        }
    }
}

fn create_segment(dir: &Path, id: u64) -> io::Result<File> {
    let open = |direct: bool| {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(libc::O_NOFOLLOW | if direct { libc::O_DIRECT } else { 0 })
            .open(segment_path(dir, id))
    };
    match open(true) {
        Err(e) if e.raw_os_error() == Some(libc::EINVAL) => {
            std::fs::remove_file(segment_path(dir, id)).or_else(|e| match e.kind() {
                io::ErrorKind::NotFound => Ok(()),
                _ => Err(e),
            })?;
            open(false)
        }
        result => result,
    }
}

#[derive(Default)]
struct Progress {
    in_flight: usize,
    /// Whether bytes were written since the file was last synchronized.
    dirty: bool,
    /// Completed ranges past `durable`, keyed by start.
    completed: BTreeMap<u64, u64>,
    /// Every byte below this offset is written.
    durable: u64,
    error: Option<io::Error>,
}

struct SegmentFile {
    id: u64,
    file: File,
    progress: Mutex<Progress>,
    done: Condvar,
}

impl SegmentFile {
    fn complete(&self, start: u64, end: u64, result: io::Result<()>) {
        let mut progress = self.progress.lock().unwrap();
        progress.in_flight -= 1;
        match result {
            Ok(()) => {
                progress.dirty = true;
                progress.completed.insert(start, end);
                loop {
                    let durable = progress.durable;
                    let Some(end) = progress.completed.remove(&durable) else {
                        break;
                    };
                    progress.durable = end;
                }
            }
            Err(error) => {
                progress.error.get_or_insert(error);
            }
        }
        drop(progress);
        self.done.notify_all();
    }

    fn durable(&self) -> u64 {
        self.progress.lock().unwrap().durable
    }

    /// Waits for submitted writes and synchronizes any written bytes.
    fn settle(&self, deadline: Instant) -> io::Result<()> {
        let mut progress = self.progress.lock().unwrap();
        while progress.in_flight > 0 && progress.error.is_none() {
            let now = Instant::now();
            if now >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "segment writes exceeded {}s timeout",
                        write_timeout().as_secs()
                    ),
                ));
            }
            progress = self.done.wait_timeout(progress, deadline - now).unwrap().0;
        }
        if let Some(error) = &progress.error {
            return Err(match error.raw_os_error() {
                Some(code) => io::Error::from_raw_os_error(code),
                None => io::Error::new(error.kind(), error.to_string()),
            });
        }
        if !std::mem::take(&mut progress.dirty) {
            return Ok(());
        }
        drop(progress);
        let result = self.file.sync_data();
        if result.is_err() {
            self.progress.lock().unwrap().dirty = true;
        }
        result
    }
}

#[cfg(test)]
pub(crate) static FAIL_SEGMENT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(u64::MAX);

fn write_unit(job: &Job) -> io::Result<()> {
    #[cfg(test)]
    if FAIL_SEGMENT.load(std::sync::atomic::Ordering::SeqCst) == job.segment.id {
        return Err(io::Error::from_raw_os_error(libc::ENOSPC));
    }
    job.segment
        .file
        .write_all_at(&job.buf.as_slice()[..job.len], job.offset)
}

struct Job {
    segment: Arc<SegmentFile>,
    offset: u64,
    buf: AlignedBuf,
    len: usize,
}

struct Budget {
    used: Mutex<usize>,
    freed: Condvar,
    limit: usize,
}

/// Write threads start on first use in each process.
struct Pool {
    workers: Mutex<Option<Workers>>,
    budget: Arc<Budget>,
}

struct Workers {
    pid: u32,
    sender: Sender<Job>,
    threads: Vec<JoinHandle<()>>,
}

impl Pool {
    fn new() -> Self {
        Self {
            workers: Mutex::new(None),
            budget: Arc::new(Budget {
                used: Mutex::new(0),
                freed: Condvar::new(),
                limit: in_flight_limit(),
            }),
        }
    }

    fn sender(&self) -> Sender<Job> {
        let mut workers = self.workers.lock().unwrap();
        let pid = std::process::id();
        if workers.as_ref().is_none_or(|workers| workers.pid != pid) {
            // Threads of another process cannot be joined or detached here.
            std::mem::forget(workers.take());
            let (sender, receiver) = unbounded::<Job>();
            let threads = (0..(self.budget.limit / UNIT).clamp(1, 64))
                .map(|i| {
                    let receiver: Receiver<Job> = receiver.clone();
                    let budget = self.budget.clone();
                    std::thread::Builder::new()
                        .name(format!("backup-fs-segment-{i}"))
                        .spawn(move || {
                            while let Ok(job) = receiver.recv() {
                                let end = job.offset + job.len as u64;
                                let result = write_unit(&job);
                                job.segment.complete(job.offset, end, result);
                                let mut used = budget.used.lock().unwrap();
                                *used -= job.len;
                                drop(used);
                                budget.freed.notify_all();
                            }
                        })
                        .expect("spawn segment writer")
                })
                .collect();
            *workers = Some(Workers {
                pid,
                sender,
                threads,
            });
        }
        workers.as_ref().unwrap().sender.clone()
    }

    fn submit(&self, job: Job) -> io::Result<()> {
        let deadline = Instant::now() + write_timeout();
        let mut used = self.budget.used.lock().unwrap();
        while *used > 0 && *used + job.len > self.budget.limit {
            let now = Instant::now();
            if now >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "segment writes made no progress before the timeout",
                ));
            }
            used = self
                .budget
                .freed
                .wait_timeout(used, deadline - now)
                .unwrap()
                .0;
        }
        *used += job.len;
        drop(used);
        job.segment.progress.lock().unwrap().in_flight += 1;
        self.sender()
            .send(job)
            .map_err(|_| io::Error::other("segment writer stopped"))
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        if let Some(workers) = self.workers.get_mut().unwrap().take() {
            if workers.pid != std::process::id() {
                std::mem::forget(workers);
                return;
            }
            drop(workers.sender);
            for thread in workers.threads {
                let _ = thread.join();
            }
        }
    }
}

struct Active {
    segment: Arc<SegmentFile>,
    end: u64,
    buf: AlignedBuf,
    base: u64,
    len: usize,
}

impl Active {
    fn submit(&mut self, pool: &Pool, pad: bool) -> io::Result<()> {
        if self.len == 0 {
            return Ok(());
        }
        let len = if pad {
            let aligned = (self.len as u64).div_ceil(ALIGN) * ALIGN;
            self.buf.as_mut_slice()[self.len..aligned as usize].fill(0);
            aligned as usize
        } else {
            self.len
        };
        let buf = std::mem::replace(&mut self.buf, AlignedBuf::new(UNIT));
        pool.submit(Job {
            segment: self.segment.clone(),
            offset: self.base,
            buf,
            len,
        })?;
        self.base += len as u64;
        self.end = self.base;
        self.len = 0;
        Ok(())
    }

    fn write(&mut self, pool: &Pool, mut bytes: &[u8]) -> io::Result<()> {
        while !bytes.is_empty() {
            let take = bytes.len().min(UNIT - self.len);
            self.buf.as_mut_slice()[self.len..self.len + take].copy_from_slice(&bytes[..take]);
            self.len += take;
            self.end += take as u64;
            bytes = &bytes[take..];
            if self.len == UNIT {
                self.submit(pool, false)?;
            }
        }
        Ok(())
    }
}

/// Appends frames to one active segment per [`Stream`].
pub(crate) struct SegmentWriter {
    dir: PathBuf,
    limit: u64,
    next_id: u64,
    active: [Option<Active>; 2],
    unsynced: Vec<Arc<SegmentFile>>,
    /// Frames not yet known to be readable from their files, in append order.
    pending: HashMap<u64, VecDeque<(u64, Arc<[u8]>)>>,
    /// Lengths of segments written since they were last reported.
    lengths: HashMap<u64, u64>,
    /// The errno of the first failed backing operation.
    failed: Option<i32>,
    pool: Pool,
}

impl SegmentWriter {
    /// The first segment id must exceed every existing one.
    pub fn new(dir: &Path, limit: u64, next_id: u64) -> Self {
        Self {
            dir: dir.to_owned(),
            limit,
            next_id,
            active: [None, None],
            unsynced: Vec::new(),
            pending: HashMap::new(),
            lengths: HashMap::new(),
            failed: None,
            pool: Pool::new(),
        }
    }

    pub fn check(&self) -> BkfsResult<()> {
        match self.failed {
            Some(code) => BkfsResult::errno_notrace(code),
            None => Ok(()),
        }
    }

    fn fail<T>(&mut self, error: io::Error) -> BkfsResult<T> {
        self.failed = Some(error.raw_os_error().unwrap_or(libc::EIO));
        Err(error.into())
    }

    /// Segments still accepting frames.
    pub fn active_segments(&self) -> impl Iterator<Item = u64> + '_ {
        self.active.iter().flatten().map(|active| active.segment.id)
    }

    /// Returns the segment and offset of the appended frame.
    pub fn append(&mut self, stream: Stream, frame: Vec<u8>) -> BkfsResult<(u64, u64)> {
        self.check()?;
        let len = frame.len() as u64;
        let slot = stream.slot();
        if self.active[slot]
            .as_ref()
            .is_some_and(|active| active.end > 0 && active.end + len > self.limit)
        {
            let mut active = self.active[slot].take().unwrap();
            if let Err(error) = active.submit(&self.pool, true) {
                return self.fail(error);
            }
            self.lengths.insert(active.segment.id, active.end);
            self.unsynced.push(active.segment);
        }
        if self.active[slot].is_none() {
            let id = self.next_id;
            self.next_id = id
                .checked_add(1)
                .ok_or_else(|| BkfsError::unsupported("data segment id exhausted"))?;
            let file = match create_segment(&self.dir, id) {
                Ok(file) => file,
                Err(error) => return self.fail(error),
            };
            self.active[slot] = Some(Active {
                segment: Arc::new(SegmentFile {
                    id,
                    file,
                    progress: Mutex::new(Progress::default()),
                    done: Condvar::new(),
                }),
                end: 0,
                buf: AlignedBuf::new(UNIT),
                base: 0,
                len: 0,
            });
        }
        let active = self.active[slot].as_mut().unwrap();
        let (segment, offset) = (active.segment.id, active.end);
        let result = active.write(&self.pool, &frame);
        let end = active.end;
        if let Err(error) = result {
            return self.fail(error);
        }
        self.lengths.insert(segment, end);
        self.pending
            .entry(segment)
            .or_default()
            .push_back((offset, frame.into()));
        self.release_written();
        Ok((segment, offset))
    }

    /// Drops in-memory copies of frames their files now hold.
    fn release_written(&mut self) {
        let files: HashMap<u64, u64> = self
            .active
            .iter()
            .flatten()
            .map(|active| &active.segment)
            .chain(&self.unsynced)
            .map(|file| (file.id, file.durable()))
            .collect();
        self.pending.retain(|segment, frames| {
            let durable = files.get(segment).copied().unwrap_or(u64::MAX);
            while frames
                .front()
                .is_some_and(|(offset, frame)| offset + frame.len() as u64 <= durable)
            {
                frames.pop_front();
            }
            !frames.is_empty()
        });
    }

    /// The bytes of a frame its file may not hold yet.
    pub fn pending(&self, segment: u64, offset: u64) -> Option<Arc<[u8]>> {
        let frames = self.pending.get(&segment)?;
        let index = frames
            .binary_search_by_key(&offset, |(offset, _)| *offset)
            .ok()?;
        Some(frames[index].1.clone())
    }

    /// Whether appended frames await [`Self::sync`].
    pub fn has_unsynced(&self) -> bool {
        !self.lengths.is_empty() || !self.unsynced.is_empty()
    }

    /// Lengths of segments written since the previous call.
    pub fn take_lengths(&mut self) -> HashMap<u64, u64> {
        std::mem::take(&mut self.lengths)
    }

    /// Makes every appended frame durable on the backing store.
    pub fn sync(&mut self) -> BkfsResult<()> {
        self.check()?;
        for active in self.active.iter_mut().flatten() {
            if active.len == 0 {
                continue;
            }
            if let Err(error) = active.submit(&self.pool, true) {
                self.failed = Some(error.raw_os_error().unwrap_or(libc::EIO));
                return Err(error.into());
            }
            self.lengths.insert(active.segment.id, active.end);
        }
        let deadline = Instant::now() + write_timeout();
        let files: Vec<_> = self
            .unsynced
            .iter()
            .cloned()
            .chain(self.active.iter().flatten().map(|a| a.segment.clone()))
            .collect();
        for file in &files {
            if let Err(error) = file.settle(deadline) {
                return self.fail(error);
            }
        }
        self.unsynced.clear();
        self.pending.clear();
        Ok(())
    }
}

#[cfg(test)]
#[path = "segwriter_tests.rs"]
mod tests;
