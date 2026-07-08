use std::os::fd::AsFd;
use std::path::Path;
use std::str::FromStr;

use clap::Parser;
use clap::builder::ValueParserFactory;
use digest::Digest;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use ts_rs::TS;

use super::FileSystem;
use crate::disk::mount::filesystem::MountType;
use crate::disk::mount::filesystem::syscall::{
    DetachedMount, open_tree_attr_idmap, userns_fd_from_idmap,
};
use crate::disk::mount::guard::{GenericMountGuard, TmpMountGuard};
use crate::prelude::*;
use crate::util::FromStrParser;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct IdMap {
    pub from_id: u32,
    pub to_id: u32,
    pub range: u32,
}
impl FromStr for IdMap {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let split = s.splitn(3, ":").collect::<Vec<_>>();
        if let Some([u, k, r]) = split.get(0..3) {
            Ok(Self {
                from_id: u.parse()?,
                to_id: k.parse()?,
                range: r.parse()?,
            })
        } else if let Some([u, k]) = split.get(0..2) {
            Ok(Self {
                from_id: u.parse()?,
                to_id: k.parse()?,
                range: 1,
            })
        } else {
            Err(Error::new(
                eyre!("{s} is not a valid idmap"),
                ErrorKind::ParseNumber,
            ))
        }
    }
}
impl ValueParserFactory for IdMap {
    type Parser = FromStrParser<IdMap>;
    fn value_parser() -> Self::Parser {
        <Self::Parser>::new()
    }
}

/// Wraps another filesystem and applies an idmap (uid/gid remapping) to the
/// resulting mount via `mount_setattr(MOUNT_ATTR_IDMAP, userns_fd)`.
///
/// The inner filesystem is mounted on a tmp staging path (deduplicated by
/// `TmpMountGuard`), cloned into a detached fd via
/// `open_tree(OPEN_TREE_CLONE | AT_RECURSIVE)`, idmapped, and finally
/// `move_mount`-ed onto the requested mountpoint. The staging mount is
/// released when this method returns; the detached clone holds independent
/// mount state.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdMapped<Fs: FileSystem> {
    filesystem: Fs,
    idmap: Vec<IdMap>,
}
impl<Fs: FileSystem> IdMapped<Fs> {
    pub fn new(filesystem: Fs, idmap: Vec<IdMap>) -> Self {
        Self { filesystem, idmap }
    }
}
impl<Fs: FileSystem> FileSystem for IdMapped<Fs> {
    async fn mount<P: AsRef<Path> + Send>(
        &self,
        mountpoint: P,
        mount_type: MountType,
    ) -> Result<(), Error> {
        let mp = mountpoint.as_ref();

        if self.idmap.is_empty() {
            return self.filesystem.mount(mp, mount_type).await;
        }

        tokio::fs::create_dir_all(mp).await?;

        let staging = TmpMountGuard::mount(&self.filesystem, mount_type).await?;

        // open_tree_attr atomically clones the staging mount and applies
        // the idmap — required so the kernel doesn't see (and reject) a
        // window where the cloned mount exists without the idmap, which
        // is the recursive-idmap failure mode for inners that are
        // themselves idmapped.
        let userns = userns_fd_from_idmap(&self.idmap).await?;
        let fd = open_tree_attr_idmap(staging.path(), true, userns.as_fd())?;
        drop(userns);
        let detached = DetachedMount::from_fd(fd);

        if matches!(mount_type, MountType::ReadOnly) {
            detached.set_readonly(true)?;
        }

        detached.attach(mp)?;

        // The detached clone we just attached is independent of `staging`,
        // so unmounting the staging copy here doesn't disturb it. Unmount
        // explicitly (rather than relying on Drop's fire-and-forget spawn)
        // so the staging mount is gone before we return and any failure is
        // surfaced.
        staging.unmount().await?;

        Ok(())
    }
    async fn source_hash(&self) -> Result<digest::Output<Sha256>, Error> {
        let mut sha = Sha256::new();
        sha.update("IdMapped");
        sha.update(self.filesystem.source_hash().await?);
        sha.update(usize::to_be_bytes(self.idmap.len()));
        for i in &self.idmap {
            sha.update(u32::to_be_bytes(i.from_id));
            sha.update(u32::to_be_bytes(i.to_id));
            sha.update(u32::to_be_bytes(i.range));
        }
        Ok(sha.finalize())
    }
}
