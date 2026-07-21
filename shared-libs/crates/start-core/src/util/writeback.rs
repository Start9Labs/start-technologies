//! Bounded-memory streaming writes for large s9pk transfers.
//!
//! Replaces O_DIRECT on the download/sideload/install path. Writes stay
//! buffered — so the file remains seekable for mirror-resume and the
//! concurrent archive reader keeps hitting the page cache — but writeback is
//! paced with `sync_file_range` so dirty pages never accumulate into a
//! multi-gigabyte final `fdatasync` that locks up the box. Paired with
//! [`preallocate`] and [`set_no_cow`] to keep the file contiguous on CoW
//! btrfs.
//!
//! Linux-only mechanics; on other targets (start-cli on macOS) pacing,
//! preallocation, and the nodatacow hint are no-ops and the kernel's own
//! writeback throttling bounds dirty memory.

use std::os::fd::{AsRawFd, RawFd};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

#[cfg(target_os = "linux")]
use tokio::process::Command;
use tokio::task::JoinHandle;

#[cfg(target_os = "linux")]
use crate::prelude::*;
#[cfg(target_os = "linux")]
use crate::util::Invoke;

/// Issue asynchronous writeback once this many bytes have piled up unkicked.
#[cfg(target_os = "linux")]
const KICK_EVERY: u64 = 16 << 20;
/// Cap on bytes left un-flushed behind the write head (bounds the final fsync).
#[cfg(target_os = "linux")]
const DIRTY_WINDOW: u64 = 64 << 20;

/// Paces page-cache writeback for a buffered file being streamed to.
///
/// Drive it either with [`pace`](Self::pace) after each write, or from an
/// `AsyncWrite` impl via [`poll_pace`](Self::poll_pace).
// Fields drive the linux `sync_file_range` path; elsewhere pacing is a no-op.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct WritebackPacer {
    fd: RawFd,
    kicked: u64,
    written_back: u64,
    pending: Option<JoinHandle<std::io::Result<u64>>>,
}
impl WritebackPacer {
    pub fn new(fd: RawFd) -> Self {
        Self {
            fd,
            kicked: 0,
            written_back: 0,
            pending: None,
        }
    }

    /// Reset offsets after the file is truncated back to empty (mirror restart).
    /// Detaching an in-flight flush is harmless: its range no longer has pages.
    pub fn reset(&mut self) {
        self.kicked = 0;
        self.written_back = 0;
        self.pending = None;
    }

    pub async fn pace(&mut self, written: u64) -> std::io::Result<()> {
        std::future::poll_fn(|cx| self.poll_pace(cx, written)).await
    }

