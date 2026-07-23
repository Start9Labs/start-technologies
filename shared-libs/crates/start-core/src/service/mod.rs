use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::IsTerminal;
use std::ops::Deref;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Weak};
use std::time::Duration;

use axum::extract::ws::Utf8Bytes;
use clap::Parser;
use futures::future::BoxFuture;
use futures::stream::FusedStream;
use futures::{FutureExt, SinkExt, StreamExt, TryStreamExt};
use imbl_value::{InternedString, json};
use itertools::Itertools;
use nix::sys::signal::Signal;
use persistent_container::PersistentContainer;
use rpc_toolkit::HandlerArgs;
use serde::{Deserialize, Serialize};
use service_actor::ServiceActor;
use termion::raw::IntoRawMode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use ts_rs::TS;
use url::Url;

use crate::context::{CliContext, RpcContext};
use crate::db::model::package::{
    InstalledState, ManifestPreference, PackageState, PackageStateMatchModelRef, TaskSeverity,
    UpdatingState,
};
use crate::disk::mount::filesystem::ReadOnly;
use crate::disk::mount::guard::{GenericMountGuard, MountGuard};
use crate::lxc::ContainerId;
use crate::prelude::*;
use crate::rpc_continuations::{Guid, RpcContinuation};
use crate::s9pk::S9pk;
use crate::service::action::update_tasks;
use crate::service::rpc::{ExitParams, InitKind};
use crate::service::service_map::InstallProgressHandles;
use crate::service::uninstall::cleanup;
use crate::util::Never;
use crate::util::actor::concurrent::ConcurrentActor;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::{
    AsyncReadStream, AtomicFile, TermSize, delete_file, maybe_read_file_to_string,
};
use crate::util::net::WebSocket;
use crate::util::serde::Pem;
use crate::util::sync::SyncMutex;
use crate::util::tui::choose;
use crate::volume::{PKG_VOLUME_DIR, data_dir};
use crate::{ActionId, CAP_1_KiB, DATA_DIR, ImageId, PackageId};

pub mod action;
pub mod cli;
pub mod effects;
pub mod persistent_container;
pub mod procedure_name;
mod rpc;
mod service_actor;
pub mod service_map;
pub mod start_stop;
mod transition;
pub mod uninstall;

pub use procedure_name::ProcedureName;
pub use service_map::ServiceMap;

pub const HEALTH_CHECK_COOLDOWN_SECONDS: u64 = 15;
pub const HEALTH_CHECK_GRACE_PERIOD_SECONDS: u64 = 5;
pub const SYNC_RETRY_COOLDOWN_SECONDS: u64 = 10;

pub type Task<'a> = BoxFuture<'a, Result<(), Error>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LoadDisposition {
    Retry,
    Undo,
}

/// Read the data version file for a service from disk.
/// Returns `Ok(None)` if the file does not exist (fresh install).
pub async fn get_data_version(id: &PackageId) -> Result<Option<String>, Error> {
    let path = Path::new(DATA_DIR)
        .join(PKG_VOLUME_DIR)
        .join(id)
        .join("data")
        .join(".version");
    let s = maybe_read_file_to_string(&path).await?;
    Ok(s.map(|s| s.trim().to_string()))
}

struct RootCommand(pub String);

#[derive(Clone, Debug, Serialize, Deserialize, Default, TS)]
pub struct MiB(pub u64);

impl MiB {
    pub fn new(value: u64) -> Self {
        Self(value / 1024 / 1024)
    }
    #[allow(non_snake_case)]
    fn from_MiB(value: u64) -> Self {
        Self(value)
    }
}

impl std::fmt::Display for MiB {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} MiB", self.0)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, TS)]
pub struct ServiceStats {
    pub container_id: Arc<ContainerId>,
    pub memory_usage: MiB,
    pub memory_limit: MiB,
}

pub struct ServiceRef(Arc<Service>);
impl ServiceRef {
    pub fn weak(&self) -> Weak<Service> {
        Arc::downgrade(&self.0)
    }
    pub async fn uninstall(
        self,
        uninit: ExitParams,
        soft: bool,
        force: bool,
    ) -> Result<BoxFuture<'static, Result<(), Error>>, Error> {
        let id = self.seed.persistent_container.s9pk.as_manifest().id.clone();
        let ctx = self.seed.ctx.clone();
        let uninit_res = self.shutdown(Some(uninit.clone())).await;
        if force {
            uninit_res.log_err();
        } else {
            uninit_res?;
        }

        let s9pk_path = ctx
            .db
            .peek()
            .await
            .into_public()
            .into_package_data()
            .into_idx(&id)
            .map(|pde| pde.into_s9pk());

