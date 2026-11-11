//! Network UPS Tools configuration, service management, and status reporting.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use rpc_toolkit::Empty;
use serde::{Deserialize, Serialize};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufStream,
};
use tokio::net::TcpStream;
use tokio::process::Command;

use crate::context::RpcContext;
use crate::prelude::*;
use crate::util::Invoke;
use crate::util::io::{delete_file, write_file_atomic};

const NUT_ETC_DIR: &str = "/etc/nut";
const NUT_OVERLAY_DIR: &str = "/media/startos/config/overlay/etc/nut";
const NUT_PORT: u16 = 3493;
const LOCAL_UPS: &str = "startos";
const LOCAL_MONITOR_USERNAME: &str = "startos";
const SHUTDOWN_DELAY: u16 = 5;

/// Stored NUT configuration and its independent enabled state.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct NutConfig {
    /// Whether StartOS should run NUT monitoring.
    #[serde(default)]
    pub enabled: bool,
    /// Retained server or client settings, including while monitoring is disabled.
    #[serde(default)]
    pub settings: Option<NutSettings>,
}

rpc_toolkit::reflect_ts!(NutConfig);
rpc_toolkit::ts_export!(NutConfig, namespaces = [""]);

/// Directly connected UPS server settings or remote NUT client settings.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, VisitVariants)]
#[serde(
    tag = "mode",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum NutSettings {
    /// Serve and monitor a UPS connected directly to this StartOS host.
    Server {
        device: NutDevice,
        #[serde(default)]
        listen_all: bool,
        #[serde(default)]
        remote_username: Option<String>,
        #[serde(default)]
        remote_password: Option<String>,
    },
    /// Monitor a UPS exposed by another NUT server.
    Client {
        ups_name: String,
        host: String,
        #[serde(default = "default_nut_port")]
        port: u16,
        monitor_username: String,
        monitor_password: String,
    },
}

rpc_toolkit::reflect_ts!(NutSettings);
rpc_toolkit::ts_export!(NutSettings, namespaces = [""]);

/// A UPS detected on USB, with the driver settings that address it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct NutDevice {
    pub driver: String,
    pub port: String,
    #[serde(default)]
    pub vendor_id: Option<String>,
    #[serde(default)]
    pub product_id: Option<String>,
    #[serde(default)]
    pub serial: Option<String>,
    #[serde(default)]
    pub bus: Option<String>,
    /// Pins devices without a serial number to the selected USB port.
    #[serde(default)]
    pub bus_port: Option<String>,
    #[serde(default)]
    pub vendor: Option<String>,
    #[serde(default)]
    pub product: Option<String>,
}

rpc_toolkit::reflect_ts!(NutDevice);
rpc_toolkit::ts_export!(NutDevice, namespaces = [""]);

/// A UPS served by a NUT server, as `LIST UPS` reports it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct NutRemoteUps {
    pub name: String,
    pub description: String,
}

rpc_toolkit::reflect_ts!(NutRemoteUps);
rpc_toolkit::ts_export!(NutRemoteUps, namespaces = [""]);

/// Address of the NUT server to list UPS devices from.
#[derive(Debug, Clone, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct ListNutUpsParams {
    pub host: String,
    #[serde(default = "default_nut_port")]
    pub port: u16,
}

rpc_toolkit::reflect_ts!(ListNutUpsParams);
rpc_toolkit::ts_export!(ListNutUpsParams, namespaces = [""]);

/// Parameters for replacing the stored NUT configuration.
#[derive(Debug, Clone, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SetNutParams {
    pub config: NutConfig,
}

rpc_toolkit::reflect_ts!(SetNutParams);
rpc_toolkit::ts_export!(SetNutParams, namespaces = [""]);

/// Current variables reported by the configured UPS.
#[derive(Debug, Clone, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct NutStatus {
    pub target: String,
    pub variables: BTreeMap<String, String>,
    pub monitoring_error: Option<String>,
}

rpc_toolkit::reflect_ts!(NutStatus);
rpc_toolkit::ts_export!(NutStatus, namespaces = [""]);

const fn default_nut_port() -> u16 {
    NUT_PORT
}

/// Apply and persist a complete NUT configuration.
pub async fn set_nut(ctx: RpcContext, SetNutParams { config }: SetNutParams) -> Result<(), Error> {
    if config.enabled && config.settings.is_none() {
        return Err(Error::new(
            eyre!("{}", t!("nut.enabled-without-settings")),
            ErrorKind::InvalidRequest,
        ));
    }
    sync_nut(config.clone()).await?;
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_nut_mut()
                .ser(&config)
        })
        .await
        .result
}

/// Disable NUT and clear its retained settings.
pub async fn clear_nut(ctx: RpcContext, _: Empty) -> Result<(), Error> {
    set_nut(
        ctx,
        SetNutParams {
            config: NutConfig::default(),
        },
    )
    .await
}

