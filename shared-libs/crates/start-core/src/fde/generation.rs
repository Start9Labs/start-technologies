//! Authenticated enable/commit authorizes the whole captured prospective configuration,
//! including executable, library, NSS and OpenSSL-provider overrides. A later capture
//! requires fresh authorization. Snapshot integrity alone confers no source authority.

#[cfg(test)]
#[path = "generation_tests.rs"]
mod tests;

use std::path::{Path, PathBuf};

use openssl::pkey::{PKey, Private};

use super::private_config::{LegacyReplacements, ProjectedConfig};
use super::root::VerifiedMounts;
use crate::disk::mount::filesystem::bind::Bind;
use crate::disk::mount::filesystem::loop_dev::LoopDev;
use crate::disk::mount::filesystem::overlayfs::OverlayFs;
use crate::disk::mount::filesystem::private_tmpfs::PrivateTmpfs;
use crate::disk::mount::filesystem::{ReadOnly, ReadWrite};
use crate::disk::mount::guard::{GenericMountGuard, MountGuard};
use crate::os_overlay::{PreparedGeneration, VerityMount};
use crate::prelude::*;
use crate::s9pk::merkle_archive::source::DynFileSource;
use crate::util::future::complete_owned_operation;
use crate::util::io::{TmpDir, create_file_mod};
use crate::util::squashfs::Squashfs;

#[derive(Debug)]
struct ConfigurationRoots<D: GenericMountGuard> {
    public: VerityMount,
    data: D,
    workspace: PathBuf,
}

impl<D: GenericMountGuard> GenericMountGuard for ConfigurationRoots<D> {
    fn path(&self) -> &Path {
        self.data.path()
    }

    async fn unmount(self) -> Result<(), Error> {
        let public = self.public.unmount().await;
        let data = self.data.unmount().await;
        // Mount failures can leave live data beneath the workspace.
        let workspace = tokio::fs::remove_dir(self.workspace)
            .await
            .with_kind(ErrorKind::Filesystem);
        public?;
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

/// Requires fresh enable/commit authorization for the whole prospective snapshot.
/// The caller quiesces configuration, base-image and query-input writers through completion.
/// Consumes and unmounts a disposable data view, never the live system mount guard.
/// The retained data tree contains the unlocked encrypted main/package filesystems.
/// Returns an unpublished signed configuration and a source-independent legacy plan.
pub(crate) async fn prepare_configuration<D: GenericMountGuard>(
    snapshot: Squashfs<DynFileSource>,
    public: VerityMount,
    data: D,
    store: PathBuf,
    key: PKey<Private>,
) -> Result<(PreparedGeneration, LegacyReplacements), Error> {
    complete_owned_operation(async move {
        let memory = super::memory::ProcessSecretMemory::protect()?;
        let workspace = TmpDir::new().await?;
        let directory = workspace.to_path_buf();
        workspace.leak();
        let mut layers = VerifiedMounts::new(ConfigurationRoots {
            public,
            data,
            workspace: directory.clone(),
        });
        let result = async {
            layers.push(MountGuard::mount(&PrivateTmpfs::new(), &directory, ReadWrite).await?);
            let captured = directory.join("configuration.squashfs");
            let mut image = create_file_mod(&captured, 0o600).await?;
            snapshot.serialize(&mut image).await?;
            image.sync_all().await?;
            drop(image);
            let configuration = directory.join("configuration");
            layers.push(
                MountGuard::mount(&LoopDev::new(&captured, 0, 0), &configuration, ReadOnly).await?,
            );
            let root = directory.join("root");
            layers.push(
                MountGuard::mount(
                    &OverlayFs::new(
                        vec![
                            configuration.clone(),
                            layers.lower().public.path().to_owned(),
                        ],
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
            let main = layers.lower().data.path().join(
                crate::MAIN_DATA
                    .strip_prefix(crate::DATA_DIR)
                    .unwrap()
                    .strip_prefix('/')
                    .unwrap(),
            );
            let private = main.join(super::private_config::DIRECTORY);
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
            let snapshot = Squashfs::from_directory(&configuration).await?.into_dyn();
            let projected = ProjectedConfig::stage(snapshot, &main, &keys).await?;
            let prepared = projected
                .prepare_generation(layers.lower().public.image_path(), &store, &key)
                .await?;
            let legacy = projected.legacy_replacements();
            memory.check()?;
            Ok((prepared, legacy))
        }
        .await;
        let cleanup = layers.unmount().await;
        match result {
            Ok(value) => {
                cleanup?;
                Ok(value)
            }
            Err(error) => {
                cleanup.log_err();
                Err(error)
            }
        }
    })
    .await
}
