use std::path::{Path, PathBuf};

use super::boot::MountedGeneration;
use crate::disk::mount::filesystem::bind::Bind;
use crate::disk::mount::filesystem::overlayfs::OverlayFs;
use crate::disk::mount::filesystem::{ReadOnly, ReadWrite};
use crate::disk::mount::guard::MountGuard;
use crate::prelude::*;

/// Running-root writes and working configuration edits have disjoint RAM uppers.
pub(super) struct RootComposition {
    layers: VerifiedMounts,
    working_upper: PathBuf,
}

/// Verified lower mappings outlive every dependent mount.
pub(super) struct VerifiedMounts {
    mounts: Vec<MountGuard>,
    lower: Option<MountedGeneration>,
}

impl VerifiedMounts {
    pub fn new(lower: MountedGeneration) -> Self {
        Self {
            mounts: Vec::new(),
            lower: Some(lower),
        }
    }

    pub fn lower(&self) -> &MountedGeneration {
        self.lower.as_ref().unwrap()
    }

    pub fn push(&mut self, mount: MountGuard) {
        self.mounts.push(mount);
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
            layers: VerifiedMounts::new(lower),
            working_upper: runtime.join("working-upper"),
        };
        let result = async {
            let lower = composition.layers.lower.as_ref().unwrap();
            composition.layers.mounts.push(
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
                    .layers
                    .mounts
                    .push(MountGuard::mount(&Bind::new(source), target, mode).await?);
            }
            composition.layers.mounts.push(
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
        self.layers.lower()
    }

    pub fn working_upper(&self) -> &Path {
        &self.working_upper
    }

    pub fn persist(self) {
        self.layers.persist();
    }

    pub async fn unmount(self) -> Result<(), Error> {
        self.layers.unmount().await
    }
}

impl Drop for VerifiedMounts {
    fn drop(&mut self) {
        if let Some(lower) = self.lower.take() {
            let mounts = std::mem::take(&mut self.mounts);
            tokio::spawn(async move {
                teardown(mounts, lower).await.log_err();
            });
        }
    }
}
