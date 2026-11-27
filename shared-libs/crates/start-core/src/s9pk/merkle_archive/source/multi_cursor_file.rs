use std::io::SeekFrom;
use std::os::fd::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;

use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, ReadBuf, Take};
use tokio::sync::{Mutex, OwnedMutexGuard};

use crate::disk::mount::filesystem::loop_dev::LoopDev;
use crate::prelude::*;
use crate::s9pk::merkle_archive::source::{ArchiveSource, Section};
use crate::util::io::open_file;

fn path_from_fd(fd: RawFd) -> Result<PathBuf, Error> {
    #[cfg(target_os = "linux")]
    let path = Path::new("/proc/self/fd").join(fd.to_string());
    #[cfg(target_os = "macos")]
    let path = unsafe {
        let mut buf = [0u8; libc::PATH_MAX as usize];
        if libc::fcntl(fd, libc::F_GETPATH, buf.as_mut_ptr().cast::<libc::c_char>()) == -1 {
            return Err(std::io::Error::last_os_error().into());
        }
        Path::new(
            &*std::ffi::CStr::from_bytes_until_nul(&buf)
                .with_kind(ErrorKind::Utf8)?
                .to_string_lossy(),
        )
        .to_owned()
    };
    Ok(path)
}

#[derive(Clone)]
pub struct MultiCursorFile {
    fd: RawFd,
    file: Arc<Mutex<File>>,
}
impl MultiCursorFile {
    pub fn path(&self) -> Result<PathBuf, Error> {
        path_from_fd(self.fd)
    }
    pub async fn open(fd: &impl AsRawFd) -> Result<Self, Error> {
        let f = open_file(path_from_fd(fd.as_raw_fd())?).await?;
        Ok(Self::from(f))
    }
    pub async fn cursor(&self) -> Result<FileCursor, Error> {
        Ok(FileCursor(
            if let Ok(file) = self.file.clone().try_lock_owned() {
                file
            } else {
                Arc::new(Mutex::new(open_file(self.path()?).await?))
                    .try_lock_owned()
                    .expect("freshly created")
            },
        ))
    }
    pub async fn blake3_mmap(&self) -> Result<blake3::Hash, Error> {
        let path = self.path()?;
        let file = self.file.clone();
        crate::util::cleanup::run_filesystem_blocking(move || {
            let _file = file;
            let mut hasher = blake3::Hasher::new();
            hasher.update_mmap_rayon(path)?;
            Ok(hasher.finalize())
        })
        .await
        .with_kind(ErrorKind::Unknown)?
    }
}
impl From<File> for MultiCursorFile {
    fn from(value: File) -> Self {
        Self {
            fd: value.as_raw_fd(),
            file: Arc::new(Mutex::new(value)),
        }
    }
}

#[pin_project::pin_project]
pub struct FileCursor(#[pin] OwnedMutexGuard<File>);
impl AsyncRead for FileCursor {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.project();
        Pin::new(&mut (&mut **this.0.get_mut())).poll_read(cx, buf)
    }
}
impl AsyncSeek for FileCursor {
    fn start_seek(self: Pin<&mut Self>, position: SeekFrom) -> std::io::Result<()> {
        let this = self.project();
        Pin::new(&mut (&mut **this.0.get_mut())).start_seek(position)
    }
    fn poll_complete(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::io::Result<u64>> {
        let this = self.project();
        Pin::new(&mut (&mut **this.0.get_mut())).poll_complete(cx)
    }
}
impl std::ops::Deref for FileCursor {
    type Target = File;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}
impl std::ops::DerefMut for FileCursor {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.0
    }
}

impl ArchiveSource for MultiCursorFile {
    type FetchReader = Take<FileCursor>;
    type FetchAllReader = FileCursor;
    async fn size(&self) -> Option<u64> {
        tokio::fs::metadata(self.path().ok()?)
            .await
            .ok()
            .map(|m| m.len())
    }
    async fn fetch_all(&self) -> Result<Self::FetchAllReader, Error> {
        use tokio::io::AsyncSeekExt;

        let mut file = self.cursor().await?;
        file.0.seek(SeekFrom::Start(0)).await?;
        Ok(file)
    }
    async fn fetch(&self, position: u64, size: u64) -> Result<Self::FetchReader, Error> {
        use tokio::io::AsyncSeekExt;

        let mut file = self.cursor().await?;
        file.0.seek(SeekFrom::Start(position)).await?;
        Ok(file.take(size))
    }
}

impl From<&Section<MultiCursorFile>> for LoopDev<PathBuf> {
    fn from(value: &Section<MultiCursorFile>) -> Self {
        LoopDev::new(value.source.path().unwrap(), value.position, value.size)
    }
}
