use std::collections::BTreeSet;
use std::fmt;
use std::time::Duration;

use chrono::Utc;
use clap::Parser;
use color_eyre::eyre::eyre;
use futures::FutureExt;
use imbl_value::InternedString;
use rpc_toolkit::{Context, Empty, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tokio::process::Command;
use tokio::sync::broadcast::Receiver;
use tracing::instrument;
use ts_rs::TS;

use crate::bins::set_locale;
use crate::context::{CliContext, RpcContext};
use crate::db::model::public::RestartReason;
use crate::disk::util::{get_available, get_used};
use crate::logs::{LogSource, LogsParams, SYSTEM_UNIT};
use crate::prelude::*;
use crate::registry::device_info::DeviceInfo;
use crate::rpc_continuations::{Guid, RpcContinuation, RpcContinuations};
use crate::shutdown::Shutdown;
use crate::util::Invoke;
use crate::util::cpupower::{Governor, get_available_governors, set_governor};
use crate::util::io::{copy_file, open_file, write_file_atomic};
use crate::util::serde::{HandlerExtSerde, WithIoFormat, display_serializable};
use crate::util::sync::Watch;
use crate::{MAIN_DATA, PACKAGE_DATA};

pub fn experimental<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "zram",
            from_fn_async(zram)
                .no_display()
                .with_about("about.enable-zram")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "governor",
            from_fn_async(governor)
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| {
                    display_governor_info(handle.params, result)
                })
                .with_about("about.show-cpu-governors")
                .with_call_remote::<CliContext>(),
        )
}

pub async fn enable_zram() -> Result<(), Error> {
    let mem_info = get_mem_info().await?;
    Command::new("modprobe")
        .arg("zram")
        .invoke(ErrorKind::Zram)
        .await?;
    tokio::fs::write("/sys/block/zram0/comp_algorithm", "lz4")
        .await
        .with_kind(ErrorKind::Zram)?;
    tokio::fs::write(
        "/sys/block/zram0/disksize",
        format!("{}M", mem_info.total.0 as u64 / 4),
    )
    .await
    .with_kind(ErrorKind::Zram)?;
    Command::new("mkswap")
        .arg("/dev/zram0")
        .invoke(ErrorKind::Zram)
        .await?;
    Command::new("swapon")
        .arg("-p")
        .arg("5")
        .arg("/dev/zram0")
        .invoke(ErrorKind::Zram)
        .await?;
    Ok(())
}

/// The systemd slice that holds every service container's cgroup — lxc places
/// containers under it (see `core/src/lxc/config.template`) and the unit
/// (`core/services.slice`) opts it into `systemd-oomd` PSI monitoring. Capping
/// it bounds the *aggregate* memory of all service containers.
const CONTAINER_SLICE: &str = "services.slice";

