use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use patch_db::HasModel;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::task;
use tracing::instrument;
use zeroize::Zeroizing;

use super::{BackupTargetFS, BackupTargetId};
use crate::context::RpcContext;
use crate::db::model::{Database, DatabaseModel};
use crate::disk::mount::backup::BackupMountGuard;
use crate::disk::mount::filesystem::{FileSystem, ReadWrite, backupfs};
use crate::disk::mount::guard::{GenericMountGuard, TmpMountGuard};
use crate::notifications::{NotificationLevel, notify};
use crate::prelude::*;
use crate::progress::FullProgress;
use crate::shutdown::Shutdown;
use crate::util::future::NonDetachingJoinHandle;

/// Lifecycle of a daemon-owned conversion attempt.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, VisitVariants)]
#[serde(rename_all = "camelCase")]
pub enum ConversionState {
    Running,
    Complete,
    Failed,
}
rpc_toolkit::reflect_ts!(ConversionState);

/// Latest observed conversion attempt for one target/server backup.
#[derive(Clone, Debug, Deserialize, Serialize, HasModel, VisitFields)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct ConversionJob {
    pub state: ConversionState,
    pub progress: FullProgress,
    pub error: Option<String>,
}
rpc_toolkit::reflect_ts!(ConversionJob);

#[cfg(test)]
mod tests;

impl ConversionJob {
    fn finished(progress: FullProgress, result: &Result<(), Error>) -> Self {
        Self {
            state: if result.is_ok() {
                ConversionState::Complete
            } else {
                ConversionState::Failed
            },
            progress,
            error: result.as_ref().err().map(ToString::to_string),
        }
    }

    fn notification(&self, label: &str) -> (NotificationLevel, String, String) {
        match &self.error {
            None => (
                NotificationLevel::Success,
                t!("backup.conversion.complete-title").to_string(),
                t!("backup.conversion.complete-message", label = label).to_string(),
            ),
            Some(error) => (
                NotificationLevel::Error,
                t!("backup.conversion.failed-title").to_string(),
                t!(
                    "backup.conversion.failed-message",
                    label = label,
                    error = error
                )
                .to_string(),
            ),
        }
    }
}

pub type ConversionJobs = BTreeMap<String, BTreeMap<String, ConversionJob>>;

/// Selects an existing server backup and its unlocking credential.
#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct ConvertParams {
    target_id: BackupTargetId,
    server_id: String,
    password: crate::auth::PasswordType,
}
rpc_toolkit::reflect_ts!(ConvertParams);

