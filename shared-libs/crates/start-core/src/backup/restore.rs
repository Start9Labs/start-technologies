use std::collections::BTreeMap;
use std::sync::Arc;

use clap::Parser;
use futures::{StreamExt, stream};
use patch_db::json_ptr::ROOT;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::instrument;
use ts_rs::TS;

use super::target::BackupTargetId;
use crate::PackageId;
use crate::backup::os::OsBackup;
use crate::context::setup::SetupResult;
use crate::context::{RpcContext, SetupContext};
use crate::db::model::Database;
use crate::disk::mount::backup::BackupMountGuard;
use crate::disk::mount::filesystem::ReadWrite;
use crate::disk::mount::guard::{GenericMountGuard, TmpMountGuard};
use crate::hostname::{ServerHostname, repair_hostname};
use crate::init::init;
use crate::prelude::*;
use crate::progress::ProgressUnits;
use crate::s9pk::S9pk;
use crate::service::service_map::DownloadInstallFuture;
use crate::setup::SetupExecuteProgress;
use crate::system::{save_language, sync_kiosk};
use crate::util::serde::{IoFormat, Pem};

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
#[ts(export)]
pub struct RestorePackageParams {
    #[arg(help = "help.arg.backup-target-id")]
    pub target_id: BackupTargetId,
    #[arg(help = "help.arg.backup-password")]
    pub password: String,
    #[arg(help = "help.arg.package-ids")]
    pub ids: Vec<PackageId>,
    #[arg(long, help = "help.arg.server-id")]
    pub server_id: Option<String>,
}

// #[command(rename = "restore", display(display_none))]
#[instrument(skip(ctx, password))]
pub async fn restore_packages_rpc(
    ctx: RpcContext,
    RestorePackageParams {
        ids,
        target_id,
        password,
        server_id,
    }: RestorePackageParams,
) -> Result<(), Error> {
    let peek = ctx.db.peek().await;
    let fs = target_id.load(&peek)?;
    let server_id = match server_id {
        Some(id) => id,
        None => peek.as_public().as_server_info().as_id().de()?,
    };
    let backup_guard = BackupMountGuard::mount(
        TmpMountGuard::mount(&fs, ReadWrite).await?,
        &server_id,
        &password,
    )
    .await?;

    let tasks = restore_packages(&ctx, backup_guard, ids).await?;

    tokio::spawn(async move {
        stream::iter(tasks)
            .for_each_concurrent(5, |(id, res)| async move {
                match async { res.await?.await }.await {
                    Ok(_) => (),
                    Err(err) => {
                        tracing::error!(
                            "{}",
                            t!("backup.restore.package-error", id = id, error = err)
                        );
                        tracing::debug!("{:?}", err);
                    }
                }
            })
            .await;
    });

    Ok(())
}

fn restored_hostname(
    backup_hostname: ServerHostname,
    requested_hostname: Option<ServerHostname>,
) -> ServerHostname {
    requested_hostname.unwrap_or_else(|| repair_hostname(backup_hostname.as_ref()))
}