/// List the UPS devices `nut-scanner` finds on USB.
pub async fn scan_nut(_: RpcContext, _: Empty) -> Result<Vec<NutDevice>, Error> {
    let output = tokio::time::timeout(
        Duration::from_secs(30),
        Command::new("nut-scanner")
            .args(["--usb_scan", "--quiet"])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| Error::new(eyre!("{}", t!("nut.scan-timeout")), ErrorKind::Timeout))?
    .with_ctx(|_| (ErrorKind::Network, "nut-scanner"))?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(Error::new(
            eyre!(
                "{}",
                t!(
                    "nut.scan-failed",
                    error = if error.is_empty() {
                        output.status.to_string()
                    } else {
                        error
                    }
                )
            ),
            ErrorKind::Network,
        ));
    }

    Ok(parse_nut_scanner(
        &String::from_utf8(output.stdout).with_kind(ErrorKind::Utf8)?,
    ))
}

/// List the UPS devices a NUT server serves.
pub async fn list_nut_ups(
    _: RpcContext,
    ListNutUpsParams { host, port }: ListNutUpsParams,
) -> Result<Vec<NutRemoteUps>, Error> {
    validate_token(&t!("nut.field-server-host"), &host)?;
    validate_nut_port(port)?;
    let failed = |error: &dyn std::fmt::Display| {
        Error::new(
            eyre!(
                "{}",
                t!(
                    "nut.list-ups-failed",
                    target = nut_address(&host, port),
                    error = error.to_string()
                )
            ),
            ErrorKind::Network,
        )
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        let address = host
            .strip_prefix('[')
            .and_then(|host| host.strip_suffix(']'))
            .unwrap_or(&host);
        let stream = TcpStream::connect((address, port))
            .await
            .map_err(|error| failed(&error))?;
        read_ups_list(BufStream::new(stream))
            .await
            .map_err(|error| failed(&error))
    })
    .await
    .map_err(|_| failed(&t!("nut.list-ups-timeout")))?
}

/// Query the UPS configured in the database through `upsc`.
pub async fn get_nut_status(ctx: RpcContext, _: Empty) -> Result<NutStatus, Error> {
    let config = ctx
        .db
        .peek()
        .await
        .as_public()
        .as_server_info()
        .as_nut()
        .de()
        .unwrap_or_default();

    nut_status(config).await
}

/// Reconcile NUT configuration files and systemd units with stored settings.
pub async fn sync_nut(config: NutConfig) -> Result<(), Error> {
    match config {
        NutConfig {
            enabled: true,
            settings: Some(settings),
        } => apply_nut(settings).await,
        _ => disable_nut().await,
    }
}

async fn disable_nut() -> Result<(), Error> {
    write_nut_file("nut.conf", "MODE=none\n", 0o644).await?;
    delete_nut_file("ups.conf").await?;
    delete_nut_file("upsd.conf").await?;
    delete_nut_file("upsd.users").await?;
    delete_nut_file("upsmon.conf").await?;
    stop_disable(&[
        "nut.target",
        "nut-monitor.service",
        "nut-server.service",
        "nut-driver.target",
        "nut-driver-enumerator.path",
        "nut-driver-enumerator.service",
    ])
    .await
}

