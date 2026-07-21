use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use color_eyre::eyre::eyre;
use exver::VersionRange;
use futures::future::{BoxFuture, Fuse};
use futures::{Future, FutureExt, StreamExt, TryFutureExt, stream};
use imbl::OrdMap;
use tokio::io::AsyncSeekExt;
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock, oneshot};
use tracing::instrument;
use url::Url;

use crate::DATA_DIR;
use crate::context::RpcContext;
use crate::db::model::package::{
    InstallingInfo, InstallingState, PackageDataEntry, PackageState, UpdatingState,
};
use crate::disk::mount::guard::GenericMountGuard;
use crate::error::ErrorData;
use crate::install::PKG_ARCHIVE_DIR;
use crate::notifications::{NotificationLevel, notify};
use crate::prelude::*;
use crate::progress::{FullProgressTracker, PhaseProgressTrackerHandle, ProgressTrackerWriter};
use crate::s9pk::S9pk;
use crate::s9pk::manifest::PackageId;
use crate::s9pk::merkle_archive::source::FileSource;
use crate::service::rpc::{ExitParams, InitKind};
use crate::service::{LoadDisposition, Service, ServiceRef, get_data_version};
use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;
use crate::status::StatusInfo;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::serde::{Base32, Pem};
use crate::util::sync::SyncMutex;

const SERVICE_INIT_CONCURRENCY: usize = 4;

pub type DownloadInstallFuture = BoxFuture<'static, Result<InstallFuture, Error>>;
pub type InstallFuture = BoxFuture<'static, Result<(), Error>>;

pub struct InstallProgressHandles {
    pub finalization_progress: PhaseProgressTrackerHandle,
    pub progress: FullProgressTracker,
}

fn s9pk_download_path(commitment: &MerkleArchiveCommitment) -> PathBuf {
    Path::new(DATA_DIR)
        .join(PKG_ARCHIVE_DIR)
        .join("downloading")
        .join(Base32(commitment.root_sighash.0).to_lower_string())
        .with_extension("s9pk")
}

fn s9pk_installed_path(commitment: &MerkleArchiveCommitment) -> PathBuf {
    Path::new(DATA_DIR)
        .join(PKG_ARCHIVE_DIR)
        .join("installed")
        .join(Base32(commitment.root_sighash.0).to_lower_string())
        .with_extension("s9pk")
}

/// This is the structure to contain all the services
#[derive(Default)]
pub struct ServiceMap(SyncMutex<OrdMap<PackageId, Arc<RwLock<Option<ServiceRef>>>>>);
impl ServiceMap {
    fn entry(&self, id: &PackageId) -> Arc<RwLock<Option<ServiceRef>>> {
        self.0.mutate(|lock| {
            lock.entry(id.clone())
                .or_insert_with(|| Arc::new(RwLock::new(None)))
                .clone()
        })
    }

    #[instrument(skip_all)]
    pub fn try_get(&self, id: &PackageId) -> Option<OwnedRwLockReadGuard<Option<ServiceRef>>> {
        self.entry(id).try_read_owned().ok()
    }

    #[instrument(skip_all)]
    pub async fn get(&self, id: &PackageId) -> OwnedRwLockReadGuard<Option<ServiceRef>> {
        self.entry(id).read_owned().await
    }

    #[instrument(skip_all)]
    pub async fn get_mut(&self, id: &PackageId) -> OwnedRwLockWriteGuard<Option<ServiceRef>> {
        self.entry(id).write_owned().await
    }

