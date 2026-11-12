use std::ffi::{CStr, CString};
use std::fmt::Display;
use std::fs::File;
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use digest::Digest;
use itertools::Itertools;
use sha2::Sha256;

use crate::disk::mount::filesystem::syscall::{AttachedMount, DetachedMount};
use crate::disk::mount::filesystem::{FileSystem, MountType, ReadWrite};
use crate::disk::mount::guard::{GenericMountGuard, MountGuard};
use crate::disk::mount::util::{sync_filesystem, sync_filesystem_at};
use crate::prelude::*;
use crate::util::directory::create_directory_at;
use crate::util::io::TmpDir;

#[cfg(all(test, target_os = "linux"))]
#[path = "overlayfs_tests.rs"]
mod tests;

const OVERLAY_FILESYSTEM: &CStr = c"overlay";

pub struct OverlayFs<P0: AsRef<Path>, P1: AsRef<Path>, P2: AsRef<Path>> {
    lower: Vec<P0>,
    upper: P1,
    work: P2,
}
impl<P0: AsRef<Path>, P1: AsRef<Path>, P2: AsRef<Path>> OverlayFs<P0, P1, P2> {
    /// Layers are top to bottom.
    pub fn new(lower: Vec<P0>, upper: P1, work: P2) -> Self {
        Self { lower, upper, work }
    }

    fn options(&self) -> [(&'static CStr, String); 3] {
        [
            (
                c"lowerdir",
                self.lower
                    .iter()
                    .map(|path| path.as_ref().display())
                    .join(":"),
            ),
            (c"upperdir", self.upper.as_ref().display().to_string()),
            (c"workdir", self.work.as_ref().display().to_string()),
        ]
    }

    async fn prepare_directories(&self) -> Result<(), Error> {
        tokio::fs::create_dir_all(self.upper.as_ref()).await?;
        tokio::fs::create_dir_all(self.work.as_ref()).await?;
        Ok(())
    }
}
impl<P0: AsRef<Path> + Send + Sync, P1: AsRef<Path> + Send + Sync, P2: AsRef<Path> + Send + Sync>
    FileSystem for OverlayFs<P0, P1, P2>
{
    fn mount_type(&self) -> Option<impl AsRef<str>> {
        Some(OVERLAY_FILESYSTEM.to_str().unwrap())
    }
    async fn source(&self) -> Result<Option<impl AsRef<Path>>, Error> {
        Ok(Some(OVERLAY_FILESYSTEM.to_str().unwrap()))
    }
    fn mount_options(&self) -> impl IntoIterator<Item = impl Display> {
        self.options()
            .map(|(key, value)| format!("{}={value}", key.to_str().unwrap()))
    }
    async fn pre_mount(&self, mountpoint: &Path, _: MountType) -> Result<(), Error> {
        self.prepare_directories().await?;
        tokio::fs::create_dir_all(mountpoint).await?;
        Ok(())
    }
    async fn source_hash(&self) -> Result<digest::Output<Sha256>, Error> {
        self.prepare_directories().await?;
        let mut sha = Sha256::new();
        sha.update("OverlayFs");
        for lower in &self.lower {
            sha.update(
                tokio::fs::canonicalize(lower.as_ref())
                    .await
                    .with_ctx(|_| (ErrorKind::Filesystem, lower.as_ref().display().to_string()))?
                    .as_os_str()
                    .as_bytes(),
            );
            sha.update(b"\0");
        }
        for path in [self.upper.as_ref(), self.work.as_ref()] {
            sha.update(
                tokio::fs::canonicalize(path)
                    .await
                    .with_ctx(|_| (ErrorKind::Filesystem, path.display().to_string()))?
                    .as_os_str()
                    .as_bytes(),
            );
            sha.update(b"\0");
        }
        Ok(sha.finalize())
    }
}

#[derive(Debug)]
struct OverlayBacking<G: GenericMountGuard> {
    lower: Option<G>,
    upper: Option<TmpDir>,
}
impl<G: GenericMountGuard> OverlayBacking<G> {
    async fn new(lower: G) -> Result<Self, Error> {
        Ok(Self {
            lower: Some(lower),
            upper: Some(TmpDir::new().await?),
        })
    }

    fn filesystem<'a, P: AsRef<Path>>(
        &'a self,
        pre: &'a [P],
        post: &'a [P],
    ) -> OverlayFs<&'a Path, PathBuf, PathBuf> {
        let upper = self.upper.as_ref().unwrap();
        OverlayFs::new(
            pre.iter()
                .map(|path| path.as_ref())
                .chain([self.lower.as_ref().unwrap().path()])
                .chain(post.iter().map(|path| path.as_ref()))
                .collect(),
            upper.join("upper"),
            upper.join("work"),
        )
    }

    fn take(&mut self) -> Self {
        Self {
            lower: self.lower.take(),
            upper: self.upper.take(),
        }
    }

    fn active(&self) -> bool {
        self.lower.is_some() || self.upper.is_some()
    }

    async fn unmount(mut self) -> Result<(), Error> {
        if let Some(lower) = self.lower.take() {
            lower.unmount().await?;
        }
        if let Some(upper) = self.upper.take() {
            upper.delete().await?;
        }
        Ok(())
    }
}

/// The view detaches before backing storage is released, including on drop.
#[derive(Debug)]
pub struct AnchoredOverlayGuard<G: GenericMountGuard> {
    view: Option<AttachedMount>,
    backing: OverlayBacking<G>,
    target: Option<OverlayTarget>,
}
impl<G: GenericMountGuard> GenericMountGuard for AnchoredOverlayGuard<G> {
    fn path(&self) -> &Path {
        self.view.as_ref().unwrap().path()
    }

