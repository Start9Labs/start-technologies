use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::Utc;
use clap::Parser;
use imbl::OrdSet;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use super::target::{BackupTargetId, SharedVolumeBackupInfo};
use super::{PackageBackupReport, VolumeBackupReport};
use crate::context::RpcContext;
use crate::disk::mount::backup::BackupMountGuard;
use crate::disk::mount::filesystem::{ReadOnly, ReadWrite};
use crate::disk::mount::guard::{GenericMountGuard, TmpMountGuard};
use crate::disk::mount::util::sync_directory;
use crate::prelude::*;
use crate::shared_volumes::{
    SharedVolume, SharedVolumeAvailability, SharedVolumeId, SharedVolumeSource, local_path, source,
};
use crate::util::Invoke;
use crate::util::io::delete_dir;

pub(super) fn parse_id(value: &str) -> Result<SharedVolumeId, uuid::Error> {
    SharedVolumeId::try_from(value.to_owned())
}

pub(super) fn selected_ids(
    requested: Option<Vec<SharedVolumeId>>,
    defined: impl IntoIterator<Item = SharedVolumeId>,
) -> OrdSet<SharedVolumeId> {
    requested.map_or_else(
        || defined.into_iter().collect(),
        |ids| ids.into_iter().collect(),
    )
}

/// Preserves symlinks, hard links, permissions, numeric owners, times, ACLs and xattrs.
/// CIFS metadata fidelity is limited by what the mounted source exposes.
async fn copy_folder(source: &Path, destination: &Path) -> Result<(), Error> {
    let mut contents = source.as_os_str().to_os_string();
    contents.push("/");
    Command::new("rsync")
        .args(["-aHAXS", "--numeric-ids", "--timeout=300", "--"])
        .arg(contents)
        .arg(destination)
        .timeout(Some(Duration::from_secs(24 * 60 * 60)))
        .invoke(ErrorKind::Backup)
        .await?;
    Ok(())
}

async fn stage_copy(data: &Path, source: &Path) -> Result<String, Error> {
    let generation = uuid::Uuid::new_v4().to_string();
    let staging = data.join(format!(".{generation}.staging"));
    tokio::fs::create_dir_all(&staging).await?;
    let result = async {
        copy_folder(source, &staging).await?;
        sync_directory(&staging).await?;
        tokio::fs::rename(&staging, data.join(&generation)).await?;
        sync_directory(data).await?;
        Ok(generation)
    }
    .await;
    if result.is_err() {
        delete_dir(&staging).await.log_err();
    }
    result
}

fn generation_path(data: &Path, info: &SharedVolumeBackupInfo) -> Result<PathBuf, Error> {
    let generation = uuid::Uuid::parse_str(&info.generation).map_err(|_| {
        Error::new(
            eyre!(t!("backup.volume.invalid-generation")),
            ErrorKind::Backup,
        )
    })?;
    Ok(data.join(generation.to_string()))
}

pub(super) async fn measure_backup(
    name: String,
    operation: impl Future<Output = Result<SharedVolumeBackupInfo, Error>>,
) -> (Option<SharedVolumeBackupInfo>, VolumeBackupReport) {
    let started = Instant::now();
    let (info, error) = match operation.await {
        Ok(info) => (Some(info), None),
        Err(error) => (None, Some(error.to_string())),
    };
    (
        info,
        VolumeBackupReport {
            name,
            result: PackageBackupReport {
                error,
                duration_ms: started.elapsed().as_millis() as u64,
            },
        },
    )
}

pub(super) async fn backup_volume(
    ctx: &RpcContext,
    backup: &BackupMountGuard<TmpMountGuard>,
    id: &SharedVolumeId,
) -> Result<SharedVolumeBackupInfo, Error> {
    let name = ctx
        .db
        .peek()
        .await
        .as_public()
        .as_shared_volumes()
        .as_idx(id)
        .or_not_found(id)?
        .as_name()
        .de()?;
    let source = source::open_source(ctx, id, ReadOnly).await?;
    let result = stage_copy(&backup.shared_volume_data(id), source.path()).await;
    source.unmount().await?;
    Ok(SharedVolumeBackupInfo {
        name,
        timestamp: Utc::now(),
        generation: result?,
    })
}