        Ok(async move {
            if let Some(s9pk_path) = s9pk_path {
                delete_file(s9pk_path.de()?).await?;
            }

            if uninit.is_uninstall() {
                uninstall::cleanup(&ctx, &id, soft).await?;
            }
            Ok(())
        }
        .boxed())
    }
    pub async fn shutdown(self, uninit: Option<ExitParams>) -> Result<(), Error> {
        if let Some((hdl, shutdown)) = self.seed.persistent_container.rpc_server.send_replace(None)
        {
            self.seed
                .persistent_container
                .rpc_client
                .request(
                    rpc::Exit,
                    uninit.clone().unwrap_or_else(|| {
                        ExitParams::target_version(
                            &*self.seed.persistent_container.s9pk.as_manifest().version,
                        )
                    }),
                )
                .await?;
            shutdown.shutdown();
            tokio::time::timeout(Duration::from_secs(30), hdl)
                .await
                .with_kind(ErrorKind::Timeout)?
                .with_kind(ErrorKind::Cancelled)?;
        }
        let service = Arc::try_unwrap(self.0).map_err(|_| {
            Error::new(
                eyre!("{}", t!("service.mod.service-actor-held-after-shutdown")),
                ErrorKind::Unknown,
            )
        })?;
        service
            .actor
            .shutdown(crate::util::actor::PendingMessageStrategy::FinishAll { timeout: None }) // TODO timeout
            .await;
        Arc::try_unwrap(service.seed)
            .map_err(|_| {
                Error::new(
                    eyre!(
                        "{}",
                        t!("service.mod.service-actor-seed-held-after-shutdown")
                    ),
                    ErrorKind::Unknown,
                )
            })?
            .persistent_container
            .exit(uninit)
            .await?;
        Ok(())
    }
}

impl Deref for ServiceRef {
    type Target = Service;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl From<Service> for ServiceRef {
    fn from(value: Service) -> Self {
        Self(Arc::new(value))
    }
}

pub struct Service {
    actor: ConcurrentActor<ServiceActor>,
    seed: Arc<ServiceActorSeed>,
}
impl Service {
    pub fn is_initialized(&self) -> bool {
        self.seed.persistent_container.state.borrow().rt_initialized
    }

    /// Re-evaluate all tasks that reference this service's actions.
    /// Called after every service init to update task active state.
    #[instrument(skip_all)]
    pub async fn recheck_tasks(&self) -> Result<(), Error> {
        let service_id = &self.seed.id;
        let peek = self.seed.ctx.db.peek().await;
        let mut action_input: BTreeMap<ActionId, Value> = BTreeMap::new();
        let tasks: BTreeSet<_> = peek
            .as_public()
            .as_package_data()
            .as_entries()?
            .into_iter()
            .map(|(_, pde)| {
                Ok(pde
                    .as_tasks()
                    .as_entries()?
                    .into_iter()
                    .map(|(_, r)| {
                        let t = r.as_task();
                        Ok::<_, Error>(
                            if t.as_package_id().de()? == *service_id
                                && t.as_input().transpose_ref().is_some()
                            {
                                Some(t.as_action_id().de()?)
                            } else {
                                None
                            },
                        )
                    })
                    .filter_map_ok(|a| a))
            })
            .flatten_ok()
            .map(|a| a.and_then(|a| a))
            .try_collect()?;
        let procedure_id = Guid::new();
        for action_id in tasks {
            if let Some(input) = self
                .get_action_input(procedure_id.clone(), action_id.clone(), Value::Null)
                .await
                .log_err()
                .flatten()
                .and_then(|i| i.value)
            {
                action_input.insert(action_id, input);
            }
        }
        self.seed
            .ctx
            .db
            .mutate(|db| {
                for (action_id, input) in &action_input {
                    for (_, pde) in db.as_public_mut().as_package_data_mut().as_entries_mut()? {
                        pde.as_tasks_mut().mutate(|tasks| {
                            Ok(update_tasks(tasks, service_id, action_id, input, false))
                        })?;
                    }
                }
                for (_, pde) in db.as_public_mut().as_package_data_mut().as_entries_mut()? {
                    if pde
                        .as_tasks()
                        .de()?
                        .into_iter()
                        .any(|(_, t)| t.active && t.task.severity == TaskSeverity::Critical)
                    {
                        pde.as_status_info_mut().stop()?;
                    }
                }
                Ok(())
            })
            .await
            .result?;
        Ok(())
    }

    #[instrument(skip_all)]
    async fn new(
        ctx: RpcContext,
        s9pk: S9pk,
        procedure_id: Guid,
        init_kind: Option<InitKind>,
        recovery_source: Option<impl GenericMountGuard>,
        init_progress: Option<crate::progress::PhaseProgressTrackerHandle>,
    ) -> Result<ServiceRef, Error> {
        let id = s9pk.as_manifest().id.clone();
        ctx.db
            .mutate(|db| {
                db.as_public_mut()
                    .as_package_data_mut()
                    .as_idx_mut(&id)
                    .or_not_found(&id)?
                    .as_status_info_mut()
                    .init()
            })
            .await
            .result?;
        let persistent_container = PersistentContainer::new(&ctx, s9pk).await?;
        let seed = Arc::new(ServiceActorSeed {
            id,
            persistent_container,
            ctx,
            backup: SyncMutex::new(None),
            backup_phase: SyncMutex::new(None),
            init_phase: SyncMutex::new(init_progress),
        });
        let service: ServiceRef = Self {
            actor: ConcurrentActor::new(ServiceActor(seed.clone())),
            seed,
        }
        .into();
        let recovery_guard = if let Some(recovery_source) = &recovery_source {
            Some(
                service
                    .seed
                    .persistent_container
                    .mount_backup(recovery_source.path().join("data"), ReadOnly)
                    .await?,
            )
        } else {
            None
        };
        service
            .seed
            .persistent_container
            .init(service.weak(), procedure_id, init_kind)
            .await?;
        service.recheck_tasks().await?;
        if let Some(recovery_guard) = recovery_guard {
            recovery_guard.unmount(true).await?;
        }
        if let Some(recovery_source) = recovery_source {
            recovery_source.unmount().await?;
        }
        Ok(service)
    }

