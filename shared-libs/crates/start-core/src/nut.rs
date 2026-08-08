//! Network UPS Tools configuration, service management, and status reporting.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use rpc_toolkit::Empty;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use ts_rs::TS;

use crate::context::RpcContext;
use crate::prelude::*;
use crate::util::Invoke;
use crate::util::io::{delete_file, write_file_atomic};

const NUT_ETC_DIR: &str = "/etc/nut";
const NUT_OVERLAY_DIR: &str = "/media/startos/config/overlay/etc/nut";
const NUT_PORT: u16 = 3493;

/// Stored NUT configuration and its independent enabled state.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NutConfig {
    /// Whether StartOS should run NUT monitoring.
    #[serde(default)]
    pub enabled: bool,
    /// Retained server or client settings, including while monitoring is disabled.
    #[serde(default)]
    pub settings: Option<NutSettings>,
}

/// Directly connected UPS server settings or remote NUT client settings.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(
    tag = "mode",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum NutSettings {
    /// Serve and monitor a UPS connected directly to this StartOS host.
    Server {
        ups_name: String,
        driver: String,
        port: String,
        monitor_username: String,
        monitor_password: String,
        #[serde(default)]
        listen_all: bool,
        #[serde(default)]
        remote_username: Option<String>,
        #[serde(default)]
        remote_password: Option<String>,
        #[serde(default = "default_shutdown_delay")]
        shutdown_delay: u16,
    },
    /// Monitor a UPS exposed by another NUT server.
    Client {
        ups_name: String,
        host: String,
        #[serde(default = "default_nut_port")]
        port: u16,
        monitor_username: String,
        monitor_password: String,
        #[serde(default = "default_shutdown_delay")]
        shutdown_delay: u16,
    },
}

/// Parameters for replacing the stored NUT configuration.
#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SetNutParams {
    pub config: NutConfig,
}

/// Current variables reported by the configured UPS.
#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NutStatus {
    pub target: String,
    pub variables: BTreeMap<String, String>,
}

const fn default_nut_port() -> u16 {
    NUT_PORT
}

const fn default_shutdown_delay() -> u16 {
    5
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
            ups_name,
            driver,
            port,
            monitor_username,
            monitor_password,
            listen_all,
            remote_username,
            remote_password,
            shutdown_delay,
        } => {
            validate_identifier(&t!("nut.field-ups-name"), &ups_name)?;
            validate_identifier(&t!("nut.field-driver"), &driver)?;
            validate_token(&t!("nut.field-device-or-address"), &port)?;
            validate_identifier(&t!("nut.field-monitor-username"), &monitor_username)?;
            validate_token(&t!("nut.field-monitor-password"), &monitor_password)?;
            validate_shutdown_delay(shutdown_delay)?;
            if listen_all {
                validate_required_identifier(
                    &t!("nut.field-network-client-username"),
                    remote_username.as_deref(),
                )?;
                validate_required_token(
                    &t!("nut.field-network-client-password"),
                    remote_password.as_deref(),
                )?;
            }

            write_nut_file("nut.conf", "MODE=netserver\n", 0o644).await?;
            write_nut_file(
                "ups.conf",
                &format!(
                    "[{ups_name}]\n    driver = {driver}\n    port = {port}\n    desc = \"StartOS UPS\"\n",
                ),
                0o640,
            )
            .await?;
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

            let mut users = format!(
                "[{monitor_username}]\n    password = {monitor_password}\n    upsmon primary\n",
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
                    &ups_name,
                    "localhost",
                    NUT_PORT,
                    &monitor_username,
                    &monitor_password,
                    "primary",
                    shutdown_delay,
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
            shutdown_delay,
        } => {
            validate_identifier(&t!("nut.field-ups-name"), &ups_name)?;
            validate_token(&t!("nut.field-server-host"), &host)?;
            validate_identifier(&t!("nut.field-monitor-username"), &monitor_username)?;
            validate_token(&t!("nut.field-monitor-password"), &monitor_password)?;
            validate_nut_port(port)?;
            validate_shutdown_delay(shutdown_delay)?;

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
                    shutdown_delay,
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

    Ok(NutStatus { target, variables })
}

fn upsc_target(config: &NutConfig) -> Option<String> {
    if !config.enabled {
        return None;
    }
    match config.settings.as_ref()? {
        NutSettings::Server { ups_name, .. } => Some(format!("{ups_name}@localhost:{NUT_PORT}")),
        NutSettings::Client {
            ups_name,
            host,
            port,
            ..
        } => Some(format!("{ups_name}@{}:{port}", format_upsc_host(host))),
    }
}

fn format_upsc_host(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_owned()
    }
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

fn upsmon_conf(
    ups_name: &str,
    host: &str,
    port: u16,
    username: &str,
    password: &str,
    role: &str,
    shutdown_delay: u16,
) -> String {
    format!(
        "\
MONITOR {ups_name}@{host}:{port} 1 {username} {password} {role}
MINSUPPLIES 1
SHUTDOWNCMD \"/sbin/shutdown -h +0\"
POLLFREQ 5
POLLFREQALERT 5
HOSTSYNC 15
DEADTIME 15
FINALDELAY {shutdown_delay}
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
    for unit in units {
        Command::new("systemctl")
            .arg("restart")
            .arg(unit)
            .invoke(ErrorKind::Systemd)
            .await?;
    }
    Ok(())
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

fn validate_token(field: &str, value: &str) -> Result<(), Error> {
    if value.is_empty()
        || !value.is_ascii()
        || value.bytes().any(|b| b == 0 || b.is_ascii_whitespace())
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

fn validate_shutdown_delay(delay: u16) -> Result<(), Error> {
    if delay > 300 {
        return Err(Error::new(
            eyre!("{}", t!("nut.invalid-shutdown-delay")),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_upsc_output() {
        let status = parse_upsc("battery.charge: 96\nups.status: OL\nignored\n");

        assert_eq!(status.get("battery.charge").map(String::as_str), Some("96"));
        assert_eq!(status.get("ups.status").map(String::as_str), Some("OL"));
        assert!(!status.contains_key("ignored"));
    }

    #[test]
    fn formats_ipv6_upsc_targets() {
        let config = NutConfig {
            enabled: true,
            settings: Some(NutSettings::Client {
                ups_name: "ups".to_owned(),
                host: "2001:db8::1".to_owned(),
                port: NUT_PORT,
                monitor_username: "monitor".to_owned(),
                monitor_password: "secret".to_owned(),
                shutdown_delay: 5,
            }),
        };

        assert_eq!(
            upsc_target(&config).as_deref(),
            Some("ups@[2001:db8::1]:3493")
        );
    }

    #[test]
    fn rejects_config_tokens_with_whitespace() {
        assert!(validate_token("field", "two words").is_err());
        assert!(validate_token("field", "one-word").is_ok());
    }
}
