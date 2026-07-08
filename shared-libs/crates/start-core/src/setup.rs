use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::Parser;
use color_eyre::eyre::eyre;
use const_format::formatcp;
use itertools::Itertools;
use josekit::jwk::Jwk;
use patch_db::json_ptr::ROOT;
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{Context, Empty, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::try_join;
use tracing::instrument;
use ts_rs::TS;

use crate::account::AccountInfo;
use crate::auth::write_shadow;
use crate::backup::restore::recover_full_server;
use crate::backup::target::BackupTargetFS;
use crate::bins::set_locale;
use crate::context::rpc::InitRpcContextPhases;
use crate::context::setup::SetupResult;
use crate::context::{CliContext, RpcContext, SetupContext};
use crate::db::model::Database;
use crate::disk::REPAIR_DISK_PATH;
use crate::disk::fsck::RepairStrategy;
use crate::disk::main::DEFAULT_PASSWORD;
use crate::disk::mount::filesystem::ReadWrite;
use crate::disk::mount::filesystem::cifs::Cifs;
use crate::disk::mount::guard::{GenericMountGuard, TmpMountGuard};
use crate::disk::util::{DiskInfo, StartOsRecoveryInfo, pvscan, recovery_info};
use crate::hostname::ServerHostnameInfo;
use crate::init::{InitPhases, InitResult, init};
use crate::net::ssl::root_ca_start_time;
use crate::prelude::*;
use crate::progress::{FullProgress, PhaseProgressTrackerHandle, ProgressUnits};
use crate::rpc_continuations::Guid;
use crate::shutdown::Shutdown;
use crate::system::{KeyboardOptions, SetLanguageParams, save_language, sync_kiosk};
use crate::util::Invoke;
use crate::util::crypto::EncryptedWire;
use crate::util::io::{Counter, create_file, dir_copy, dir_size, read_file_to_string};
use crate::util::serde::{HandlerExtSerde, IoFormat, Pem};
use crate::{DATA_DIR, Error, ErrorKind, MAIN_DATA, PACKAGE_DATA, PLATFORM, ResultExt};

pub fn setup<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "status",
            from_fn_async(status)
                .with_metadata("authenticated", Value::Bool(false))
                .with_display_serializable()
                .with_about("about.setup-status")
                .with_call_remote::<CliContext>(),
        )
        .subcommand("disk", disk::<C>())
        .subcommand("attach", from_fn_async(attach).no_cli())
        .subcommand(
            "attach",
            from_fn_async(cli_attach)
                .no_display()
                .with_about("about.setup-attach"),
        )
        .subcommand(
            "install-os",
            from_fn_async(crate::os_install::install_os).no_cli(),
        )
        .subcommand(
            "install-os",
            from_fn_async(cli_install_os)
                .no_display()
                .with_about("about.setup-install-os"),
        )
        .subcommand("execute", from_fn_async(execute).no_cli())
        .subcommand(
            "execute",
            from_fn_async(cli_execute)
                .no_display()
                .with_about("about.setup-execute"),
        )
        .subcommand("cifs", cifs::<C>())
        .subcommand(
            "complete",
            from_fn_async(complete)
                .with_display_serializable()
                .with_about("about.setup-complete")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "get-pubkey",
            from_fn_async(get_pubkey)
                .with_metadata("authenticated", Value::Bool(false))
                .with_display_serializable()
                .with_about("about.get-pubkey-from-server")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "exit",
            from_fn_async(exit)
                .no_display()
                .with_about("about.setup-exit")
                .with_call_remote::<CliContext>(),
        )
        .subcommand("logs", crate::system::logs::<SetupContext>())
        .subcommand(
            "logs",
            from_fn_async(crate::logs::cli_logs::<SetupContext, Empty>)
                .no_display()
                .with_about("about.display-os-logs"),
        )
        .subcommand(
            "restart",
            from_fn_async(restart)
                .no_display()
                .with_about("about.setup-restart")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "shutdown",
            from_fn_async(shutdown)
                .no_display()
                .with_about("about.setup-shutdown")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-language",
            from_fn_async(set_language)
                .no_display()
                .with_about("about.set-language")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-keyboard",
            from_fn_async(set_keyboard)
                .no_display()
                .with_about("about.set-keyboard")
                .with_call_remote::<CliContext>(),
        )
}