    #[instrument(skip_all)]
    pub async fn load(
        ctx: &RpcContext,
        id: &PackageId,
        disposition: LoadDisposition,
    ) -> Result<Option<ServiceRef>, Error> {
        let handle_installed = {
            let ctx = ctx.clone();
            move |s9pk: S9pk| async move {
                for volume_id in &s9pk.as_manifest().volumes {
                    let path = data_dir(DATA_DIR, &s9pk.as_manifest().id, volume_id);
                    if tokio::fs::metadata(&path).await.is_err() {
                        tokio::fs::create_dir_all(&path).await?;
                    }
                }
                Self::new(ctx, s9pk, Guid::new(), None, None::<MountGuard>, None)
                    .await
                    .map(Some)
            }
        };
        let Some(entry) = ctx
            .db
            .peek()
            .await
            .into_public()
            .into_package_data()
            .into_idx(id)
        else {
            return Ok(None);
        };
        let s9pk_path = entry.as_s9pk().de()?;
        let state = entry.as_state_info().as_match();
        match state {
            PackageStateMatchModelRef::Installing(_) => {
                if disposition == LoadDisposition::Retry {
                    if let Ok(s9pk) = S9pk::open(&s9pk_path, Some(id)).await.map_err(|e| {
                        tracing::error!("Error opening s9pk for install: {e}");
                        tracing::debug!("{e:?}")
                    }) {
                        let init_kind = if get_data_version(id).await.ok().flatten().is_some() {
                            InitKind::Update
                        } else {
                            InitKind::Install
                        };
                        if let Ok(service) = Self::install(
                            ctx.clone(),
                            s9pk,
                            &s9pk_path,
                            &None,
                            init_kind,
                            None::<Never>,
                            None,
                        )
                        .await
                        .map_err(|e| {
                            tracing::error!("Error installing service: {e}");
                            tracing::debug!("{e:?}")
                        }) {
                            crate::volume::remove_install_backup(id).await.log_err();
                            return Ok(Some(service));
                        }
                    }
                }
                cleanup(ctx, id, false).await.log_err();
                crate::volume::restore_volumes_from_install_backup(id)
                    .await
                    .log_err();
                ctx.db
                    .mutate(|v| v.as_public_mut().as_package_data_mut().remove(id))
                    .await
                    .result?;
                Ok(None)
            }
            PackageStateMatchModelRef::Updating(s) => {
                let new_s9pk = s.as_s9pk().de()?;
                if disposition == LoadDisposition::Retry
                    && tokio::fs::metadata(&new_s9pk).await.is_ok()
                {
                    if let Ok(s9pk) = S9pk::open(&new_s9pk, Some(id)).await.map_err(|e| {
                        tracing::error!("Error opening s9pk for update: {e}");
                        tracing::debug!("{e:?}")
                    }) {
                        let init_kind = if get_data_version(id).await.ok().flatten().is_some() {
                            InitKind::Update
                        } else {
                            InitKind::Install
                        };
                        if let Ok(service) = Self::install(
                            ctx.clone(),
                            s9pk,
                            &s9pk_path,
                            &None,
                            init_kind,
                            None::<Never>,
                            None,
                        )
                        .await
                        .map_err(|e| {
                            tracing::error!("Error installing service: {e}");
                            tracing::debug!("{e:?}")
                        }) {
                            crate::volume::remove_install_backup(id).await.log_err();
                            return Ok(Some(service));
                        }
                    }
                }
                match async {
                    let s9pk = S9pk::open(s9pk_path, Some(id)).await?;
                    ctx.db
                        .mutate({
                            |db| {
                                db.as_public_mut()
                                    .as_package_data_mut()
                                    .as_idx_mut(id)
                                    .or_not_found(id)?
                                    .as_state_info_mut()
                                    .map_mutate(|s| {
                                        if let PackageState::Updating(UpdatingState {
                                            manifest,
                                            ..
                                        }) = s
                                        {
                                            Ok(PackageState::Installed(InstalledState { manifest }))
                                        } else {
                                            Err(Error::new(
                                                eyre!(
                                                    "{}",
                                                    t!("service.mod.race-condition-detected")
                                                ),
                                                ErrorKind::Database,
                                            ))
                                        }
                                    })
                            }
                        })
                        .await
                        .result?;
                    // Roll the filesystem back before reloading the old service, so its
                    // init sees old-version data instead of an impossible new->old migration.
                    crate::volume::restore_volumes_from_install_backup(id)
                        .await
                        .log_err();
                    handle_installed(s9pk).await
                }
                .await
                {
                    Ok(service) => {
                        crate::volume::remove_install_backup(id).await.log_err();
                        Ok(service)
                    }
                    Err(e) => {
                        tracing::error!("Update rollback failed for {id}: {e}");
                        Err(e)
                    }
                }
            }
            PackageStateMatchModelRef::Removing(_) | PackageStateMatchModelRef::Restoring(_) => {
                if let Ok(s9pk) = S9pk::open(s9pk_path, Some(id)).await.map_err(|e| {
                    tracing::error!("Error opening s9pk for removal: {e}");
                    tracing::debug!("{e:?}")
                }) {
                    let err_state = |e: Error| async move {
                        let e = e.into();
                        ctx.db
                            .mutate(move |db| {
                                if let Some(pde) =
                                    db.as_public_mut().as_package_data_mut().as_idx_mut(&id)
                                {
                                    pde.as_state_info_mut().map_mutate(|s| {
                                        Ok(PackageState::Installed(InstalledState {
                                            manifest: s
                                                .as_manifest(ManifestPreference::Old)
                                                .clone(),
                                        }))
                                    })?;
                                    pde.as_status_info_mut().as_error_mut().ser(&Some(e))?;
                                }
                                Ok(())
                            })
                            .await
                            .result
                    };
                    match Self::new(
                        ctx.clone(),
                        s9pk,
                        Guid::new(),
                        None,
                        None::<MountGuard>,
                        None,
                    )
                    .await
                    {
                        Ok(service) => match async {
                            service
                                .uninstall(ExitParams::uninstall(), false, false)
                                .await?
                                .await
                        }
                        .await
                        {
                            Err(e) => {
                                tracing::error!("Error uninstalling service: {e}");
                                tracing::debug!("{e:?}");
                                err_state(e).await?;
                            }
                            Ok(()) => return Ok(None),
                        },
                        Err(e) => {
                            tracing::error!("Error loading service for removal: {e}");
                            tracing::debug!("{e:?}");
                            err_state(e).await?;
                        }
                    }
                }

                if disposition == LoadDisposition::Retry
                    || matches!(state, PackageStateMatchModelRef::Restoring(_))
                {
                    cleanup(ctx, id, false).await?;
                }

                Ok(None)
            }
            PackageStateMatchModelRef::Installed(_) => {
                handle_installed(S9pk::open(s9pk_path, Some(id)).await?).await
            }
            PackageStateMatchModelRef::Error(e) => Err(Error::new(
                eyre!(
                    "{}",
                    t!(
                        "service.mod.failed-to-parse-package-data-entry",
                        error = format!("{e:?}")
                    )
                ),
                ErrorKind::Deserialization,
            )),
        }
    }

