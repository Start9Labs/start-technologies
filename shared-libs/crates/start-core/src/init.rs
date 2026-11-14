use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use itertools::Itertools;
use patch_db::ModelExt;
use rpc_toolkit::{Context, Empty, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::instrument;

use crate::account::AccountInfo;
use crate::context::config::ServerConfig;
use crate::context::{CliContext, InitContext, RpcContext};
use crate::db::model::Database;
use crate::db::model::public::ServerStatus;
use crate::developer::OS_ID_KEY_PATH;
use crate::hostname::ServerHostname;
use crate::middleware::auth::local::LocalAuthContext;
use crate::net::gateway::WildcardListener;
use crate::net::net_controller::{NetController, NetService};
use crate::net::socks::DEFAULT_SOCKS_LISTEN;
use crate::net::web_server::WebServerAcceptorSetter;
use crate::prelude::*;
use crate::progress::{
    FullProgress, FullProgressTracker, PhaseProgressTrackerHandle, ProgressUnits,
};
use crate::rpc_continuations::{Guid, RpcContinuation, WebSocketPolicy};
use crate::ssh::SSH_DIR;
use crate::system::{get_mem_info, sync_kiosk};
use crate::util::io::{IOHook, dir_copy, open_file};
use crate::util::lshw::lshw;
use crate::util::{Invoke, cpupower};
use crate::{Error, MAIN_DATA, PACKAGE_DATA, ResultExt};

pub const SYSTEM_REBUILD_PATH: &str = "/media/startos/config/system-rebuild";
pub const STANDBY_MODE_PATH: &str = "/media/startos/config/standby";

/// Observes systemd-compatible clock synchronization without subprocesses on Linux.
pub async fn check_time_is_synchronized() -> Result<bool, Error> {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: all timex fields permit zero; modes zero requests a read.
        let mut sample: libc::timex = unsafe { std::mem::zeroed() };
        // SAFETY: the kernel receives a valid writable timex with modes zero.
        let result = unsafe { libc::adjtimex(&mut sample) };
        Ok(ntp_synchronized(result, &sample))
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(String::from_utf8(
            Command::new("timedatectl")
                .arg("show")
                .arg("-p")
                .arg("NTPSynchronized")
                .invoke(crate::ErrorKind::Unknown)
                .await?,
        )?
        .trim()
            == "NTPSynchronized=yes")
    }
}

#[cfg(target_os = "linux")]
/// Matches [systemd's NTPSynchronized](https://sources.debian.org/src/systemd/252.39-1~deb12u2/src/timedate/timedated.c/#L516); `STA_UNSYNC` may encode RTC policy.
fn ntp_synchronized(query_result: libc::c_int, sample: &libc::timex) -> bool {
    query_result >= 0 && sample.maxerror < 16_000_000
}

pub struct InitResult {
    pub net_ctrl: Arc<NetController>,
    pub os_net_service: NetService,
}