pub fn disk<C: Context>() -> ParentHandler<C> {
    ParentHandler::new().subcommand(
        "list",
        from_fn_async(list_disks)
            .with_metadata("authenticated", Value::Bool(false))
            .with_display_serializable()
            .with_about("about.setup-disk-list")
            .with_call_remote::<CliContext>(),
    )
}

const LIVE_MEDIUM_PATH: &str = "/run/live/medium";

pub async fn list_disks(_ctx: SetupContext) -> Result<Vec<DiskInfo>, Error> {
    let mut disks = crate::disk::util::list(
        &crate::disk::OsPartitionInfo::from_fstab()
            .await
            .unwrap_or_default(),
        None,
    )
    .await?;

    // Filter out the disk containing the live medium (installer USB)
    if let Ok(Some(live_medium_source)) =
        crate::disk::util::get_mount_source(LIVE_MEDIUM_PATH).await
    {
        disks.retain(|disk| disk.logicalname != live_medium_source);
    }

    Ok(disks)
}

#[instrument(skip_all)]
async fn setup_init(
    ctx: &SetupContext,
    password: Option<String>,
    kiosk: bool,
    hostname: Option<ServerHostnameInfo>,
    init_phases: InitPhases,
) -> Result<(AccountInfo, InitResult), Error> {
    let init_result = init(&ctx.webserver, &ctx.config.peek(|c| c.clone()), init_phases).await?;
    let language = ctx.language.peek(|a| a.clone());
    let keyboard = ctx.keyboard.peek(|a| a.clone());

    let account = init_result
        .net_ctrl
        .db
        .mutate(|m| {
            let mut account = AccountInfo::load(m)?;
            if let Some(password) = &password {
                account.set_password(password)?;
            }
            if let Some(hostname) = hostname {
                account.hostname = hostname;
            }
            account.save(m)?;
            let info = m.as_public_mut().as_server_info_mut();
            info.as_password_hash_mut().ser(&account.password)?;
            info.as_kiosk_mut()
                .ser(&Some(kiosk).filter(|_| &*PLATFORM != "raspberrypi"))?;
            if let Some(language) = language.clone() {
                info.as_language_mut().ser(&Some(language))?;
            }
            if let Some(keyboard) = keyboard.clone() {
                info.as_keyboard_mut().ser(&Some(keyboard))?;
            }

            Ok(account)
        })
        .await
        .result?;

    sync_kiosk(kiosk).await?;

    if let Some(language) = language {
        save_language(&*language).await?;
    }
    if let Some(keyboard) = keyboard {
        keyboard.save().await?;
    }

    if let Some(password) = &password {
        write_shadow(&password).await?;
    }

    Ok((account, init_result))
}

#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AttachParams {
    pub password: Option<EncryptedWire>,
    pub guid: InternedString,
    pub kiosk: bool,
}

/// CLI-only flags for `setup attach`. The password is read from the
/// PASSWORD environment variable (matching `start-cli auth login`),
/// or prompted on a TTY — never accepted as a CLI argument so it
/// can't leak via `ps` or shell history.
#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct SetupAttachCliParams {
    /// Disk GUID of the existing data drive to attach
    #[arg(long)]
    pub guid: InternedString,
    /// Enable kiosk mode
    #[arg(long)]
    pub kiosk: bool,
    /// Treat an unset PASSWORD env var as "no password" (only valid for
    /// drives whose GUID ends in `_UNENC`). Without this, missing
    /// PASSWORD falls back to a TTY prompt.
    #[arg(long)]
    pub no_password: bool,
}