async fn apply_nut(settings: NutSettings) -> Result<(), Error> {
    match settings {
        NutSettings::Server {
            device,
            listen_all,
            remote_username,
            remote_password,
        } => {
            validate_identifier(&t!("nut.field-driver"), &device.driver)?;
            validate_token(&t!("nut.field-device-or-address"), &device.port)?;
            for (field, value) in [
                (t!("nut.field-vendor-id"), &device.vendor_id),
                (t!("nut.field-product-id"), &device.product_id),
                (t!("nut.field-device-or-address"), &device.bus),
                (t!("nut.field-device-or-address"), &device.bus_port),
            ] {
                if let Some(value) = value {
                    validate_identifier(&field, value)?;
                }
            }
            if let Some(serial) = &device.serial {
                validate_quoted(&t!("nut.field-serial"), serial)?;
            }
            if listen_all {
                validate_remote_username(remote_username.as_deref())?;
                validate_required_token(
                    &t!("nut.field-network-client-password"),
                    remote_password.as_deref(),
                )?;
            }

            write_nut_file("nut.conf", "MODE=netserver\n", 0o644).await?;
            write_nut_file("ups.conf", &ups_conf(&device), 0o640).await?;
            write_nut_file(
                "upsd.conf",
                if listen_all {
                    "LISTEN 0.0.0.0 3493\nLISTEN :: 3493\n"
                } else {
                    "LISTEN 127.0.0.1 3493\nLISTEN ::1 3493\n"
                },
                0o640,
            )
            .await?;

            let monitor_password = hex::encode(rand::random::<[u8; 16]>());
            let mut users = format!(
                "[{LOCAL_MONITOR_USERNAME}]\n    password = {monitor_password}\n    upsmon primary\n",
            );
            if listen_all {
                let username = remote_username.expect("remote username validated");
                let password = remote_password.expect("remote password validated");
                users.push_str(&format!(
                    "\n[{username}]\n    password = {password}\n    upsmon secondary\n",
                ));
            }
            write_nut_file("upsd.users", &users, 0o640).await?;
            write_nut_file(
                "upsmon.conf",
                &upsmon_conf(
                    LOCAL_UPS,
                    "localhost",
                    NUT_PORT,
                    LOCAL_MONITOR_USERNAME,
                    &monitor_password,
                    "primary",
                ),
                0o640,
            )
            .await?;
            enable_units(&[
                "nut.target",
                "nut-driver.target",
                "nut-driver-enumerator.path",
                "nut-driver-enumerator.service",
                "nut-server.service",
                "nut-monitor.service",
            ])
            .await?;
            restart_units(&[
                "nut.target",
                "nut-driver-enumerator.path",
                "nut-driver-enumerator.service",
                "nut-server.service",
                "nut-monitor.service",
            ])
            .await
        }
        NutSettings::Client {
            ups_name,
            host,
            port,
            monitor_username,
            monitor_password,
        } => {
            validate_identifier(&t!("nut.field-ups-name"), &ups_name)?;
            validate_token(&t!("nut.field-server-host"), &host)?;
            validate_identifier(&t!("nut.field-monitor-username"), &monitor_username)?;
            validate_token(&t!("nut.field-monitor-password"), &monitor_password)?;
            validate_nut_port(port)?;

            write_nut_file("nut.conf", "MODE=netclient\n", 0o644).await?;
            delete_nut_file("ups.conf").await?;
            delete_nut_file("upsd.conf").await?;
            delete_nut_file("upsd.users").await?;
            write_nut_file(
                "upsmon.conf",
                &upsmon_conf(
                    &ups_name,
                    &host,
                    port,
                    &monitor_username,
                    &monitor_password,
                    "secondary",
                ),
                0o640,
            )
            .await?;
            enable_units(&["nut.target", "nut-monitor.service"]).await?;
            restart_units(&["nut.target", "nut-monitor.service"]).await?;
            stop_disable(&[
                "nut-server.service",
                "nut-driver.target",
                "nut-driver-enumerator.path",
                "nut-driver-enumerator.service",
            ])
            .await
        }
    }
}

async fn nut_status(config: NutConfig) -> Result<NutStatus, Error> {
    let target = upsc_target(&config)
        .ok_or_else(|| Error::new(eyre!("{}", t!("nut.disabled")), ErrorKind::InvalidRequest))?;

    let target_for_ctx = target.clone();
    let output = Command::new("upsc")
        .arg(&target)
        .output()
        .await
        .with_ctx(|_| (ErrorKind::Network, lazy_format!("upsc {target_for_ctx}")))?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(Error::new(
            eyre!(
                "{}",
                t!(
                    "nut.status-command-failed",
                    target = &target,
                    error = if error.is_empty() {
                        output.status.to_string()
                    } else {
                        error
                    }
                )
            ),
            ErrorKind::Network,
        ));
    }

    let stdout = String::from_utf8(output.stdout).with_kind(ErrorKind::Utf8)?;
    let variables = parse_upsc(&stdout);
    if !variables.contains_key("ups.status") {
        return Err(Error::new(
            eyre!("{}", t!("nut.status-missing", target = &target)),
            ErrorKind::Network,
        ));
    }

    let monitoring_error = monitor_status(config.settings.as_ref().expect("enabled settings"))
        .await
        .err()
        .map(|error| error.to_string());

    Ok(NutStatus {
        target,
        variables,
        monitoring_error,
    })
}

async fn monitor_status(settings: &NutSettings) -> Result<(), Error> {
    tokio::time::timeout(Duration::from_secs(5), async {
        Command::new("systemctl")
            .args(["is-active", "--quiet", "nut-monitor.service"])
            .kill_on_drop(true)
            .invoke(ErrorKind::Systemd)
            .await
            .map_err(|_| {
                Error::new(
                    eyre!("{}", t!("nut.monitor-not-running")),
                    ErrorKind::Systemd,
                )
            })?;

        match settings {
            NutSettings::Server { .. } => {
                authenticate_monitor(
                    "localhost",
                    NUT_PORT,
                    LOCAL_UPS,
                    LOCAL_MONITOR_USERNAME,
                    &local_monitor_password().await?,
                    true,
                )
                .await
            }
            NutSettings::Client {
                ups_name,
                host,
                port,
                monitor_username,
                monitor_password,
                ..
            } => {
                authenticate_monitor(
                    host,
                    *port,
                    ups_name,
                    monitor_username,
                    monitor_password,
                    false,
                )
                .await
            }
        }
    })
    .await
    .map_err(|_| {
        Error::new(
            eyre!("{}", t!("nut.monitor-check-timeout")),
            ErrorKind::Network,
        )
    })?
}

