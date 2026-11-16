#[cfg(test)]
#[path = "shared_tree_tests.rs"]
mod shared_tree_tests;
#[cfg(test)]
#[path = "bind_tests.rs"]
mod tests;

use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use digest::Digest;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use super::FileSystem;
use super::syscall::{DetachedMount, open_tree_clone};
use crate::disk::mount::filesystem::MountType;
use crate::prelude::*;
use crate::util::io::{canonicalize, create_file};

#[derive(Debug, Clone, Serialize, Deserialize, VisitVariants)]
#[serde(rename_all = "kebab-case")]
pub enum FileType {
    File,
    Directory,
    Infer,
}

rpc_toolkit::reflect_ts!(FileType);
rpc_toolkit::ts_export!(FileType, namespaces = [""]);
impl Default for FileType {
    fn default() -> Self {
        FileType::Directory
    }
}

pub struct Bind<Src: AsRef<Path>> {
    src: Src,
    filetype: FileType,
    recursive: bool,
}
impl<Src: AsRef<Path>> Bind<Src> {
    pub fn new(src: Src) -> Self {
        Self {
            src,
            filetype: FileType::Directory,
            recursive: false,
        }
    }
    pub fn with_type(mut self, filetype: FileType) -> Self {
        self.filetype = filetype;
        self
    }
    /// Cloned submounts share the attachment guard's lifetime and read-only policy.
    /// Read-only clones are private; writable clones retain source propagation until retirement.
    pub fn recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }
}
impl<Src: AsRef<Path> + Send + Sync> FileSystem for Bind<Src> {
    async fn source(&self) -> Result<Option<impl AsRef<Path>>, Error> {
        Ok(Some(&self.src))
    }
    fn extra_args(&self) -> impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>> {
        [if self.recursive { "--rbind" } else { "--bind" }]
    }
    async fn pre_mount(&self, mountpoint: &Path, mount_type: MountType) -> Result<(), Error> {
        let from_meta = tokio::fs::metadata(&self.src).await.ok();
        let mut to_meta = tokio::fs::metadata(&mountpoint).await.ok();
        if matches!(self.filetype, FileType::File)
            || (matches!(self.filetype, FileType::Infer)
                && from_meta.as_ref().map_or(false, |m| m.is_file()))
        {
            if to_meta.as_ref().map_or(false, |m| m.is_dir()) {
                tokio::fs::remove_dir(mountpoint).await?;
                to_meta = None;
            }
            if from_meta.is_none() && mount_type != MountType::ReadOnly {
                create_file(self.src.as_ref()).await?.sync_all().await?;
            }
            if to_meta.is_none() {
                create_file(mountpoint).await?.sync_all().await?;
            }
        } else {
            if to_meta.as_ref().map_or(false, |m| m.is_file()) {
                tokio::fs::remove_file(mountpoint).await?;
                to_meta = None;
            }
            if from_meta.is_none() && mount_type != MountType::ReadOnly {
                tokio::fs::create_dir_all(self.src.as_ref()).await?;
            }
            if to_meta.is_none() {
                tokio::fs::create_dir_all(mountpoint).await?;
            }
        }
        Ok(())
    }
    fn is_mount_tree(&self) -> bool {
        self.recursive
    }
    async fn mount<P: AsRef<Path> + Send>(
        &self,
        mountpoint: P,
        mount_type: MountType,
    ) -> Result<(), Error> {
        if self.recursive && mount_type == MountType::ReadOnly {
            self.pre_mount(mountpoint.as_ref(), mount_type).await?;
            let detached = DetachedMount::from_fd(open_tree_clone(self.src.as_ref(), true)?);
            detached.set_private()?;
            detached.set_readonly(true)?;
            detached.attach(mountpoint.as_ref())
        } else {
            super::default_mount_impl(self, mountpoint, mount_type).await
        }
    }
    async fn source_hash(&self) -> Result<digest::Output<Sha256>, Error> {
        let mut sha = Sha256::new();
        sha.update("Bind");
        if self.recursive {
            sha.update(b"\0recursive");
        }
        sha.update(
            canonicalize(self.src.as_ref(), false)
                .await?
                .as_os_str()
                .as_bytes(),
        );
        Ok(sha.finalize())
    }
    /// Recursive read-only views never share a writable cache attachment.
    async fn mount_hash(&self, mount_type: MountType) -> Result<digest::Output<Sha256>, Error> {
        let source = self.source_hash().await?;
        if self.recursive {
            let mut sha = Sha256::new();
            sha.update(source);
            sha.update([u8::from(mount_type == MountType::ReadOnly)]);
            Ok(sha.finalize())
        } else {
            Ok(source)
        }
    }
}