#[instrument(skip_all)]
pub async fn attach(
    ctx: SetupContext,
    AttachParams {
        password,
        guid: disk_guid,
        kiosk,
    }: AttachParams,
) -> Result<SetupProgress, Error> {
    let setup_ctx = ctx.clone();
    ctx.run_setup(move || async move {
        let progress = &setup_ctx.progress;
        let mut disk_phase = progress.add_phase(t!("setup.opening-data-drive").into(), Some(10));
        let init_phases = InitPhases::new(&progress);
        let rpc_ctx_phases = InitRpcContextPhases::new(&progress);

        let password: Option<String> = match password {
            Some(a) => match a.decrypt(&setup_ctx) {
                a @ Some(_) => a,
                None => {
                    return Err(Error::new(
                        color_eyre::eyre::eyre!("{}", t!("setup.couldnt-decode-password")),
                        crate::ErrorKind::Unknown,
                    ));
                }
            },
            None => None,
        };

        disk_phase.start();
        let requires_reboot = crate::disk::main::import(
            &*disk_guid,
            DATA_DIR,
            if tokio::fs::metadata(REPAIR_DISK_PATH).await.is_ok() {
                RepairStrategy::Aggressive
            } else {
                RepairStrategy::Preen
            },
            if disk_guid.ends_with("_UNENC") {
                None
            } else {
                Some(DEFAULT_PASSWORD)
            },
            Some(&*progress),
        )
        .await?;
        let _ = setup_ctx.disk_guid.set(disk_guid.clone());
        if tokio::fs::metadata(REPAIR_DISK_PATH).await.is_ok() {
            tokio::fs::remove_file(REPAIR_DISK_PATH)
                .await
                .with_ctx(|_| (ErrorKind::Filesystem, REPAIR_DISK_PATH))?;
        }
        if requires_reboot.0 {
            crate::disk::main::export(&*disk_guid, DATA_DIR).await?;
            return Err(Error::new(
                eyre!("{}", t!("setup.disk-errors-corrected-restart-required")),
                ErrorKind::DiskManagement,
            ));
        }
        disk_phase.complete();

        let (account, net_ctrl) =
            setup_init(&setup_ctx, password, kiosk, None, init_phases).await?;

        let rpc_ctx = RpcContext::init(
            &setup_ctx.webserver,
            &setup_ctx.config.peek(|c| c.clone()),
            disk_guid,
            Some(net_ctrl),
            rpc_ctx_phases,
        )
        .await?;

        Ok((
            SetupResult {
                hostname: account.hostname.hostname,
                root_ca: Pem(account.root_ca_cert),
                needs_restart: setup_ctx.install_rootfs.peek(|a| a.is_some()),
            },
            rpc_ctx,
        ))
    })?;

    Ok(ctx.progress().await)
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
#[serde(tag = "status")]
pub enum SetupStatusRes {
    NeedsInstall,
    Incomplete(SetupInfo),
    Running(SetupProgress),
    Complete(SetupResult),
}

#[derive(Default, Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SetupInfo {
    pub guid: Option<InternedString>,
    pub attach: bool,
    pub mok_enrolled: bool,
    /// The whole disk the OS is installed on, recorded by whatever installed it
    /// (os_install, or init_resize for a pre-installed image). When the device
    /// is pre-installed the wizard fixes this as the OS drive and asks only for
    /// a data drive.
    #[serde(default)]
    pub os_drive: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SetupProgress {
    pub progress: FullProgress,
    pub guid: Guid,
}

pub async fn status(ctx: SetupContext) -> Result<SetupStatusRes, Error> {
    if let Some(res) = ctx.result.get() {
        match res {
            Ok((res, _)) => Ok(SetupStatusRes::Complete(res.clone())),
            Err(e) => Err(e.clone_output()),
        }
    } else {
        if ctx.task.initialized() {
            Ok(SetupStatusRes::Running(ctx.progress().await))
        } else {
            let path = Path::new("/media/startos/config/setup.json");
            if tokio::fs::metadata(path).await.is_err() {
                return Ok(SetupStatusRes::NeedsInstall);
            }
            IoFormat::Json
                .from_slice(read_file_to_string(path).await?.as_bytes())
                .map(SetupStatusRes::Incomplete)
        }
    }
}

/// We want to be able to get a secret, a shared private key with the frontend
/// This way the frontend can send a secret, like the password for the setup/ recovory
/// without knowing the password over clearnet. We use the public key shared across the network
/// since it is fine to share the public, and encrypt against the public.
pub async fn get_pubkey(ctx: SetupContext) -> Result<Jwk, RpcError> {
    let secret = AsRef::<Jwk>::as_ref(&ctx).clone();
    let pub_key = secret.to_public_key()?;
    Ok(pub_key)
}

pub fn cifs<C: Context>() -> ParentHandler<C> {
    ParentHandler::new().subcommand("verify", from_fn_async(verify_cifs).no_cli())
}

#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct VerifyCifsParams {
    hostname: String,
    path: PathBuf,
    username: String,
    password: Option<EncryptedWire>,
}