    #[instrument(skip_all)]
    pub async fn install(
        ctx: RpcContext,
        s9pk: S9pk,
        s9pk_path: &PathBuf,
        registry: &Option<Url>,
        kind: InitKind,
        recovery_source: Option<impl GenericMountGuard>,
        progress: Option<InstallProgressHandles>,
    ) -> Result<ServiceRef, Error> {
        let manifest = s9pk.as_manifest().clone();
        let developer_key = s9pk.as_archive().signer();
        let icon = s9pk.icon_data_url().await?;
        let procedure_id = Guid::new();
        let (finalization_progress, overall_progress) = match progress {
            Some(InstallProgressHandles {
                finalization_progress,
                progress,
            }) => (Some(finalization_progress), Some(progress)),
            None => (None, None),
        };
        let service = Self::new(
            ctx.clone(),
            s9pk,
            procedure_id.clone(),
            Some(kind),
            recovery_source,
            finalization_progress,
        )
        .await?;

        ctx.db
            .mutate(|db| {
                let entry = db
                    .as_public_mut()
                    .as_package_data_mut()
                    .as_idx_mut(&manifest.id)
                    .or_not_found(&manifest.id)?;
                let actions = entry.as_actions().keys()?;
                if entry.as_tasks_mut().mutate(|t| {
                    t.retain(|id, v| {
                        v.task.package_id != manifest.id
                            || if actions.contains(&v.task.action_id) {
                                true
                            } else {
                                tracing::warn!(
                                    "{}",
                                    t!(
                                        "service.mod.deleting-task-action-no-longer-exists",
                                        id = id
                                    )
                                );
                                false
                            }
                    });
                    Ok(t.iter()
                        .any(|(_, t)| t.active && t.task.severity == TaskSeverity::Critical))
                })? {
                    entry.as_status_info_mut().stop()?;
                }
                entry
                    .as_state_info_mut()
                    .ser(&PackageState::Installed(InstalledState { manifest }))?;
                entry.as_s9pk_mut().ser(s9pk_path)?;
                entry.as_developer_key_mut().ser(&Pem::new(developer_key))?;
                entry.as_icon_mut().ser(&icon)?;
                entry.as_registry_mut().ser(registry)?;
                entry.as_status_info_mut().as_error_mut().ser(&None)?;

                Ok(())
            })
            .await
            .result?;

        if let Some(mut finalization_progress) = service.seed.init_phase.replace(None) {
            finalization_progress.complete();
        }
        if let Some(overall_progress) = overall_progress {
            overall_progress.complete();
            tokio::task::yield_now().await;
        }

        Ok(service)
    }