pub struct InitPhases {
    preinit: Option<PhaseProgressTrackerHandle>,
    local_auth: PhaseProgressTrackerHandle,
    load_database: PhaseProgressTrackerHandle,
    load_ssh_keys: PhaseProgressTrackerHandle,
    start_net: PhaseProgressTrackerHandle,
    mount_logs: PhaseProgressTrackerHandle,
    load_ca_cert: PhaseProgressTrackerHandle,
    load_wifi: PhaseProgressTrackerHandle,
    init_tmp: PhaseProgressTrackerHandle,
    set_governor: PhaseProgressTrackerHandle,
    sync_clock: PhaseProgressTrackerHandle,
    enable_zram: PhaseProgressTrackerHandle,
    update_server_info: PhaseProgressTrackerHandle,
    launch_service_network: PhaseProgressTrackerHandle,
    validate_db: PhaseProgressTrackerHandle,
    postinit: Option<PhaseProgressTrackerHandle>,
}
impl InitPhases {
    pub fn new(handle: &FullProgressTracker) -> Self {
        Self {
            preinit: if Path::new("/media/startos/config/preinit.sh").exists() {
                Some(handle.add_phase(t!("init.running-preinit").into(), Some(5)))
            } else {
                None
            },
            local_auth: handle.add_phase(t!("init.enabling-local-auth").into(), Some(1)),
            load_database: handle.add_phase(t!("init.loading-database").into(), Some(5)),
            load_ssh_keys: handle.add_phase(t!("init.loading-ssh-keys").into(), Some(1)),
            start_net: handle.add_phase(t!("init.starting-network-controller").into(), Some(1)),
            mount_logs: handle.add_phase(t!("init.switching-logs-to-data-drive").into(), Some(1)),
            load_ca_cert: handle.add_phase(t!("init.loading-ca-certificate").into(), Some(1)),
            load_wifi: handle.add_phase(t!("init.loading-wifi-configuration").into(), Some(1)),
            init_tmp: handle.add_phase(t!("init.initializing-temporary-files").into(), Some(1)),
            set_governor: handle
                .add_phase(t!("init.setting-cpu-performance-profile").into(), Some(1)),
            sync_clock: handle.add_phase(t!("init.synchronizing-system-clock").into(), Some(10)),
            enable_zram: handle.add_phase(t!("init.enabling-zram").into(), Some(1)),
            update_server_info: handle.add_phase(t!("init.updating-server-info").into(), Some(1)),
            launch_service_network: handle
                .add_phase(t!("init.launching-service-intranet").into(), Some(1)),
            validate_db: handle.add_phase(t!("init.validating-database").into(), Some(1)),
            postinit: if Path::new("/media/startos/config/postinit.sh").exists() {
                Some(handle.add_phase(t!("init.running-postinit").into(), Some(5)))
            } else {
                None
            },
        }
    }
}

pub async fn run_script<P: AsRef<Path>>(path: P, mut progress: PhaseProgressTrackerHandle) {
    let script = path.as_ref();
    progress.start();
    if let Err(e) = async {
        let script = tokio::fs::read_to_string(script).await?;
        progress.set_total(script.as_bytes().iter().filter(|b| **b == b'\n').count() as u64);
        progress.set_units(Some(ProgressUnits::Bytes));
        let mut reader = IOHook::new(Cursor::new(script.as_bytes()));
        reader.post_read(|buf| progress += buf.iter().filter(|b| **b == b'\n').count() as u64);
        Command::new("/bin/bash")
            .input(Some(&mut reader))
            .invoke(ErrorKind::Unknown)
            .await?;
        Ok::<_, Error>(())
    }
    .await
    {
        tracing::error!(
            "{}",
            t!(
                "init.error-running-script",
                script = script.display(),
                error = e
            )
        );
        tracing::debug!("{:?}", e);
    }
    progress.complete();
}