/// Bound the memory all service containers may collectively use, reserving a
/// fixed amount for the host management plane (startd, sshd, journald). The cap
/// is applied with `systemctl set-property` so systemd owns it (survives a
/// daemon-reload) and so it composes with the slice's `ManagedOOMMemoryPressure`
/// — under sustained pressure `systemd-oomd` kills the heaviest **container**
/// cgroup in this slice, never the host plane (which `MemoryMin` protects).
///
/// `HOST_RESERVE_MIB` is a constant, not a fraction of RAM: the host plane's
/// footprint (~210 MiB idle + install/admin headroom) doesn't scale with box
/// size. It is kept in lock-step with the `MemoryMin` drop-ins shipped by
/// `projects/start-os/debian/postinst` (`system.slice` 768M + `user.slice` 256M = 1 GiB).
pub async fn limit_container_memory() -> Result<(), Error> {
    const HOST_RESERVE_MIB: u64 = 1024;
    let total_mib = get_mem_info().await?.total.0 as u64;
    let container_max_mib = total_mib.saturating_sub(HOST_RESERVE_MIB).max(512);
    // begin proactive reclaim a little before the hard cap so the limit
    // degrades gracefully (throttle) instead of going straight to OOM.
    let container_high_mib = container_max_mib - container_max_mib / 10;

    Command::new("systemctl")
        .arg("set-property")
        .arg("--runtime")
        .arg(CONTAINER_SLICE)
        .arg(format!("MemoryHigh={container_high_mib}M"))
        .arg(format!("MemoryMax={container_max_mib}M"))
        .invoke(ErrorKind::Systemd)
        .await?;
    // Materialize the slice now so its cgroup exists with the cap + ManagedOOM
    // settings applied before any container is placed in it.
    Command::new("systemctl")
        .arg("start")
        .arg(CONTAINER_SLICE)
        .invoke(ErrorKind::Systemd)
        .await?;

    tracing::info!(
        "service containers ({CONTAINER_SLICE}) capped at {container_max_mib} MiB \
         (high {container_high_mib} MiB); {HOST_RESERVE_MIB} MiB reserved for the \
         host plane of {total_mib} MiB total"
    );
    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ZramParams {
    #[arg(long, help = "help.arg.enable-zram")]
    enable: bool,
}

pub async fn zram(ctx: RpcContext, ZramParams { enable }: ZramParams) -> Result<(), Error> {
    let db = ctx.db.peek().await;

    let zram = db.as_public().as_server_info().as_zram().de()?;
    if enable == zram {
        return Ok(());
    }
    if enable {
        enable_zram().await?;
    } else {
        Command::new("swapoff")
            .arg("/dev/zram0")
            .invoke(ErrorKind::Zram)
            .await?;
        tokio::fs::write("/sys/block/zram0/reset", "1")
            .await
            .with_kind(ErrorKind::Zram)?;
    }
    ctx.db
        .mutate(|v| {
            v.as_public_mut()
                .as_server_info_mut()
                .as_zram_mut()
                .ser(&enable)?;
            Ok(())
        })
        .await
        .result?;
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GovernorInfo {
    current: Option<Governor>,
    available: BTreeSet<Governor>,
}

fn display_governor_info(
    params: WithIoFormat<GovernorParams>,
    result: GovernorInfo,
) -> Result<(), Error> {
    use prettytable::*;

    if let Some(format) = params.format {
        return display_serializable(format, params);
    }

    let mut table = Table::new();
    table.add_row(row![bc -> "GOVERNORS"]);
    for entry in result.available {
        if Some(&entry) == result.current.as_ref() {
            table.add_row(row![g -> format!("* {entry} (current)")]);
        } else {
            table.add_row(row![entry]);
        }
    }
    table.print_tty(false)?;
    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct GovernorParams {
    #[arg(help = "help.arg.governor-name")]
    set: Option<Governor>,
}

pub async fn governor(
    ctx: RpcContext,
    GovernorParams { set, .. }: GovernorParams,
) -> Result<GovernorInfo, Error> {
    let available = get_available_governors().await?;
    if let Some(set) = set {
        if !available.contains(&set) {
            return Err(Error::new(
                eyre!(
                    "{}",
                    t!("system.governor-not-available", governor = set.to_string())
                ),
                ErrorKind::InvalidRequest,
            ));
        }
        set_governor(&set).await?;
        ctx.db
            .mutate(|d| {
                d.as_public_mut()
                    .as_server_info_mut()
                    .as_governor_mut()
                    .ser(&Some(set))
            })
            .await
            .result?;
    }
    let current = ctx
        .db
        .peek()
        .await
        .as_public()
        .as_server_info()
        .as_governor()
        .de()?;
    Ok(GovernorInfo { current, available })
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TimeInfo {
    now: String,
    uptime: u64,
}

pub fn display_time(params: WithIoFormat<Empty>, arg: TimeInfo) -> Result<(), Error> {
    use std::fmt::Write;

    use prettytable::*;

    if let Some(format) = params.format {
        return display_serializable(format, arg);
    }

    let days = arg.uptime / (24 * 60 * 60);
    let days_s = arg.uptime % (24 * 60 * 60);
    let hours = days_s / (60 * 60);
    let hours_s = arg.uptime % (60 * 60);
    let minutes = hours_s / 60;
    let seconds = arg.uptime % 60;
    let mut uptime_string = String::new();
    if days > 0 {
        write!(&mut uptime_string, "{days} days").unwrap();
    }
    if hours > 0 {
        if !uptime_string.is_empty() {
            uptime_string += ", ";
        }
        write!(&mut uptime_string, "{hours} hours").unwrap();
    }
    if minutes > 0 {
        if !uptime_string.is_empty() {
            uptime_string += ", ";
        }
        write!(&mut uptime_string, "{minutes} minutes").unwrap();
    }
    if !uptime_string.is_empty() {
        uptime_string += ", ";
    }
    write!(&mut uptime_string, "{seconds} seconds").unwrap();

    let mut table = Table::new();
    table.add_row(row![bc -> "NOW", &arg.now]);
    table.add_row(row![bc -> "UPTIME", &uptime_string]);
    table.print_tty(false)?;
    Ok(())
}

pub async fn time(ctx: RpcContext, _: Empty) -> Result<TimeInfo, Error> {
    Ok(TimeInfo {
        now: Utc::now().to_rfc3339(),
        uptime: ctx.start_time.elapsed().as_secs(),
    })
}

pub async fn device_info(ctx: RpcContext) -> Result<DeviceInfo, Error> {
    DeviceInfo::load(&ctx).await
}

pub fn display_device_info(params: WithIoFormat<Empty>, info: DeviceInfo) -> Result<(), Error> {
    use prettytable::*;

    if let Some(format) = params.format {
        return display_serializable(format, info);
    }

    let mut table = Table::new();
    table.add_row(row![br -> "PLATFORM", &*info.os.platform]);
    table.add_row(row![br -> "OS VERSION", info.os.version.to_string()]);
    table.add_row(row![br -> "OS COMPAT", info.os.compat.to_string()]);
    if let Some(lang) = &info.os.language {
        table.add_row(row![br -> "LANGUAGE", &**lang]);
    }
    if let Some(hw) = &info.hardware {
        table.add_row(row![br -> "ARCH", &*hw.arch]);
        table.add_row(row![br -> "RAM", format_ram(hw.ram)]);
        if let Some(devices) = &hw.devices {
            for dev in devices {
                let (class, desc) = match dev {
                    crate::util::lshw::LshwDevice::Processor(p) => (
                        "PROCESSOR",
                        p.product.as_deref().unwrap_or("unknown").to_string(),
                    ),
                    crate::util::lshw::LshwDevice::Display(d) => (
                        "DISPLAY",
                        format!(
                            "{}{}",
                            d.product.as_deref().unwrap_or("unknown"),
                            d.driver
                                .as_deref()
                                .map(|drv| format!(" ({})", drv))
                                .unwrap_or_default()
                        ),
                    ),
                };
                table.add_row(row![br -> class, desc]);
            }
        }
    }
    table.print_tty(false)?;
    Ok(())
}

fn format_ram(bytes: u64) -> String {
    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    }
}

pub fn logs<C: Context + AsRef<RpcContinuations>>() -> ParentHandler<C, LogsParams> {
    crate::logs::logs(|_: &C, _| async { Ok(LogSource::Unit(SYSTEM_UNIT)) })
}

pub fn kernel_logs<C: Context + AsRef<RpcContinuations>>() -> ParentHandler<C, LogsParams> {
    crate::logs::logs(|_: &C, _| async { Ok(LogSource::Kernel) })
}

const DISABLE_KIOSK_PATH: &str =
    "/media/startos/config/overlay/etc/systemd/system/getty@tty1.service.d/autologin.conf";

pub async fn sync_kiosk(kiosk: bool) -> Result<(), Error> {
    if kiosk {
        enable_kiosk().await?;
    } else {
        disable_kiosk().await?;
    }
    Ok(())
}

pub async fn enable_kiosk() -> Result<(), Error> {
    if tokio::fs::metadata(DISABLE_KIOSK_PATH).await.is_ok() {
        crate::util::io::delete_file(DISABLE_KIOSK_PATH).await?;
    }
    Ok(())
}

pub async fn disable_kiosk() -> Result<(), Error> {
    crate::util::io::create_file(DISABLE_KIOSK_PATH)
        .await?
        .sync_all()
        .await?;
    Ok(())
}

pub fn kiosk<C: Context>() -> ParentHandler<C> {
    ParentHandler::<C>::new()
        .subcommand(
            "enable",
            from_fn_async(|ctx: RpcContext| async move {
                ctx.db
                    .mutate(|db| {
                        let server_info = db.as_public_mut().as_server_info_mut();
                        server_info.as_kiosk_mut().ser(&Some(true))?;
                        server_info
                            .as_status_info_mut()
                            .as_restart_mut()
                            .ser(&Some(RestartReason::Kiosk))
                    })
                    .await
                    .result?;
                enable_kiosk().await
            })
            .no_display()
            .with_about("about.enable-kiosk-mode")
            .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "disable",
            from_fn_async(|ctx: RpcContext| async move {
                ctx.db
                    .mutate(|db| {
                        let server_info = db.as_public_mut().as_server_info_mut();
                        server_info.as_kiosk_mut().ser(&Some(false))?;
                        server_info
                            .as_status_info_mut()
                            .as_restart_mut()
                            .ser(&Some(RestartReason::Kiosk))
                    })
                    .await
                    .result?;
                disable_kiosk().await
            })
            .no_display()
            .with_about("about.disable-kiosk-mode")
            .with_call_remote::<CliContext>(),
        )
}

#[derive(Serialize, Deserialize)]
pub struct MetricLeaf<T> {
    value: T,
    unit: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, TS)]
#[ts(type = "{ value: string, unit: string }")]
pub struct Celsius(f64);
impl fmt::Display for Celsius {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:.1}°C", self.0)
    }
}
impl Serialize for Celsius {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        MetricLeaf {
            value: format!("{:.1}", self.0),
            unit: Some(String::from("°C")),
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Celsius {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = MetricLeaf::<String>::deserialize(deserializer)?;
        Ok(Celsius(s.value.parse().map_err(serde::de::Error::custom)?))
    }
}
#[derive(Clone, Debug, PartialEq, PartialOrd, TS)]
#[ts(type = "{ value: string, unit: string }")]
pub struct Percentage(f64);
impl Serialize for Percentage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        MetricLeaf {
            value: format!("{:.1}", self.0),
            unit: Some(String::from("%")),
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Percentage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = MetricLeaf::<String>::deserialize(deserializer)?;
        Ok(Percentage(
            s.value.parse().map_err(serde::de::Error::custom)?,
        ))
    }
}