    async fn unmount(mut self) -> Result<(), Error> {
        let mut errors = ErrorCollection::new();
        let root = File::open(self.path()).with_kind(ErrorKind::Filesystem);
        let flushed = match root {
            Ok(root) => sync_filesystem_at(&root).await,
            Err(error) => Err(error),
        };
        errors.handle(flushed);
        errors.handle(self.view.take().unwrap().unmount().await);
        errors.handle(self.backing.take().unmount().await);
        errors.handle(
            self.target
                .take()
                .unwrap()
                .remove_entry()
                .with_kind(ErrorKind::Filesystem),
        );
        errors.into_result()
    }
}
impl<G: GenericMountGuard> Drop for AnchoredOverlayGuard<G> {
    fn drop(&mut self) {
        if let Some(view) = self.view.take() {
            let backing = self.backing.take();
            let target = self.target.take();
            tokio::spawn(async move {
                if let Ok(root) = File::open(view.path()) {
                    sync_filesystem_at(&root).await.log_err();
                }
                view.unmount().await.log_err();
                backing.unmount().await.log_err();
                drop(target);
            });
        }
    }
}

#[derive(Debug)]
struct OverlayTarget {
    parent: File,
    name: Option<CString>,
}
impl OverlayTarget {
    fn remove_entry(&mut self) -> std::io::Result<()> {
        let Some(name) = self.name.take() else {
            return Ok(());
        };
        if unsafe { libc::unlinkat(self.parent.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) } < 0
        {
            let error = std::io::Error::last_os_error();
            if !matches!(
                error.kind(),
                std::io::ErrorKind::NotFound
                    | std::io::ErrorKind::DirectoryNotEmpty
                    | std::io::ErrorKind::NotADirectory
            ) {
                return Err(error);
            }
        }
        Ok(())
    }
}
impl Drop for OverlayTarget {
    fn drop(&mut self) {
        if let Err(error) = self.remove_entry() {
            tracing::error!(?error, "Failed to remove overlay directory entry");
        }
    }
}

#[derive(Debug)]
pub struct OverlayGuard<G: GenericMountGuard> {
    backing: OverlayBacking<G>,
    inner_guard: MountGuard,
}
impl<G: GenericMountGuard> OverlayGuard<G> {
    pub async fn mount_layers<P: AsRef<Path>>(
        pre: &[P],
        guard: G,
        post: &[P],
        mountpoint: impl AsRef<Path>,
    ) -> Result<Self, Error> {
        let backing = OverlayBacking::new(guard).await?;
        let inner_guard =
            MountGuard::mount(&backing.filesystem(pre, post), mountpoint, ReadWrite).await?;
        Ok(Self {
            backing,
            inner_guard,
        })
    }

    /// Stages anonymously; the destination name is one entry beneath the pinned parent.
    pub async fn mount_layers_at<P: AsRef<Path>>(
        pre: &[P],
        guard: G,
        post: &[P],
        parent: &File,
        name: &CStr,
    ) -> Result<AnchoredOverlayGuard<G>, Error> {
        let target = OverlayTarget {
            parent: parent.try_clone()?,
            name: Some(name.to_owned()),
        };
        let directory = create_directory_at(&target.parent, name)?.0;
        let backing = OverlayBacking::new(guard).await?;
        let filesystem = backing.filesystem(pre, post);
        filesystem.prepare_directories().await?;
        let options: Vec<_> = filesystem
            .options()
            .into_iter()
            .map(|(key, value)| Ok((key, CString::new(value).with_kind(ErrorKind::Filesystem)?)))
            .collect::<Result<_, Error>>()?;
        let view = DetachedMount::create(
            OVERLAY_FILESYSTEM,
            options.iter().map(|(key, value)| (*key, value.as_c_str())),
        )?
        .attach_at(directory.as_fd())?;
        Ok(AnchoredOverlayGuard {
            view: Some(view),
            backing,
            target: Some(target),
        })
    }

    pub async fn mount(lower: G, mountpoint: impl AsRef<Path>) -> Result<Self, Error> {
        Self::mount_layers::<&Path>(&[], lower, &[], mountpoint).await
    }
    pub async fn unmount(mut self, delete_mountpoint: bool) -> Result<(), Error> {
        sync_filesystem(self.inner_guard.path()).await?;
        self.inner_guard.take().unmount(delete_mountpoint).await?;
        self.backing.take().unmount().await
    }
    pub fn take(&mut self) -> Self {
        Self {
            backing: self.backing.take(),
            inner_guard: self.inner_guard.take(),
        }
    }
}
impl<G: GenericMountGuard> GenericMountGuard for OverlayGuard<G> {
    fn path(&self) -> &Path {
        self.inner_guard.path()
    }
    async fn unmount(self) -> Result<(), Error> {
        self.unmount(false).await
    }
}
impl<G: GenericMountGuard> Drop for OverlayGuard<G> {
    fn drop(&mut self) {
        let backing = self.backing.take();
        let guard = self.inner_guard.take();
        if backing.active() || guard.mounted {
            tokio::spawn(async move {
                if guard.mounted {
                    sync_filesystem(guard.path()).await.log_err();
                }
                guard.unmount(false).await.log_err();
                backing.unmount().await.log_err();
            });
        }
    }
}