    #[instrument(skip_all)]
    pub async fn backup(
        &self,
        guard: impl GenericMountGuard,
        progress: crate::progress::PhaseProgressTrackerHandle,
    ) -> Result<(), Error> {
        let id = &self.seed.id;
        // Prepare the backup future in the actor first, so the cell is
        // populated before the actor reacts to the DesiredStatus change.
        let backup = self
            .actor
            .send(
                Guid::new(),
                transition::backup::Backup {
                    path: guard.path().join("data"),
                    progress,
                },
            )
            .await??;
        // Set status early so the UI reflects "backing up" while the s9pk
        // serialization and service stop happen concurrently.
        self.seed
            .ctx
            .db
            .mutate(|db| {
                db.as_public_mut()
                    .as_package_data_mut()
                    .as_idx_mut(id)
                    .or_not_found(id)?
                    .as_status_info_mut()
                    .as_desired_mut()
                    .map_mutate(|s| Ok(s.backing_up()))
            })
            .await
            .result?;
        let s9pk_res = async {
            let mut file =
                AtomicFile::new(guard.path().join(id).with_extension("s9pk"), None::<&str>).await?;
            self.seed
                .persistent_container
                .s9pk
                .clone()
                .serialize(&mut *file, true)
                .await?;
            file.save().await
        }
        .await;

        // Awaiting the handle reads the backup result but does not drive it —
        // the actor drives the backup once the service has stopped, and the
        // handle doesn't resolve until the service has left the backing-up
        // state. So the backup never starts before the service is stopped, and
        // this call doesn't return until the backing-up transition is complete.
        let backup_res = backup.await;

        // Complete the phase only now — after the s9pk write — so a package
        // isn't reported done while its image is still streaming to the target.
        if let Some(mut handle) = self.seed.backup_phase.replace(None) {
            handle.complete();
        }

        backup_res?;
        s9pk_res
    }