#[derive(Clone, Debug, TS)]
#[ts(type = "{ value: string, unit: string }")]
pub struct MebiBytes(pub f64);
impl Serialize for MebiBytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        MetricLeaf {
            value: format!("{:.2}", self.0),
            unit: Some(String::from("MiB")),
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for MebiBytes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = MetricLeaf::<String>::deserialize(deserializer)?;
        Ok(MebiBytes(
            s.value.parse().map_err(serde::de::Error::custom)?,
        ))
    }
}

#[derive(Clone, Debug, PartialEq, PartialOrd, TS)]
#[ts(type = "{ value: string, unit: string }")]
pub struct GigaBytes(f64);
impl Serialize for GigaBytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        MetricLeaf {
            value: format!("{:.2}", self.0),
            unit: Some(String::from("GB")),
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for GigaBytes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = MetricLeaf::<String>::deserialize(deserializer)?;
        Ok(GigaBytes(
            s.value.parse().map_err(serde::de::Error::custom)?,
        ))
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
pub struct MetricsGeneral {
    pub temperature: Option<Celsius>,
}

#[derive(Deserialize, Serialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
pub struct MetricsMemory {
    pub percentage_used: Percentage,
    pub total: MebiBytes,
    pub available: MebiBytes,
    pub used: MebiBytes,
    pub zram_total: MebiBytes,
    pub zram_available: MebiBytes,
    pub zram_used: MebiBytes,
}

#[derive(Deserialize, Serialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
pub struct MetricsCpu {
    percentage_used: Percentage,
    idle: Percentage,
    user_space: Percentage,
    kernel_space: Percentage,
    wait: Percentage,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct MetricsDisk {
    percentage_used: Percentage,
    used: GigaBytes,
    available: GigaBytes,
    capacity: GigaBytes,
}

#[derive(Deserialize, Serialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Metrics {
    general: MetricsGeneral,
    memory: MetricsMemory,
    cpu: MetricsCpu,
    disk: MetricsDisk,
}

// #[command(display(display_serializable))]
pub async fn metrics(ctx: RpcContext) -> Result<Metrics, Error> {
    ctx.metrics_cache.read().or_not_found("No Metrics Found")
}

#[derive(Deserialize, Serialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MetricsFollowResponse {
    pub guid: Guid,
    pub metrics: Metrics,
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct MetricsFollowParams {
    #[ts(skip)]
    #[serde(rename = "__auth_session")] // from Auth middleware
    session: Option<InternedString>,
}

pub async fn metrics_follow(
    ctx: RpcContext,
    MetricsFollowParams { session }: MetricsFollowParams,
) -> Result<MetricsFollowResponse, Error> {
    let mut local_cache = ctx.metrics_cache.clone();
    let metrics = local_cache
        .peek_and_mark_seen(|m| m.clone())
        .or_not_found("No Metrics Found")?;
    let guid = Guid::new();
    ctx.rpc_continuations
        .add(
            guid.clone(),
            RpcContinuation::ws_authed(
                ctx.clone(),
                session,
                |mut ws| async move {
                    let res = async {
                        loop {
                            use axum::extract::ws::Message;
                            tokio::select! {
                                _ = local_cache.changed() => {
                                    ws.send(Message::Text(
                                        local_cache
                                            .peek(|m| serde_json::to_string(&m))
                                            .with_kind(ErrorKind::Serialization)?
                                            .into(),
                                    )).await.with_kind(ErrorKind::Network)?;
                                }
                                msg = ws.recv() => {
                                    if msg.transpose().with_kind(crate::ErrorKind::Network)?.is_none() {
                                        break;
                                    }
                                }
                            }
                        }
                        Ok::<_, Error>("complete")
                    }
                    .await;
                    ws.close_result(res).await.log_err();
                },
                Duration::from_secs(30),
            ),
        )
        .await;
    Ok(MetricsFollowResponse { guid, metrics })
}

pub async fn launch_metrics_task<F: FnMut() -> Receiver<Option<Shutdown>>>(
    cache: &Watch<Option<Metrics>>,
    mut mk_shutdown: F,
) {
    // fetch init temp
    let init_temp = match get_temp().await {
        Ok(a) => Some(a),
        Err(e) => {
            tracing::error!(
                "{}",
                t!(
                    "system.could-not-get-initial-temperature",
                    error = e.to_string()
                )
            );
            tracing::debug!("{:?}", e);
            None
        }
    };
    // fetch init cpu
    let init_cpu;
    let proc_stat;
    loop {
        match get_proc_stat().await {
            Ok(mut ps) => match get_cpu_info(&mut ps).await {
                Ok(mc) => {
                    proc_stat = ps;
                    init_cpu = mc;
                    break;
                }
                Err(e) => {
                    tracing::error!(
                        "{}",
                        t!(
                            "system.could-not-get-initial-cpu-info",
                            error = e.to_string()
                        )
                    );
                    tracing::debug!("{:?}", e);
                }
            },
            Err(e) => {
                tracing::error!(
                    "{}",
                    t!(
                        "system.could-not-get-initial-proc-stat",
                        error = e.to_string()
                    )
                );
                tracing::debug!("{:?}", e);
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
    // fetch init memory
    let init_mem;
    loop {
        match get_mem_info().await {
            Ok(a) => {
                init_mem = a;
                break;
            }
            Err(e) => {
                tracing::error!(
                    "{}",
                    t!(
                        "system.could-not-get-initial-mem-info",
                        error = e.to_string()
                    )
                );
                tracing::debug!("{:?}", e);
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
    // fetch init disk usage
    let init_disk;
    loop {
        match get_disk_info().await {
            Ok(a) => {
                init_disk = a;
                break;
            }
            Err(e) => {
                tracing::error!(
                    "{}",
                    t!(
                        "system.could-not-get-initial-disk-info",
                        error = e.to_string()
                    )
                );
                tracing::debug!("{:?}", e);
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    let should_launch_temp_task = init_temp.is_some();

    cache.send(Some(Metrics {
        general: MetricsGeneral {
            temperature: init_temp,
        },
        memory: init_mem,
        cpu: init_cpu,
        disk: init_disk,
    }));

    let mut task_vec = Vec::new();
    // launch persistent temp task
    if should_launch_temp_task {
        task_vec.push(launch_temp_task(cache, mk_shutdown()).boxed());
    }
    // launch persistent cpu task
    task_vec.push(launch_cpu_task(cache, proc_stat, mk_shutdown()).boxed());
    // launch persistent mem task
    task_vec.push(launch_mem_task(cache, mk_shutdown()).boxed());
    // launch persistent disk task
    task_vec.push(launch_disk_task(cache, mk_shutdown()).boxed());

    futures::future::join_all(task_vec).await;
}

async fn launch_temp_task(
    cache: &Watch<Option<Metrics>>,
    mut shutdown: Receiver<Option<Shutdown>>,
) {
    loop {
        match get_temp().await {
            Ok(a) => {
                cache.send_if_modified(|c| {
                    c.as_mut().unwrap().general.temperature.replace(a) != Some(a)
                });
            }
            Err(e) => {
                tracing::error!(
                    "{}",
                    t!(
                        "system.could-not-get-new-temperature",
                        error = e.to_string()
                    )
                );
                tracing::debug!("{:?}", e);
            }
        }
        tokio::select! {
            _ = shutdown.recv() => return,
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(4)) => (),
        }
    }
}

async fn launch_cpu_task(
    cache: &Watch<Option<Metrics>>,
    mut init: ProcStat,
    mut shutdown: Receiver<Option<Shutdown>>,
) {
    loop {
        // read /proc/stat, diff against previous metrics, compute cpu load
        match get_cpu_info(&mut init).await {
            Ok(info) => {
                cache.send_modify(|c| c.as_mut().unwrap().cpu = info);
            }
            Err(e) => {
                tracing::error!(
                    "{}",
                    t!(
                        "system.could-not-get-new-cpu-metrics",
                        error = e.to_string()
                    )
                );
                tracing::debug!("{:?}", e);
            }
        }
        tokio::select! {
            _ = shutdown.recv() => return,
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(4)) => (),
        }
    }
}

async fn launch_mem_task(cache: &Watch<Option<Metrics>>, mut shutdown: Receiver<Option<Shutdown>>) {
    loop {
        // read /proc/meminfo
        match get_mem_info().await {
            Ok(a) => {
                cache.send_modify(|c| c.as_mut().unwrap().memory = a);
            }
            Err(e) => {
                tracing::error!(
                    "{}",
                    t!(
                        "system.could-not-get-new-memory-metrics",
                        error = e.to_string()
                    )
                );
                tracing::debug!("{:?}", e);
            }
        }
        tokio::select! {
            _ = shutdown.recv() => return,
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(4)) => (),
        }
    }
}
async fn launch_disk_task(
    cache: &Watch<Option<Metrics>>,
    mut shutdown: Receiver<Option<Shutdown>>,
) {
    loop {
        // run df and capture output
        match get_disk_info().await {
            Ok(a) => {
                cache.send_if_modified(|c| {
                    let c = c.as_mut().unwrap();
                    if c.disk != a {
                        c.disk = a;
                        true
                    } else {
                        false
                    }
                });
            }
            Err(e) => {
                tracing::error!(
                    "{}",
                    t!(
                        "system.could-not-get-new-disk-metrics",
                        error = e.to_string()
                    )
                );
                tracing::debug!("{:?}", e);
            }
        }
        tokio::select! {
            _ = shutdown.recv() => return,
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(60)) => (),
        }
    }
}

#[instrument(skip_all)]
async fn get_temp() -> Result<Celsius, Error> {
    let temp = serde_json::from_slice::<serde_json::Value>(
        &Command::new("sensors")
            .arg("-j")
            .invoke(ErrorKind::Filesystem)
            .await?,
    )
    .with_kind(ErrorKind::Deserialization)?
    .as_object()
    .into_iter()
    .flatten()
    .flat_map(|(_, v)| v.as_object())
    .flatten()
    .flat_map(|(_, v)| v.as_object())
    .flatten()
    .filter_map(|(k, v)| {
        // we have seen so far that `temp1` is always a composite reading of some sort, so we should just use that for each chip
        if k.trim() == "temp1_input" {
            v.as_f64()
        } else {
            None
        }
    })
    .reduce(f64::max)
    .ok_or_else(|| {
        Error::new(
            eyre!("{}", t!("system.no-temperatures-available")),
            ErrorKind::Filesystem,
        )
    })?;
    Ok(Celsius(temp))
}

#[derive(Debug, Clone)]
pub struct ProcStat {
    user: u64,
    nice: u64,
    system: u64,
    idle: u64,
    iowait: u64,
    irq: u64,
    softirq: u64,
    // below are only applicable to virtualized environments
    // steal: u64,
    // guest: u64,
    // guest_nice: u64,
}
impl ProcStat {
    fn total(&self) -> u64 {
        self.user + self.nice + self.system + self.idle + self.iowait + self.irq + self.softirq
    }
    fn user(&self) -> u64 {
        self.user + self.nice
    }
    fn system(&self) -> u64 {
        self.system + self.irq + self.softirq
    }
    fn used(&self) -> u64 {
        self.user() + self.system()
    }
}

#[instrument(skip_all)]
async fn get_proc_stat() -> Result<ProcStat, Error> {
    use tokio::io::AsyncBufReadExt;
    let mut cpu_line = String::new();
    let _n = tokio::io::BufReader::new(open_file("/proc/stat").await?)
        .read_line(&mut cpu_line)
        .await?;
    let stats: Vec<u64> = cpu_line
        .split_whitespace()
        .skip(1)
        .map(|s| {
            s.parse::<u64>().map_err(|e| {
                Error::new(
                    color_eyre::eyre::eyre!(
                        "{}",
                        t!("system.invalid-proc-stat-column", error = e.to_string())
                    ),
                    ErrorKind::ParseSysInfo,
                )
            })
        })
        .collect::<Result<Vec<u64>, Error>>()?;

    if stats.len() < 10 {
        Err(Error::new(
            eyre!(
                "{}",
                t!("system.columns-missing-from-proc-stat", count = stats.len())
            ),
            ErrorKind::ParseSysInfo,
        ))
    } else {
        Ok(ProcStat {
            user: stats[0],
            nice: stats[1],
            system: stats[2],
            idle: stats[3],
            iowait: stats[4],
            irq: stats[5],
            softirq: stats[6],
        })
    }
}

#[instrument(skip_all)]
async fn get_cpu_info(last: &mut ProcStat) -> Result<MetricsCpu, Error> {
    let new = get_proc_stat().await?;
    let total_old = last.total();
    let total_new = new.total();
    let total_diff = total_new - total_old;
    let res = MetricsCpu {
        user_space: Percentage((new.user() - last.user()) as f64 * 100.0 / total_diff as f64),
        kernel_space: Percentage((new.system() - last.system()) as f64 * 100.0 / total_diff as f64),
        idle: Percentage((new.idle - last.idle) as f64 * 100.0 / total_diff as f64),
        wait: Percentage((new.iowait - last.iowait) as f64 * 100.0 / total_diff as f64),
        percentage_used: Percentage((new.used() - last.used()) as f64 * 100.0 / total_diff as f64),
    };
    *last = new;
    Ok(res)
}

pub struct MemInfo {
    mem_total: Option<u64>,
    mem_free: Option<u64>,
    mem_available: Option<u64>,
    buffers: Option<u64>,
    cached: Option<u64>,
    slab: Option<u64>,
    zram_total: Option<u64>,
    zram_free: Option<u64>,
}
#[instrument(skip_all)]
pub async fn get_mem_info() -> Result<MetricsMemory, Error> {
    let contents = tokio::fs::read_to_string("/proc/meminfo").await?;
    let mut mem_info = MemInfo {
        mem_total: None,
        mem_free: None,
        mem_available: None,
        buffers: None,
        cached: None,
        slab: None,
        zram_total: None,
        zram_free: None,
    };
    fn get_num_kb(l: &str) -> Result<u64, Error> {
        let line = l.to_string();
        let e = || {
            Error::new(
                color_eyre::eyre::eyre!(
                    "{}",
                    t!("system.invalid-meminfo-line", line = line.clone())
                ),
                ErrorKind::ParseSysInfo,
            )
        };
        match l.split_whitespace().skip(1).next() {
            Some(x) => match x.parse() {
                Ok(y) => Ok(y),
                Err(_) => Err(e()),
            },
            None => Err(e()),
        }
    }
    for entry in contents.lines() {
        match entry {
            _ if entry.starts_with("MemTotal") => mem_info.mem_total = Some(get_num_kb(entry)?),
            _ if entry.starts_with("MemFree") => mem_info.mem_free = Some(get_num_kb(entry)?),
            _ if entry.starts_with("MemAvailable") => {
                mem_info.mem_available = Some(get_num_kb(entry)?)
            }
            _ if entry.starts_with("Buffers") => mem_info.buffers = Some(get_num_kb(entry)?),
            _ if entry.starts_with("Cached") => mem_info.cached = Some(get_num_kb(entry)?),
            _ if entry.starts_with("Slab") => mem_info.slab = Some(get_num_kb(entry)?),
            _ if entry.starts_with("SwapTotal") => mem_info.zram_total = Some(get_num_kb(entry)?),
            _ if entry.starts_with("SwapFree") => mem_info.zram_free = Some(get_num_kb(entry)?),
            _ => (),
        }
    }
    fn ensure_present(a: Option<u64>, field: &str) -> Result<u64, Error> {
        let field_str = field.to_string();
        a.ok_or_else(|| {
            Error::new(
                color_eyre::eyre::eyre!(
                    "{}",
                    t!(
                        "system.field-missing-from-meminfo",
                        field = field_str.clone()
                    )
                ),
                ErrorKind::ParseSysInfo,
            )
        })
    }
    let mem_total = ensure_present(mem_info.mem_total, "MemTotal")?;
    let mem_free = ensure_present(mem_info.mem_free, "MemFree")?;
    let mem_available = ensure_present(mem_info.mem_available, "MemAvailable")?;
    let buffers = ensure_present(mem_info.buffers, "Buffers")?;
    let cached = ensure_present(mem_info.cached, "Cached")?;
    let slab = ensure_present(mem_info.slab, "Slab")?;
    let zram_total_k = ensure_present(mem_info.zram_total, "SwapTotal")?;
    let zram_free_k = ensure_present(mem_info.zram_free, "SwapFree")?;

    let total = MebiBytes(mem_total as f64 / 1024.0);
    let available = MebiBytes(mem_available as f64 / 1024.0);
    let used = MebiBytes((mem_total - mem_free - buffers - cached - slab) as f64 / 1024.0);
    let zram_total = MebiBytes(zram_total_k as f64 / 1024.0);
    let zram_available = MebiBytes(zram_free_k as f64 / 1024.0);
    let zram_used = MebiBytes((zram_total_k - zram_free_k) as f64 / 1024.0);
    let percentage_used = Percentage((total.0 - available.0) / total.0 * 100.0);
    Ok(MetricsMemory {
        percentage_used,
        total,
        available,
        used,
        zram_total,
        zram_available,
        zram_used,
    })
}

#[instrument(skip_all)]
async fn get_disk_info() -> Result<MetricsDisk, Error> {
    let package_used_task = get_used(PACKAGE_DATA);
    let package_available_task = get_available(PACKAGE_DATA);
    let os_used_task = get_used(MAIN_DATA);
    let os_available_task = get_available(MAIN_DATA);

    let (package_used, package_available, os_used, os_available) = futures::try_join!(
        package_used_task,
        package_available_task,
        os_used_task,
        os_available_task,
    )?;

    let total_used = package_used + os_used;
    let total_available = package_available + os_available;
    let total_size = total_used + total_available;
    let total_percentage = total_used as f64 / total_size as f64 * 100.0f64;

    Ok(MetricsDisk {
        capacity: GigaBytes(total_size as f64 / 1_000_000_000.0),
        used: GigaBytes(total_used as f64 / 1_000_000_000.0),
        available: GigaBytes(total_available as f64 / 1_000_000_000.0),
        percentage_used: Percentage(total_percentage as f64),
    })
}

#[derive(
    Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize, TS, clap::ValueEnum,
)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum SmtpSecurity {
    #[default]
    Starttls,
    Tls,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SmtpValue {
    #[arg(long, help = "help.arg.smtp-host")]
    #[serde(alias = "server")]
    pub host: String,
    #[arg(long, help = "help.arg.smtp-port")]
    pub port: u16,
    #[arg(long, help = "help.arg.smtp-from")]
    pub from: String,
    #[arg(long, help = "help.arg.smtp-username")]
    #[serde(alias = "login")]
    pub username: String,
    #[arg(long, help = "help.arg.smtp-password")]
    pub password: Option<String>,
    #[arg(long, help = "help.arg.smtp-security")]
    #[serde(default)]
    pub security: SmtpSecurity,
}
pub async fn set_system_smtp(ctx: RpcContext, smtp: SmtpValue) -> Result<(), Error> {
    let smtp = Some(smtp);
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_smtp_mut()
                .ser(&smtp)
        })
        .await
        .result?;
    Ok(())
}
pub async fn clear_system_smtp(ctx: RpcContext) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_smtp_mut()
                .ser(&None)
        })
        .await
        .result?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Serialize, Parser)]