async fn authenticate_monitor(
    host: &str,
    port: u16,
    ups: &str,
    username: &str,
    password: &str,
    primary: bool,
) -> Result<(), Error> {
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    let mut stream = BufStream::new(
        TcpStream::connect((host, port))
            .await
            .with_kind(ErrorKind::Network)?,
    );
    match nut_command(&mut stream, "STARTTLS").await?.as_str() {
        "OK STARTTLS" => {
            // Matches upsmon's default opportunistic TLS policy.
            let tls = native_tls::TlsConnector::builder()
                .danger_accept_invalid_certs(true)
                .danger_accept_invalid_hostnames(true)
                .build()
                .with_kind(ErrorKind::Network)?;
            let stream = tokio_native_tls::TlsConnector::from(tls)
                .connect(host, stream.into_inner())
                .await
                .with_kind(ErrorKind::Network)?;
            monitor_login(BufStream::new(stream), ups, username, password, primary).await
        }
        "ERR FEATURE-NOT-SUPPORTED" | "ERR FEATURE-NOT-CONFIGURED" | "ERR UNKNOWN-COMMAND" => {
            monitor_login(stream, ups, username, password, primary).await
        }
        _ => Err(Error::new(
            eyre!("{}", t!("nut.monitor-auth-failed")),
            ErrorKind::Network,
        )),
    }
}

async fn monitor_login<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: BufStream<S>,
    ups: &str,
    username: &str,
    password: &str,
    primary: bool,
) -> Result<(), Error> {
    let mut commands = vec![
        format!("USERNAME {username}"),
        format!("PASSWORD {password}"),
        format!("LOGIN {ups}"),
    ];
    if primary {
        commands.push(format!("PRIMARY {ups}"));
    }
    commands.push("LOGOUT".to_owned());
    for command in commands {
        let response = nut_command(&mut stream, &command).await?;
        if response.split_whitespace().next() != Some("OK")
            && !(command == "LOGOUT" && response == "Goodbye...")
        {
            return Err(Error::new(
                eyre!("{}", t!("nut.monitor-auth-failed")),
                ErrorKind::Network,
            ));
        }
    }
    Ok(())
}

async fn nut_command<S: AsyncBufRead + AsyncWrite + Unpin>(
    stream: &mut S,
    command: &str,
) -> Result<String, Error> {
    stream
        .write_all(command.as_bytes())
        .await
        .with_kind(ErrorKind::Network)?;
    stream
        .write_all(b"\n")
        .await
        .with_kind(ErrorKind::Network)?;
    stream.flush().await.with_kind(ErrorKind::Network)?;
    let mut response = Vec::new();
    stream
        .take(1024)
        .read_until(b'\n', &mut response)
        .await
        .with_kind(ErrorKind::Network)?;
    if !response.ends_with(b"\n") {
        return Err(Error::new(
            eyre!("{}", t!("nut.monitor-auth-failed")),
            ErrorKind::Network,
        ));
    }
    Ok(String::from_utf8(response)
        .with_kind(ErrorKind::Utf8)?
        .trim_end_matches(['\r', '\n'])
        .to_owned())
}

fn upsc_target(config: &NutConfig) -> Option<String> {
    if !config.enabled {
        return None;
    }
    match config.settings.as_ref()? {
        NutSettings::Server { .. } => Some(nut_target(LOCAL_UPS, "localhost", NUT_PORT)),
        NutSettings::Client {
            ups_name,
            host,
            port,
            ..
        } => Some(nut_target(ups_name, host, *port)),
    }
}

fn nut_target(ups_name: &str, host: &str, port: u16) -> String {
    format!("{ups_name}@{}", nut_address(host, port))
}

fn nut_address(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

async fn read_ups_list<S: AsyncBufRead + AsyncWrite + Unpin>(
    mut stream: S,
) -> Result<Vec<NutRemoteUps>, Error> {
    let first = nut_command(&mut stream, "LIST UPS").await?;
    if first != "BEGIN LIST UPS" {
        return Err(Error::new(eyre!("{first}"), ErrorKind::Network));
    }
    let mut upses = Vec::new();
    for _ in 0..256 {
        let mut line = Vec::new();
        (&mut stream)
            .take(1024)
            .read_until(b'\n', &mut line)
            .await
            .with_kind(ErrorKind::Network)?;
        let line = String::from_utf8(line).with_kind(ErrorKind::Utf8)?;
        let line = line.trim_end_matches(['\r', '\n']);
        if line == "END LIST UPS" {
            return Ok(upses);
        }
        let Some((name, description)) = line
            .strip_prefix("UPS ")
            .and_then(|rest| rest.split_once(' '))
        else {
            return Err(Error::new(eyre!("{line}"), ErrorKind::Network));
        };
        upses.push(NutRemoteUps {
            name: name.to_owned(),
            description: description.trim_matches('"').to_owned(),
        });
    }
    Err(Error::new(eyre!("LIST UPS"), ErrorKind::Network))
}

fn parse_upsc(stdout: &str) -> BTreeMap<String, String> {
    stdout
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            Some((
                key.to_owned(),
                value.strip_prefix(' ').unwrap_or(value).to_owned(),
            ))
        })
        .collect()
}