/// Reclaims unreferenced generations after the replacement metadata has been saved.
pub(super) async fn prune_copies(data: &Path, info: &SharedVolumeBackupInfo) -> Result<(), Error> {
    let retained = generation_path(data, info)?;
    let mut entries = tokio::fs::read_dir(data).await?;
    while let Some(entry) = entries.next_entry().await? {
        if entry.path() != retained {
            delete_dir(entry.path()).await?;
        }
    }
    Ok(())
}

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct RestoreVolumeParams {
    #[arg(help = "help.arg.backup-target-id")]
    pub target_id: BackupTargetId,
    #[arg(help = "help.arg.backup-password")]
    pub password: String,
    #[arg(help = "help.arg.volume-ids-to-restore", value_parser = parse_id)]
    pub ids: Vec<SharedVolumeId>,
    #[arg(long, help = "help.arg.server-id")]
    pub server_id: Option<String>,
}

rpc_toolkit::reflect_ts!(RestoreVolumeParams);
rpc_toolkit::ts_export!(RestoreVolumeParams, namespaces = [""]);

#[derive(Debug, Default, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct RestoreVolumeResult {
    pub restored: BTreeMap<SharedVolumeId, SharedVolumeId>,
    pub errors: BTreeMap<SharedVolumeId, String>,
}

rpc_toolkit::reflect_ts!(RestoreVolumeResult);
rpc_toolkit::ts_export!(RestoreVolumeResult, namespaces = [""]);

#[tracing::instrument(skip_all)]
pub async fn restore_volumes_rpc(
    ctx: RpcContext,
    RestoreVolumeParams {
        target_id,
        password,
        ids,
        server_id,
    }: RestoreVolumeParams,
) -> Result<RestoreVolumeResult, Error> {
    let db = ctx.db.peek().await;
    let fs = target_id.load(&db)?;
    let server_id = match server_id {
        Some(id) => id,
        None => db.as_public().as_server_info().as_id().de()?,
    };
    let backup = BackupMountGuard::mount(
        TmpMountGuard::mount(&fs, ReadWrite).await?,
        &server_id,
        &password,
    )
    .await?;
    tokio::spawn(async move {
        let mut result = RestoreVolumeResult::default();
        for id in selected_ids(Some(ids), []) {
            match restore_volume(&ctx, &backup, &id).await {
                Ok(created) => {
                    result.restored.insert(id, created);
                }
                Err(error) => {
                    result.errors.insert(id, error.to_string());
                }
            }
        }
        backup.unmount().await.log_err();
        Ok(result)
    })
    .await
    .with_kind(ErrorKind::Unknown)?
}

struct IncompleteRestore(Option<PathBuf>);

impl IncompleteRestore {
    async fn clean(&mut self) {
        if let Some(path) = self.0.as_ref() {
            if delete_dir(path).await.log_err().is_some() {
                self.0.take();
            }
        }
    }
}

impl Drop for IncompleteRestore {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            tokio::spawn(async move {
                delete_dir(path).await.log_err();
            });
        }
    }
}

fn new_local_restore(name: String) -> (SharedVolumeId, SharedVolume) {
    (
        SharedVolumeId::new(),
        SharedVolume {
            name,
            source: SharedVolumeSource::Local,
            grants: BTreeMap::new(),
            availability: SharedVolumeAvailability::Available,
        },
    )
}

async fn restore_local_copy<F: Future<Output = Result<(), Error>>>(
    data_dir: &Path,
    source: &Path,
    name: String,
    publish: impl FnOnce(SharedVolumeId, SharedVolume) -> F,
) -> Result<SharedVolumeId, Error> {
    let (id, volume) = new_local_restore(name);
    let path = local_path(data_dir, &id);
    tokio::fs::create_dir_all(path.parent().expect("local volume parent")).await?;
    tokio::fs::create_dir(&path).await?;
    let mut incomplete = IncompleteRestore(Some(path.clone()));
    let result = async {
        source::create_local_directory(&path).await?;
        copy_folder(source, &path).await?;
        source::create_local_directory(&path).await?;
        sync_directory(&path).await?;
        incomplete.0.take();
        if let Err(error) = publish(id.clone(), volume).await {
            delete_dir(&path).await.log_err();
            return Err(error);
        }
        Ok(id)
    }
    .await;
    if result.is_ok() {
        incomplete.0.take();
    } else {
        incomplete.clean().await;
    }
    result
}

