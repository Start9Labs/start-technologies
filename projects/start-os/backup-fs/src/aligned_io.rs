use std::alloc::{self, Layout};
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::{io, ptr};

const BLOCK_SIZE: usize = 4096;
const ALIGN_MASK: usize = BLOCK_SIZE - 1;
const WINDOW: usize = 1 << 20;

/// Serves ascending or descending range reads from one open file and one
/// cached window.
#[derive(Default)]
pub(crate) struct RangeReader {
    file: Option<(PathBuf, File)>,
    window: Vec<u8>,
    base: u64,
}

impl RangeReader {
    pub fn read_exact(&mut self, path: &Path, offset: u64, bytes: &mut [u8]) -> io::Result<()> {
        if self.file.as_ref().map(|(cached, _)| cached.as_path()) != Some(path) {
            self.file = Some((path.to_owned(), File::open(path)?));
            self.window.clear();
        }
        let file = &self.file.as_ref().unwrap().1;
        let end = offset + bytes.len() as u64;
        if bytes.len() > WINDOW {
            return file.read_exact_at(bytes, offset);
        }
        if offset < self.base || end > self.base + self.window.len() as u64 {
            self.base = offset / WINDOW as u64 * WINDOW as u64;
            if end > self.base + WINDOW as u64 {
                self.base = offset;
            }
            self.window.resize(WINDOW, 0);
            let mut filled = 0;
            while filled < WINDOW {
                match file.read_at(&mut self.window[filled..], self.base + filled as u64) {
                    Ok(0) => break,
                    Ok(n) => filled += n,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(e) => {
                        self.window.clear();
                        return Err(e);
                    }
                }
            }
            self.window.truncate(filled);
            if end > self.base + filled as u64 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
        }
        let start = (offset - self.base) as usize;
        bytes.copy_from_slice(&self.window[start..start + bytes.len()]);
        Ok(())
    }
}

/// A buffer with guaranteed 4096-byte alignment for O_DIRECT I/O.
pub(crate) struct AlignedBuf {
    ptr: ptr::NonNull<u8>,
    len: usize,
}

unsafe impl Send for AlignedBuf {}

impl AlignedBuf {
    pub(crate) fn new(len: usize) -> Self {
        let len = (len + ALIGN_MASK) & !ALIGN_MASK;
        let len = len.max(BLOCK_SIZE);
        let layout = Layout::from_size_align(len, BLOCK_SIZE).unwrap();
        let ptr = unsafe { alloc::alloc_zeroed(layout) };
        let ptr = ptr::NonNull::new(ptr).unwrap_or_else(|| alloc::handle_alloc_error(layout));
        Self { ptr, len }
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    pub(crate) fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl Drop for AlignedBuf {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.len, BLOCK_SIZE).unwrap();
        unsafe { alloc::dealloc(self.ptr.as_ptr(), layout) };
    }
}

#[cfg(test)]
#[path = "aligned_io_tests.rs"]
mod tests;
