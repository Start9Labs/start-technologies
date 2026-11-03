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

rpc_toolkit::reflect_ts!(NutSettings);
rpc_toolkit::ts_export!(NutSettings, namespaces = [""]);

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
            NutSettings::Server {
                ups_name,
                monitor_username,
                monitor_password,
                ..
            } => {
                authenticate_monitor(
                    "localhost",
                    NUT_PORT,
                    ups_name,
                    monitor_username,
                    monitor_password,
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
        NutSettings::Server { ups_name, .. } => Some(nut_target(ups_name, "localhost", NUT_PORT)),
        NutSettings::Client {
            ups_name,
            host,
            port,
            ..
        } => Some(nut_target(ups_name, host, *port)),
    }
}

fn nut_target(ups_name: &str, host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("{ups_name}@[{host}]:{port}")
    } else {
        format!("{ups_name}@{host}:{port}")
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
                    shutdown_delay: 5,
                }),
            };

            assert_eq!(upsc_target(&config).as_deref(), Some(target));
            assert_eq!(
                upsmon_conf("ups", host, NUT_PORT, "monitor", "secret", "secondary", 5)
                    .lines()
                    .next(),
                Some(format!("MONITOR {target} 1 monitor secret secondary").as_str())
            );
        }
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