async fn persist_pstore_archive(source: &Path, destination: &Path) -> Result<(), Error> {
    let mut archive = match tokio::fs::read_dir(source).await {
        Ok(archive) => archive,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if archive.next_entry().await?.is_some() {
        dir_copy(source, destination, None).await?;
    }
    Ok(())
}

#[instrument(skip_all)]
pub async fn init(
    webserver: &WebServerAcceptorSetter<WildcardListener>,
    dns: Arc<crate::net::dns::DnsController>,
    cfg: &ServerConfig,
    InitPhases {
        preinit,
        mut local_auth,
        mut load_database,
        mut load_ssh_keys,
        mut start_net,
        mut mount_logs,
        mut load_ca_cert,
        mut load_wifi,
        mut init_tmp,
        mut set_governor,
        mut sync_clock,
        mut enable_zram,
        mut update_server_info,
        mut launch_service_network,
        mut validate_db,
        postinit,
    }: InitPhases,
) -> Result<InitResult, Error> {
    if let Err(e) = persist_pstore_archive(
        Path::new("/var/lib/systemd/pstore"),
        &Path::new(MAIN_DATA).join("pstore"),
    )
    .await
    {
        tracing::warn!("could not persist systemd-pstore archive: {e}");
        tracing::debug!("{e:?}");
    }

    if let Some(progress) = preinit {
        run_script("/media/startos/config/preinit.sh", progress).await;
    }

    local_auth.start();
    RpcContext::init_auth_cookie().await?;
    local_auth.complete();

    // Re-enroll MOK on every boot if Secure Boot key exists but isn't enrolled yet
    if let Err(e) = crate::util::mok::enroll_mok(std::path::Path::new("/")).await {
        tracing::warn!("MOK enrollment failed: {e}");
    }

    load_database.start();
    let db = cfg.db().await?;
    crate::version::Current::default().pre_init(&db).await?;
    let db = TypedPatchDb::<Database>::load_unchecked(db);
    db.mutate(crate::net::ssl::repair_root_cert).await.result?;
    let peek = db.peek().await;
    load_database.complete();

    load_ssh_keys.start();
    crate::developer::write_signing_key(
        &peek.as_private().as_server_identity_key().de()?.0,
        OS_ID_KEY_PATH,
    )
    .await?;
    Command::new("chown")
        .arg("root:startos")
        .arg(OS_ID_KEY_PATH)
        .invoke(ErrorKind::Filesystem)
        .await?;
    let hostname = ServerHostname::load(peek.as_public().as_server_info())?;
    crate::ssh::sync_keys(
        &hostname,
        &peek.as_private().as_ssh_privkey().de()?,
        &peek.as_private().as_ssh_pubkeys().de()?,
        SSH_DIR,
    )
    .await?;
    crate::ssh::sync_keys(
        &hostname,
        &peek.as_private().as_ssh_privkey().de()?,
        &Default::default(),
        "/root/.ssh",
    )
    .await?;
    load_ssh_keys.complete();

    let account = AccountInfo::load(&peek)?;

    start_net.start();
    let net_ctrl = Arc::new(
        NetController::init(
            db.clone(),
            dns,
            webserver,
            cfg.socks_listen.unwrap_or(DEFAULT_SOCKS_LISTEN),
            cfg.max_proxy_conns_per_target
                .unwrap_or(crate::net::vhost::MAX_PROXY_CONNS_PER_TARGET),
        )
        .await?,
    );
    let os_net_service = net_ctrl.os_bindings().await?;
    start_net.complete();

    mount_logs.start();
    let log_dir = Path::new(MAIN_DATA).join("logs");
    if tokio::fs::metadata(&log_dir).await.is_err() {
        tokio::fs::create_dir_all(&log_dir).await?;
    }
    let current_machine_id = tokio::fs::read_to_string("/etc/machine-id").await?;
    let mut machine_ids = tokio::fs::read_dir(&log_dir).await?;
    while let Some(machine_id) = machine_ids.next_entry().await? {
        if machine_id.file_name().to_string_lossy().trim() != current_machine_id.trim() {
            tokio::fs::remove_dir_all(machine_id.path()).await?;
        }
    }
    crate::disk::mount::util::bind(&log_dir, "/var/log/journal", false).await?;
    match Command::new("chattr")
        .arg("-R")
        .arg("+C")
        .arg("/var/log/journal")
        .env("LANG", "C.UTF-8")
        .invoke(ErrorKind::Filesystem)
        .await
    {
        Ok(_) => Ok(()),
        Err(e) if e.source.to_string().contains("Operation not supported") => Ok(()),
        Err(e) => Err(e),
    }?;
    Command::new("systemctl")
        .arg("restart")
        .arg("systemd-journald")
        .invoke(crate::ErrorKind::Journald)
        .await?;
    Command::new("killall")
        .arg("journalctl")
        .invoke(crate::ErrorKind::Journald)
        .await
        .ok();
    mount_logs.complete();
    tokio::io::copy(
        &mut open_file("/run/startos/init.log").await?,
        &mut tokio::io::stderr(),
    )
    .await?;

    load_ca_cert.start();
    // write to ca cert store
    tokio::fs::write(
        "/usr/local/share/ca-certificates/startos-root-ca.crt",
        account.root_ca_cert.to_pem()?,
    )
    .await?;
    crate::system::trust_ca::update_trust_store().await?;
    load_ca_cert.complete();

    load_wifi.start();
    let system_product_name = crate::firmware::system_product_name().await.log_err();
    crate::net::wifi::synchronize_network_manager(
        MAIN_DATA,
        &peek
            .as_public()
            .as_server_info()
            .as_network()
            .as_wifi()
            .de()
            .unwrap_or_default(),
        system_product_name.as_deref(),
    )
    .await?;
    load_wifi.complete();

    init_tmp.start();
    let tmp_dir = Path::new(PACKAGE_DATA).join("tmp");
    crate::util::io::delete_dir(&tmp_dir).await?;
    if tokio::fs::metadata(&tmp_dir).await.is_err() {
        tokio::fs::create_dir_all(&tmp_dir).await?;
    }
    let tmp_var = Path::new(PACKAGE_DATA).join("tmp/var");
    crate::util::io::delete_dir(&tmp_var).await?;
    crate::disk::mount::util::bind(&tmp_var, "/var/tmp", false).await?;
    let downloading = Path::new(PACKAGE_DATA).join("archive/downloading");
    crate::util::io::delete_dir(&downloading).await?;
    init_tmp.complete();

    let server_info = db.peek().await.into_public().into_server_info();
    set_governor.start();
    let selected_governor = server_info.as_governor().de()?;
    let available_governors = cpupower::get_available_governors()
        .await
        .log_err()
        .unwrap_or_default();
    let governor = match &selected_governor {
        Some(governor) if available_governors.contains(governor) => Some(governor),
        Some(governor) => {
            tracing::warn!(
                "{}",
                t!("init.cpu-governor-not-available", governor = governor)
            );
            None
        }
        None => cpupower::preferred_governor(&available_governors),
    };
    if let Some(governor) = governor {
        tracing::info!("{}", t!("init.setting-cpu-governor", governor = governor));
        cpupower::set_governor(governor).await.log_err();
    }

    let selected_epp = server_info.as_epp().de()?;
    let epp = cpupower::preferred_epp(selected_epp, system_product_name.as_deref());
    if let Some(epp) = &epp {
        let available_epps = cpupower::get_available_epps()
            .await
            .log_err()
            .unwrap_or_default();
        if available_epps.contains(epp) {
            tracing::info!("{}", t!("init.setting-cpu-epp", epp = epp));
            cpupower::set_epp(epp).await.log_err();
        } else {
            tracing::warn!("{}", t!("init.cpu-epp-not-available", epp = epp));
        }
    }
    set_governor.complete();

    sync_clock.start();
    let mut ntp_synced = false;
    let mut not_made_progress = 0u32;
    for _ in 0..1800 {
        // a failed query is "don't know yet", not a boot failure — an Err escaping
        // this loop drops the server into Diagnostic Mode
        if check_time_is_synchronized()
            .await
            .log_err()
            .unwrap_or(false)
        {
            ntp_synced = true;
            break;
        }
        let t = SystemTime::now();
        tokio::time::sleep(Duration::from_secs(1)).await;
        if t.elapsed()
            .map(|t| t > Duration::from_secs_f64(1.1))
            .unwrap_or(true)
        {
            not_made_progress = 0;
        } else {
            not_made_progress += 1;
        }
        if not_made_progress > 30 {
            break;
        }
    }
    if !ntp_synced {
        tracing::warn!("{}", t!("init.clock-sync-timeout"));
    }
    sync_clock.complete();

    enable_zram.start();
    if server_info.as_zram().de()? {
        crate::system::enable_zram().await?;
        tracing::info!("{}", t!("init.enabled-zram"));
    }
    enable_zram.complete();

    // Cap the aggregate memory of the service-container slice (services.slice)
    // so the host management plane (startd, sshd) always keeps its reservation
    // and a burst of concurrent installs can't swap-thrash the box into a wedge;
    // systemd-oomd reclaims/kills within the slice under sustained pressure.
    // Best-effort: a failure here must never block boot.
    if let Err(e) = crate::system::limit_container_memory().await {
        tracing::warn!("could not cap service-container memory: {e}");
        tracing::debug!("{e:?}");
    }

    update_server_info.start();
    sync_kiosk(server_info.as_kiosk().de()?.unwrap_or(false)).await?;
    if let Err(e) = crate::nut::sync_nut(server_info.as_nut().de().unwrap_or_default()).await {
        tracing::warn!("could not synchronize NUT configuration: {e}");
        tracing::debug!("{e:?}");
    }
    let ram = get_mem_info().await?.total.0 as u64 * 1024 * 1024;
    let devices = lshw().await?;
    let status_info = ServerStatus {
        update_progress: None,
        backup_progress: None,
        backup_conversions: Default::default(),
        shutting_down: false,
        restarting: false,
        restart: None,
    };
    db.mutate(|v| {
        let server_info = v.as_public_mut().as_server_info_mut();
        server_info.as_ntp_synced_mut().ser(&ntp_synced)?;
        server_info.as_ram_mut().ser(&ram)?;
        server_info.as_devices_mut().ser(&devices)?;
        server_info.as_status_info_mut().ser(&status_info)?;
        Ok(())
    })
    .await
    .result?;
    update_server_info.complete();

    launch_service_network.start();
    Command::new("systemctl")
        .arg("start")
        .arg("lxc-net.service")
        .invoke(ErrorKind::Lxc)
        .await?;
    launch_service_network.complete();

    validate_db.start();
    db.mutate(|d| {
        let mut model = d.de()?;
        crate::shared_volumes::recover_pending_deletions(
            &mut model.public.shared_volumes,
            d.as_private().as_value(),
        )?;
        d.ser(&model)?;
        crate::backup::target::catalog::initialize(d)
    })
    .await
    .result?;
    validate_db.complete();

    if let Some(progress) = postinit {
        run_script("/media/startos/config/postinit.sh", progress).await;
    }

    tracing::info!("{}", t!("init.system-initialized"));

    Ok(InitResult {
        net_ctrl,
        os_net_service,
    })
}

pub fn init_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "logs",
            crate::system::logs::<InitContext>(WebSocketPolicy::Public)
                .with_about("about.display-os-logs"),
        )
        .subcommand(
            "logs",
            from_fn_async(crate::logs::cli_logs::<InitContext, Empty>)
                .no_ts()
                .no_display()
                .with_about("about.display-os-logs"),
        )
        .subcommand(
            "kernel-logs",
            crate::system::kernel_logs::<InitContext>(WebSocketPolicy::Public)
                .with_about("about.display-kernel-logs"),
        )
        .subcommand(
            "kernel-logs",
            from_fn_async(crate::logs::cli_logs::<InitContext, Empty>)
                .no_ts()
                .no_display()
                .with_about("about.display-kernel-logs"),
        )
        .subcommand("subscribe", from_fn_async(init_progress).no_cli())
        .subcommand(
            "subscribe",
            from_fn_async(cli_init_progress)
                .no_display()
                .with_about("about.get-initialization-progress"),
        )
}

