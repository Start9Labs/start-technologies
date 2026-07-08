use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use clap::Parser;
use color_eyre::eyre::eyre;
use imbl::OrdSet;
use imbl_value::InternedString;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tracing::instrument;
use ts_rs::TS;

use super::PackageBackupReport;
use super::target::{BackupTargetId, PackageBackupInfo};
use crate::PackageId;
use crate::backup::os::OsBackup;
use crate::backup::{BackupReport, ServerBackupReport};
use crate::context::RpcContext;
use crate::db::model::{Database, DatabaseModel};
use crate::disk::mount::backup::BackupMountGuard;
use crate::disk::mount::filesystem::BackupWrite;
use crate::disk::mount::guard::{GenericMountGuard, TmpMountGuard};
use crate::middleware::auth::session::SessionAuthContext;
use crate::notifications::{NotificationLevel, notify};
use crate::prelude::*;
use crate::progress::{FullProgress, FullProgressTracker};
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::{AtomicFile, dir_copy};
use crate::util::serde::IoFormat;
use crate::version::VersionT;

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct BackupParams {
    #[arg(help = "help.arg.backup-target-id")]
    target_id: BackupTargetId,
    #[arg(long = "old-password", help = "help.arg.old-backup-password")]
    old_password: Option<crate::auth::PasswordType>,
    #[arg(long = "package-ids", help = "help.arg.package-ids-to-backup")]
    package_ids: Option<Vec<PackageId>>,
    #[arg(help = "help.arg.backup-password")]
    password: crate::auth::PasswordType,
}

struct BackupStatusGuard(Option<TypedPatchDb<Database>>);
impl BackupStatusGuard {
    fn new(db: TypedPatchDb<Database>) -> Self {
        Self(Some(db))
    }
    async fn handle_result(
        mut self,
        legacy_backup: bool,
        result: Result<BTreeMap<PackageId, PackageBackupReport>, Error>,
    ) -> Result<(), Error> {
        if let Some(db) = self.0.as_ref() {
            db.mutate(|v| {
                v.as_public_mut()
                    .as_server_info_mut()
                    .as_status_info_mut()
                    .as_backup_progress_mut()
                    .ser(&None)
            })
            .await
            .result?;
        }
        if let Some(db) = self.0.take() {
            match result {
                Ok(report) if report.iter().all(|(_, rep)| rep.error.is_none()) => {
                    db.mutate(|db| {
                        notify(
                            db,
                            None,
                            NotificationLevel::Success,
                            t!("backup.bulk.complete-title").to_string(),
                            t!("backup.bulk.complete-message").to_string(),
                            BackupReport {
                                server: ServerBackupReport {
                                    attempted: true,
                                    error: None,
                                },
                                packages: report,
                            },
                        )?;
                        if legacy_backup {
                            notify_legacy_present(db)?;
                        }
                        Ok(())
                    })
                    .await
                }
                Ok(report) => {
                    db.mutate(|db| {
                        notify(
                            db,
                            None,
                            NotificationLevel::Warning,
                            t!("backup.bulk.complete-title").to_string(),
                            t!("backup.bulk.complete-with-failures").to_string(),
                            BackupReport {
                                server: ServerBackupReport {
                                    attempted: true,
                                    error: None,
                                },
                                packages: report,
                            },
                        )?;
                        if legacy_backup {
                            notify_legacy_present(db)?;
                        }
                        Ok(())
                    })
                    .await
                }
                Err(e) => {
                    tracing::error!("{}", t!("backup.bulk.failed-error", error = e));
                    tracing::debug!("{:?}", e);
                    let err_string = e.to_string();
                    db.mutate(|db| {
                        notify(
                            db,
                            None,
                            NotificationLevel::Error,
                            t!("backup.bulk.failed-title").to_string(),
                            t!("backup.bulk.failed-message").to_string(),
                            BackupReport {
                                server: ServerBackupReport {
                                    attempted: true,
                                    error: Some(err_string),
                                },
                                packages: BTreeMap::new(),
                            },
                        )
                    })
                    .await
                }
            }
            .result?;
        }
        Ok(())
    }
}
impl Drop for BackupStatusGuard {
    fn drop(&mut self) {
        if let Some(db) = self.0.take() {
            tokio::spawn(async move {
                db.mutate(|v| {
                    v.as_public_mut()
                        .as_server_info_mut()
                        .as_status_info_mut()
                        .as_backup_progress_mut()
                        .ser(&None)
                })
                .await
                .result
                .log_err()
            });
        }
    }
}

