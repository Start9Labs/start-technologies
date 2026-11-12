use std::path::{Path, PathBuf};

use super::boot::MountedGeneration;
use crate::disk::mount::filesystem::bind::Bind;
use crate::disk::mount::filesystem::overlayfs::OverlayFs;
use crate::disk::mount::filesystem::{ReadOnly, ReadWrite};
use crate::disk::mount::guard::MountGuard;
use crate::prelude::*;

/// Running-root writes and working configuration edits have disjoint RAM uppers.
/// Verified lower mappings outlive every dependent mount.
pub(super) struct RootComposition {
    mounts: Vec<MountGuard>,
    lower: Option<MountedGeneration>,
    working_upper: PathBuf,
}

async fn teardown(mut mounts: Vec<MountGuard>, lower: MountedGeneration) -> Result<(), Error> {
    let mut result = Ok(());
    while let Some(mount) = mounts.pop() {
        if let Err(error) = mount.unmount(false).await {
            if result.is_ok() {
                result = Err(error);
            } else {
                Err::<(), _>(error).log_err();
            }
        }
    }
    let lower_result = lower.unmount().await;
    result?;
    lower_result
}

impl RootComposition {
    pub async fn mount(
        lower: MountedGeneration,
        volume: &Path,
        images: &Path,
        config: &Path,
        root: &Path,
        runtime: &Path,
    ) -> Result<Self, Error> {
        let mut composition = Self {
            mounts: Vec::new(),
            lower: Some(lower),
            working_upper: runtime.join("working-upper"),
        };
        let result = async {
            let lower = composition.lower.as_ref().unwrap();
            composition.mounts.push(
                MountGuard::mount(
                    &OverlayFs::new(
                        vec![lower.overlay(), lower.rootfs()],
                        runtime.join("running-upper"),
                        runtime.join("running-work"),
                    ),
                    root,
                    ReadWrite,
                )
                .await?,
            );
            for (source, target, mode) in [
                (
                    config.to_owned(),
                    root.join("media/startos/config"),
                    ReadWrite,
                ),
                (
                    images.to_owned(),
                    root.join("media/startos/images"),
                    ReadWrite,
                ),
                (volume.to_owned(), root.join("media/startos/root"), ReadOnly),
                (
                    lower.rootfs().to_owned(),
                    root.join("media/startos/current"),
                    ReadOnly,
                ),
            ] {
                composition
                    .mounts
                    .push(MountGuard::mount(&Bind::new(source), target, mode).await?);
            }
            composition.mounts.push(
                MountGuard::mount(
                    &crate::os_overlay::working_filesystem(
                        lower.overlay(),
                        &composition.working_upper,
                        &runtime.join("working-work"),
                    ),
                    root.join("media/startos/config/overlay"),
                    ReadWrite,
                )
                .await?,
            );
            Ok::<_, Error>(())
        }
        .await;
        match result {
            Ok(()) => Ok(composition),
            Err(error) => {
                composition.unmount().await.log_err();
                Err(error)
            }
        }
    }

    pub fn verified(&self) -> &MountedGeneration {
        self.lower.as_ref().unwrap()
    }

    pub fn working_upper(&self) -> &Path {
        &self.working_upper
    }

    pub fn persist(mut self) {
        for mount in std::mem::take(&mut self.mounts) {
            mount.persist();
        }
        self.lower.take().unwrap().persist();
    }

    pub async fn unmount(mut self) -> Result<(), Error> {
        teardown(std::mem::take(&mut self.mounts), self.lower.take().unwrap()).await
    }
}

impl Drop for RootComposition {
    fn drop(&mut self) {
        if let Some(lower) = self.lower.take() {
            let mounts = std::mem::take(&mut self.mounts);
            tokio::spawn(async move {
                teardown(mounts, lower).await.log_err();
            });
        }
    }
}