// #[command(rename = "verify", rpc_only)]
pub async fn verify_cifs(
    ctx: SetupContext,
    VerifyCifsParams {
        hostname,
        path,
        username,
        password,
    }: VerifyCifsParams,
) -> Result<BTreeMap<String, StartOsRecoveryInfo>, Error> {
    let password: Option<String> = password.map(|x| x.decrypt(&ctx)).flatten();
    let guard = TmpMountGuard::mount(
        &Cifs {
            hostname,
            path,
            username,
            password,
        },
        ReadWrite,
    )
    .await?;
    let start_os = recovery_info(guard.path()).await?;
    guard.unmount().await?;
    if start_os.is_empty() {
        return Err(Error::new(
            eyre!("{}", t!("setup.no-backup-found")),
            crate::ErrorKind::NotFound,
        ));
    }
    Ok(start_os)
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
#[serde(rename_all_fields = "camelCase")]
pub enum RecoverySource<Password> {
    Migrate {
        guid: String,
    },
    Backup {
        target: BackupTargetFS,
        password: Password,
        server_id: String,
    },
}

pub async fn setup_data_drive(
    ctx: &SetupContext,
    logicalname: &Path,
) -> Result<InternedString, Error> {
    let encryption_password = if ctx.disable_encryption {
        None
    } else {
        Some(DEFAULT_PASSWORD)
    };
    let guid = crate::disk::main::create(
        &[logicalname],
        &pvscan().await?,
        DATA_DIR,
        encryption_password,
    )
    .await?;
    let _ = crate::disk::main::import(
        &*guid,
        DATA_DIR,
        RepairStrategy::Preen,
        encryption_password,
        None,
    )
    .await?;
    let _ = ctx.disk_guid.set(guid.clone());
    Ok(guid)
}

#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SetupExecuteParams {
    guid: InternedString,
    password: Option<EncryptedWire>,
    recovery_source: Option<RecoverySource<EncryptedWire>>,
    kiosk: bool,
    name: Option<InternedString>,
    hostname: Option<InternedString>,
}

/// CLI-only flags for `setup execute`. The password is read from the
/// PASSWORD environment variable (matching `start-cli auth login`),
/// or prompted on a TTY — never accepted as a CLI argument so it
/// can't leak via `ps` or shell history. Recovery / migration callers
/// should hit the JSON-RPC endpoint directly.
#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct SetupExecuteCliParams {
    /// Disk GUID returned by `setup install-os` (or an existing data drive)
    #[arg(long)]
    guid: InternedString,
    /// Enable kiosk mode
    #[arg(long)]
    kiosk: bool,
    /// Friendly server name
    #[arg(long)]
    name: Option<InternedString>,
    /// Hostname (LAN advertised) — defaults to a random adjective-noun pair
    #[arg(long)]
    hostname: Option<InternedString>,
}

/// CLI-only flags for `setup install-os`. Mirrors
/// `os_install::InstallOsParams` but lives here so we can keep
/// `#[group(skip)]` on `os_install::DataDrive` (manpage generation
/// requires it on every `Parser`-derived struct, and clap rejects
/// `#[command(flatten)] Option<DataDrive>` when the inner type has
/// `group(skip)`).
#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct SetupInstallOsCliParams {
    /// Path to the OS drive
    #[arg(value_name = "OS_DRIVE")]
    pub os_drive: PathBuf,
    /// Path to the data drive (omit for an OS-only install). May be the
    /// same logical name as <OS_DRIVE>; the installer will carve a data
    /// partition out of the same disk.
    #[arg(long)]
    pub data_drive: Option<PathBuf>,
    /// Wipe the data drive before use
    #[arg(long)]
    pub wipe: bool,
}

#[instrument(skip_all)]
async fn cli_install_os(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        params:
            SetupInstallOsCliParams {
                os_drive,
                data_drive,
                wipe,
            },
        ..
    }: HandlerArgs<CliContext, SetupInstallOsCliParams>,
) -> Result<(), RpcError> {
    let body = if let Some(data_drive) = data_drive {
        imbl_value::json!({
            "osDrive": os_drive,
            "dataDrive": { "logicalname": data_drive, "wipe": wipe },
        })
    } else {
        imbl_value::json!({ "osDrive": os_drive })
    };
    let res = ctx
        .call_remote::<SetupContext>(&parent_method.into_iter().chain(method).join("."), body)
        .await?;
    print_remote_result(res)
}