#[group(skip)]
pub struct SetEchoipUrlsParams {
    #[arg(help = "help.arg.echoip-urls")]
    pub urls: Vec<url::Url>,
}

pub async fn set_echoip_urls(
    ctx: RpcContext,
    SetEchoipUrlsParams { urls }: SetEchoipUrlsParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_echoip_urls_mut()
                .ser(&urls)
        })
        .await
        .result
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TestSmtpParams {
    #[arg(long, help = "help.arg.smtp-host")]
    pub host: String,
    #[arg(long, help = "help.arg.smtp-port")]
    pub port: u16,
    #[arg(long, help = "help.arg.smtp-from")]
    pub from: String,
    #[arg(long, help = "help.arg.smtp-to")]
    pub to: String,
    #[arg(long, help = "help.arg.smtp-username")]
    pub username: String,
    #[arg(long, help = "help.arg.smtp-password")]
    pub password: String,
    #[arg(long, help = "help.arg.smtp-security")]
    #[serde(default)]
    pub security: SmtpSecurity,
}
pub async fn test_smtp(
    _: RpcContext,
    TestSmtpParams {
        host,
        port,
        from,
        to,
        username,
        password,
        security,
    }: TestSmtpParams,
) -> Result<(), Error> {
    use lettre::message::header::ContentType;
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::transport::smtp::client::{Tls, TlsParameters};
    use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

    let creds = Credentials::new(username, password);
    let message = Message::builder()
        .from(from.parse()?)
        .to(to.parse()?)
        .subject("StartOS Test Email")
        .header(ContentType::TEXT_PLAIN)
        .body("This is a test email sent from your StartOS Server".to_owned())?;

    let tls_parameters = TlsParameters::new(host.clone())?;
    let transport = match security {
        SmtpSecurity::Starttls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)?
            .port(port)
            .tls(Tls::Required(tls_parameters)),
        SmtpSecurity::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&host)?
            .port(port)
            .tls(Tls::Wrapper(tls_parameters)),
    }
    .credentials(creds)
    .build();

    transport.send(message).await?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