    #[instrument(skip_all)]
    pub async fn init(&self, ctx: &RpcContext) -> Result<(), Error> {
        let ids = ctx.db.peek().await.as_public().as_package_data().keys()?;
        stream::iter(&ids)
            .for_each_concurrent(Some(SERVICE_INIT_CONCURRENCY), |id| async move {
                if let Err(e) = self.load(ctx, id, LoadDisposition::Retry).await {
                    tracing::error!("Error loading installed package as service: {e}");
                    tracing::debug!("{e:?}");
                }
            })
            .await;
        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn load(
        &self,
        ctx: &RpcContext,
        id: &PackageId,
        disposition: LoadDisposition,
    ) -> Result<(), Error> {
        let mut shutdown_err = Ok(());
        let mut service = self.get_mut(id).await;
        if let Some(service) = service.take() {
            shutdown_err = service.shutdown(None).await;
        }
        match Service::load(ctx, id, disposition).await {
            Ok(s) => {
                ctx.db
                    .mutate(|db| {
                        if let Some(pde) = db.as_public_mut().as_package_data_mut().as_idx_mut(id) {
                            pde.as_status_info_mut().as_error_mut().ser(&None)?;
                        }
                        Ok(())
                    })
                    .await
                    .result?;
                *service = s.into();
            }
            Err(e) => {
                tracing::error!("Error loading service: {e}");
                tracing::debug!("{e:?}");
                let e = ErrorData::from(e);
                ctx.db
                    .mutate(|db| {
                        if let Some(pde) = db.as_public_mut().as_package_data_mut().as_idx_mut(id) {
                            pde.as_status_info_mut().as_error_mut().ser(&Some(e))?;
                        }
                        Ok(())
                    })
                    .await
                    .result?;
            }
        }
        shutdown_err?;
        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn install<F, Fut, S: FileSource + Clone>(
        &self,
        ctx: RpcContext,
        s9pk: F,
        registry: Option<Url>,
        recovery_source: Option<impl GenericMountGuard>,
        progress: Option<FullProgressTracker>,
    ) -> Result<DownloadInstallFuture, Error>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<S9pk<S>, Error>>,
        S: FileSource + Clone,
    {
        let progress = progress.unwrap_or_else(|| FullProgressTracker::new());
        let mut validate_progress = progress.add_phase("Validating Headers".into(), Some(1));
        let mut unpack_progress = progress.add_phase("Unpacking".into(), Some(100));

        let mut s9pk = s9pk().await?;
        validate_progress.start();
        s9pk.validate_and_filter(ctx.s9pk_arch)?;
        validate_progress.complete();
        let commitment = s9pk.as_archive().commitment().await?;
        let mut installed_path = s9pk_installed_path(&commitment);
        while tokio::fs::metadata(&installed_path).await.is_ok() {
            let prev = installed_path.file_stem().unwrap_or_default();
            installed_path.set_file_name(prev.to_string_lossy().into_owned() + "x.s9pk");
            // append an x if already exists to avoid reference counting when reinstalling same s9pk
        }
        let manifest = s9pk.as_manifest().clone();
        let id = manifest.id.clone();
        if id.is_start_os() {
            return Err(Error::new(
                eyre!(
                    "{}",
                    t!("service.service-map.reserved-package-id", id = &*id)
                ),
                ErrorKind::InvalidRequest,
            ));
        }
        let icon = s9pk.icon_data_url().await?;
        let developer_key = s9pk.as_archive().signer();
        let mut service = self.get_mut(&id).await;
        let size = s9pk.size();
        let op_name = if recovery_source.is_none() {
            if service.is_none() {
                "Installing"
            } else {
                "Updating"
            }
        } else {
            "Restoring"
        };
        let mut finalization_progress = progress.add_phase(op_name.into(), Some(50));
        let restoring = recovery_source.is_some();

        let (cancel_send, cancel_recv) = oneshot::channel();
        ctx.cancellable_installs
            .mutate(|c| c.insert(id.clone(), cancel_send));

        let mut reload_guard =
            ServiceRefReloadCancelGuard::new(ctx.clone(), id.clone(), op_name, Some(cancel_recv));

        reload_guard
            .handle(async {
                ctx.db
                    .mutate({
                        let installed_path = installed_path.clone();
                        let manifest = manifest.clone();
                        let id = id.clone();
                        let install_progress = progress.snapshot();
                        let registry = registry.clone();
                        move |db| {
                            if let Some(pde) =
                                db.as_public_mut().as_package_data_mut().as_idx_mut(&id)
                            {
                                let prev = pde.as_state_info().expect_installed()?.de()?;
                                pde.as_state_info_mut().ser(&PackageState::Updating(
                                    UpdatingState {
                                        manifest: prev.manifest,
                                        s9pk: installed_path,
                                        installing_info: InstallingInfo {
                                            new_manifest: manifest,
                                            progress: install_progress,
                                        },
                                    },
                                ))?;
                            } else {
                                let installing = InstallingState {
                                    installing_info: InstallingInfo {
                                        new_manifest: manifest,
                                        progress: install_progress,
                                    },
                                };
                                db.as_public_mut().as_package_data_mut().insert(
                                    &id,
                                    &PackageDataEntry {
                                        state_info: if restoring {
                                            PackageState::Restoring(installing)
                                        } else {
                                            PackageState::Installing(installing)
                                        },
                                        s9pk: installed_path,
                                        status_info: StatusInfo::default(),
                                        registry,
                                        developer_key: Pem::new(developer_key),
                                        icon,
                                        last_backup: None,
                                        current_dependencies: Default::default(),
                                        actions: Default::default(),
                                        tasks: Default::default(),
                                        hosts: Default::default(),
                                        store_exposed_dependents: Default::default(),
                                        outbound_gateway: None,
                                        plugin: Default::default(),
                                    },
                                )?;
                            };
                            Ok(())
                        }
                    })
                    .await
                    .result
            })
            .await?;

        Ok(async move {
            let sync_progress_task = reload_guard
                .handle(async {
                    let download_path = s9pk_download_path(&commitment);

                    let deref_id = id.clone();
                    let sync_progress_task =
                        NonDetachingJoinHandle::from(tokio::spawn(progress.clone().sync_to_db(
                            ctx.db.clone(),
                            move |v| {
                                v.as_public_mut()
                                    .as_package_data_mut()
                                    .as_idx_mut(&deref_id)
                                    .and_then(|e| e.as_state_info_mut().as_installing_info_mut())
                                    .map(|i| i.as_progress_mut())
                            },
                            Some(Duration::from_millis(100)),
                        )));

                    let out = crate::util::io::create_file(&download_path).await?;
                    // `+C` before any extents exist, so fallocate lands nodatacow.
                    crate::util::writeback::set_no_cow(&download_path).await;
                    // Reserve while the phase is still indeterminate, so it shows a spinner
                    // during the reserve (which can stall on fragmented btrfs) not a frozen 0%.
                    unpack_progress.start();
                    if let Some(size) = size {
                        crate::util::writeback::preallocate(out.as_raw_fd(), size)
                            .await
                            .log_err();
                        unpack_progress.set_total(size);
                    }
                    let mut progress_writer = ProgressTrackerWriter::new(
                        crate::util::writeback::PacedWriter::new(out),
                        unpack_progress,
                    );
                    s9pk.serialize(&mut progress_writer, true).await?;
                    let (paced_writer, mut unpack_progress) = progress_writer.into_inner();
                    let mut file = paced_writer.into_inner();
                    // `size` is the pre-filter source size; free the reserved tail that
                    // arch-filtering didn't use.
                    let written = file.stream_position().await?;
                    if let Some(size) = size {
                        crate::util::writeback::release_beyond(file.as_raw_fd(), written, size)
                            .await
                            .log_err();
                    }
                    file.sync_all().await?;

                    crate::util::io::rename(&download_path, &installed_path).await?;

                    unpack_progress.complete();

                    Ok::<_, Error>(sync_progress_task)
                })
                .await?;
            Ok(reload_guard
                .handle_last(async move {
                    finalization_progress.start();
                    let s9pk = S9pk::open(&installed_path, Some(&id)).await?;
                    let data_version = get_data_version(&id).await?;
                    // Snapshot existing volumes before install/update modifies them
                    crate::volume::snapshot_volumes_for_install(&id).await?;
                    let prev = if let Some(service) = service.take() {
                        ensure_code!(
                            recovery_source.is_none(),
                            ErrorKind::InvalidRequest,
                            "cannot restore over existing package"
                        );
                        let uninit = if let Some(ref data_ver) = data_version {
                            let prev_can_migrate_to = &service
                                .seed
                                .persistent_container
                                .s9pk
                                .as_manifest()
                                .can_migrate_to;
                            let next_version = &s9pk.as_manifest().version;
                            let next_can_migrate_from = &s9pk.as_manifest().can_migrate_from;
                            let next_accepts_data_ver = if let Ok(data_ver_ev) =
                                data_ver.parse::<exver::ExtendedVersion>()
                            {
                                data_ver_ev.satisfies(next_can_migrate_from)
                            } else if let Ok(data_ver_range) = data_ver.parse::<VersionRange>() {
                                data_ver_range.intersects(next_can_migrate_from)
                            } else {
                                false
                            };
                            if next_accepts_data_ver {
                                ExitParams::target_str(data_ver)
                            } else if next_version.satisfies(prev_can_migrate_to) {
                                ExitParams::target_version(&s9pk.as_manifest().version)
                            } else {
                                ExitParams::target_range(&VersionRange::and(
                                    prev_can_migrate_to.clone(),
                                    next_can_migrate_from.clone(),
                                ))
                            }
                        } else {
                            ExitParams::target_version(
                                &*service.seed.persistent_container.s9pk.as_manifest().version,
                            )
                        };
                        let cleanup = service.uninstall(uninit, false, false).await?;
                        Some(cleanup)
                    } else {
                        None
                    };
                    let new_service = Service::install(
                        ctx,
                        s9pk,
                        &installed_path,
                        &registry,
                        if recovery_source.is_some() {
                            InitKind::Restore
                        } else if data_version.is_some() {
                            InitKind::Update
                        } else {
                            InitKind::Install
                        },
                        recovery_source,
                        Some(InstallProgressHandles {
                            finalization_progress,
                            progress,
                        }),
                    )
                    .await?;
                    *service = Some(new_service.into());

                    if let Some(cleanup) = prev {
                        cleanup.await?;
                    }

                    crate::volume::remove_install_backup(&id).await.log_err();

                    drop(service);

                    sync_progress_task.await.map_err(|_| {
                        Error::new(eyre!("progress sync task panicked"), ErrorKind::Unknown)
                    })??;
                    Ok(())
                })
                .boxed())
        }
        .boxed())
    }

    /// This is ran during the cleanup, so when we are uninstalling the service
    #[instrument(skip_all)]
    pub async fn uninstall(
        &self,
        ctx: RpcContext,
        id: PackageId,
        soft: bool,
        force: bool,
    ) -> Result<impl Future<Output = Result<(), Error>> + Send + 'static, Error> {
        let mut guard = self.get_mut(&id).await;
        ctx.db
            .mutate(|db| {
                let entry = db
                    .as_public_mut()
                    .as_package_data_mut()
                    .as_idx_mut(&id)
                    .or_not_found(&id)?;
                entry.as_state_info_mut().map_mutate(|s| match s {
                    PackageState::Installed(s) => Ok(PackageState::Removing(s)),
                    _ => Err(Error::new(
                        eyre!("Package {id} is not installed."),
                        crate::ErrorKind::NotFound,
                    )),
                })
            })
            .await
            .result?;
        Ok(async move {
            ServiceRefReloadCancelGuard::new(ctx.clone(), id.clone(), "Uninstall", None)
                .handle_last(async move {
                    if let Some(service) = guard.take() {
                        let res = service
                            .uninstall(ExitParams::uninstall(), soft, force)
                            .await;
                        drop(guard);
                        res?.await
                    } else {
                        if force {
                            super::uninstall::cleanup(&ctx, &id, soft).await?;
                            Ok(())
                        } else {
                            Err(Error::new(
                                eyre!(
                                    "service {id} failed to initialize - cannot remove gracefully"
                                ),
                                ErrorKind::Uninitialized,
                            ))
                        }
                    }
                })
                .await?;

            Ok(())
        }
        .or_else(|e: Error| e.wait().map(Err)))
    }

    pub async fn shutdown_all(&self) -> Result<(), Error> {
        let futs = self.0.mutate(|lock| {
            let mut futs = Vec::with_capacity(lock.len());
            for service in lock.values().cloned() {
                futs.push(async move {
                    if let Some(service) = service.write_owned().await.take() {
                        service.shutdown(None).await?
                    }
                    Ok::<_, Error>(())
                });
            }
            futs
        });
        let mut errors = ErrorCollection::new();
        for res in futures::future::join_all(futs).await {
            errors.handle(res);
        }
        errors.into_result()
    }
}

pub struct ServiceRefReloadCancelGuard(
    Option<ServiceRefReloadInfo>,
    Option<Fuse<oneshot::Receiver<()>>>,
);
impl Drop for ServiceRefReloadCancelGuard {
    fn drop(&mut self) {
        if let Some(info) = self.0.take() {
            tokio::spawn(info.reload(None));
        }
    }
}
impl ServiceRefReloadCancelGuard {
    pub fn new(
        ctx: RpcContext,
        id: PackageId,
        operation: &'static str,
        cancel: Option<oneshot::Receiver<()>>,
    ) -> Self {
        Self(
            Some(ServiceRefReloadInfo { ctx, id, operation }),
            cancel.map(|c| c.fuse()),
        )
    }