#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct InitProgressRes {
    pub progress: FullProgress,
    /// Present for ordinary replies; typed streams use same-ID events.
    pub guid: Option<Guid>,
}

rpc_toolkit::reflect_ts!(InitProgressRes);
rpc_toolkit::ts_export!(InitProgressRes, namespaces = [""]);

pub async fn init_progress(
    HandlerArgs {
        context: ctx,
        rpc_stream,
        ..
    }: HandlerArgs<InitContext, Empty, Empty, FullProgress>,
) -> Result<InitProgressRes, Error> {
    let progress = ctx.progress.snapshot();
    let source = ctx.progress_stream();
    if let Some(stream) = rpc_stream {
        let observer = ctx.observers.subscribe()?;
        tokio::spawn(async move {
            let _observer = observer;
            let result = stream.forward(source).await;
            let _ = stream.close_and_flush(result).await;
        });
        return Ok(InitProgressRes {
            progress,
            guid: None,
        });
    }
    drop(ctx.observers.subscribe()?);
    let observers = ctx.observers.clone();
    let guid = Guid::new();
    ctx.rpc_continuations
        .add(
            guid.clone(),
            RpcContinuation::ws(
                WebSocketPolicy::Public,
                ctx.secure_session().cloned(),
                |ws| async move {
                    let result = match observers.subscribe() {
                        Ok(observer) => {
                            let _observer = observer;
                            crate::progress::forward_progress(ws, source).await
                        }
                        Err(error) => ws.close_result(Err::<String, _>(error)).await,
                    };
                    if let Err(e) = result {
                        if !crate::util::net::is_ws_reset_without_close(&e) {
                            tracing::error!("{}", t!("init.error-closing-websocket", error = e));
                            tracing::debug!("{e:?}");
                        }
                    }
                },
                Duration::from_secs(30),
            ),
        )
        .await;
    Ok(InitProgressRes {
        progress,
        guid: Some(guid),
    })
}