pub struct KeyboardOptions {
    #[arg(help = "help.arg.keyboard-layout")]
    pub layout: InternedString,
    #[arg(short, long, help = "help.arg.keyboard-keymap")]
    pub keymap: Option<InternedString>,
    #[arg(short, long, help = "help.arg.keyboard-model")]
    pub model: Option<InternedString>,
    #[arg(short, long, help = "help.arg.keyboard-variant")]
    pub variant: Option<InternedString>,
    #[arg(short, long = "option", help = "help.arg.keyboard-option")]
    #[serde(default)]
    pub options: Vec<InternedString>,
}
impl KeyboardOptions {
    /// NOTE: will error if kiosk inactive
    pub async fn apply_to_session(&self) -> Result<(), Error> {
        let mut cmd = Command::new("setxkbmap");
        cmd.env("DISPLAY", ":0")
            .env("XAUTHORITY", "/home/kiosk/.Xauthority");
        cmd.arg("-layout").arg(&*self.layout);
        if let Some(variant) = self.variant.as_deref() {
            cmd.arg("-variant").arg(variant);
        }
        for option in &self.options {
            cmd.arg("-option").arg(&**option);
        }
        cmd.invoke(ErrorKind::SetSysInfo).await?;
        Ok(())
    }

    pub async fn save(&self) -> Result<(), Error> {
        write_file_atomic(
            "/media/startos/config/overlay/etc/vconsole.conf",
            format!(
                include_str!("./vconsole.conf.template"),
                model = self.model.as_deref().unwrap_or_default(),
                layout = &*self.layout,
                variant = self.variant.as_deref().unwrap_or_default(),
                options = self.options.join(","),
                keymap = self.keymap.as_deref().unwrap_or(&*self.layout),
            ),
        )
        .await?;
        write_file_atomic(
            "/media/startos/config/overlay/etc/X11/xorg.conf.d/00-keyboard.conf",
            format!(
                include_str!("./keyboard.conf.template"),
                model = self.model.as_deref().unwrap_or_default(),
                layout = &*self.layout,
                variant = self.variant.as_deref().unwrap_or_default(),
                options = self.options.join(","),
            ),
        )
        .await
    }
}

