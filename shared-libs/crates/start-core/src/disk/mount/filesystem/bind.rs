use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use digest::Digest;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use ts_rs::TS;

use super::FileSystem;
use crate::disk::mount::filesystem::MountType;
use crate::prelude::*;
use crate::util::io::{canonicalize, create_file};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum FileType {
    File,
    Directory,
    Infer,
}
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
        let to_meta = tokio::fs::metadata(&mountpoint).await.ok();
        if matches!(self.filetype, FileType::File)
            || (matches!(self.filetype, FileType::Infer)
                && from_meta.as_ref().map_or(false, |m| m.is_file()))
        {
            if to_meta.as_ref().map_or(false, |m| m.is_dir()) {
                tokio::fs::remove_dir(mountpoint).await?;
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
    async fn source_hash(&self) -> Result<digest::Output<Sha256>, Error> {
        let mut sha = Sha256::new();
        sha.update("Bind");
        // tolerant of a not-yet-created source dir; pre_mount creates the leaf
        sha.update(
            canonicalize(self.src.as_ref(), false)
                .await?
                .as_os_str()
                .as_bytes(),
        );
        Ok(sha.finalize())
    }
}
