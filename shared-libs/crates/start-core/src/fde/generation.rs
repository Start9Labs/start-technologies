//! Authenticated enable/commit authorizes the whole captured prospective configuration,
//! including executable, library, NSS and OpenSSL-provider overrides. A later capture
//! requires fresh authorization. Snapshot integrity alone confers no source authority.

#[cfg(test)]
#[path = "generation_tests.rs"]
mod tests;

use std::path::{Path, PathBuf};

use super::root::VerifiedMounts;
use crate::disk::mount::filesystem::bind::Bind;
use crate::disk::mount::filesystem::loop_dev::LoopDev;
use crate::disk::mount::filesystem::overlayfs::OverlayFs;
use crate::disk::mount::filesystem::private_tmpfs::PrivateTmpfs;
use crate::disk::mount::filesystem::{ReadOnly, ReadWrite};
use crate::disk::mount::guard::{GenericMountGuard, MountGuard};
use crate::prelude::*;
use crate::s9pk::merkle_archive::source::DynFileSource;
use crate::util::future::complete_owned_operation;
use crate::util::io::{TmpDir, create_file_mod};
use crate::util::squashfs::Squashfs;

#[derive(Debug)]
struct ConfigurationRoots<P: GenericMountGuard, D: GenericMountGuard> {
    base: P,
    data: D,
    workspace: PathBuf,
    private_backing_uses: Vec<crate::util::FileLock>,
}

impl<P: GenericMountGuard, D: GenericMountGuard> GenericMountGuard for ConfigurationRoots<P, D> {
    fn path(&self) -> &Path {
        self.data.path()
    }

    async fn unmount(self) -> Result<(), Error> {
        let base = self.base.unmount().await;
        drop(self.private_backing_uses);
        let data = self.data.unmount().await;
        // Mount failures can leave live data beneath the workspace.
        let workspace = tokio::fs::remove_dir(self.workspace)
            .await
            .with_kind(ErrorKind::Filesystem);
        base?;
        data?;
        workspace
    }
}

async fn query(root: &Path, configuration: &Path) -> Result<Vec<PathBuf>, Error> {
    let query = crate::ssh::host_key_command(configuration);
    let mut command = super::namespace::command(root);
    command
        .arg("--")
        .arg(query.as_std().get_program())
        .args(query.as_std().get_args());
    command.envs(
        query
            .as_std()
            .get_envs()
            .filter_map(|(name, value)| value.map(|value| (name, value))),
    );
    crate::ssh::query_host_key_files(&mut command).await
}

/// Private immutable inputs; borrowed file sources require these mounts to remain alive.
/// Whole-ancestor backing reservations follow the owned query views through cleanup.
/// Capture completion releases no source reservations owned by the caller.
pub(crate) struct CapturedConfiguration<P: GenericMountGuard, D: GenericMountGuard> {
    layers: VerifiedMounts<ConfigurationRoots<P, D>>,
    image: PathBuf,
    keys: Vec<PathBuf>,
}

impl<P: GenericMountGuard, D: GenericMountGuard> CapturedConfiguration<P, D> {
    pub(crate) fn image(&self) -> &Path {
        &self.image
    }

    pub(crate) fn host_keys(&self) -> &[PathBuf] {
        &self.keys
    }

    pub(crate) async fn unmount(self) -> Result<(), Error> {
        self.layers.unmount().await
    }
}

/// Requires fresh authorization and a trusted running base; read-only mounting grants no authority.
/// The caller excludes configuration, base-image and native-query input writers until return.
/// Consumes a disposable unlocked data view, never the live system mount guard.
/// Performs no writes to data or the generation store.
pub(crate) async fn capture_configuration<P: GenericMountGuard, D: GenericMountGuard>(
    snapshot: Squashfs<DynFileSource>,
    base: P,
    data: D,
) -> Result<CapturedConfiguration<P, D>, Error> {
    complete_owned_operation(async move {
        let memory = super::memory::ProcessSecretMemory::protect()?;
        let private_backing_uses = super::private_config::reserve_visible_backings(
            &data.path().join(crate::MAIN_FILESYSTEM),
        )
        .await?;
        let workspace = TmpDir::new().await?;
        let directory = workspace.to_path_buf();
        workspace.leak();
        let mut layers = VerifiedMounts::new(ConfigurationRoots {
            base,
            data,
            workspace: directory.clone(),
            private_backing_uses,
        });
        let captured = directory.join("configuration.squashfs");
        let configuration = directory.join("configuration");
        let result = async {
            layers.push(MountGuard::mount(&PrivateTmpfs::new(), &directory, ReadWrite).await?);
            let mut image = create_file_mod(&captured, 0o600).await?;
            snapshot.serialize(&mut image).await?;
            image.sync_all().await?;
            drop(image);
            layers.push(
                MountGuard::mount(&LoopDev::new(&captured, 0, 0), &configuration, ReadOnly).await?,
            );
            let root = directory.join("root");
            layers.push(
                MountGuard::mount(
                    &OverlayFs::new(
                        vec![configuration.clone(), layers.lower().base.path().to_owned()],
                        directory.join("upper"),
                        directory.join("work"),
                    ),
                    &root,
                    ReadWrite,
                )
                .await?,
            );
            super::namespace::prepare(&mut layers, &root).await?;
            let data_target = root.join(crate::DATA_DIR.strip_prefix('/').unwrap());
            let relative_data = data_target.strip_prefix(&root).unwrap();
            crate::util::io::create_directory_beneath(&root, relative_data)?;
            super::namespace::mountpoint(&root, relative_data).await?;
            layers.push(
                MountGuard::mount(
                    &Bind::new(layers.lower().data.path()).recursive(true),
                    &data_target,
                    ReadOnly,
                )
                .await?,
            );
            let private = layers
                .lower()
                .data
                .path()
                .join(crate::MAIN_FILESYSTEM)
                .join(super::private_config::DIRECTORY);
            if tokio::fs::try_exists(&private).await? {
                let runtime = Path::new(super::private_config::RUNTIME)
                    .strip_prefix("/")
                    .unwrap();
                crate::util::io::create_directory_beneath(&root, runtime.parent().unwrap())?;
                layers.push(
                    MountGuard::mount(&Bind::new(&private), root.join(runtime), ReadOnly).await?,
                );
            }
            let mut keys = crate::ssh::host_key_files(Path::new("/etc/ssh/sshd_config")).await?;
            keys.extend(query(&root, Path::new("none")).await?);
            keys.extend(query(&root, Path::new("/etc/ssh/sshd_config")).await?);
            keys.sort();
            keys.dedup();
            memory.check()?;
            Ok(keys)
        }
        .await;
        match result {
            Ok(keys) => Ok(CapturedConfiguration {
                layers,
                image: captured,
                keys,
            }),
            Err(error) => {
                layers.unmount().await.log_err();
                Err(error)
            }
        }
    })
    .await
}