    #[cfg(target_os = "linux")]
    pub fn poll_pace(&mut self, cx: &mut Context<'_>, written: u64) -> Poll<std::io::Result<()>> {
        if let Some(pending) = &mut self.pending {
            match Pin::new(pending).poll(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Ok(Ok(waited_to))) => {
                    self.pending = None;
                    self.written_back = self.written_back.max(waited_to);
                }
                Poll::Ready(Ok(Err(e))) => {
                    self.pending = None;
                    return Poll::Ready(Err(e));
                }
                Poll::Ready(Err(e)) => {
                    self.pending = None;
                    return Poll::Ready(Err(std::io::Error::other(e)));
                }
            }
        }
        if written.saturating_sub(self.kicked) < KICK_EVERY {
            return Poll::Ready(Ok(()));
        }
        let fd = self.fd;
        let kick_from = self.kicked;
        let wait_from = self.written_back;
        let wait_to = written.saturating_sub(DIRTY_WINDOW);
        self.kicked = written;
        self.pending = Some(tokio::task::spawn_blocking(move || {
            sync_range(
                fd,
                kick_from,
                written - kick_from,
                libc::SYNC_FILE_RANGE_WRITE,
            )?;
            if wait_to > wait_from {
                sync_range(
                    fd,
                    wait_from,
                    wait_to - wait_from,
                    libc::SYNC_FILE_RANGE_WAIT_BEFORE
                        | libc::SYNC_FILE_RANGE_WRITE
                        | libc::SYNC_FILE_RANGE_WAIT_AFTER,
                )?;
            }
            Ok(wait_to)
        }));
        Poll::Ready(Ok(()))
    }

    #[cfg(not(target_os = "linux"))]
    pub fn poll_pace(&mut self, _cx: &mut Context<'_>, _written: u64) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[cfg(target_os = "linux")]
fn sync_range(fd: RawFd, offset: u64, nbytes: u64, flags: libc::c_uint) -> std::io::Result<()> {
    // SAFETY: fd is owned by the caller's File for the duration of this call.
    let ret = unsafe {
        libc::sync_file_range(fd, offset as libc::off64_t, nbytes as libc::off64_t, flags)
    };
    if ret == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Wraps a writer and paces writeback as bytes pass through.
#[pin_project::pin_project]
pub struct PacedWriter<W> {
    #[pin]
    inner: W,
    pacer: WritebackPacer,
    written: u64,
}
impl<W: AsRawFd> PacedWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            pacer: WritebackPacer::new(inner.as_raw_fd()),
            inner,
            written: 0,
        }
    }
    pub fn into_inner(self) -> W {
        self.inner
    }
}
impl<W: tokio::io::AsyncWrite> tokio::io::AsyncWrite for PacedWriter<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.project();
        std::task::ready!(this.pacer.poll_pace(cx, *this.written))?;
        match this.inner.poll_write(cx, buf) {
            Poll::Ready(Ok(n)) => {
                *this.written += n as u64;
                Poll::Ready(Ok(n))
            }
            a => a,
        }
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.project().inner.poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.project().inner.poll_shutdown(cx)
    }
    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.project();
        std::task::ready!(this.pacer.poll_pace(cx, *this.written))?;
        match this.inner.poll_write_vectored(cx, bufs) {
            Poll::Ready(Ok(n)) => {
                *this.written += n as u64;
                Poll::Ready(Ok(n))
            }
            a => a,
        }
    }
}
impl<W: tokio::io::AsyncSeek> tokio::io::AsyncSeek for PacedWriter<W> {
    fn start_seek(self: Pin<&mut Self>, position: std::io::SeekFrom) -> std::io::Result<()> {
        self.project().inner.start_seek(position)
    }
    fn poll_complete(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<u64>> {
        self.project().inner.poll_complete(cx)
    }
}

/// Reserve `len` contiguous bytes up front so a buffered stream doesn't scatter
/// across fragmented free space on CoW btrfs. Best-effort — callers ignore the
/// error on filesystems that don't support `fallocate`.
#[cfg(target_os = "linux")]
pub async fn preallocate(fd: RawFd, len: u64) -> std::io::Result<()> {
    // `len` comes from an untrusted Content-Length; skip rather than wrap the signed cast.
    let Ok(len) = libc::off_t::try_from(len) else {
        return Ok(());
    };
    if len == 0 {
        return Ok(());
    }
    tokio::task::spawn_blocking(move || {
        // SAFETY: fd is a valid open file descriptor owned by the caller.
        let ret = unsafe { libc::fallocate(fd, libc::FALLOC_FL_KEEP_SIZE, 0, len) };
        if ret == -1 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    })
    .await
    .map_err(std::io::Error::other)?
}

#[cfg(not(target_os = "linux"))]
pub async fn preallocate(_fd: RawFd, _len: u64) -> std::io::Result<()> {
    Ok(())
}

/// Free the tail of a [`preallocate`] reservation past what was actually
/// written. Beyond-EOF reservations are otherwise kept for the life of the
/// file. Best-effort, like `preallocate`.
#[cfg(target_os = "linux")]
pub async fn release_beyond(fd: RawFd, written: u64, reserved: u64) -> std::io::Result<()> {
    if written >= reserved {
        return Ok(());
    }
    let (Ok(offset), Ok(len)) = (
        libc::off_t::try_from(written),
        libc::off_t::try_from(reserved - written),
    ) else {
        return Ok(());
    };
    tokio::task::spawn_blocking(move || {
        // SAFETY: fd is a valid open file descriptor owned by the caller.
        let ret = unsafe {
            libc::fallocate(
                fd,
                libc::FALLOC_FL_PUNCH_HOLE | libc::FALLOC_FL_KEEP_SIZE,
                offset,
                len,
            )
        };
        if ret == -1 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    })
    .await
    .map_err(std::io::Error::other)?
}

#[cfg(not(target_os = "linux"))]
pub async fn release_beyond(_fd: RawFd, _written: u64, _reserved: u64) -> std::io::Result<()> {
    Ok(())
}

/// Mark a path `nodatacow` (`chattr +C`) so writes don't fragment via CoW on
/// btrfs. Only takes effect on an empty file, so call it right after creation,
/// before any write or `fallocate`. Best-effort: off btrfs the flag is
/// unsupported, an expected miss logged at debug rather than as an error.
#[cfg(target_os = "linux")]
pub async fn set_no_cow(path: impl AsRef<Path>) {
    let path = path.as_ref();
    if let Err(e) = Command::new("chattr")
        .arg("+C")
        .arg(path)
        .invoke(ErrorKind::Filesystem)
        .await
    {
        tracing::debug!("nodatacow unavailable for {}: {e}", path.display());
    }
}

#[cfg(not(target_os = "linux"))]
pub async fn set_no_cow(_path: impl AsRef<Path>) {}