/// Read a password for an existing data drive: `PASSWORD` env first,
/// otherwise prompt once on the TTY.
fn read_password_env_or_prompt(prompt: &str) -> Result<String, RpcError> {
    if let Ok(p) = std::env::var("PASSWORD") {
        Ok(p)
    } else {
        Ok(rpassword::prompt_password(prompt)?)
    }
}

/// Read a *new* password (for fresh setup): `PASSWORD` env if set,
/// otherwise prompt twice on the TTY and confirm. Mirrors
/// `cli_reset_password` in `auth.rs`.
fn read_new_password_env_or_prompt() -> Result<String, RpcError> {
    if let Ok(p) = std::env::var("PASSWORD") {
        return Ok(p);
    }
    let new_password = rpassword::prompt_password(&t!("auth.prompt-new-password"))?;
    let confirm = rpassword::prompt_password(&t!("auth.prompt-confirm"))?;
    if new_password != confirm {
        return Err(crate::Error::new(
            eyre!("{}", t!("auth.passwords-do-not-match")),
            crate::ErrorKind::IncorrectPassword,
        )
        .into());
    }
    Ok(new_password)
}

/// Fetch the setup-mode public key via JSON-RPC and JWE-encrypt
/// `plaintext` against it, producing the `EncryptedWire` blob the
/// server expects on the wire (same shape the web UI sends).
async fn encrypt_for_setup(ctx: &CliContext, plaintext: &str) -> Result<EncryptedWire, RpcError> {
    let pubkey_value = ctx
        .call_remote::<SetupContext>("setup.get-pubkey", imbl_value::json!({}))
        .await?;
    let pubkey: josekit::jwk::Jwk = imbl_value::from_value(pubkey_value).map_err(|e| {
        crate::Error::new(
            eyre!("setup get-pubkey: {e}"),
            crate::ErrorKind::Deserialization,
        )
    })?;
    EncryptedWire::encrypt(plaintext, &pubkey).map_err(RpcError::from)
}

fn print_remote_result(res: imbl_value::Value) -> Result<(), RpcError> {
    match serde_json::to_string_pretty(&res) {
        Ok(s) => {
            println!("{s}");
            Ok(())
        }
        Err(e) => Err(RpcError::from(crate::Error::new(
            eyre!("{e}"),
            crate::ErrorKind::Serialization,
        ))),
    }
}

#[instrument(skip_all)]
async fn cli_attach(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        params:
            SetupAttachCliParams {
                guid,
                kiosk,
                no_password,
            },
        ..
    }: HandlerArgs<CliContext, SetupAttachCliParams>,
) -> Result<(), RpcError> {
    let password = if no_password && std::env::var_os("PASSWORD").is_none() {
        None
    } else {
        let plaintext = read_password_env_or_prompt("Data drive password: ")?;
        Some(encrypt_for_setup(&ctx, &plaintext).await?)
    };

    let res = ctx
        .call_remote::<SetupContext>(
            &parent_method.into_iter().chain(method).join("."),
            imbl_value::json!({
                "guid": guid,
                "password": password,
                "kiosk": kiosk,
            }),
        )
        .await?;
    print_remote_result(res)
}

#[instrument(skip_all)]
async fn cli_execute(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        params:
            SetupExecuteCliParams {
                guid,
                kiosk,
                name,
                hostname,
            },
        ..
    }: HandlerArgs<CliContext, SetupExecuteCliParams>,
) -> Result<(), RpcError> {
    let plaintext = read_new_password_env_or_prompt()?;
    let password = encrypt_for_setup(&ctx, &plaintext).await?;

    let res = ctx
        .call_remote::<SetupContext>(
            &parent_method.into_iter().chain(method).join("."),
            imbl_value::json!({
                "guid": guid,
                "password": password,
                "kiosk": kiosk,
                "name": name,
                "hostname": hostname,
            }),
        )
        .await?;
    print_remote_result(res)
}

