//! Non-Linux stub for [`super::syscall`].
//!
//! The mount API and the LXC userns dance are Linux-only, but `start-cli` is
//! cross-built for macOS from the same crate, so the lib must still compile
//! off Linux. These stubs are signature-compatible and error at runtime;
//! `start-cli` never invokes the mount layer, so they should never be called.

use std::os::fd::{BorrowedFd, OwnedFd};
use std::path::Path;

use crate::prelude::*;

fn unsupported<T>() -> Result<T, Error> {
    Err(Error::new(
        eyre!("mount syscall API is only available on Linux"),
        ErrorKind::Filesystem,
    ))
}

pub fn open_tree_clone(_path: &Path, _recursive: bool) -> Result<OwnedFd, Error> {
    unsupported()
}

pub fn open_tree_attr_idmap(
    _path: &Path,
    _recursive: bool,
    _userns: BorrowedFd,
) -> Result<OwnedFd, Error> {
    unsupported()
}

pub struct DetachedMount;
impl DetachedMount {
    pub fn from_fd(_fd: OwnedFd) -> Self {
        Self
    }
    pub fn set_readonly(&self, _ro: bool) -> Result<(), Error> {
        unsupported()
    }
    pub fn attach(self, _mountpoint: &Path) -> Result<(), Error> {
        unsupported()
    }
}

pub async fn userns_fd_from_idmap(
    _idmap: &[crate::disk::mount::filesystem::idmapped::IdMap],
) -> Result<OwnedFd, Error> {
    unsupported()
}

pub fn unshare_userns_main() -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "unshare-userns is Linux-only",
    ))
}