pub async fn set_keyboard(ctx: RpcContext, options: KeyboardOptions) -> Result<(), Error> {
    options.apply_to_session().await.log_err();
    options.save().await?;
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_keyboard_mut()
                .ser(&Some(options))
        })
        .await
        .result?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, Parser)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SetLanguageParams {
    #[arg(help = "help.arg.language-code")]
    pub language: InternedString,
}

pub async fn save_language(language: &str) -> Result<(), Error> {
    write_file_atomic(
        "/etc/locale.gen",
        format!("{language}.UTF-8 UTF-8\n").as_bytes(),
    )
    .await?;
    Command::new("locale-gen")
        .invoke(ErrorKind::SetSysInfo)
        .await?;
    copy_file(
        "/usr/lib/locale/locale-archive",
        "/media/startos/config/overlay/usr/lib/locale/locale-archive",
    )
    .await?;
    let locale_content = format!("LANG={language}.UTF-8\n");
    write_file_atomic(
        "/media/startos/config/overlay/etc/default/locale",
        locale_content.as_bytes(),
    )
    .await?;
    write_file_atomic(
        "/media/startos/config/overlay/etc/locale.conf",
        locale_content.as_bytes(),
    )
    .await?;
    Ok(())
}

pub async fn set_language(
    ctx: RpcContext,
    SetLanguageParams { language }: SetLanguageParams,
) -> Result<(), Error> {
    set_locale(&*language);
    save_language(&*language).await?;
    ctx.db
        .mutate(|db| {
            let server_info = db.as_public_mut().as_server_info_mut();
            if server_info.as_language().de()?.as_ref() == Some(&language) {
                return Ok(());
            }
            server_info.as_language_mut().ser(&Some(language.clone()))?;
            server_info
                .as_status_info_mut()
                .as_restart_mut()
                .ser(&Some(RestartReason::Language))
        })
        .await
        .result?;
    Ok(())
}

#[tokio::test]
#[ignore]
pub async fn test_get_temp() {
    println!("{}", get_temp().await.unwrap())
}

#[tokio::test]
pub async fn test_get_proc_stat() {
    println!("{:?}", get_proc_stat().await.unwrap())
}

#[tokio::test]
pub async fn test_get_mem_info() {
    println!("{:?}", get_mem_info().await.unwrap())
}

#[tokio::test]
#[ignore]
pub async fn test_get_disk_usage() {
    println!("{:?}", get_disk_info().await.unwrap())
}