pub async fn cli_init_progress(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        raw_params,
        ..
    }: HandlerArgs<CliContext>,
) -> Result<(), Error> {
    let response = ctx
        .call_startup_stream::<FullProgress, ()>(
            &parent_method.into_iter().chain(method).join("."),
            raw_params,
            http::HeaderMap::new(),
        )
        .await?;
    crate::progress::with_cli_progress(response.items, &t!("init.initializing")).await
}

#[cfg(test)]
mod tests {
    use super::persist_pstore_archive;

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn clock_observation_does_not_launch_timedatectl() {
        use std::os::unix::fs::PermissionsExt;

        const CHILD: &str = "STARTOS_CLOCK_OBSERVATION_TEST";
        if std::env::var_os(CHILD).is_none() {
            let directory = tempfile::tempdir().unwrap();
            let program = directory.path().join("timedatectl");
            let marker = directory.path().join("invocations");
            std::fs::write(
                &program,
                "#!/bin/sh\nprintf x >> \"$STARTOS_CLOCK_INVOCATIONS\"\nprintf 'NTPSynchronized=yes\\n'\n",
            )
            .unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
            let output = tokio::time::timeout(
                std::time::Duration::from_secs(15),
                tokio::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "init::tests::clock_observation_does_not_launch_timedatectl",
                        "--nocapture",
                    ])
                    .env(CHILD, "1")
                    .env("PATH", directory.path())
                    .env("STARTOS_CLOCK_INVOCATIONS", &marker)
                    .kill_on_drop(true)
                    .output(),
            )
            .await
            .unwrap()
            .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let launches = if marker.exists() {
                std::fs::read(marker).unwrap().len()
            } else {
                0
            };
            assert_eq!(launches, 0, "clock observations launched timedatectl");
            return;
        }
        for _ in 0..16 {
            assert!(super::check_time_is_synchronized().await.is_ok());
            crate::context::startup::StartupRpc::clock_synced().await;
        }
        crate::net::ssl::root_ca_start_time().await;
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn kernel_ntp_observation_matches_the_systemd_maximum_error_boundary() {
        let mut sample: libc::timex = unsafe { std::mem::zeroed() };
        sample.maxerror = 15_999_999;
        assert!(super::ntp_synchronized(libc::TIME_OK, &sample));
        sample.maxerror = 16_000_000;
        assert!(!super::ntp_synchronized(libc::TIME_OK, &sample));
        sample.maxerror = 0;
        sample.status = libc::STA_UNSYNC;
        assert!(super::ntp_synchronized(libc::TIME_ERROR, &sample));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn kernel_ntp_query_failure_does_not_establish_synchronization() {
        let sample: libc::timex = unsafe { std::mem::zeroed() };
        assert!(!super::ntp_synchronized(-1, &sample));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn kernel_ntp_observations_retain_changes_in_both_directions() {
        let mut sample: libc::timex = unsafe { std::mem::zeroed() };
        for (maximum_error, synchronized) in [(16_000_000, false), (0, true), (16_000_000, false)] {
            sample.maxerror = maximum_error;
            assert_eq!(
                super::ntp_synchronized(libc::TIME_OK, &sample),
                synchronized
            );
        }
    }

    #[tokio::test]
    async fn pstore_archive_preserves_nested_and_flat_files() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("archive");
        let destination = root.path().join("main/pstore");
        let efi = "2026-06-01-120000/dmesg-efi-123456789";
        let ramoops = "dmesg-ramoops-0";
        let console = "console-ramoops-0";
        tokio::fs::create_dir_all(source.join("2026-06-01-120000"))
            .await
            .unwrap();
        tokio::fs::write(source.join(efi), b"EFI panic\0\xff")
            .await
            .unwrap();
        tokio::fs::write(source.join(ramoops), b"ramoops panic")
            .await
            .unwrap();
        tokio::fs::write(source.join(console), b"console output")
            .await
            .unwrap();

        persist_pstore_archive(&source, &destination).await.unwrap();
        persist_pstore_archive(&source, &destination).await.unwrap();
        tokio::fs::remove_dir_all(&source).await.unwrap();
        tokio::fs::create_dir(&source).await.unwrap();
        persist_pstore_archive(&source, &destination).await.unwrap();
        tokio::fs::remove_dir(&source).await.unwrap();
        persist_pstore_archive(&source, &destination).await.unwrap();

        assert_eq!(
            tokio::fs::read(destination.join(efi)).await.unwrap(),
            b"EFI panic\0\xff"
        );
        assert_eq!(
            tokio::fs::read(destination.join(ramoops)).await.unwrap(),
            b"ramoops panic"
        );
        assert_eq!(
            tokio::fs::read(destination.join(console)).await.unwrap(),
            b"console output"
        );

        tokio::fs::create_dir_all(&source).await.unwrap();
        tokio::fs::write(source.join("dmesg-ramoops-1"), b"later panic")
            .await
            .unwrap();
        persist_pstore_archive(&source, &destination).await.unwrap();
        assert_eq!(
            tokio::fs::read(destination.join("dmesg-ramoops-1"))
                .await
                .unwrap(),
            b"later panic"
        );
        assert_eq!(
            tokio::fs::read(destination.join(ramoops)).await.unwrap(),
            b"ramoops panic"
        );
    }

    #[tokio::test]
    async fn pstore_archive_empty_or_absent_source_leaves_destination_untouched() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("archive");
        let destination = root.path().join("main/pstore");
        persist_pstore_archive(&source, &destination).await.unwrap();
        assert!(!destination.exists());
        tokio::fs::create_dir(&source).await.unwrap();
        persist_pstore_archive(&source, &destination).await.unwrap();
        assert!(!destination.exists());
    }

    #[tokio::test]
    async fn pstore_archive_reports_copy_errors() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("archive");
        let destination = root.path().join("pstore");
        tokio::fs::create_dir(&source).await.unwrap();
        tokio::fs::write(source.join("dmesg-ramoops-0"), b"panic")
            .await
            .unwrap();
        tokio::fs::write(&destination, b"existing evidence")
            .await
            .unwrap();

        assert!(persist_pstore_archive(&source, &destination).await.is_err());
        assert_eq!(
            tokio::fs::read(&destination).await.unwrap(),
            b"existing evidence"
        );
        assert_eq!(
            tokio::fs::read(source.join("dmesg-ramoops-0"))
                .await
                .unwrap(),
            b"panic"
        );
    }
}