// #[command(rpc_only)]
pub async fn execute(
    ctx: SetupContext,
    SetupExecuteParams {
        guid,
        password,
        recovery_source,
        kiosk,
        name,
        hostname,
    }: SetupExecuteParams,
) -> Result<SetupProgress, Error> {
    let password = password
        .map(|p| {
            p.decrypt(&ctx).ok_or_else(|| {
                Error::new(
                    color_eyre::eyre::eyre!("{}", t!("setup.couldnt-decode-startos-password")),
                    crate::ErrorKind::Unknown,
                )
            })
        })
        .transpose()?;
    let recovery = match recovery_source {
        Some(RecoverySource::Backup {
            target,
            password,
            server_id,
        }) => Some(RecoverySource::Backup {
            target,
            password: password.decrypt(&ctx).ok_or_else(|| {
                Error::new(
                    color_eyre::eyre::eyre!("{}", t!("setup.couldnt-decode-recovery-password")),
                    crate::ErrorKind::Unknown,
                )
            })?,
            server_id,
        }),
        Some(RecoverySource::Migrate { guid }) => Some(RecoverySource::Migrate { guid }),
        None => None,
    };

    let hostname = ServerHostnameInfo::new_opt(name, hostname)?;

    let setup_ctx = ctx.clone();
    ctx.run_setup(move || execute_inner(setup_ctx, guid, password, recovery, kiosk, hostname))?;

    Ok(ctx.progress().await)
}

#[instrument(skip_all)]
// #[command(rpc_only)]
pub async fn complete(ctx: SetupContext) -> Result<SetupResult, Error> {
    match ctx.result.get() {
        Some(Ok((res, ctx))) => {
            let mut guid_file = create_file("/media/startos/config/disk.guid").await?;
            guid_file.write_all(ctx.disk_guid.as_bytes()).await?;
            guid_file.sync_all().await?;
            Command::new("systemd-firstboot")
                .arg("--root=/media/startos/config/overlay/")
                .arg(format!("--hostname={}", res.hostname.as_ref()))
                .invoke(ErrorKind::ParseSysInfo)
                .await?;
            Command::new("sync").invoke(ErrorKind::Filesystem).await?;

            Ok(res.clone())
        }
        Some(Err(e)) => Err(e.clone_output()),
        None => Err(Error::new(
            eyre!("{}", t!("setup.execute-not-completed")),
            crate::ErrorKind::InvalidRequest,
        )),
    }
}

#[instrument(skip_all)]
pub async fn exit(ctx: SetupContext) -> Result<(), Error> {
    let shutdown = if let Some((rootfs, config)) = ctx.install_rootfs.replace(None) {
        config.unmount(false).await?;
        rootfs.unmount().await?;
        Some(Shutdown {
            disk_guid: ctx.disk_guid.get().cloned(),
            restart: true,
        })
    } else {
        None
    };

    ctx.shutdown
        .send(shutdown)
        .map_err(|e| eyre!("failed to shutdown: {e}"))
        .log_err();
    Ok(())
}

#[instrument(skip_all)]
pub async fn restart(ctx: SetupContext) -> Result<(), Error> {
    if let Some((rootfs, config)) = ctx.install_rootfs.replace(None) {
        config.unmount(false).await?;
        rootfs.unmount().await?;
    }
    ctx.shutdown
        .send(Some(Shutdown {
            disk_guid: ctx.disk_guid.get().cloned(),
            restart: true,
        }))
        .map_err(|e| eyre!("failed to shutdown: {e}"))
        .log_err();
    Ok(())
}

#[instrument(skip_all)]
pub async fn shutdown(ctx: SetupContext) -> Result<(), Error> {
    if let Some((rootfs, config)) = ctx.install_rootfs.replace(None) {
        config.unmount(false).await?;
        rootfs.unmount().await?;
    }
    ctx.shutdown
        .send(Some(Shutdown {
            disk_guid: ctx.disk_guid.get().cloned(),
            restart: false,
        }))
        .map_err(|e| eyre!("failed to shutdown: {e}"))
        .log_err();
    Ok(())
}