    pub fn container_id(&self) -> Result<ContainerId, Error> {
        let id = &self.seed.id;
        let container_id = (*self
            .seed
            .persistent_container
            .lxc_container
            .get()
            .or_not_found(format!("container for {id}"))?
            .guid)
            .clone();
        Ok(container_id)
    }
    #[instrument(skip_all)]
    pub async fn stats(&self) -> Result<ServiceStats, Error> {
        let container = &self.seed.persistent_container;
        let lxc_container = container.lxc_container.get().or_not_found("container")?;
        let (total, used) = lxc_container
            .command(&["free", "-m"])
            .await?
            .split("\n")
            .map(|x| x.split_whitespace().collect::<Vec<_>>())
            .skip(1)
            .filter_map(|x| {
                Some((
                    x.get(1)?.parse::<u64>().ok()?,
                    x.get(2)?.parse::<u64>().ok()?,
                ))
            })
            .fold((0, 0), |acc, (total, used)| (acc.0 + total, acc.1 + used));
        Ok(ServiceStats {
            container_id: lxc_container.guid.clone(),
            memory_limit: MiB::from_MiB(total),
            memory_usage: MiB::from_MiB(used),
        })
    }
}

struct ServiceActorSeed {
    ctx: RpcContext,
    id: PackageId,
    /// Needed to interact with the container for the service
    persistent_container: PersistentContainer,
    /// The backup work, stored for the actor to drive once the service has
    /// stopped. Its result is delivered to the caller through the paired
    /// `RemoteHandle` (see `transition::backup`), so this half only needs to be
    /// driven to completion.
    backup: SyncMutex<Option<BoxFuture<'static, ()>>>,
    /// Set while a backup procedure is running so the service container can
    /// stream progress updates back via the `setBackupProgress` effect.
    backup_phase: SyncMutex<Option<crate::progress::PhaseProgressTrackerHandle>>,
    /// Set while the init procedure is running so the service container can
    /// stream progress updates back via the `setInitProgress` effect. Routed
    /// to the install/update finalization phase of the install progress.
    init_phase: SyncMutex<Option<crate::progress::PhaseProgressTrackerHandle>>,
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
pub struct RebuildParams {
    #[arg(help = "help.arg.package-id")]
    pub id: PackageId,
}
pub async fn rebuild(ctx: RpcContext, RebuildParams { id }: RebuildParams) -> Result<(), Error> {
    ctx.services.load(&ctx, &id, LoadDisposition::Retry).await?;
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SubcontainerInfo {
    pub id: Guid,
    pub name: InternedString,
    pub image_id: ImageId,
}
impl std::fmt::Display for SubcontainerInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let SubcontainerInfo { id, name, image_id } = self;
        write!(f, "{id} => Name: {name}; Image: {image_id}")
    }
}

#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachParams {
    pub id: PackageId,
    #[ts(type = "string[]")]
    pub command: Vec<OsString>,
    pub tty: bool,
    pub stderr_tty: bool,
    pub pty_size: Option<TermSize>,
    #[ts(skip)]
    #[serde(rename = "__Auth_signer")]
    signer: Option<InternedString>,
    #[ts(type = "string | null")]
    subcontainer: Option<Guid>,
    #[ts(type = "string | null")]
    name: Option<InternedString>,
    #[ts(type = "string | null")]
    image_id: Option<ImageId>,
    #[ts(type = "string | null")]
    user: Option<InternedString>,
}
pub async fn attach(
    ctx: RpcContext,
    AttachParams {
        id,
        command,
        tty,
        stderr_tty,
        pty_size,
        signer,
        subcontainer,
        image_id,
        name,
        user,
    }: AttachParams,
) -> Result<Guid, Error> {
    let (container_id, subcontainer_id, image_id, user, workdir, root_command) = {
        let id = &id;

        let service = ctx.services.get(id).await;

        let service_ref = service.as_ref().or_not_found(id)?;

        let container = &service_ref.seed.persistent_container;
        let root_dir = container
            .lxc_container
            .get()
            .map(|x| x.rootfs_dir().to_owned())
            .or_not_found(format!("container for {id}"))?;

        let subcontainer = subcontainer.map(|x| AsRef::<str>::as_ref(&x).to_uppercase());
        let name = name.map(|x| AsRef::<str>::as_ref(&x).to_uppercase());
        let image_id = image_id.map(|x| AsRef::<Path>::as_ref(&x).to_string_lossy().to_uppercase());

        let subcontainers = container.subcontainers.lock().await;
        let subcontainer_ids: Vec<_> = subcontainers
            .iter()
            .filter(|(x, wrapper)| {
                if let Some(subcontainer) = subcontainer.as_ref() {
                    AsRef::<str>::as_ref(x).contains(AsRef::<str>::as_ref(subcontainer))
                } else if let Some(name) = name.as_ref() {
                    AsRef::<str>::as_ref(&wrapper.name)
                        .to_uppercase()
                        .contains(AsRef::<str>::as_ref(name))
                } else if let Some(image_id) = image_id.as_ref() {
                    let Some(wrapper_image_id) = AsRef::<Path>::as_ref(&wrapper.image_id).to_str()
                    else {
                        return false;
                    };
                    wrapper_image_id
                        .to_uppercase()
                        .contains(AsRef::<str>::as_ref(&image_id))
                } else {
                    true
                }
            })
            .collect();
        let Some((subcontainer_id, image_id)) = subcontainer_ids
            .first()
            .map::<(Guid, ImageId), _>(|&x| (x.0.clone(), x.1.image_id.clone()))
        else {
            drop(subcontainers);
            let subcontainers = container
                .subcontainers
                .lock()
                .await
                .iter()
                .map(|(g, s)| SubcontainerInfo {
                    id: g.clone(),
                    name: s.name.clone(),
                    image_id: s.image_id.clone(),
                })
                .collect::<Vec<_>>();
            return Err(Error::new(
                eyre!("{}", t!("service.mod.no-matching-subcontainers", id = id)),
                ErrorKind::NotFound,
            )
            .with_info(to_value(&subcontainers)?));
        };

        let passwd = root_dir
            .join("media/startos/subcontainers")
            .join(subcontainer_id.as_ref())
            .join("etc")
            .join("passwd");

        let image_meta = serde_json::from_str::<Value>(
            &tokio::fs::read_to_string(
                root_dir
                    .join("media/startos/images/")
                    .join(&image_id)
                    .with_extension("json"),
            )
            .await?,
        )
        .with_kind(ErrorKind::Deserialization)?;

        let user = user
            .clone()
            .or_else(|| image_meta["user"].as_str().map(InternedString::intern))
            .unwrap_or_else(|| InternedString::intern("root"));

        let root_command = get_passwd_command(passwd, &*user).await;

        let workdir = image_meta["workdir"].as_str().map(|s| s.to_owned());

        if subcontainer_ids.len() > 1 {
            let subcontainers = subcontainer_ids
                .into_iter()
                .map(|(g, s)| SubcontainerInfo {
                    id: g.clone(),
                    name: s.name.clone(),
                    image_id: s.image_id.clone(),
                })
                .collect::<Vec<_>>();
            return Err(Error::new(
                eyre!(
                    "{}",
                    t!("service.mod.multiple-subcontainers-found", id = id,)
                ),
                ErrorKind::InvalidRequest,
            )
            .with_info(to_value(&subcontainers)?));
        }

        (
            service_ref.container_id()?,
            subcontainer_id,
            image_id,
            user.into(),
            workdir,
            root_command,
        )
    };

    let guid = Guid::new();
    async fn handler(
        ws: &mut WebSocket,
        container_id: ContainerId,
        subcontainer_id: Guid,
        command: Vec<OsString>,
        tty: bool,
        stderr_tty: bool,
        pty_size: Option<TermSize>,
        image_id: ImageId,
        workdir: Option<String>,
        user: InternedString,
        root_command: &RootCommand,
    ) -> Result<(), Error> {
        use axum::extract::ws::Message;

        let mut ws = ws.fuse();

        let mut cmd = Command::new("lxc-attach");
        let root_path = Path::new("/media/startos/subcontainers").join(subcontainer_id.as_ref());
        cmd.kill_on_drop(true);

        cmd.arg(&*container_id)
            .arg("--")
            .arg("start-container")
            .arg("subcontainer")
            .arg("exec")
            .arg("--env-file")
            .arg(
                Path::new("/media/startos/images")
                    .join(image_id)
                    .with_extension("env"),
            )
            .arg("--user")
            .arg(&*user);

        if let Some(workdir) = workdir {
            cmd.arg("--workdir").arg(workdir);
        }

        if tty {
            cmd.arg("--force-tty");
        }

        if stderr_tty {
            cmd.arg("--force-stderr-tty");
        }

        if let Some(pty_size) = pty_size {
            cmd.arg(format!("--pty-size={pty_size}"));
        }

        cmd.arg(&root_path).arg("--");

        if command.is_empty() {
            cmd.arg(&root_command.0);
        } else {
            cmd.args(&command);
        }

        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let pid = nix::unistd::Pid::from_raw(child.id().or_not_found("child pid")? as i32);

        let mut stdin = Some(child.stdin.take().or_not_found("child stdin")?);

        let mut current_in: Utf8Bytes = "stdin".into();
        let mut current_out = "stdout";
        ws.send(Message::Text(current_out.into()))
            .await
            .with_kind(ErrorKind::Network)?;
        let mut stdout = AsyncReadStream::new(
            child.stdout.take().or_not_found("child stdout")?,
            4 * CAP_1_KiB,
        )
        .fuse();
        let mut stderr = AsyncReadStream::new(
            child.stderr.take().or_not_found("child stderr")?,
            4 * CAP_1_KiB,
        )
        .fuse();

        loop {
            futures::select_biased! {
                out = stdout.try_next() => {
                    if let Some(out) = out? {
                        if current_out != "stdout" {
                            ws.send(Message::Text("stdout".into()))
                                .await
                                .with_kind(ErrorKind::Network)?;
                            current_out = "stdout";
                        }
                        ws.send(Message::Binary(out.into()))
                            .await
                            .with_kind(ErrorKind::Network)?;
                    } else {
                        ws.send(Message::Text("close-stdout".into()))
                                .await
                                .with_kind(ErrorKind::Network)?;
                    }
                }
                err = stderr.try_next() => {
                    if let Some(err) = err? {
                        if current_out != "stderr" {
                            ws.send(Message::Text("stderr".into()))
                                .await
                                .with_kind(ErrorKind::Network)?;
                            current_out = "stderr";
                        }
                        ws.send(Message::Binary(err.into()))
                            .await
                            .with_kind(ErrorKind::Network)?;
                    } else {
                        ws.send(Message::Text("close-stderr".into()))
                                .await
                                .with_kind(ErrorKind::Network)?;
                    }
                }
                msg = ws.try_next() => {
                    if let Some(msg) = msg.with_kind(ErrorKind::Network)? {
                        match msg {
                            Message::Text(in_ty) => {
                                current_in = in_ty;
                            }
                            Message::Binary(data) => {
                                match &*current_in {
                                    "stdin" => {
                                        if let Some(stdin) = &mut stdin {
                                            stdin.write_all(&data).await?;
                                        }
                                    }
                                    "close-stdin" => {
                                        stdin.take();
                                    }
                                    "signal" => {
                                        if data.len() != 4 {
                                            return Err(Error::new(
                                                eyre!("{}", t!("service.mod.invalid-byte-length-for-signal", length = data.len())),
                                                ErrorKind::InvalidRequest
                                            ));
                                        }
                                        let mut sig_buf = [0u8; 4];
                                        sig_buf.clone_from_slice(&data);
                                        nix::sys::signal::kill(
                                            pid,
                                            Signal::try_from(i32::from_be_bytes(sig_buf))
                                                .with_kind(ErrorKind::InvalidRequest)?
                                        ).with_kind(ErrorKind::Filesystem)?;
                                    }
                                    _ => (),
                                }
                            }
                            _ => ()
                        }
                    } else {
                        return Ok(())
                    }
                }
            }
            if stdout.is_terminated() && stderr.is_terminated() {
                break;
            }
        }

        let exit = child.wait().await?;
        ws.send(Message::Text("exit".into()))
            .await
            .with_kind(ErrorKind::Network)?;
        ws.send(Message::Binary(
            i32::to_be_bytes(exit.into_raw()).to_vec().into(),
        ))
        .await
        .with_kind(ErrorKind::Network)?;

        Ok(())
    }
    ctx.rpc_continuations
        .add(
            guid.clone(),
            RpcContinuation::ws_authed(
                &ctx,
                signer,
                move |mut ws| async move {
                    if let Err(e) = handler(
                        &mut ws,
                        container_id,
                        subcontainer_id,
                        command,
                        tty,
                        stderr_tty,
                        pty_size,
                        image_id,
                        workdir,
                        user,
                        &root_command,
                    )
                    .await
                    {
                        if !crate::util::net::is_ws_reset_without_close(&e) {
                            tracing::error!("Error in attach websocket: {e}");
                            tracing::debug!("{e:?}");
                        }
                        ws.close_result(Err::<&str, _>(e)).await.log_err();
                    } else {
                        ws.normal_close("exit").await.log_err();
                    }
                },
                Duration::from_secs(30),
            ),
        )
        .await;

    Ok(guid)
}

