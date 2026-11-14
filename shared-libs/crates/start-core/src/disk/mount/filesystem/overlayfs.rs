use std::fmt::Display;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use digest::Digest;
use itertools::Itertools;
use sha2::Sha256;

use crate::disk::mount::filesystem::{FileSystem, MountType, ReadWrite};
use crate::disk::mount::guard::{GenericMountGuard, MountGuard};
use crate::disk::mount::util::sync_filesystem;
use crate::prelude::*;
use crate::util::io::TmpDir;

pub struct OverlayFs<P0: AsRef<Path>, P1: AsRef<Path>, P2: AsRef<Path>> {
    lower: Vec<P0>,
    writable: Option<(P1, P2)>,
    options: Vec<String>,
}
impl<P0: AsRef<Path>, P1: AsRef<Path>, P2: AsRef<Path>> OverlayFs<P0, P1, P2> {
    /// Lower layers have descending priority.
    pub fn new(lower: Vec<P0>, upper: P1, work: P2) -> Self {
        Self {
            lower,
            writable: Some((upper, work)),
            options: Vec::new(),
        }
    }

    pub fn with_options(mut self, options: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.options = options.into_iter().map(Into::into).collect();
        self
    }
}
impl<P0: AsRef<Path>> OverlayFs<P0, PathBuf, PathBuf> {
    /// Lower contents remain stable only while their owners exclude writes.
    pub fn read_only(lower: Vec<P0>) -> Self {
        Self {
            lower,
            writable: None,
            options: Vec::new(),
        }
    }
}
impl<P0: AsRef<Path> + Send + Sync, P1: AsRef<Path> + Send + Sync, P2: AsRef<Path> + Send + Sync>
    FileSystem for OverlayFs<P0, P1, P2>
{
    fn mount_type(&self) -> Option<impl AsRef<str>> {
        Some("overlay")
    }
    async fn source(&self) -> Result<Option<impl AsRef<Path>>, Error> {
        Ok(Some("overlay"))
    }
    fn mount_options(&self) -> impl IntoIterator<Item = impl Display> {
        let mut options: Vec<Box<dyn Display + '_>> = vec![Box::new(lazy_format!(
            "lowerdir={}",
            self.lower.iter().map(|p| p.as_ref().display()).join(":")
        ))];
        if let Some((upper, work)) = &self.writable {
            options.push(Box::new(lazy_format!(
                "upperdir={}",
                upper.as_ref().display()
            )));
            options.push(Box::new(lazy_format!(
                "workdir={}",
                work.as_ref().display()
            )));
        }
        options.into_iter().chain(
            self.options
                .iter()
                .map(|option| Box::new(option) as Box<dyn Display>),
        )
    }
    async fn pre_mount(&self, mountpoint: &Path, _: MountType) -> Result<(), Error> {
        if let Some((upper, work)) = &self.writable {
            tokio::fs::create_dir_all(upper.as_ref()).await?;
            tokio::fs::create_dir_all(work.as_ref()).await?;
        }
        tokio::fs::create_dir_all(mountpoint).await?;
        Ok(())
    }
    async fn source_hash(&self) -> Result<digest::Output<Sha256>, Error> {
        if let Some((upper, work)) = &self.writable {
            tokio::fs::create_dir_all(upper.as_ref()).await?;
            tokio::fs::create_dir_all(work.as_ref()).await?;
        }
        let mut sha = Sha256::new();
        sha.update("OverlayFs");
        if self.writable.is_none() {
            sha.update("ReadOnly");
        }
        for lower in &self.lower {
            sha.update(
                tokio::fs::canonicalize(lower.as_ref())
                    .await
                    .with_ctx(|_| {
                        (
                            crate::ErrorKind::Filesystem,
                            lower.as_ref().display().to_string(),
                        )
                    })?
                    .as_os_str()
                    .as_bytes(),
            );
            sha.update(b"\0");
        }
        if let Some((upper, work)) = &self.writable {
            for path in [upper.as_ref(), work.as_ref()] {
                sha.update(
                    tokio::fs::canonicalize(path)
                        .await
                        .with_ctx(|_| (crate::ErrorKind::Filesystem, path.display().to_string()))?
                        .as_os_str()
                        .as_bytes(),
                );
                sha.update(b"\0");
            }
        }
        for option in &self.options {
            sha.update(option.as_bytes());
            sha.update(b"\0");
        }
        Ok(sha.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn read_only_layers_have_no_writable_storage_or_identity_alias() {
        let temp = tempfile::tempdir().unwrap();
        let upper = temp.path().join("upper");
        let work = temp.path().join("work");
        let writable = OverlayFs::new(vec![temp.path()], &upper, &work);
        writable.source_hash().await.unwrap();
        let read_only = OverlayFs::read_only(vec![temp.path(), &upper, &work]);
        let options: Vec<_> = read_only
            .mount_options()
            .into_iter()
            .map(|p| p.to_string())
            .collect();
        assert_eq!(options.len(), 1);
        assert!(options[0].starts_with("lowerdir="));
        assert_ne!(
            read_only.source_hash().await.unwrap(),
            writable.source_hash().await.unwrap()
        );
        tokio::fs::remove_dir(&upper).await.unwrap();
        tokio::fs::remove_dir(&work).await.unwrap();
        let source = OverlayFs::read_only(vec![temp.path()]);
        source.source_hash().await.unwrap();
        source
            .pre_mount(
                &temp.path().join("mount"),
                crate::disk::mount::filesystem::ReadOnly,
            )
            .await
            .unwrap();
        assert!(!upper.exists());
        assert!(!work.exists());
    }

    #[tokio::test]
    async fn mount_identity_includes_extra_options_without_changing_default_options() {
        let temp = tempfile::tempdir().unwrap();
        let normal = OverlayFs::new(
            vec![temp.path()],
            temp.path().join("upper"),
            temp.path().join("work"),
        );
        let indexed = OverlayFs::new(
            vec![temp.path()],
            temp.path().join("upper"),
            temp.path().join("work"),
        )
        .with_options(["index=off"]);
        let metadata = OverlayFs::new(
            vec![temp.path()],
            temp.path().join("upper"),
            temp.path().join("work"),
        )
        .with_options(["metacopy=off"]);
        assert_eq!(normal.mount_options().into_iter().count(), 3);
        assert!(
            indexed
                .mount_options()
                .into_iter()
                .any(|option| option.to_string() == "index=off")
        );
        assert_ne!(
            normal.source_hash().await.unwrap(),
            indexed.source_hash().await.unwrap()
        );
        assert_ne!(
            indexed.source_hash().await.unwrap(),
            metadata.source_hash().await.unwrap()
        );
    }
}

#[derive(Debug)]
pub struct OverlayGuard<G: GenericMountGuard> {
    lower: Option<G>,
    upper: Option<TmpDir>,
    inner_guard: MountGuard,
}
impl<G: GenericMountGuard> OverlayGuard<G> {
    pub async fn mount_layers<P: AsRef<Path>>(
        pre: &[P],
        guard: G,
        post: &[P],
        mountpoint: impl AsRef<Path>,
    ) -> Result<Self, Error> {
        let upper = TmpDir::new().await?;
        let inner_guard = MountGuard::mount(
            &OverlayFs::new(
                std::iter::empty()
                    .chain(pre.into_iter().map(|p| p.as_ref()))
                    .chain([guard.path()])
                    .chain(post.into_iter().map(|p| p.as_ref()))
                    .collect(),
                upper.as_ref().join("upper"),
                upper.as_ref().join("work"),
            ),
            mountpoint,
            ReadWrite,
        )
        .await?;
        Ok(Self {
            lower: Some(guard),
            upper: Some(upper),
            inner_guard,
        })
    }
    pub async fn mount(lower: G, mountpoint: impl AsRef<Path>) -> Result<Self, Error> {
        Self::mount_layers::<&Path>(&[], lower, &[], mountpoint).await
    }
    pub async fn unmount(mut self, delete_mountpoint: bool) -> Result<(), Error> {
        sync_filesystem(self.inner_guard.path()).await?;
        self.inner_guard.take().unmount(delete_mountpoint).await?;
        if let Some(lower) = self.lower.take() {
            lower.unmount().await?;
        }
        if let Some(upper) = self.upper.take() {
            upper.delete().await?;
        }
        Ok(())
    }
    pub fn take(&mut self) -> Self {
        Self {
            lower: self.lower.take(),
            upper: self.upper.take(),
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
        let lower = self.lower.take();
        let upper = self.upper.take();
        let guard = self.inner_guard.take();
        if lower.is_some() || upper.is_some() || guard.is_mounted() {
            tokio::spawn(async move {
                if guard.is_mounted() {
                    sync_filesystem(guard.path()).await.log_err();
                }
                guard.unmount(false).await.log_err();
                if let Some(lower) = lower {
                    lower.unmount().await.log_err();
                }
                if let Some(upper) = upper {
                    upper.delete().await.log_err();
                }
            });
        }
    }
}