    pub async fn handle<T>(
        &mut self,
        operation: impl Future<Output = Result<T, Error>>,
    ) -> Result<T, Error> {
        let res = async {
            if let Some(cancel) = self.1.as_mut() {
                tokio::select! {
                    res = operation => res,
                    _ = cancel => Err(Error::new(eyre!("Operation Cancelled"), ErrorKind::Cancelled)),
                }
            } else {
                operation.await
            }
        }.await;
        match res {
            Ok(a) => Ok(a),
            Err(e) => {
                if let Some(info) = self.0.take() {
                    let task_e = e.clone_output();
                    Err(e.with_task(tokio::spawn(async move {
                        info.reload(Some(task_e)).await.log_err();
                    })))
                } else {
                    Err(e)
                }
            }
        }
    }
    pub async fn handle_last<T>(
        mut self,
        operation: impl Future<Output = Result<T, Error>>,
    ) -> Result<T, Error> {
        let res = self.handle(operation).await;
        self.0.take();
        res
    }
}

struct ServiceRefReloadInfo {
    ctx: RpcContext,
    id: PackageId,
    operation: &'static str,
}
impl ServiceRefReloadInfo {
    async fn reload(self, error: Option<Error>) -> Result<(), Error> {
        self.ctx
            .services
            .load(&self.ctx, &self.id, LoadDisposition::Undo)
            .await?;
        if let Some(error) = error {
            let error_string = error.to_string();
            let title = match self.operation {
                "Installing" => t!("service.service-map.installing-failed"),
                "Updating" => t!("service.service-map.updating-failed"),
                "Restoring" => t!("service.service-map.restoring-failed"),
                "Uninstall" => t!("service.service-map.uninstall-failed"),
                other => t!("service.service-map.operation-failed", operation = other),
            }
            .to_string();
            self.ctx
                .db
                .mutate(|db| {
                    notify(
                        db,
                        Some(self.id.clone()),
                        NotificationLevel::Error,
                        title,
                        error_string,
                        (),
                    )
                })
                .await
                .result?;
        }
        Ok(())
    }
}