async fn get_passwd_command(etc_passwd_path: PathBuf, user: &str) -> RootCommand {
    async {
        let mut file = tokio::fs::File::open(etc_passwd_path).await?;

        let mut contents = vec![];
        file.read_to_end(&mut contents).await?;

        let contents = String::from_utf8_lossy(&contents);

        for line in contents.split('\n') {
            let line_information = line.split(':').collect::<Vec<_>>();
            if let (Some(&u), Some(shell)) = (line_information.first(), line_information.last())
                && u == user
            {
                return Ok(shell.to_string());
            }
        }
        Err(Error::new(
            eyre!(
                "{}",
                t!(
                    "service.mod.could-not-parse-etc-passwd",
                    contents = contents
                )
            ),
            ErrorKind::Filesystem,
        ))
    }
    .await
    .map(RootCommand)
    .unwrap_or_else(|e| {
        tracing::error!("{}", t!("service.mod.could-not-get-etc-passwd", error = e));
        tracing::debug!("{e:?}");
        RootCommand("/bin/sh".to_string())
    })
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
pub struct CliAttachParams {
    #[arg(help = "help.arg.package-id")]
    pub id: PackageId,
    #[arg(long, help = "help.arg.force-tty")]
    pub force_tty: bool,
    #[arg(trailing_var_arg = true, help = "help.arg.command-to-execute")]
    pub command: Vec<OsString>,
    #[arg(long, short, help = "help.arg.subcontainer-name")]
    subcontainer: Option<InternedString>,
    #[arg(long, short, help = "help.arg.container-name")]
    name: Option<InternedString>,
    #[arg(long, short, help = "help.arg.user-name")]
    user: Option<InternedString>,
    #[arg(long, short, help = "help.arg.image-id")]
    image_id: Option<ImageId>,
}
#[instrument[skip_all]]
pub async fn cli_attach(
    HandlerArgs {
        context,
        parent_method,
        method,
        params,
        ..
    }: HandlerArgs<CliContext, CliAttachParams>,
) -> Result<(), Error> {
    use std::io::Write;

    use tokio_tungstenite::tungstenite::Message;

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();

    let tty = params.force_tty || (stdin.is_terminal() && stdout.is_terminal());

    let raw = if stdin.is_terminal() && stdout.is_terminal() {
        Some(termion::get_tty()?.into_raw_mode()?)
    } else {
        None
    };

    let method = parent_method.into_iter().chain(method).join(".");
    let mut params = json!({
        "id": params.id,
        "command": params.command,
        "tty": tty,
        "stderrTty": stderr.is_terminal(),
        "ptySize": if tty { TermSize::get_current() } else { None },
        "subcontainer": params.subcontainer,
        "imageId": params.image_id,
        "name": params.name,
        "user": params.user,
    });
    let guid: Guid = from_value(
        match context
            .call_remote::<RpcContext>(&method, params.clone())
            .await
        {
            Ok(a) => a,
            Err(e) => {
                if e.kind != ErrorKind::InvalidRequest {
                    return Err(e);
                }
                let prompt = e.to_string();
                let options: Vec<SubcontainerInfo> = from_value(e.info)?;
                let choice = choose(&prompt, &options).await?;
                params["subcontainer"] = to_value(&choice.id)?;
                context
                    .call_remote::<RpcContext>(&method, params.clone())
                    .await?
            }
        },
    )?;
    let mut ws = context.ws_continuation(guid).await?;

    print!("\r");
    let (kill, thread_kill) = tokio::sync::oneshot::channel();
    let (thread_send, recv) = tokio::sync::mpsc::channel(4 * CAP_1_KiB);
    let stdin_thread: NonDetachingJoinHandle<()> = tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let mut stdin = stdin.lock().bytes();

        while thread_kill.is_empty() {
            if let Some(b) = stdin.next() {
                if let Err(_) = thread_send.blocking_send(b) {
                    break;
                }
            } else {
                break;
            }
        }
    })
    .into();
    let mut stdin = Some(recv);

    let mut current_in = "stdin";
    let mut current_out: tokio_tungstenite::tungstenite::Utf8Bytes = "stdout".into();
    ws.send(Message::Text(current_in.into()))
        .await
        .with_kind(ErrorKind::Network)?;
    let mut stdout = Some(stdout);
    let mut stderr = Some(stderr);
    loop {
        futures::select_biased! {
            input = stdin.as_mut().map_or(
                futures::future::Either::Left(futures::future::pending()),
                |s| futures::future::Either::Right(s.recv())
            ).fuse() => {
                if let (Some(input), Some(stdin)) = (input.transpose()?, &mut stdin) {
                    let mut input = vec![input];
                    while let Ok(b) = stdin.try_recv() {
                        input.push(b?);
                    }
                    if current_in != "stdin" {
                        ws.send(Message::Text("stdin".into()))
                            .await
                            .with_kind(ErrorKind::Network)?;
                        current_in = "stdin";
                    }
                    ws.send(Message::Binary(input.into()))
                        .await
                        .with_kind(ErrorKind::Network)?;
                } else {
                    ws.send(Message::Text("close-stdin".into()))
                        .await
                        .with_kind(ErrorKind::Network)?;
                    stdin.take();
                }
            }
            msg = ws.try_next() => {
                if let Some(msg) = msg.with_kind(ErrorKind::Network)? {
                    match msg {
                        Message::Text(out_ty) => {
                            current_out = out_ty;
                        }
                        Message::Binary(data) => {
                            match &*current_out {
                                "stdout" => {
                                    if let Some(stdout) = &mut stdout {
                                        stdout.write_all(&data)?;
                                        stdout.flush()?;
                                    }
                                }
                                "stderr" => {
                                    if let Some(stderr) = &mut stderr {
                                        stderr.write_all(&data)?;
                                        stderr.flush()?;
                                    }
                                }
                                "close-stdout" => {
                                    stdout.take();
                                }
                                "close-stderr" => {
                                    stderr.take();
                                }
                                "exit" => {
                                    if data.len() != 4 {
                                        return Err(Error::new(
                                            eyre!("{}", t!("service.mod.invalid-byte-length-for-exit-code", length = data.len())),
                                            ErrorKind::InvalidRequest
                                        ));
                                    }
                                    let mut exit_buf = [0u8; 4];
                                    exit_buf.clone_from_slice(&data);
                                    let code = i32::from_be_bytes(exit_buf);
                                    drop(raw);
                                    std::process::exit(code);
                                }
                                _ => (),
                            }
                        }
                        Message::Close(Some(close)) => {
                            if close.code != CloseCode::Normal {
                                return Err(Error::new(
                                    color_eyre::eyre::Report::msg(close.reason),
                                    ErrorKind::Network
                                ));
                            }
                        }
                        _ => ()
                    }
                } else {
                    kill.send(()).ok();
                    stdin_thread.wait_for_abort().await.log_err();
                    return Ok(())
                }
            }
        }
    }
}
