use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use clap::Parser;
use imbl::OrdSet;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use super::target::{BackupTargetId, BackupTargetSource, SharedVolumeBackupInfo, TargetMount};
use super::{PackageBackupReport, VolumeBackupReport};
use crate::context::{CliContext, RpcContext};
use crate::disk::mount::backup::BackupMountGuard;
use crate::disk::mount::filesystem::block_dev::BlockDev;
use crate::disk::mount::filesystem::{FileSystem, ReadOnly, ReadWrite};
use crate::disk::mount::guard::GenericMountGuard;
use crate::disk::mount::util::sync_directory;
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::shared_volumes::{
    SharedVolume, SharedVolumeAvailability, SharedVolumeId, SharedVolumeSource, local_path,
    parse_id, source,
};
use crate::util::Invoke;
use crate::util::cleanup::{CleanupTasks, spawn_cleanup};
use crate::util::io::delete_dir;

pub(super) fn selected_ids(
    requested: Option<Vec<SharedVolumeId>>,
    defined: impl IntoIterator<Item = SharedVolumeId>,
) -> OrdSet<SharedVolumeId> {
    requested.map_or_else(
        || defined.into_iter().collect(),
        |ids| ids.into_iter().collect(),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owners {
    /// Numeric owners as the source holds them.
    Keep,
    /// Root, the container root, for a view whose owners are forced rather than stored.
    Root,
}

/// Preserves symlinks, hard links, permissions, times, ACLs and xattrs, and owners as requested.
/// CIFS metadata fidelity is limited by what the mounted source exposes.
async fn copy_folder(source: &Path, destination: &Path, owners: Owners) -> Result<(), Error> {
    let mut contents = source.as_os_str().to_os_string();
    contents.push("/");
    Command::new("rsync")
        .args(["-aHAXS", "--numeric-ids", "--timeout=300"])
        .args(match owners {
            Owners::Keep => &[][..],
            Owners::Root => &["--no-owner", "--no-group"][..],
        })
        .arg("--")
        .arg(contents)
        .arg(destination)
        .timeout(Some(Duration::from_secs(24 * 60 * 60)))
        .invoke(ErrorKind::Backup)
        .await?;
    Ok(())
}

async fn stage_copy(data: &Path, source: &Path, owners: Owners) -> Result<String, Error> {
    let generation = Guid::new().to_string();
    let staging = data.join(format!(".{generation}.staging"));
    tokio::fs::create_dir_all(&staging).await?;
    let result = async {
        copy_folder(source, &staging, owners).await?;
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
    let generation = info.generation.parse::<Guid>().map_err(|_| {
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

/// Copies the host view: container ids, as package backups hold. A CIFS copy is owned by root.
pub(super) async fn backup_volume(
    ctx: &RpcContext,
    backup: &BackupMountGuard<TargetMount>,
    id: &SharedVolumeId,
    destination: &BackupTargetSource,
) -> Result<SharedVolumeBackupInfo, Error> {
    let volume = ctx
        .db
        .peek()
        .await
        .as_public()
        .as_shared_volumes()
        .as_idx(id)
        .or_not_found(id)?
        .de()?;
    if source_overlaps_target(
        &volume.source,
        &destination.source,
        &backup.encrypted_relative_path(),
    )
    .await?
    {
        return Err(overlapping_target_error());
    }
    let source = source::open_source(ctx, id, ReadOnly).await?;
    let result = async {
        let target_data = backup
            .backup_disk_path()
            .join(backup.encrypted_relative_path());
        if crate::util::io::directories_overlap(source.path(), &target_data).await? {
            return Err(overlapping_target_error());
        }
        let owners = if source.stores_container_ids() {
            Owners::Keep
        } else {
            Owners::Root
        };
        stage_copy(&backup.shared_volume_data(id), source.path(), owners).await
    }
    .await;
    source.unmount().await?;
    Ok(SharedVolumeBackupInfo {
        name: volume.name,
        timestamp: Utc::now(),
        generation: result?,
    })
}

fn overlapping_target_error() -> Error {
    Error::new(
        eyre!(t!("backup.volume.overlapping-target")),
        ErrorKind::Backup,
    )
}

async fn source_overlaps_target(
    source: &SharedVolumeSource,
    destination: &SharedVolumeSource,
    encrypted_relative_path: &Path,
) -> Result<bool, Error> {
    Ok(match (source, destination) {
        (
            SharedVolumeSource::Cifs { hostname, path, .. },
            SharedVolumeSource::Cifs {
                hostname: target_hostname,
                path: target_path,
                ..
            },
        ) => {
            let target_path = Path::new("/")
                .join(target_path)
                .join(encrypted_relative_path);
            crate::disk::mount::filesystem::cifs::locations_overlap(
                (hostname, path),
                (target_hostname, &target_path),
            )
            .await?
        }
        (
            SharedVolumeSource::Disk {
                logicalname, path, ..
            },
            SharedVolumeSource::Disk {
                logicalname: target,
                path: target_path,
                ..
            },
        ) => {
            let target_path = target_path.join(encrypted_relative_path);
            crate::util::io::paths_overlap(
                &crate::util::io::case_folded(path),
                &crate::util::io::case_folded(&target_path),
            ) && BlockDev::new(logicalname).source_hash().await?
                == BlockDev::new(target).source_hash().await?
        }
        _ => false,
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
    #[arg(skip)]
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
pub(super) async fn cli_restore_volumes(
    mut args: rpc_toolkit::HandlerArgs<CliContext, RestoreVolumeParams>,
) -> Result<Value, Error> {
    args.params.password = super::read_password()?;
    super::call_remote(args).await
}

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
    let backup = BackupMountGuard::mount(fs.mount(ReadWrite).await?, &server_id, &password).await?;
    let owner = ctx.clone();
    let result = owner.spawn_background_result(async move {
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
    })?;
    result.await.with_kind(ErrorKind::Unknown)?
}

struct IncompleteRestore {
    path: Option<PathBuf>,
    tasks: Arc<CleanupTasks>,
}

impl IncompleteRestore {
    async fn clean(&mut self) {
        if let Some(path) = self.path.as_ref() {
            if crate::util::btrfs::delete_tree(path)
                .await
                .log_err()
                .is_some()
            {
                self.path.take();
            }
        }
    }
}

impl Drop for IncompleteRestore {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = spawn_cleanup(&self.tasks, async move {
                crate::util::btrfs::delete_tree(path).await.log_err();
            });
        }
    }
}

fn new_local_restore(name: String) -> (SharedVolumeId, SharedVolume) {
    let mut volume = SharedVolume::new(name, SharedVolumeSource::Local);
    volume.inspection.availability = SharedVolumeAvailability::Available;
    (SharedVolumeId::new(), volume)
}

async fn restore_local_copy<F: Future<Output = Result<(), Error>>>(
    package_data: &Path,
    source: &Path,
    name: String,
    tasks: Arc<CleanupTasks>,
    publish: impl FnOnce(SharedVolumeId, SharedVolume) -> F,
) -> Result<SharedVolumeId, Error> {
    let (id, volume) = new_local_restore(name);
    let path = local_path(package_data, &id);
    tokio::fs::create_dir_all(path.parent().expect("local volume parent")).await?;
    crate::util::btrfs::create_subvolume(&path).await?;
    let mut incomplete = IncompleteRestore {
        path: Some(path.clone()),
        tasks,
    };
    let result = async {
        source::set_local_permissions(&path).await?;
        copy_folder(source, &path, Owners::Keep).await?;
        source::set_local_permissions(&path).await?;
        sync_directory(&path).await?;
        // Publication can commit after its caller cancels.
        incomplete.path.take();
        source::finish_directory_publication(&path, publish(id.clone(), volume).await).await?;
        Ok(id)
    }
    .await;
    if result.is_ok() {
        incomplete.path.take();
    } else {
        incomplete.clean().await;
    }
    result
}

pub(super) async fn restore_volume<G: GenericMountGuard>(
    ctx: &RpcContext,
    backup: &BackupMountGuard<G>,
    id: &SharedVolumeId,
) -> Result<SharedVolumeId, Error> {
    let info = backup
        .metadata
        .shared_volume_backups
        .get(id)
        .or_not_found(id)?;
    let source = generation_path(&backup.shared_volume_data(id), info)?;
    restore_local_copy(
        Path::new(crate::PACKAGE_DATA),
        &source,
        info.name.clone(),
        ctx.cleanup_tasks.clone(),
        |id, volume| async move {
            ctx.db
                .mutate(|db| {
                    crate::shared_volumes::insert_with_unused_name(
                        db.as_public_mut().as_shared_volumes_mut(),
                        &id,
                        volume.clone(),
                    )
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

    fn cleanup_tasks() -> Arc<CleanupTasks> {
        Arc::new(crate::util::sync::SyncMutex::new(Vec::new()))
    }

    #[tokio::test]
    async fn external_volume_backups_reject_overlapping_folders_through_any_device_alias() {
        let fixture = tempfile::tempdir().unwrap();
        let disk = fixture.path().join("disk");
        let alias = fixture.path().join("filesystem-uuid");
        let other = fixture.path().join("other-disk");
        std::fs::write(&disk, "source").unwrap();
        std::fs::write(&other, "destination").unwrap();
        symlink(&disk, &alias).unwrap();
        let source = |path: &str| SharedVolumeSource::Disk {
            logicalname: alias.clone(),
            path: path.into(),
            drive: None,
        };
        let target = |logicalname, path: &str| SharedVolumeSource::Disk {
            logicalname,
            path: path.into(),
            drive: None,
        };
        for (source_path, target_path, overlaps) in [
            ("", "", true),
            ("media", "backups", false),
            ("media", "media", true),
            ("media/backup/photos", "media", true),
            ("media/backups", "media", false),
            ("Photos", "photos", true),
            ("MEDIA/BACKUP/photos", "media", true),
            ("Photos-old", "photos", false),
        ] {
            assert_eq!(
                source_overlaps_target(
                    &source(source_path),
                    &target(disk.clone(), target_path),
                    Path::new("backup")
                )
                .await
                .unwrap(),
                overlaps
            );
            assert!(
                !source_overlaps_target(
                    &source(source_path),
                    &target(other.clone(), target_path),
                    Path::new("backup")
                )
                .await
                .unwrap()
            );
        }
    }

    fn info(generation: String) -> SharedVolumeBackupInfo {
        SharedVolumeBackupInfo {
            name: "Photos".into(),
            timestamp: Utc::now(),
            generation,
        }
    }

    #[test]
    fn restore_cli_omits_password_but_keeps_it_in_rpc_params() {
        let id = SharedVolumeId::new();
        let target = BackupTargetId::new();
        let mut params = RestoreVolumeParams::try_parse_from([
            "restore",
            target.as_ref(),
            id.as_ref(),
            "--server-id",
            "old-server",
        ])
        .unwrap();
        assert!(params.password.is_empty());
        assert_eq!(params.ids, vec![id.clone()]);
        assert_eq!(params.server_id.as_deref(), Some("old-server"));
        assert!(
            RestoreVolumeParams::try_parse_from([
                "restore",
                target.as_ref(),
                "plaintext-password",
                id.as_ref(),
            ])
            .is_err()
        );
        params.password = "provided out of band".into();
        assert_eq!(
            serde_json::to_value(params).unwrap()["password"],
            "provided out of band"
        );
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
        let generation = Guid::new().to_string();
        let data = Path::new("/backup/data");
        assert_eq!(
            generation_path(data, &info(generation.clone())).unwrap(),
            data.join(generation)
        );
        for generation in [
            "../other",
            "/etc",
            "",
            "../../shared-volumes",
            "7cb12a88-783d-4909-9923-d5eb47c7e01f",
        ] {
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
        let generation = stage_copy(&data, &source, Owners::Keep).await.unwrap();
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
        let original = info(stage_copy(&data, &source, Owners::Keep).await.unwrap());
        let original_file = generation_path(&data, &original).unwrap().join("sentinel");
        assert!(
            stage_copy(&data, &fixture.path().join("missing-source"), Owners::Keep)
                .await
                .is_err()
        );
        assert_eq!(
            std::fs::read(&original_file).unwrap(),
            b"old successful backup"
        );
        std::fs::write(source.join("sentinel"), b"replacement not published").unwrap();
        let unpublished = info(stage_copy(&data, &source, Owners::Keep).await.unwrap());
        assert_eq!(
            std::fs::read(&original_file).unwrap(),
            b"old successful backup"
        );
        prune_copies(&data, &original).await.unwrap();
        assert!(original_file.exists());
        assert!(!generation_path(&data, &unpublished).unwrap().exists());
    }

    fn container_owned_source(root: &Path) -> Option<PathBuf> {
        let source = root.join("source");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("sentinel"), b"container data").unwrap();
        std::os::unix::fs::chown(source.join("sentinel"), Some(1000), Some(2000)).ok()?;
        Some(source)
    }

    #[tokio::test]
    async fn a_backup_copy_keeps_container_ids() {
        let root = tempfile::tempdir().unwrap();
        let Some(source) = container_owned_source(root.path()) else {
            eprintln!("skipped: needs permission to chown to container ids");
            return;
        };
        let data = root.path().join("data");
        let generation = stage_copy(&data, &source, Owners::Keep).await.unwrap();
        let metadata = std::fs::metadata(data.join(generation).join("sentinel")).unwrap();
        assert_eq!((metadata.uid(), metadata.gid()), (1000, 2000));
        let generation = stage_copy(&data, &source, Owners::Root).await.unwrap();
        let metadata = std::fs::metadata(data.join(generation).join("sentinel")).unwrap();
        assert_eq!(
            (metadata.uid(), metadata.gid()),
            (
                nix::unistd::geteuid().as_raw(),
                nix::unistd::getegid().as_raw()
            )
        );
    }

    #[tokio::test]
    #[ignore = "requires root, BTRFS_TEST_VOLUMES_DIR and a private Linux mount namespace"]
    async fn backup_and_restore_round_trip_preserves_container_ids() {
        let fixture = crate::util::btrfs::tests::directory();
        let source = container_owned_source(fixture.path()).unwrap();
        let data = fixture.path().join("data");
        let generation = stage_copy(&data, &source, Owners::Keep).await.unwrap();
        let created = restore_local_copy(
            fixture.path(),
            &data.join(generation),
            "Photos".into(),
            cleanup_tasks(),
            |_, _| async { Ok(()) },
        )
        .await
        .unwrap();
        let metadata =
            std::fs::metadata(local_path(fixture.path(), &created).join("sentinel")).unwrap();
        assert_eq!((metadata.uid(), metadata.gid()), (1000, 2000));
    }

    #[tokio::test]
    #[ignore = "requires root, BTRFS_TEST_VOLUMES_DIR and a private Linux mount namespace"]
    async fn restore_creates_a_new_local_volume_without_touching_existing_data() {
        let fixture = crate::util::btrfs::tests::directory();
        let old_id = SharedVolumeId::new();
        let old_path = local_path(fixture.path(), &old_id);
        let source = fixture.path().join("backup");
        std::fs::create_dir_all(&old_path).unwrap();
        std::fs::create_dir(&source).unwrap();
        std::fs::write(old_path.join("sentinel"), b"existing data").unwrap();
        std::fs::write(source.join("sentinel"), b"restored data").unwrap();
        std::os::unix::fs::chown(source.join("sentinel"), Some(1000), Some(2000)).unwrap();
        let created = restore_local_copy(
            fixture.path(),
            &source,
            "Photos".into(),
            cleanup_tasks(),
            |id, volume| {
                assert_ne!(id, old_id);
                assert_eq!(volume.name, "Photos");
                assert_eq!(volume.source, SharedVolumeSource::Local);
                assert!(volume.grants.is_empty());
                assert_eq!(
                    std::fs::read(local_path(fixture.path(), &id).join("sentinel")).unwrap(),
                    b"restored data"
                );
                async { Ok(()) }
            },
        )
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
        assert!(crate::util::btrfs::is_subvolume(local_path(fixture.path(), &created)).await);
        let metadata = std::fs::metadata(local_path(fixture.path(), &created)).unwrap();
        assert_eq!(metadata.uid(), 0);
        assert_eq!(metadata.gid(), 0);
        assert_eq!(metadata.permissions().mode() & 0o777, 0o777);
        let restored = local_path(fixture.path(), &created).join("sentinel");
        let metadata = std::fs::metadata(&restored).unwrap();
        assert_eq!((metadata.uid(), metadata.gid()), (1000, 2000));
    }

    #[tokio::test]
    #[ignore = "requires root, BTRFS_TEST_VOLUMES_DIR and a private Linux mount namespace"]
    async fn failed_restore_copy_or_publication_leaves_no_new_data() {
        let fixture = crate::util::btrfs::tests::directory();
        let source = fixture.path().join("backup");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("sentinel"), b"backup data").unwrap();
        let failed_copy = restore_local_copy(
            fixture.path(),
            &fixture.path().join("missing"),
            "Photos".into(),
            cleanup_tasks(),
            |_, _| async {
                panic!("failed copy must not publish a catalog entry");
            },
        )
        .await;
        assert_eq!(failed_copy.unwrap_err().kind, ErrorKind::Backup);
        let failed_publication = restore_local_copy(
            fixture.path(),
            &source,
            "Photos".into(),
            cleanup_tasks(),
            |_, _| async {
                Err(Error::new(
                    eyre!("database unavailable"),
                    ErrorKind::Database,
                ))
            },
        )
        .await;
        assert_eq!(failed_publication.unwrap_err().kind, ErrorKind::Database);
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
        let expected = Guid::new().to_string();
        let (success, report) =
            measure_backup("Photos".into(), async { Ok(info(expected.clone())) }).await;
        assert_eq!(success.unwrap().generation, expected);
        assert!(report.result.error.is_none());
    }
}
