//! Bounded-memory streaming writes for large s9pk transfers.
//!
//! Replaces O_DIRECT on the download/sideload path. Writes stay buffered — so
//! the file remains seekable for mirror-resume and the concurrent archive
//! reader keeps hitting the page cache — but writeback is paced with
//! `sync_file_range` so dirty pages never accumulate into a multi-gigabyte
//! final `fdatasync` that locks up the box. Paired with [`preallocate`] and
//! [`set_no_cow`] to keep the file contiguous on CoW btrfs.
//!
//! Linux-only mechanics; on other targets (start-cli on macOS) pacing,
//! preallocation, and the nodatacow hint are no-ops and the kernel's own
//! writeback throttling bounds dirty memory.

use std::os::fd::RawFd;
use std::path::Path;

#[cfg(target_os = "linux")]
use tokio::process::Command;

#[cfg(target_os = "linux")]
use crate::prelude::*;
#[cfg(target_os = "linux")]
use crate::util::Invoke;

/// Issue asynchronous writeback once this many bytes have piled up unkicked.
#[cfg(target_os = "linux")]
const KICK_EVERY: u64 = 16 << 20;
/// Cap on bytes left un-durable behind the write head (bounds the final fsync).
#[cfg(target_os = "linux")]
const DIRTY_WINDOW: u64 = 64 << 20;

/// Paces page-cache writeback for a buffered file being streamed to.
///
/// Call [`pace`](Self::pace) after each write with the running byte total.
// Fields drive the linux `sync_file_range` path; elsewhere `pace` is a no-op.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct WritebackPacer {
    fd: RawFd,
    kicked: u64,
    durable: u64,
}
impl WritebackPacer {
    pub fn new(fd: RawFd) -> Self {
        Self {
            fd,
            kicked: 0,
            durable: 0,
        }
    }

    /// Reset offsets after the file is truncated back to empty (mirror restart).
    pub fn reset(&mut self) {
        self.kicked = 0;
        self.durable = 0;
    }

    #[cfg(target_os = "linux")]
    pub async fn pace(&mut self, written: u64) -> std::io::Result<()> {
        if written.saturating_sub(self.kicked) < KICK_EVERY {
            return Ok(());
        }
        let fd = self.fd;
        let kick_from = self.kicked;
        let wait_from = self.durable;
        let want_durable = written.saturating_sub(DIRTY_WINDOW);
        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            sync_range(
                fd,
                kick_from,
                written - kick_from,
                libc::SYNC_FILE_RANGE_WRITE,
            )?;
            if want_durable > wait_from {
                sync_range(
                    fd,
                    wait_from,
                    want_durable - wait_from,
                    libc::SYNC_FILE_RANGE_WAIT_BEFORE
                        | libc::SYNC_FILE_RANGE_WRITE
                        | libc::SYNC_FILE_RANGE_WAIT_AFTER,
                )?;
            }
            Ok(())
        })
        .await
        .map_err(std::io::Error::other)??;
        self.kicked = written;
        self.durable = want_durable.max(self.durable);
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    pub async fn pace(&mut self, _written: u64) -> std::io::Result<()> {
        Ok(())
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

/// Mark a path `nodatacow` (`chattr +C`) so writes don't fragment via CoW on
/// btrfs. Set it on an empty file/dir (files created inside a `+C` dir inherit
/// it) — the flag has no effect once extents exist. Best-effort: off btrfs the
/// flag is unsupported, an expected miss logged at debug rather than as an error.
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