#[instrument(skip_all)]
pub async fn recover_full_server(
    ctx: &SetupContext,
    disk_guid: InternedString,
    password: Option<String>,
    recovery_source: TmpMountGuard,
    server_id: &str,
    recovery_password: &str,
    kiosk: bool,
    hostname: Option<ServerHostname>,
    SetupExecuteProgress {
        init_phases,
        restore_phase,
        rpc_ctx_phases,
    }: SetupExecuteProgress,
) -> Result<(SetupResult, RpcContext), Error> {
    let mut restore_phase = restore_phase.or_not_found("restore progress")?;

    let backup_guard =
        BackupMountGuard::mount(recovery_source, server_id, recovery_password).await?;

    let os_backup_path = backup_guard.path().join("os-backup.json");
    let mut os_backup: OsBackup = IoFormat::Json.from_slice(
        &tokio::fs::read(&os_backup_path)
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, os_backup_path.display().to_string()))?,
    )?;

    if let Some(password) = password {
        os_backup.account.password = argon2::hash_encoded(
            password.as_bytes(),
            &rand::random::<[u8; 16]>()[..],
            &argon2::Config::rfc9106_low_mem(),
        )
        .with_kind(ErrorKind::PasswordHashGeneration)?;
    }

    os_backup.account.hostname = restored_hostname(os_backup.account.hostname, hostname);

    sync_kiosk(kiosk).await?;

    let language = ctx.language.peek(|a| a.clone());
    let keyboard = ctx.keyboard.peek(|a| a.clone());

    if let Some(language) = &language {
        save_language(&**language).await?;
    }

    if let Some(keyboard) = &keyboard {
        keyboard.save().await?;
    }

    let db = ctx.db().await?;
    db.put(
        &ROOT,
        &Database::init(&os_backup.account, kiosk, language, keyboard)?,
    )
    .await?;
    drop(db);

    let config = ctx.config.peek(|c| c.clone());

    let init_result = init(&ctx.webserver, &config, init_phases).await?;

    let rpc_ctx = RpcContext::init(
        &ctx.webserver,
        &config,
        disk_guid.clone(),
        Some(init_result),
        rpc_ctx_phases,
    )
    .await?;

    restore_phase.start();
    let ids: Vec<_> = backup_guard
        .metadata
        .package_backups
        .keys()
        .cloned()
        .collect();
    let tasks = restore_packages(&rpc_ctx, backup_guard, ids).await?;
    restore_phase.set_total(tasks.len() as u64);
    restore_phase.set_units(Some(ProgressUnits::Steps));
    let restore_phase = Arc::new(Mutex::new(restore_phase));
    stream::iter(tasks)
        .for_each_concurrent(5, |(id, res)| {
            let restore_phase = restore_phase.clone();
            async move {
                match async { res.await?.await }.await {
                    Ok(_) => (),
                    Err(err) => {
                        tracing::error!(
                            "{}",
                            t!("backup.restore.package-error", id = id, error = err)
                        );
                        tracing::debug!("{:?}", err);
                    }
                }
                *restore_phase.lock().await += 1;
            }
        })
        .await;
    restore_phase.lock().await.complete();

    Ok((
        SetupResult {
            hostname: os_backup.account.hostname,
            root_ca: Pem(os_backup.account.root_ca_cert),
            needs_restart: ctx.install_rootfs.peek(|a| a.is_some()),
        },
        rpc_ctx,
    ))
}

#[instrument(skip(ctx, backup_guard))]
async fn restore_packages(
    ctx: &RpcContext,
    backup_guard: BackupMountGuard<TmpMountGuard>,
    ids: Vec<PackageId>,
) -> Result<BTreeMap<PackageId, DownloadInstallFuture>, Error> {
    let backup_guard = Arc::new(backup_guard);
    let mut tasks = BTreeMap::new();
    for id in ids {
        let backup_dir = backup_guard.clone().package_backup(&id).await?;
        let s9pk_path = backup_dir.path().join(&id).with_extension("s9pk");
        let task = ctx
            .services
            .install(
                ctx.clone(),
                || S9pk::open(s9pk_path, Some(&id)),
                None, // TODO: pull from metadata?
                Some(backup_dir),
                None,
            )
            .await?;
        tasks.insert(id, task);
    }

    Ok(tasks)
}

#[cfg(test)]
mod test {
    use super::*;

    fn hostname(value: &str) -> ServerHostname {
        ServerHostname::new(InternedString::intern(value)).unwrap()
    }

    #[test]
    fn restore_preserves_the_backup_hostname() {
        assert_eq!(
            restored_hostname(hostname("preserved-host"), None).as_ref(),
            "preserved-host"
        );
    }

    #[test]
    fn restore_repairs_a_backup_hostname_over_the_limit() {
        assert_eq!(
            restored_hostname(hostname(&"a".repeat(50)), None).as_ref(),
            "a".repeat(32)
        );
    }

    #[test]
    fn restore_uses_an_explicit_replacement_hostname() {
        assert_eq!(
            restored_hostname(
                hostname("preserved-host"),
                Some(hostname("replacement-host"))
            )
            .as_ref(),
            "replacement-host"
        );
    }
}