/// Reads the generated local monitor password back from `upsmon.conf`.
async fn local_monitor_password() -> Result<String, Error> {
    let path = Path::new(NUT_ETC_DIR).join("upsmon.conf");
    tokio::fs::read_to_string(&path)
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("read {path:?}")))?
        .lines()
        .find_map(|line| {
            let mut fields = line.strip_prefix("MONITOR ")?.split_whitespace();
            fields.nth(3).map(str::to_owned)
        })
        .ok_or_else(|| {
            Error::new(
                eyre!("{}", t!("nut.monitor-not-running")),
                ErrorKind::Systemd,
            )
        })
}

fn parse_nut_scanner(stdout: &str) -> Vec<NutDevice> {
    let mut sections: Vec<BTreeMap<&str, &str>> = Vec::new();
    for line in stdout.lines().map(str::trim) {
        if line.starts_with('[') && line.ends_with(']') {
            sections.push(BTreeMap::new());
        } else if let (Some(section), Some((key, value))) =
            (sections.last_mut(), line.split_once('='))
        {
            section.insert(key.trim(), value.trim().trim_matches('"'));
        }
    }
    sections
        .into_iter()
        .filter_map(|section| {
            let field = |key: &str| section.get(key).map(|value| (*value).to_owned());
            Some(NutDevice {
                driver: field("driver")?,
                port: field("port")?,
                vendor_id: field("vendorid"),
                product_id: field("productid"),
                serial: field("serial").filter(|serial| !serial.is_empty()),
                bus: field("bus"),
                bus_port: field("busport").filter(|port| port != ".*"),
                vendor: field("vendor"),
                product: field("product"),
            })
        })
        .collect()
}

fn ups_conf(device: &NutDevice) -> String {
    let mut conf = format!(
        "[{LOCAL_UPS}]\n    driver = {}\n    port = {}\n",
        device.driver, device.port
    );
    for (key, value) in [
        ("vendorid", &device.vendor_id),
        ("productid", &device.product_id),
        ("serial", &device.serial),
    ] {
        if let Some(value) = value {
            conf.push_str(&format!("    {key} = \"{}\"\n", value.replace('#', "\\#")));
        }
    }
    if device.serial.is_none() {
        for (key, value) in [("bus", &device.bus), ("busport", &device.bus_port)] {
            if let Some(value) = value {
                conf.push_str(&format!("    {key} = \"{value}\"\n"));
            }
        }
    }
    conf.push_str("    desc = \"StartOS UPS\"\n");
    conf
}

fn upsmon_conf(
    ups_name: &str,
    host: &str,
    port: u16,
    username: &str,
    password: &str,
    role: &str,
) -> String {
    let target = nut_target(ups_name, host, port);
    format!(
        "\
MONITOR {target} 1 {username} {password} {role}
MINSUPPLIES 1
SHUTDOWNCMD \"/sbin/shutdown -h +0\"
POLLFREQ 5
POLLFREQALERT 5
HOSTSYNC 15
DEADTIME 15
FINALDELAY {SHUTDOWN_DELAY}
POWERDOWNFLAG /etc/killpower
NOTIFYFLAG ONLINE SYSLOG
NOTIFYFLAG ONBATT SYSLOG
NOTIFYFLAG LOWBATT SYSLOG
NOTIFYFLAG FSD SYSLOG
NOTIFYFLAG COMMOK SYSLOG
NOTIFYFLAG COMMBAD SYSLOG
NOTIFYFLAG SHUTDOWN SYSLOG
",
    )
}

async fn write_nut_file(name: &str, contents: &str, mode: u32) -> Result<(), Error> {
    for dir in [NUT_ETC_DIR, NUT_OVERLAY_DIR] {
        let path = Path::new(dir).join(name);
        write_file_atomic(&path, contents).await?;
        let chmod_path = path.clone();
        tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("chmod {chmod_path:?}")))?;
        if mode != 0o644 {
            Command::new("chown")
                .arg("root:nut")
                .arg(&path)
                .invoke(ErrorKind::Filesystem)
                .await?;
        }
    }
    Ok(())
}

