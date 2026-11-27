//! A process-wide allocator for `OrdValue` nodes and strings, backed by a deleted scratch file.
//!
//! Pages of a shared file mapping can be written back and evicted by the kernel without swap,
//! so cold database values leave resident memory under pressure. The file is unlinked when
//! opened and holds nothing across restarts.
//!
//! Without [`init`], segments are anonymous memory and the arena behaves like the heap.

mod ptr;
mod str;

use std::alloc::{handle_alloc_error, GlobalAlloc, Layout};
use std::io;
use std::os::fd::{IntoRawFd, RawFd};
use std::path::Path;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};

use talc::base::binning::Binning;
use talc::base::Talc;
use talc::source::Source;

pub use self::ptr::ArenaK;
pub use self::str::ArenaStr;

const SEGMENT_SIZE: usize = 64 << 20;

static BACKING_FD: AtomicI32 = AtomicI32::new(-1);
static BACKING_LEN: AtomicUsize = AtomicUsize::new(0);
static MAPPED: AtomicBool = AtomicBool::new(false);

static ARENA: talc::TalcLock<parking_lot::RawMutex, Segments> = talc::TalcLock::new(Segments);

/// Backs every later arena segment with a deleted file in the given directory.
///
/// Fails once any segment is mapped, and when the directory's filesystem refuses the file.
pub fn init(dir: &Path) -> io::Result<()> {
    if MAPPED.load(Ordering::Acquire) {
        return Err(io::Error::other("the value arena already holds data"));
    }
    let file = tempfile::tempfile_in(dir)?;
    let fd = file.into_raw_fd();
    if BACKING_FD
        .compare_exchange(-1, fd, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        unsafe { libc::close(fd) };
        return Err(io::Error::other(
            "the value arena is already backed by a file",
        ));
    }
    Ok(())
}

/// Bytes of the backing file mapped so far; zero for anonymous segments.
pub fn backing_len() -> usize {
    BACKING_LEN.load(Ordering::Relaxed)
}

pub(crate) fn alloc(layout: Layout) -> NonNull<u8> {
    match NonNull::new(unsafe { GlobalAlloc::alloc(&ARENA, layout) }) {
        Some(ptr) => ptr,
        None => handle_alloc_error(layout),
    }
}

/// The caller must pass a pointer [`alloc`] returned for the same layout, exactly once.
pub(crate) unsafe fn dealloc(ptr: NonNull<u8>, layout: Layout) {
    unsafe { GlobalAlloc::dealloc(&ARENA, ptr.as_ptr(), layout) }
}

#[derive(Debug)]
struct Segments;

impl Segments {
    /// Maps a new segment of at least the requested size; must not allocate.
    fn map(size: usize) -> Option<NonNull<u8>> {
        let fd: RawFd = BACKING_FD.load(Ordering::Acquire);
        let ptr = unsafe {
            if fd >= 0 {
                let offset = BACKING_LEN.load(Ordering::Relaxed);
                if libc::ftruncate(fd, (offset + size) as libc::off_t) != 0 {
                    return None;
                }
                let ptr = libc::mmap(
                    std::ptr::null_mut(),
                    size,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    fd,
                    offset as libc::off_t,
                );
                if ptr != libc::MAP_FAILED {
                    BACKING_LEN.store(offset + size, Ordering::Relaxed);
                }
                ptr
            } else {
                libc::mmap(
                    std::ptr::null_mut(),
                    size,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                    -1,
                    0,
                )
            }
        };
        if ptr == libc::MAP_FAILED {
            return None;
        }
        MAPPED.store(true, Ordering::Release);
        NonNull::new(ptr.cast())
    }
}

unsafe impl Source for Segments {
    fn acquire<B: Binning>(talc: &mut Talc<Self, B>, layout: Layout) -> Result<(), ()> {
        let page = 4096;
        let wanted = layout.size() + layout.align() + 2 * page;
        let size = wanted.max(SEGMENT_SIZE).next_multiple_of(page);
        let base = Segments::map(size).ok_or(())?;
        unsafe { talc.claim(base.as_ptr(), size) }
            .map(|_| ())
            .ok_or(())
    }
}

#[cfg(test)]
mod tests;
