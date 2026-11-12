use std::path::{Path, PathBuf};

use super::boot::MountedGeneration;
use super::root::VerifiedMounts;
use crate::disk::mount::filesystem::ReadOnly;
use crate::disk::mount::filesystem::loop_dev::LoopDev;
use crate::disk::mount::filesystem::overlayfs::OverlayFs;
use crate::disk::mount::guard::MountGuard;
use crate::prelude::*;

/// Read-only public providers with the packaged build runtime as fallback.
/// Verified mappings outlive the runtime image and composed source mounts.
pub struct BuildSource {
    layers: VerifiedMounts,
    path: PathBuf,
}

async fn runtime_image(public: &Path) -> Result<PathBuf, Error> {
    let public = crate::util::io::canonicalize(public, true).await?;
    let image = crate::util::io::canonicalize(
        public.join("usr/lib/startos/container-runtime/rootfs.squashfs"),
        true,
    )
    .await?;
    if !image.starts_with(&public) {
        return Err(Error::new(
            eyre!("{}", t!("fde.boot.invalid-descriptor")),
            ErrorKind::InvalidSignature,
        ));
    }
    Ok(image)
}

impl BuildSource {
    pub async fn mount(
        verified: MountedGeneration,
        runtime: &Path,
        root: &Path,
    ) -> Result<Self, Error> {
        let mut layers = VerifiedMounts::new(verified);
        let result = async {
            let public = layers.lower().rootfs().to_owned();
            layers.push(
                MountGuard::mount(
                    &LoopDev::new(runtime_image(&public).await?, 0, 0),
                    runtime,
                    ReadOnly,
                )
                .await?,
            );
            layers.push(
                MountGuard::mount(
                    &OverlayFs::read_only(vec![public, runtime.to_owned()]),
                    root,
                    ReadOnly,
                )
                .await?,
            );
            Ok::<_, Error>(())
        }
        .await;
        if let Err(error) = result {
            layers.unmount().await.log_err();
            return Err(error);
        }
        Ok(Self {
            layers,
            path: root.to_owned(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn unmount(self) -> Result<(), Error> {
        self.layers.unmount().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runtime_image_must_remain_inside_the_verified_public_filesystem() {
        let temp = tempfile::tempdir().unwrap();
        let public = temp.path().join("public");
        let image = public.join("usr/lib/startos/container-runtime/rootfs.squashfs");
        std::fs::create_dir_all(image.parent().unwrap()).unwrap();
        std::fs::write(&image, b"image data").unwrap();
        assert_eq!(
            runtime_image(&public).await.unwrap(),
            std::fs::canonicalize(&image).unwrap()
        );
        let outside = temp.path().join("outside");
        std::fs::write(&outside, b"not an admitted provider").unwrap();
        std::fs::remove_file(&image).unwrap();
        std::os::unix::fs::symlink(&outside, &image).unwrap();
        assert!(runtime_image(&public).await.is_err());
        std::fs::remove_file(&image).unwrap();
        std::fs::remove_dir(image.parent().unwrap()).unwrap();
        std::fs::create_dir(temp.path().join("container-runtime")).unwrap();
        std::fs::write(
            temp.path().join("container-runtime/rootfs.squashfs"),
            b"outside data",
        )
        .unwrap();
        std::os::unix::fs::symlink(
            temp.path().join("container-runtime"),
            image.parent().unwrap(),
        )
        .unwrap();
        assert!(runtime_image(&public).await.is_err());
    }
}
