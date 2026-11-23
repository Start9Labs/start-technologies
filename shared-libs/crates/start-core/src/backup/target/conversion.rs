use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use patch_db::HasModel;
use rpc_toolkit::HandlerArgs;
use serde::{Deserialize, Serialize};
use tokio::sync::{oneshot, watch};
use tracing::instrument;
use zeroize::Zeroizing;

use super::{BackupTargetId, TargetMount};
use crate::context::{CliContext, RpcContext};
use crate::db::model::{Database, DatabaseModel};
use crate::disk::mount::backup::BackupMountGuard;
use crate::disk::mount::filesystem::{ReadWrite, backupfs};
use crate::disk::mount::guard::GenericMountGuard;
use crate::notifications::{NotificationLevel, notify};
use crate::prelude::*;
use crate::progress::{FullProgress, run_with_progress_sync};

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
#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ConvertParams {
    #[arg(help = "help.arg.backup-target-id")]
    target_id: BackupTargetId,
    #[arg(help = "help.arg.server-id")]
    server_id: String,
    #[arg(skip = crate::auth::PasswordType::String(String::new()))]
    password: crate::auth::PasswordType,
}
rpc_toolkit::reflect_ts!(ConvertParams);

pub(super) async fn cli_start(
    mut args: HandlerArgs<CliContext, ConvertParams>,
) -> Result<(), Error> {
    args.params.password = crate::auth::PasswordType::String(crate::backup::read_password()?);
    args.context
        .call_fronted(args.parent_method, args.method, &args.params)
        .await
}

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
    let password = Zeroizing::new(password.decrypt(&ctx)?);
    let db = ctx.db.clone();
    let shutdown = ctx.shutdown.clone();
    let disk_updates = ctx.disk_updates.clone();
    let (reply, result) = oneshot::channel();
    ctx.spawn_background(async move {
        let prepared = async {
            let fs = target_id.clone().load(&db.peek().await)?;
            let label = fs.name.clone();
            let guard = fs.mount(ReadWrite).await?;
            let _disk_refresh = target_id.refresh_disks_on_drop(disk_updates.clone());
            let prepared = BackupMountGuard::<TargetMount>::prepare_conversion(
                guard.path(),
                &server_id,
                &password,
            )
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
            let reservation = db
                .mutate(|db| {
                    if shutdown.peek(|(sender, _)| sender.is_none()) {
                        return Err(Error::new(
                            eyre!(t!("backup.conversion.interrupted")),
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
            Ok((
                PreparedConversion {
                    target_id,
                    server_id,
                    label,
                    guard,
                    data,
                    key,
                },
                progress,
            ))
        }
        .await;
        match prepared {
            Ok((conversion, progress)) => {
                let _ = reply.send(Ok(()));
                conversion.run(db, progress, disk_updates).await.log_err();
            }
            Err(error) => {
                let _ = reply.send(Err(error));
            }
        }
    })?;
    result.await.map_err(|_| {
        Error::new(
            eyre!(t!("backup.conversion.interrupted")),
            ErrorKind::InvalidRequest,
        )
    })?
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
        disk_updates: watch::Sender<()>,
    ) -> Result<(), Error> {
        let Self {
            target_id,
            server_id,
            label,
            guard,
            data,
            key,
        } = self;
        let _disk_refresh = target_id.refresh_disks_on_drop(disk_updates);
        let target_for_progress = target_id.to_string();
        let server_for_progress = server_id.clone();
        let progress_sync = progress.tracker().sync_to_db(
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
        );
        let result =
            run_with_progress_sync(backupfs::convert(&data, &key, &mut progress), progress_sync)
                .await;
        drop(key);
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
        .result?;
        super::catalog::refresh(&db, &target_id).await.log_err();
        Ok(())
    }
}