pub(crate) fn ensure_available(
    jobs: &ConversionJobs,
    target: &BackupTargetId,
    server: &str,
) -> Result<(), Error> {
    if jobs
        .get(&target.to_string())
        .and_then(|entries| entries.get(server))
        .is_some_and(|job| job.state == ConversionState::Running)
    {
        return Err(Error::new(
            eyre!("{}", t!("backup.conversion.already-running")),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(())
}

fn reserve(
    jobs: &mut ConversionJobs,
    target: BackupTargetId,
    server: String,
    progress: FullProgress,
) -> Result<(), Error> {
    ensure_available(jobs, &target, &server)?;
    jobs.entry(target.to_string()).or_default().insert(
        server,
        ConversionJob {
            state: ConversionState::Running,
            progress,
            error: None,
        },
    );
    Ok(())
}

pub(crate) fn jobs(db: &DatabaseModel) -> Result<ConversionJobs, Error> {
    Ok(db
        .as_public()
        .as_server_info()
        .as_status_info()
        .as_backup_conversions()
        .de()?)
}

/// Starts conversion after credential validation, without waiting for the data rewrite.
#[instrument(skip_all)]
pub async fn start(
    ctx: RpcContext,
    ConvertParams {
        target_id,
        server_id,
        password,
    }: ConvertParams,
) -> Result<(), Error> {
    let shutdown = ctx.shutdown.subscribe();
    let password = Zeroizing::new(password.decrypt(&ctx)?);
    let fs = target_id.clone().load(&ctx.db.peek().await)?;
    let label = match &fs {
        BackupTargetFS::Disk(disk) => {
            let source = disk.source().await?.unwrap();
            crate::disk::util::get_label(&source)
                .await?
                .unwrap_or_else(|| source.as_ref().display().to_string())
        }
        BackupTargetFS::Cifs(cifs) => format!("{}:{}", cifs.hostname, cifs.path.display()),
    };
    let guard = TmpMountGuard::mount(&fs, ReadWrite).await?;
    let prepared =
        BackupMountGuard::<TmpMountGuard>::prepare_conversion(guard.path(), &server_id, &password)
            .await;
    let (data, key) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            guard.unmount().await.log_err();
            return Err(error);
        }
    };
    drop(password);
    let progress = backupfs::ConversionProgressSink::new();
    let initial = progress.tracker().snapshot();
    let reservation = ctx
        .db
        .mutate(|db| {
            let status = db.as_public().as_server_info().as_status_info();
            if status.as_shutting_down().de()? || status.as_restarting().de()? {
                return Err(Error::new(
                    eyre!("{}", t!("backup.conversion.interrupted")),
                    ErrorKind::InvalidRequest,
                ));
            }
            let mut active = jobs(db)?;
            reserve(
                &mut active,
                target_id.clone(),
                server_id.clone(),
                initial.clone(),
            )?;
            db.as_public_mut()
                .as_server_info_mut()
                .as_status_info_mut()
                .as_backup_conversions_mut()
                .ser(&active)?;
            Ok(())
        })
        .await
        .result;
    if let Err(error) = reservation {
        guard.unmount().await.log_err();
        return Err(error);
    }
    let conversion = PreparedConversion {
        target_id,
        server_id,
        label,
        guard,
        data,
        key,
    };
    let db = ctx.db.clone();
    task::spawn(async move { conversion.run(db, progress, shutdown).await.log_err() });
    Ok(())
}

struct PreparedConversion<G> {
    target_id: BackupTargetId,
    server_id: String,
    label: String,
    guard: G,
    data: PathBuf,
    key: Zeroizing<String>,
}

impl<G: GenericMountGuard> PreparedConversion<G> {
    async fn run(
        self,
        db: TypedPatchDb<Database>,
        mut progress: backupfs::ConversionProgressSink,
        mut shutdown: broadcast::Receiver<Option<Shutdown>>,
    ) -> Result<(), Error> {
        let Self {
            target_id,
            server_id,
            label,
            guard,
            data,
            key,
        } = self;
        let target_for_progress = target_id.to_string();
        let server_for_progress = server_id.clone();
        let progress_sync =
            NonDetachingJoinHandle::from(task::spawn(progress.tracker().sync_to_db(
                db.clone(),
                move |db| {
                    Some(
                        db.as_public_mut()
                            .as_server_info_mut()
                            .as_status_info_mut()
                            .as_backup_conversions_mut()
                            .as_idx_mut(&target_for_progress)?
                            .as_idx_mut(&server_for_progress)?
                            .as_progress_mut(),
                    )
                },
                Some(Duration::from_millis(300)),
            )));
        let result = tokio::select! {
            result = backupfs::convert(&data, &key, &mut progress) => result,
            _ = shutdown.recv() => Err(Error::new(eyre!("{}", t!("backup.conversion.interrupted")), ErrorKind::Backup)),
        };
        drop(key);
        let _ = progress_sync.wait_for_abort().await;
        let result = match guard.unmount().await {
            Ok(()) => result,
            Err(error) => result.and(Err(error)),
        };
        let status = ConversionJob::finished(progress.tracker().snapshot(), &result);
        let (level, title, message) = status.notification(&label);
        db.mutate(|db| {
            let mut active = jobs(db)?;
            active
                .entry(target_id.to_string())
                .or_default()
                .insert(server_id.clone(), status.clone());
            db.as_public_mut()
                .as_server_info_mut()
                .as_status_info_mut()
                .as_backup_conversions_mut()
                .ser(&active)?;
            notify(db, None, level, title, message, ())
        })
        .await
        .result
    }
}