/// Warn that the just-backed-up target still holds a pre-V2 `StartOSBackups`
/// folder, which is now redundant and can be removed from the backup create page.
fn notify_legacy_present(db: &mut DatabaseModel) -> Result<(), Error> {
    notify(
        db,
        None,
        NotificationLevel::Warning,
        t!("backup.bulk.legacy-present-title").to_string(),
        t!("backup.bulk.legacy-present-message").to_string(),
        (),
    )
}

#[instrument(skip(ctx, old_password, password))]
pub async fn backup_all(
    ctx: RpcContext,
    BackupParams {
        target_id,
        old_password,
        package_ids,
        password,
    }: BackupParams,
) -> Result<(), Error> {
    let old_password_decrypted = old_password
        .as_ref()
        .unwrap_or(&password)
        .clone()
        .decrypt(&ctx)?;
    let password = password.decrypt(&ctx)?;

    let ((fs, package_ids, server_id), status_guard) = (
        ctx.db
            .mutate(|db| {
                RpcContext::check_password(db, &password)?;
                let fs = target_id.load(db)?;
                let package_ids = if let Some(ids) = package_ids {
                    ids.into_iter().collect()
                } else {
                    db.as_public()
                        .as_package_data()
                        .as_entries()?
                        .into_iter()
                        .filter(|(_, m)| m.as_state_info().expect_installed().is_ok())
                        .map(|(id, _)| id)
                        .collect()
                };
                assure_backing_up(db, &package_ids)?;
                Ok((
                    fs,
                    package_ids,
                    db.as_public().as_server_info().as_id().de()?,
                ))
            })
            .await
            .result?,
        BackupStatusGuard::new(ctx.db.clone()),
    );

    let mut backup_guard = BackupMountGuard::mount(
        TmpMountGuard::mount(&fs, BackupWrite).await?,
        &server_id,
        &old_password_decrypted,
    )
    .await?;
    if old_password.is_some() {
        backup_guard.change_password(&password)?;
    }
    let legacy_present =
        crate::disk::util::has_legacy_backup(backup_guard.backup_disk_path()).await;
    tokio::task::spawn(async move {
        status_guard
            .handle_result(
                legacy_present,
                perform_backup(&ctx, backup_guard, &package_ids).await,
            )
            .await
            .unwrap();
    });
    Ok(())
}

#[instrument(skip(db, packages))]
fn assure_backing_up<'a>(
    db: &mut DatabaseModel,
    packages: impl IntoIterator<Item = &'a PackageId>,
) -> Result<(), Error> {
    let backing_up = db
        .as_public_mut()
        .as_server_info_mut()
        .as_status_info_mut()
        .as_backup_progress_mut();
    if backing_up.transpose_ref().is_some() {
        return Err(Error::new(
            eyre!("{}", t!("backup.bulk.already-backing-up")),
            ErrorKind::InvalidRequest,
        ));
    }
    let _ = packages;
    backing_up.ser(&Some(FullProgress::new()))?;
    Ok(())
}