pub(super) async fn restore_volume(
    ctx: &RpcContext,
    backup: &BackupMountGuard<TmpMountGuard>,
    id: &SharedVolumeId,
) -> Result<SharedVolumeId, Error> {
    let info = backup
        .metadata
        .shared_volume_backups
        .get(id)
        .or_not_found(id)?;
    let source = generation_path(&backup.shared_volume_data(id), info)?;
    restore_local_copy(
        Path::new(crate::DATA_DIR),
        &source,
        info.name.clone(),
        |id, volume| async move {
            ctx.db
                .mutate(|db| {
                    db.as_public_mut()
                        .as_shared_volumes_mut()
                        .insert(&id, &volume)?;
                    Ok(())
                })
                .await
                .result
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
    use std::time::SystemTime;

    use super::*;

    fn info(generation: String) -> SharedVolumeBackupInfo {
        SharedVolumeBackupInfo {
            name: "Photos".into(),
            timestamp: Utc::now(),
            generation,
        }
    }

    #[test]
    fn selection_distinguishes_omitted_empty_and_explicit_ids() {
        let first = SharedVolumeId::new();
        let second = SharedVolumeId::new();
        let all = [first.clone(), second.clone()];
        assert_eq!(selected_ids(None, all.clone()).len(), 2);
        assert!(selected_ids(Some(vec![]), all.clone()).is_empty());
        assert_eq!(
            selected_ids(Some(vec![second.clone(), second.clone()]), all),
            [second].into_iter().collect()
        );
    }

    #[test]
    fn every_restore_has_a_new_local_identity_and_no_grants() {
        let (first, first_volume) = new_local_restore("Photos".into());
        let (second, second_volume) = new_local_restore("Photos".into());
        assert_ne!(first, second);
        for volume in [first_volume, second_volume] {
            assert_eq!(volume.name, "Photos");
            assert_eq!(volume.source, SharedVolumeSource::Local);
            assert!(volume.grants.is_empty());
        }
    }

    #[test]
    fn backup_volume_namespace_cannot_be_a_service_id() {
        assert!(".shared-volumes".parse::<crate::PackageId>().is_err());
    }

    #[test]
    fn backup_generations_cannot_escape_the_volume_namespace() {
        for generation in ["../other", "/etc", "", "../../shared-volumes"] {
            assert!(generation_path(Path::new("/backup/data"), &info(generation.into())).is_err());
        }
    }

    #[tokio::test]
    async fn copy_preserves_the_full_tree_and_archive_metadata() {
        let fixture = tempfile::tempdir().unwrap();
        let source = fixture.path().join("source");
        let data = fixture.path().join("archive/data");
        std::fs::create_dir_all(source.join("nested")).unwrap();
        let file = source.join("nested/.contents");
        std::fs::write(&file, b"complete folder contents").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).unwrap();
        std::fs::File::open(&file)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1234567890))
            .unwrap();
        std::fs::hard_link(&file, source.join("hard-link")).unwrap();
        symlink("nested/.contents", source.join("symbolic-link")).unwrap();
        let generation = stage_copy(&data, &source).await.unwrap();
        let destination = generation_path(&data, &info(generation)).unwrap();
        let copied = destination.join("nested/.contents");
        assert_eq!(std::fs::read(&copied).unwrap(), b"complete folder contents");
        assert_eq!(
            std::fs::read_link(destination.join("symbolic-link")).unwrap(),
            Path::new("nested/.contents")
        );
        let metadata = std::fs::metadata(&copied).unwrap();
        let original = std::fs::metadata(file).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o640);
        assert_eq!(metadata.uid(), original.uid());
        assert_eq!(metadata.gid(), original.gid());
        assert_eq!(metadata.modified().unwrap(), original.modified().unwrap());
        assert_eq!(
            metadata.ino(),
            std::fs::metadata(destination.join("hard-link"))
                .unwrap()
                .ino()
        );
    }

    #[tokio::test]
    async fn failed_or_unpublished_copy_preserves_the_last_saved_generation() {
        let fixture = tempfile::tempdir().unwrap();
        let source = fixture.path().join("source");
        let data = fixture.path().join("archive/data");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("sentinel"), b"old successful backup").unwrap();
        let original = info(stage_copy(&data, &source).await.unwrap());
        let original_file = generation_path(&data, &original).unwrap().join("sentinel");
        assert!(
            stage_copy(&data, &fixture.path().join("missing-source"))
                .await
                .is_err()
        );
        assert_eq!(
            std::fs::read(&original_file).unwrap(),
            b"old successful backup"
        );
        std::fs::write(source.join("sentinel"), b"replacement not published").unwrap();
        let unpublished = info(stage_copy(&data, &source).await.unwrap());
        assert_eq!(
            std::fs::read(&original_file).unwrap(),
            b"old successful backup"
        );
        prune_copies(&data, &original).await.unwrap();
        assert!(original_file.exists());
        assert!(!generation_path(&data, &unpublished).unwrap().exists());
    }

    #[tokio::test]
    async fn restore_creates_a_new_local_volume_without_touching_existing_data() {
        let fixture = tempfile::tempdir().unwrap();
        let old_id = SharedVolumeId::new();
        let old_path = local_path(fixture.path(), &old_id);
        let source = fixture.path().join("backup");
        std::fs::create_dir_all(&old_path).unwrap();
        std::fs::create_dir(&source).unwrap();
        std::fs::write(old_path.join("sentinel"), b"existing data").unwrap();
        std::fs::write(source.join("sentinel"), b"restored data").unwrap();
        let created = restore_local_copy(fixture.path(), &source, "Photos".into(), |id, volume| {
            assert_ne!(id, old_id);
            assert_eq!(volume.name, "Photos");
            assert_eq!(volume.source, SharedVolumeSource::Local);
            assert!(volume.grants.is_empty());
            assert_eq!(
                std::fs::read(local_path(fixture.path(), &id).join("sentinel")).unwrap(),
                b"restored data"
            );
            async { Ok(()) }
        })
        .await
        .unwrap();
        assert_ne!(created, old_id);
        assert_eq!(
            std::fs::read(old_path.join("sentinel")).unwrap(),
            b"existing data"
        );
        assert_eq!(
            std::fs::read(source.join("sentinel")).unwrap(),
            b"restored data"
        );
        let metadata = std::fs::metadata(local_path(fixture.path(), &created)).unwrap();
        assert_eq!(metadata.uid(), 100000);
        assert_eq!(metadata.gid(), 100000);
        assert_eq!(metadata.permissions().mode() & 0o777, 0o777);
    }

    #[tokio::test]
    async fn failed_restore_copy_or_publication_leaves_no_new_data() {
        let fixture = tempfile::tempdir().unwrap();
        let source = fixture.path().join("backup");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("sentinel"), b"backup data").unwrap();
        let failed_copy = restore_local_copy(
            fixture.path(),
            &fixture.path().join("missing"),
            "Photos".into(),
            |_, _| async {
                panic!("failed copy must not publish a catalog entry");
            },
        )
        .await;
        assert!(failed_copy.is_err());
        let failed_publication =
            restore_local_copy(fixture.path(), &source, "Photos".into(), |_, _| async {
                Err(Error::new(
                    eyre!("database unavailable"),
                    ErrorKind::Database,
                ))
            })
            .await;
        assert!(failed_publication.is_err());
        assert_eq!(
            std::fs::read_dir(fixture.path().join("shared-volumes"))
                .unwrap()
                .count(),
            0
        );
        assert_eq!(
            std::fs::read(source.join("sentinel")).unwrap(),
            b"backup data"
        );
    }

    #[tokio::test]
    async fn source_open_failure_is_an_item_error_and_does_not_stop_the_next_item() {
        let (failed, report) = measure_backup("Offline".into(), async {
            Err(Error::new(
                eyre!("source unavailable"),
                ErrorKind::Filesystem,
            ))
        })
        .await;
        assert!(failed.is_none());
        assert_eq!(report.name, "Offline");
        assert!(report.result.error.is_some());
        let expected = uuid::Uuid::new_v4().to_string();
        let (success, report) =
            measure_backup("Photos".into(), async { Ok(info(expected.clone())) }).await;
        assert_eq!(success.unwrap().generation, expected);
        assert!(report.result.error.is_none());
    }
}
