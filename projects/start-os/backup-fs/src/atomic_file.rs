use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::ops::{Deref, DerefMut};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::PathBuf;

use crate::error::BkfsResult;

pub struct AtomicFile {
    tmp_path: PathBuf,
    path: PathBuf,
    file: Option<File>,
}
impl AtomicFile {
    pub fn new(path: PathBuf, opt: &OpenOptions) -> BkfsResult<Self> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let tmp_path = path.with_extension("tmp");
        let file = opt.open(&tmp_path)?;
        Ok(Self {
            tmp_path,
            path,
            file: Some(file),
        })
    }

    pub fn create(path: PathBuf) -> BkfsResult<Self> {
        Self::new(
            path,
            OpenOptions::new()
                .read(true)
                .write(true)
                .truncate(true)
                .create(true)
                .custom_flags(libc::O_DIRECT | libc::O_NOFOLLOW),
        )
    }

    /// Create a plain (non-O_DIRECT) atomic file. Intended for small
    /// metadata files — inodes are usually well under 4 KiB, and the
    /// O_DIRECT alignment dance + per-write device round-trips are pure
    /// overhead at that size. Content goes through the page cache and
    /// becomes durable on the next fsync/syncfs.
    pub fn create_buffered(path: PathBuf) -> BkfsResult<Self> {
        Self::new(
            path,
            OpenOptions::new()
                .read(true)
                .write(true)
                .truncate(true)
                .create(true)
                .custom_flags(libc::O_NOFOLLOW),
        )
    }

    #[allow(dead_code)]
    pub fn rollback(mut self) -> BkfsResult<()> {
        drop(self.file.take());
        std::fs::remove_file(&self.tmp_path)?;
        Ok(())
    }

    pub fn save(mut self) -> BkfsResult<()> {
        if let Some(file) = self.file.as_mut() {
            file.flush()?;
            file.sync_all()?;
        }
        drop(self.file.take());
        std::fs::rename(&self.tmp_path, &self.path)?;
        Ok(())
    }

    /// Synchronizes file content and the containing directory.
    pub fn save_durable(self) -> BkfsResult<()> {
        let directory = File::open(self.path.parent().unwrap())?;
        self.save_with_directory_sync(&directory)
    }

    /// The directory must contain the destination.
    pub(crate) fn save_with_directory_sync(self, directory: &File) -> BkfsResult<()> {
        self.save()?;
        directory.sync_all()?;
        Ok(())
    }

    /// Rename without sync_all. Data lives in the page cache (or was
    /// pushed to the block layer via O_DIRECT) and is not guaranteed
    /// durable until the caller issues a syncfs. Use this when a batch
    /// of saves will be group-committed together — trades per-file
    /// durability for throughput.
    pub fn save_fast(mut self) -> BkfsResult<()> {
        if let Some(file) = self.file.as_mut() {
            file.flush()?;
        }
        drop(self.file.take());
        std::fs::rename(&self.tmp_path, &self.path)?;
        Ok(())
    }
}
impl Deref for AtomicFile {
    type Target = File;
    fn deref(&self) -> &Self::Target {
        self.file.as_ref().unwrap()
    }
}
impl DerefMut for AtomicFile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.file.as_mut().unwrap()
    }
}
impl Drop for AtomicFile {
    fn drop(&mut self) {
        if let Some(file) = self.file.take() {
            drop(file);
            let path = std::mem::take(&mut self.tmp_path);
            if let Err(e) = std::fs::remove_file(path) {
                log::error!("failed to clean up tmp file: {e}");
            }
        }
    }
}
impl Read for AtomicFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.deref_mut().read(buf)
    }
    fn read_vectored(&mut self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        self.deref_mut().read_vectored(bufs)
    }
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        self.deref_mut().read_to_end(buf)
    }
    fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        self.deref_mut().read_to_string(buf)
    }
}
impl Write for AtomicFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.deref_mut().write(buf)
    }
    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        self.deref_mut().write_vectored(bufs)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.deref_mut().flush()
    }
}
impl Seek for AtomicFile {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.deref_mut().seek(pos)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporary_symlinks_cannot_truncate_their_targets() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        let destination = dir.path().join("destination");
        std::fs::write(&target, b"untouched").unwrap();
        std::os::unix::fs::symlink(&target, destination.with_extension("tmp")).unwrap();
        let constructors: [fn(PathBuf) -> BkfsResult<AtomicFile>; 2] =
            [AtomicFile::create, AtomicFile::create_buffered];
        for create in constructors {
            assert!(create(destination.clone()).is_err());
            assert_eq!(std::fs::read(&target).unwrap(), b"untouched");
        }
    }
}

impl AsRawFd for AtomicFile {
    fn as_raw_fd(&self) -> RawFd {
        self.deref().as_raw_fd()
    }
}
impl FileExt for AtomicFile {
    fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<usize> {
        self.deref().read_at(buf, offset)
    }
    fn write_at(&self, buf: &[u8], offset: u64) -> io::Result<usize> {
        self.deref().write_at(buf, offset)
    }
}