#[instrument(skip(ctx, backup_guard))]
async fn perform_backup(
    ctx: &RpcContext,
    backup_guard: BackupMountGuard<TmpMountGuard>,
    package_ids: &OrdSet<PackageId>,
) -> Result<BTreeMap<PackageId, PackageBackupReport>, Error> {
    let db = ctx.db.peek().await;
    let mut backup_report = BTreeMap::new();
    let backup_guard = Arc::new(backup_guard);
    let mut package_backups: BTreeMap<PackageId, PackageBackupInfo> =
        backup_guard.metadata.package_backups.clone();

    let progress = FullProgressTracker::new();
    let mut phase_handles: BTreeMap<PackageId, _> = package_ids
        .iter()
        .map(|id| {
            (
                id.clone(),
                progress.add_phase(InternedString::from(id.clone()), Some(1)),
            )
        })
        .collect();
    let mut os_data_phase = progress.add_phase("OS Data".into(), Some(1));
    let _progress_db_sync =
        NonDetachingJoinHandle::from(tokio::spawn(progress.clone().sync_to_db(
            ctx.db.clone(),
            |db| {
                db.as_public_mut()
                    .as_server_info_mut()
                    .as_status_info_mut()
                    .as_backup_progress_mut()
                    .transpose_mut()
            },
            Some(Duration::from_millis(300)),
        )));

    for id in package_ids {
        let mut phase = phase_handles.remove(id).expect("phase exists");
        phase.start();
        let started = Instant::now();
        if let Some(service) = &*ctx.services.get(id).await {
            let backup_result = service
                .backup(backup_guard.package_backup(id).await?, phase)
                .await
                .err()
                .map(|e| e.to_string());
            let duration_ms = started.elapsed().as_millis() as u64;
            if backup_result.is_none() {
                let manifest = db
                    .as_public()
                    .as_package_data()
                    .as_idx(id)
                    .or_not_found(id)?
                    .as_state_info()
                    .expect_installed()?
                    .as_manifest();

                package_backups.insert(
                    id.clone(),
                    PackageBackupInfo {
                        os_version: manifest.as_metadata().as_os_version().de()?,
                        version: manifest.as_version().de()?,
                        title: manifest.as_metadata().as_title().de()?,
                        timestamp: Utc::now(),
                    },
                );
            }
            backup_report.insert(
                id.clone(),
                PackageBackupReport {
                    error: backup_result,
                    duration_ms,
                },
            );
        } else {
            phase.complete();
            backup_report.insert(
                id.clone(),
                PackageBackupReport {
                    error: Some(t!("backup.bulk.service-not-ready").to_string()),
                    duration_ms: started.elapsed().as_millis() as u64,
                },
            );
        }
    }
    let mut backup_guard = Arc::try_unwrap(backup_guard).map_err(|_| {
        Error::new(
            eyre!("{}", t!("backup.bulk.leaked-reference")),
            ErrorKind::Incoherent,
        )
    })?;

    os_data_phase.start();

    let ui = ctx.db.peek().await.into_public().into_ui().de()?;

    let mut os_backup_file =
        AtomicFile::new(backup_guard.path().join("os-backup.json"), None::<PathBuf>).await?;
    os_backup_file
        .write_all(&IoFormat::Json.to_vec(&OsBackup {
            account: ctx.account.peek(|a| a.clone()),
            ui,
        })?)
        .await?;
    os_backup_file.save().await?;

    let luks_folder_old = backup_guard.path().join("luks.old");
    crate::util::io::delete_dir(&luks_folder_old).await?;
    let luks_folder_bak = backup_guard.path().join("luks");
    if tokio::fs::metadata(&luks_folder_bak).await.is_ok() {
        tokio::fs::rename(&luks_folder_bak, &luks_folder_old).await?;
    }
    let luks_folder = Path::new("/media/startos/config/luks");
    if tokio::fs::metadata(&luks_folder).await.is_ok() {
        dir_copy(luks_folder, &luks_folder_bak, None).await?;
    }

    os_data_phase.complete();
    progress.complete();

    let timestamp = Utc::now();

    backup_guard.unencrypted_metadata.version = crate::version::Current::default().semver().into();
    backup_guard.unencrypted_metadata.hostname = ctx.account.peek(|a| a.hostname.hostname.clone());
    backup_guard.unencrypted_metadata.timestamp = timestamp.clone();
    backup_guard.metadata.version = crate::version::Current::default().semver().into();
    backup_guard.metadata.timestamp = Some(timestamp);
    backup_guard.metadata.package_backups = package_backups;

    backup_guard.save_and_unmount().await?;

    ctx.db
        .mutate(|v| {
            v.as_public_mut()
                .as_server_info_mut()
                .as_last_backup_mut()
                .ser(&Some(timestamp))
        })
        .await
        .result?;

    Ok(backup_report)
}