#[instrument(skip_all)]
pub async fn execute_inner(
    ctx: SetupContext,
    guid: InternedString,
    password: Option<String>,
    recovery_source: Option<RecoverySource<String>>,
    kiosk: bool,
    hostname: Option<ServerHostnameInfo>,
) -> Result<(SetupResult, RpcContext), Error> {
    let progress = &ctx.progress;

    if !crate::disk::mount::util::is_mountpoint(Path::new(DATA_DIR).join("main")).await? {
        let mut disk_phase = progress.add_phase(t!("setup.opening-data-drive").into(), Some(10));
        disk_phase.start();
        let requires_reboot = crate::disk::main::import(
            &*guid,
            DATA_DIR,
            if tokio::fs::metadata(REPAIR_DISK_PATH).await.is_ok() {
                RepairStrategy::Aggressive
            } else {
                RepairStrategy::Preen
            },
            if guid.ends_with("_UNENC") {
                None
            } else {
                Some(DEFAULT_PASSWORD)
            },
            Some(progress),
        )
        .await?;
        let _ = ctx.disk_guid.set(guid.clone());
        crate::util::io::delete_file(REPAIR_DISK_PATH).await?;
        if requires_reboot.0 {
            crate::disk::main::export(&*guid, DATA_DIR).await?;
            return Err(Error::new(
                eyre!("{}", t!("setup.disk-errors-corrected-restart-required")),
                ErrorKind::DiskManagement,
            ));
        }
        disk_phase.complete();
    }

    let restore_phase = match recovery_source.as_ref() {
        Some(RecoverySource::Backup { .. }) => {
            Some(progress.add_phase(t!("setup.restoring-backup").into(), Some(100)))
        }
        Some(RecoverySource::Migrate { .. }) => {
            Some(progress.add_phase(t!("setup.transferring-data").into(), Some(100)))
        }
        None => None,
    };
    let init_phases = InitPhases::new(&progress);
    let rpc_ctx_phases = InitRpcContextPhases::new(&progress);

    let progress = SetupExecuteProgress {
        init_phases,
        restore_phase,
        rpc_ctx_phases,
    };

    match recovery_source {
        Some(RecoverySource::Backup {
            target,
            password: recovery_password,
            server_id,
        }) => {
            recover(
                &ctx,
                guid,
                password,
                target,
                server_id,
                recovery_password,
                kiosk,
                hostname,
                progress,
            )
            .await
        }
        Some(RecoverySource::Migrate { guid: old_guid }) => {
            migrate(&ctx, guid, &old_guid, password, kiosk, hostname, progress).await
        }
        None => {
            fresh_setup(
                &ctx,
                guid,
                &password.ok_or_else(|| {
                    Error::new(
                        eyre!("{}", t!("setup.password-required")),
                        ErrorKind::InvalidRequest,
                    )
                })?,
                kiosk,
                hostname,
                progress,
            )
            .await
        }
    }
}

pub struct SetupExecuteProgress {
    pub init_phases: InitPhases,
    pub restore_phase: Option<PhaseProgressTrackerHandle>,
    pub rpc_ctx_phases: InitRpcContextPhases,
}

async fn fresh_setup(
    ctx: &SetupContext,
    guid: InternedString,
    password: &str,
    kiosk: bool,
    hostname: Option<ServerHostnameInfo>,
    SetupExecuteProgress {
        init_phases,
        rpc_ctx_phases,
        ..
    }: SetupExecuteProgress,
) -> Result<(SetupResult, RpcContext), Error> {
    let account = AccountInfo::new(password, root_ca_start_time().await, hostname)?;

    let db = ctx.db().await?;
    sync_kiosk(kiosk).await?;

    let language = ctx.language.peek(|a| a.clone());
    let keyboard = ctx.keyboard.peek(|a| a.clone());

    if let Some(language) = &language {
        save_language(&**language).await?;
    }

    if let Some(keyboard) = &keyboard {
        keyboard.save().await?;
    }

    db.put(&ROOT, &Database::init(&account, kiosk, language, keyboard)?)
        .await?;
    drop(db);

    let config = ctx.config.peek(|c| c.clone());

    let init_result = init(&ctx.webserver, &config, init_phases).await?;

    let rpc_ctx = RpcContext::init(
        &ctx.webserver,
        &config,
        guid,
        Some(init_result),
        rpc_ctx_phases,
    )
    .await?;

    write_shadow(password).await?;

    Ok((
        SetupResult {
            hostname: account.hostname.hostname,
            root_ca: Pem(account.root_ca_cert),
            needs_restart: ctx.install_rootfs.peek(|a| a.is_some()),
        },
        rpc_ctx,
    ))
}