async fn delete_nut_file(name: &str) -> Result<(), Error> {
    for dir in [NUT_ETC_DIR, NUT_OVERLAY_DIR] {
        delete_file(Path::new(dir).join(name)).await?;
    }
    Ok(())
}

async fn enable_units(units: &[&str]) -> Result<(), Error> {
    let mut enable = Command::new("systemctl");
    enable.arg("enable");
    for unit in units {
        enable.arg(unit);
    }
    enable.invoke(ErrorKind::Systemd).await?;

    Ok(())
}

async fn restart_units(units: &[&str]) -> Result<(), Error> {
    // Restart in one transaction: restarting nut.target already restarts its PartOf units.
    Command::new("systemctl")
        .arg("restart")
        .args(units)
        .invoke(ErrorKind::Systemd)
        .await
        .map(|_| ())
}

async fn stop_disable(units: &[&str]) -> Result<(), Error> {
    let mut disable = Command::new("systemctl");
    disable.arg("disable").arg("--now");
    for unit in units {
        disable.arg(unit);
    }
    disable.invoke(ErrorKind::Systemd).await.map(|_| ())
}

fn validate_identifier(field: &str, value: &str) -> Result<(), Error> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return Err(Error::new(
            eyre!("{}", t!("nut.invalid-identifier", field = field)),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(())
}

fn validate_required_identifier(field: &str, value: Option<&str>) -> Result<(), Error> {
    validate_identifier(field, value.unwrap_or_default())
}

fn validate_remote_username(value: Option<&str>) -> Result<(), Error> {
    validate_required_identifier(&t!("nut.field-network-client-username"), value)?;
    if value == Some(LOCAL_MONITOR_USERNAME) {
        return Err(Error::new(
            eyre!(
                "{}",
                t!("nut.reserved-username", username = LOCAL_MONITOR_USERNAME)
            ),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(())
}

fn validate_token(field: &str, value: &str) -> Result<(), Error> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|b| b.is_ascii_graphic() && !matches!(b, b'#' | b'\\' | b'"'))
    {
        return Err(Error::new(
            eyre!("{}", t!("nut.invalid-token", field = field)),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(())
}

fn validate_required_token(field: &str, value: Option<&str>) -> Result<(), Error> {
    validate_token(field, value.unwrap_or_default())
}

fn validate_nut_port(port: u16) -> Result<(), Error> {
    if port == 0 {
        return Err(Error::new(
            eyre!("{}", t!("nut.invalid-port")),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(())
}

fn validate_quoted(field: &str, value: &str) -> Result<(), Error> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|b| (b.is_ascii_graphic() || b == b' ') && !matches!(b, b'\\' | b'"'))
    {
        return Err(Error::new(
            eyre!("{}", t!("nut.invalid-quoted", field = field)),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn verifies_monitor_login_and_primary_permissions() {
        for primary in [false, true] {
            let (client, server) = tokio::io::duplex(4096);
            let server = tokio::spawn(async move {
                let mut server = BufStream::new(server);
                let mut commands = vec!["USERNAME monitor", "PASSWORD secret!$%&*()", "LOGIN ups"];
                if primary {
                    commands.push("PRIMARY ups");
                }
                commands.push("LOGOUT");
                for command in commands {
                    let mut line = String::new();
                    server.read_line(&mut line).await.unwrap();
                    assert_eq!(line.trim_end(), command);
                    server
                        .write_all(if command == "LOGOUT" {
                            b"OK Goodbye\n"
                        } else if command == "PRIMARY ups" {
                            b"OK PRIMARY-GRANTED\n"
                        } else {
                            b"OK\n"
                        })
                        .await
                        .unwrap();
                    server.flush().await.unwrap();
                }
            });
            monitor_login(
                BufStream::new(client),
                "ups",
                "monitor",
                "secret!$%&*()",
                primary,
            )
            .await
            .unwrap();
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn rejects_monitor_authentication_and_permission_errors_without_exposing_passwords() {
        for fail_at in 0..4 {
            let (client, server) = tokio::io::duplex(4096);
            let server = tokio::spawn(async move {
                let mut server = BufStream::new(server);
                for index in 0..=fail_at {
                    let mut line = String::new();
                    server.read_line(&mut line).await.unwrap();
                    server
                        .write_all(if index == fail_at {
                            b"ERR ACCESS-DENIED secret\n"
                        } else {
                            b"OK\n"
                        })
                        .await
                        .unwrap();
                    server.flush().await.unwrap();
                }
            });
            let error = monitor_login(BufStream::new(client), "ups", "monitor", "secret", true)
                .await
                .unwrap_err();
            assert!(!error.to_string().contains("secret"));
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn rechecks_credentials_after_remote_password_changes() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            for (accepted, tls_response) in [
                (false, "ERR FEATURE-NOT-SUPPORTED\n"),
                (true, "ERR FEATURE-NOT-CONFIGURED\n"),
                (false, "ERR UNKNOWN-COMMAND\n"),
                (true, "ERR FEATURE-NOT-CONFIGURED\n"),
            ] {
                let (stream, _) = listener.accept().await.unwrap();
                let mut stream = BufStream::new(stream);
                let mut command = String::new();
                stream.read_line(&mut command).await.unwrap();
                assert_eq!(command, "STARTTLS\n");
                stream.write_all(tls_response.as_bytes()).await.unwrap();
                stream.flush().await.unwrap();
                loop {
                    command.clear();
                    if stream.read_line(&mut command).await.unwrap() == 0 {
                        break;
                    }
                    let login = command == "LOGIN ups\n";
                    stream
                        .write_all(if login && !accepted {
                            b"ERR ACCESS-DENIED\n"
                        } else {
                            b"OK\n"
                        })
                        .await
                        .unwrap();
                    stream.flush().await.unwrap();
                    if (login && !accepted) || command == "LOGOUT\n" {
                        break;
                    }
                }
            }
        });
        for accepted in [false, true, false, true] {
            assert_eq!(
                authenticate_monitor("127.0.0.1", port, "ups", "monitor", "secret", false)
                    .await
                    .is_ok(),
                accepted
            );
        }
        server.await.unwrap();
    }

    #[tokio::test]
    async fn rejects_incomplete_and_oversized_monitor_responses() {
        for response in [b"OK".to_vec(), vec![b'x'; 2048]] {
            let (client, mut server) = tokio::io::duplex(4096);
            let server = tokio::spawn(async move {
                server.write_all(&response).await.unwrap();
            });
            assert!(
                nut_command(&mut BufStream::new(client), "USERNAME monitor")
                    .await
                    .is_err()
            );
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn authenticates_over_starttls_without_sending_plaintext_credentials() {
        use openssl::asn1::Asn1Time;
        use openssl::hash::MessageDigest;
        use openssl::pkey::PKey;
        use openssl::rsa::Rsa;
        use openssl::x509::{X509, X509NameBuilder};

        let key = PKey::from_rsa(Rsa::generate(2048).unwrap()).unwrap();
        let mut name = X509NameBuilder::new().unwrap();
        name.append_entry_by_text("CN", "localhost").unwrap();
        let name = name.build();
        let mut cert = X509::builder().unwrap();
        cert.set_version(2).unwrap();
        cert.set_subject_name(&name).unwrap();
        cert.set_issuer_name(&name).unwrap();
        cert.set_not_before(&Asn1Time::days_from_now(0).unwrap())
            .unwrap();
        cert.set_not_after(&Asn1Time::days_from_now(1).unwrap())
            .unwrap();
        cert.set_pubkey(&key).unwrap();
        cert.sign(&key, MessageDigest::sha256()).unwrap();
        let identity = native_tls::Identity::from_pkcs8(
            &cert.build().to_pem().unwrap(),
            &key.private_key_to_pem_pkcs8().unwrap(),
        )
        .unwrap();
        let acceptor =
            tokio_native_tls::TlsAcceptor::from(native_tls::TlsAcceptor::new(identity).unwrap());
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut stream = BufStream::new(stream);
            let mut line = String::new();
            stream.read_line(&mut line).await.unwrap();
            assert_eq!(line, "STARTTLS\n");
            stream.write_all(b"OK STARTTLS\n").await.unwrap();
            stream.flush().await.unwrap();
            let mut stream = BufStream::new(acceptor.accept(stream.into_inner()).await.unwrap());
            for command in ["USERNAME monitor", "PASSWORD secret", "LOGIN ups", "LOGOUT"] {
                line.clear();
                stream.read_line(&mut line).await.unwrap();
                assert_eq!(line.trim_end(), command);
                stream.write_all(b"OK\n").await.unwrap();
                stream.flush().await.unwrap();
            }
        });
        authenticate_monitor("127.0.0.1", port, "ups", "monitor", "secret", false)
            .await
            .unwrap();
        server.await.unwrap();
    }

    #[test]
    fn parses_upsc_output() {
        let status = parse_upsc("battery.charge: 96\nups.status: OL\nignored\n");

        assert_eq!(status.get("battery.charge").map(String::as_str), Some("96"));
        assert_eq!(status.get("ups.status").map(String::as_str), Some("OL"));
        assert!(!status.contains_key("ignored"));
    }

    #[test]
    fn status_and_monitor_use_the_same_nut_target() {
        for (host, target) in [
            ("2001:db8::1", "ups@[2001:db8::1]:3493"),
            ("[2001:db8::1]", "ups@[2001:db8::1]:3493"),
            ("192.0.2.1", "ups@192.0.2.1:3493"),
            ("nut.example", "ups@nut.example:3493"),
        ] {
            let config = NutConfig {
                enabled: true,
                settings: Some(NutSettings::Client {
                    ups_name: "ups".to_owned(),
                    host: host.to_owned(),
                    port: NUT_PORT,
                    monitor_username: "monitor".to_owned(),
                    monitor_password: "secret".to_owned(),
                }),
            };

            assert_eq!(upsc_target(&config).as_deref(), Some(target));
            assert_eq!(
                upsmon_conf("ups", host, NUT_PORT, "monitor", "secret", "secondary")
                    .lines()
                    .next(),
                Some(format!("MONITOR {target} 1 monitor secret secondary").as_str())
            );
        }
    }

    #[test]
    fn parses_nut_scanner_devices_into_pinned_ups_conf() {
        let devices = parse_nut_scanner(
            "\
[nutdev1]
\tdriver = \"usbhid-ups\"
\tport = \"auto\"
\tvendorid = \"051D\"
\tproductid = \"0002\"
\tproduct = \"Back-UPS ES 600M1\"
\tserial = \"4B1234P56789\"
\tvendor = \"American Power Conversion\"
\tbus = \"001\"

[nutdev2]
\tdriver = \"nutdrv_qx\"
\tport = \"auto\"
\tserial = \"\"
[nutdev3]
\tport = \"auto\"
",
        );

        assert_eq!(devices.len(), 2);
        assert_eq!(devices[1].serial, None);
        assert_eq!(
            ups_conf(&devices[0]),
            "\
[startos]
    driver = usbhid-ups
    port = auto
    vendorid = \"051D\"
    productid = \"0002\"
    serial = \"4B1234P56789\"
    desc = \"StartOS UPS\"
"
        );
    }

    #[tokio::test]
    async fn lists_remote_ups_devices() {
        let (client, server) = tokio::io::duplex(4096);
        let server = tokio::spawn(async move {
            let mut server = BufStream::new(server);
            let mut line = String::new();
            server.read_line(&mut line).await.unwrap();
            assert_eq!(line, "LIST UPS\n");
            server
                .write_all(
                    b"BEGIN LIST UPS\nUPS ups \"Synology UPS\"\nUPS rack \"Eaton 5E\"\nEND LIST UPS\n",
                )
                .await
                .unwrap();
            server.flush().await.unwrap();
        });
        assert_eq!(
            read_ups_list(BufStream::new(client)).await.unwrap(),
            [
                NutRemoteUps {
                    name: "ups".to_owned(),
                    description: "Synology UPS".to_owned(),
                },
                NutRemoteUps {
                    name: "rack".to_owned(),
                    description: "Eaton 5E".to_owned(),
                },
            ]
        );
        server.await.unwrap();
    }

    #[test]
    fn keeps_serialless_usb_devices_distinct() {
        let devices = parse_nut_scanner(
            "\
[nutdev1]
    driver = \"usbhid-ups\"
    port = \"auto\"
    vendorid = \"051D\"
    productid = \"0002\"
    bus = \"001\"
    busport = \"002\"
[nutdev2]
    driver = \"usbhid-ups\"
    port = \"auto\"
    vendorid = \"051D\"
    productid = \"0002\"
    bus = \"001\"
    busport = \"003\"
",
        );
        assert_eq!(devices.len(), 2);
        assert!(ups_conf(&devices[0]).contains("    bus = \"001\"\n    busport = \"002\"\n"));
        assert!(ups_conf(&devices[1]).contains("    bus = \"001\"\n    busport = \"003\"\n"));
    }

    #[test]
    fn escapes_hashes_in_discovered_serials() {
        let devices = parse_nut_scanner(
            "[nutdev1]\n    driver = \"usbhid-ups\"\n    port = \"auto\"\n    serial = \"UPS#1\"\n",
        );
        assert_eq!(devices.len(), 1);
        assert!(validate_quoted("serial", devices[0].serial.as_deref().unwrap()).is_ok());
        assert!(ups_conf(&devices[0]).contains("    serial = \"UPS\\#1\"\n"));
    }

    #[test]
    fn sharing_username_cannot_reuse_the_local_monitor_account() {
        assert!(validate_remote_username(Some(LOCAL_MONITOR_USERNAME)).is_err());
        assert!(validate_remote_username(None).is_err());
        assert!(validate_remote_username(Some("upsclient")).is_ok());
    }

    #[test]
    fn rejects_unsafe_config_tokens() {
        for value in [
            "",
            "two words",
            "abc#def",
            r"abc\def",
            "\"secret\"",
            "abc\0def",
            "abc\x7fdef",
        ] {
            assert!(validate_token("field", value).is_err(), "{value:?}");
        }
        for value in [
            "one-word",
            "auto",
            "/dev/ttyUSB0",
            "[2001:db8::1]",
            "secret!$%&*()",
        ] {
            assert!(validate_token("field", value).is_ok(), "{value:?}");
        }
    }
}
