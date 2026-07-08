use std::ffi::OsStr;
use std::fmt::{Display, Write};
use std::path::Path;

use futures::Future;
use sha2::Sha256;
use tokio::process::Command;

use crate::prelude::*;
use crate::util::Invoke;

pub mod backupfs;
pub mod bind;
pub mod block_dev;
pub mod cifs;
pub mod ecryptfs;
pub mod efivarfs;
pub mod httpdirfs;
pub mod idmapped;
pub mod label;
pub mod loop_dev;
pub mod overlayfs;
#[cfg(target_os = "linux")]
pub mod syscall;
#[cfg(not(target_os = "linux"))]
#[path = "syscall_stub.rs"]
pub mod syscall;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MountType {
    ReadOnly,
    ReadWrite,
    BackupWrite,
}

pub use MountType::*;

pub(self) async fn default_mount_command(
    fs: &(impl FileSystem + ?Sized),
    mountpoint: impl AsRef<Path> + Send,
    mount_type: MountType,
) -> Result<std::process::Command, Error> {
    let mut cmd = std::process::Command::new("mount");
    if mount_type == ReadOnly {
        cmd.arg("-r");
    }
    cmd.args(fs.extra_args());
    if let Some(ty) = fs.mount_type() {
        cmd.arg("-t").arg(ty.as_ref());
    }
    if let Some(options) = fs
        .mount_options()
        .into_iter()
        .fold(None, |acc: Option<String>, x| match acc {
            Some(mut s) => {
                write!(s, ",{}", x).unwrap();
                Some(s)
            }
            None => Some(x.to_string()),
        })
    {
        cmd.arg("-o").arg(options);
    }
    if let Some(source) = fs.source().await? {
        cmd.arg(source.as_ref());
    }
    cmd.arg(mountpoint.as_ref());
    Ok(cmd)
}

pub(self) async fn default_mount_impl(
    fs: &(impl FileSystem + ?Sized),
    mountpoint: impl AsRef<Path> + Send,
    mount_type: MountType,
) -> Result<(), Error> {
    fs.pre_mount(mountpoint.as_ref(), mount_type).await?;
    Command::from(default_mount_command(fs, mountpoint, mount_type).await?)
        .capture(false)
        .invoke(ErrorKind::Filesystem)
        .await?;

    Ok(())
}

pub trait FileSystem: Send + Sync {
    fn mount_type(&self) -> Option<impl AsRef<str>> {
        None::<&str>
    }
    fn extra_args(&self) -> impl IntoIterator<Item = impl AsRef<OsStr>> {
        [] as [&str; 0]
    }
    fn mount_options(&self) -> impl IntoIterator<Item = impl Display> {
        [] as [&str; 0]
    }
    fn source(&self) -> impl Future<Output = Result<Option<impl AsRef<Path>>, Error>> + Send {
        async { Ok(None::<&Path>) }
    }
    fn pre_mount(
        &self,
        mountpoint: &Path,
        #[allow(unused_variables)] mount_type: MountType,
    ) -> impl Future<Output = Result<(), Error>> + Send {
        async move {
            tokio::fs::create_dir_all(mountpoint).await?;
            Ok(())
        }
    }
    fn mount<P: AsRef<Path> + Send>(
        &self,
        mountpoint: P,
        mount_type: MountType,
    ) -> impl Future<Output = Result<(), Error>> + Send {
        default_mount_impl(self, mountpoint, mount_type)
    }
    fn source_hash(&self) -> impl Future<Output = Result<digest::Output<Sha256>, Error>> + Send;
}