#[instrument(skip_all)]
async fn recover(
    ctx: &SetupContext,
    guid: InternedString,
    password: Option<String>,
    recovery_source: BackupTargetFS,
    server_id: String,
    recovery_password: String,
    kiosk: bool,
    hostname: Option<ServerHostnameInfo>,
    progress: SetupExecuteProgress,
) -> Result<(SetupResult, RpcContext), Error> {
    let recovery_source = TmpMountGuard::mount(&recovery_source, ReadWrite).await?;
    recover_full_server(
        ctx,
        guid.clone(),
        password,
        recovery_source,
        &server_id,
        &recovery_password,
        kiosk,
        hostname,
        progress,
    )
    .await
}

#[instrument(skip_all)]
async fn migrate(
    ctx: &SetupContext,
    guid: InternedString,
    old_guid: &str,
    password: Option<String>,
    kiosk: bool,
    hostname: Option<ServerHostnameInfo>,
    SetupExecuteProgress {
        init_phases,
        restore_phase,
        rpc_ctx_phases,
    }: SetupExecuteProgress,
) -> Result<(SetupResult, RpcContext), Error> {
    let mut restore_phase = restore_phase.or_not_found("restore progress")?;

    restore_phase.start();
    restore_phase.set_units(Some(ProgressUnits::Bytes));
    let _ = crate::disk::main::import(
        &old_guid,
        "/media/startos/migrate",
        RepairStrategy::Preen,
        if guid.ends_with("_UNENC") {
            None
        } else {
            Some(DEFAULT_PASSWORD)
        },
        Some(&ctx.progress),
    )
    .await?;

    let main_transfer_args = ("/media/startos/migrate/main/", formatcp!("{MAIN_DATA}/"));
    let package_data_transfer_args = (
        "/media/startos/migrate/package-data/",
        formatcp!("{PACKAGE_DATA}/"),
    );

    let tmpdir = Path::new(package_data_transfer_args.0).join("tmp");
    crate::util::io::delete_dir(&tmpdir).await?;

    let ordering = std::sync::atomic::Ordering::Relaxed;

    let main_transfer_size = Counter::new(0, ordering);
    let package_data_transfer_size = Counter::new(0, ordering);

    let size = tokio::select! {
        res = async {
            let (main_size, package_data_size) = try_join!(
                dir_size(main_transfer_args.0, Some(&main_transfer_size)),
                dir_size(package_data_transfer_args.0, Some(&package_data_transfer_size))
            )?;
            Ok::<_, Error>(main_size + package_data_size)
        } => { res? },
        res = async {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                restore_phase.set_total(main_transfer_size.load() + package_data_transfer_size.load());
            }
        } => res,
    };

    restore_phase.set_total(size);

    let main_transfer_progress = Counter::new(0, ordering);
    let package_data_transfer_progress = Counter::new(0, ordering);

    tokio::select! {
        res = async {
            try_join!(
                dir_copy(main_transfer_args.0, main_transfer_args.1, Some(&main_transfer_progress)),
                dir_copy(package_data_transfer_args.0, package_data_transfer_args.1, Some(&package_data_transfer_progress))
            )?;
            Ok::<_, Error>(())
        } => { res? },
        res = async {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                restore_phase.set_done(main_transfer_progress.load() + package_data_transfer_progress.load());
            }
        } => res,
    }

    crate::disk::main::export(&old_guid, "/media/startos/migrate").await?;
    restore_phase.complete();

    let (account, net_ctrl) = setup_init(&ctx, password, kiosk, hostname, init_phases).await?;

    let rpc_ctx = RpcContext::init(
        &ctx.webserver,
        &ctx.config.peek(|c| c.clone()),
        guid,
        Some(net_ctrl),
        rpc_ctx_phases,
    )
    .await?;

    Ok((
        SetupResult {
            hostname: account.hostname.hostname,
            root_ca: Pem(account.root_ca_cert),
            needs_restart: ctx.install_rootfs.peek(|a| a.is_some()),
        },
        rpc_ctx,
    ))
}

pub async fn set_language(
    ctx: SetupContext,
    SetLanguageParams { language }: SetLanguageParams,
) -> Result<(), Error> {
    set_locale(&*language);
    ctx.language.replace(Some(language));
    Ok(())
}

pub async fn set_keyboard(ctx: SetupContext, options: KeyboardOptions) -> Result<(), Error> {
    options.apply_to_session().await?;
    ctx.keyboard.replace(Some(options));
    Ok(())
}
