use const_format::formatcp;

rust_i18n::i18n!("locales", fallback = ["en_US"]);

pub const DATA_DIR: &str = "/media/startos/data";
pub const MAIN_DATA: &str = formatcp!("{DATA_DIR}/main");
pub const PACKAGE_DATA: &str = formatcp!("{DATA_DIR}/package-data");
pub const HOST_IP: [u8; 4] = [10, 0, 3, 1];
pub use std::env::consts::ARCH;
lazy_static::lazy_static! {
    pub static ref PLATFORM: String = {
        if let Ok(platform) = std::fs::read_to_string("/usr/lib/startos/PLATFORM.txt") {
            platform.trim().to_string()
        } else {
            ARCH.to_string()
        }
    };
    pub static ref SOURCE_DATE: SystemTime = {
        std::fs::metadata(std::env::current_exe().unwrap()).unwrap().modified().unwrap()
    };
}

/// Map a platform string to its architecture
pub fn platform_to_arch(platform: &str) -> &str {
    if let Some(arch) = platform.strip_suffix("-nonfree") {
        return arch;
    }
    if let Some(arch) = platform.strip_suffix("-nvidia") {
        return arch;
    }
    match platform {
        "raspberrypi" | "rockchip64" => "aarch64",
        _ => platform,
    }
}

mod cap {
    #![allow(non_upper_case_globals)]

    pub const CAP_1_KiB: usize = 1024;
    pub const CAP_1_MiB: usize = CAP_1_KiB * CAP_1_KiB;
    pub const CAP_10_MiB: usize = 10 * CAP_1_MiB;
}
pub use cap::*;

pub mod account;
pub mod action;
pub mod auth;
pub mod backup;
pub mod bins;
pub mod context;
pub mod control;
pub mod db;
pub mod dependencies;
pub mod developer;
pub mod diagnostic;
pub mod disk;
pub mod error;
pub mod firmware;
pub mod hostname;
pub mod id;
pub mod init;
pub mod install;
pub mod logs;
pub mod lxc;
pub mod mcp;
pub mod middleware;
pub mod net;
pub mod notifications;
pub mod os_install;
pub mod prelude;
pub mod progress;
pub mod registry;
pub mod rpc_continuations;
pub mod s9pk;
pub mod service;
pub mod setup;
pub mod shutdown;
pub mod sign;
pub mod sound;
pub mod ssh;
pub mod status;
pub mod system;
pub mod tunnel;
pub mod update;
pub mod upload;
pub mod util;
pub mod version;
pub mod volume;

use std::time::SystemTime;

use clap::Parser;
pub use error::{Error, ErrorKind, OptionExt, ResultExt};
pub use id::*;
use imbl_value::Value;
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{
    CallRemoteHandler, Context, Empty, HandlerExt, ParentHandler, from_fn, from_fn_async,
    from_fn_async_local, from_fn_blocking,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::context::{CliContext, DiagnosticContext, InitContext, RpcContext};
use crate::disk::fsck::RequiresReboot;
use crate::registry::context::{RegistryContext, RegistryUrlParams};
use crate::system::kiosk;
use crate::tunnel::context::TunnelUrlParams;
use crate::util::serde::{HandlerExtSerde, WithIoFormat, display_serializable};

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
#[ts(export)]
pub struct EchoParams {
    #[arg(help = "help.arg.echo-message")]
    message: String,
}

pub fn echo<C: Context>(_: C, EchoParams { message }: EchoParams) -> Result<String, RpcError> {
    Ok(message)
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ApiState {
    Error,
    Initializing,
    Running,
}
impl std::fmt::Display for ApiState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self, f)
    }
}

pub fn main_api<C: Context>() -> ParentHandler<C> {
    let mut api = ParentHandler::new()
        .subcommand(
            "git-info",
            from_fn(|_: C| version::git_info()).with_about("about.display-githash"),
        )
        .subcommand(
            "echo",
            from_fn(echo::<RpcContext>)
                .with_metadata("authenticated", Value::Bool(false))
                .with_about("about.echo-message")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "state",
            from_fn(|_: RpcContext| Ok::<_, Error>(ApiState::Running))
                .with_metadata("authenticated", Value::Bool(false))
                .with_about("about.display-current-api")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "state",
            from_fn(|_: InitContext| Ok::<_, Error>(ApiState::Initializing))
                .with_metadata("authenticated", Value::Bool(false))
                .with_about("about.display-current-api")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "state",
            from_fn(|_: DiagnosticContext| Ok::<_, Error>(ApiState::Error))
                .with_metadata("authenticated", Value::Bool(false))
                .with_about("about.display-current-api")
                .with_call_remote::<CliContext>(),
        )
        .subcommand("server", server::<C>().with_about("about.commands-server"))
        .subcommand(
            "package",
            package::<C>().with_about("about.commands-packages"),
        )
        .subcommand(
            "net",
            net::net_api::<C>().with_about("about.network-commands"),
        )
        .subcommand(
            "auth",
            auth::auth::<C, RpcContext>().with_about("about.commands-authentication"),
        )
        .subcommand("db", db::db::<C>().with_about("about.commands-db"))
        .subcommand("ssh", ssh::ssh::<C>().with_about("about.commands-ssh-keys"))
        .subcommand(
            "wifi",
            net::wifi::wifi::<C>().with_about("about.commands-wifi"),
        )
        .subcommand("disk", disk::disk::<C>().with_about("about.commands-disk"))
        .subcommand(
            "notification",
            notifications::notification::<C>().with_about("about.commands-notifications"),
        )
        .subcommand(
            "backup",
            backup::backup::<C>().with_about("about.commands-backup"),
        )
        .subcommand(
            "registry",
            CallRemoteHandler::<RpcContext, _, _, RegistryUrlParams>::new(
                registry::registry_api::<RegistryContext>(),
            )
            .no_cli(),
        )
        .subcommand(
            "registry",
            registry::registry_api::<CliContext>().with_about("about.commands-registry"),
        )
        .subcommand(
            "tunnel",
            CallRemoteHandler::<RpcContext, _, _, TunnelUrlParams>::new(tunnel::api::tunnel_api())
                .no_cli(),
        )
        .subcommand(
            "tunnel",
            tunnel::api::tunnel_api::<CliContext>().with_about("about.commands-tunnel"),
        )
        .subcommand("s9pk", s9pk::rpc::s9pk().with_about("about.commands-s9pk"))
        .subcommand(
            "util",
            util::rpc::util::<C>().with_about("about.command-blake3-hash"),
        )
        .subcommand(
            "init-key",
            from_fn_async(developer::init)
                .no_display()
                .with_about("about.create-developer-key"),
        )
        .subcommand(
            "pubkey",
            from_fn_blocking(developer::pubkey).with_about("about.get-developer-pubkey"),
        )
        .subcommand(
            "diagnostic",
            diagnostic::diagnostic::<C>().with_about("about.commands-diagnostic"),
        )
        .subcommand(
            "init",
            init::init_api::<C>().with_about("about.commands-init"),
        )
        .subcommand(
            "setup",
            setup::setup::<C>().with_about("about.commands-setup"),
        );
    if &*PLATFORM != "raspberrypi" {
        api = api.subcommand("kiosk", kiosk::<C>().with_about("about.commands-kiosk"));
    }
    #[cfg(target_os = "linux")]
    {
        api = api.subcommand(
            "flash-os",
            from_fn_async(os_install::cli_install_os)
                .no_display()
                .with_about("about.flash-startos"),
        );
    }
    api
}

pub fn server<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "time",
            from_fn_async(system::time)
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| {
                    system::display_time(handle.params, result)
                })
                .with_about("about.display-time-uptime")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "device-info",
            ParentHandler::<C, WithIoFormat<Empty>>::new().root_handler(
                from_fn_async(system::device_info)
                    .with_display_serializable()
                    .with_custom_display_fn(|handle, result| {
                        system::display_device_info(handle.params, result)
                    })
                    .with_about("about.get-device-info")
                    .with_call_remote::<CliContext>(),
            ),
        )
        .subcommand(
            "experimental",
            system::experimental::<C>().with_about("about.commands-experimental"),
        )
        .subcommand(
            "logs",
            system::logs::<RpcContext>().with_about("about.display-os-logs"),
        )
        .subcommand(
            "logs",
            from_fn_async(logs::cli_logs::<RpcContext, Empty>)
                .no_display()
                .with_about("about.display-os-logs"),
        )
        .subcommand(
            "kernel-logs",
            system::kernel_logs::<RpcContext>().with_about("about.display-kernel-logs"),
        )
        .subcommand(
            "kernel-logs",
            from_fn_async(logs::cli_logs::<RpcContext, Empty>)
                .no_display()
                .with_about("about.display-kernel-logs"),
        )
        .subcommand(
            "metrics",
            ParentHandler::<C, WithIoFormat<Empty>>::new()
                .root_handler(
                    from_fn_async(system::metrics)
                        .with_display_serializable()
                        .with_about("about.display-server-metrics")
                        .with_call_remote::<CliContext>(),
                )
                .subcommand("follow", from_fn_async(system::metrics_follow).no_cli()),
        )
        .subcommand(
            "shutdown",
            from_fn_async(shutdown::shutdown)
                .no_display()
                .with_about("about.shutdown-server")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "restart",
            from_fn_async(shutdown::restart)
                .no_display()
                .with_about("about.restart-server")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "rebuild",
            from_fn_async(shutdown::rebuild)
                .no_display()
                .with_about("about.teardown-rebuild-containers")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "update",
            from_fn_async(update::update_system)
                .with_metadata("sync_db", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "update",
            from_fn_async(update::cli_update_system)
                .no_display()
                .with_about("about.check-update-startos"),
        )
        .subcommand(
            "update-firmware",
            from_fn_async(|_: RpcContext| async {
                if let Some(firmware) = firmware::check_for_firmware_update().await? {
                    firmware::update_firmware(firmware).await?;
                    Ok::<_, Error>(RequiresReboot(true))
                } else {
                    Ok(RequiresReboot(false))
                }
            })
            .with_custom_display_fn(|_handle, result| {
                Ok(firmware::display_firmware_update_result(result))
            })
            .with_about("about.update-firmware")
            .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-smtp",
            from_fn_async(system::set_system_smtp)
                .no_display()
                .with_about("about.set-smtp")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "test-smtp",
            from_fn_async(system::test_smtp)
                .no_display()
                .with_about("about.test-smtp")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "clear-smtp",
            from_fn_async(system::clear_system_smtp)
                .no_display()
                .with_about("about.clear-smtp")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "host",
            net::host::server_host_api::<C>().with_about("about.commands-host-system-ui"),
        )
        .subcommand(
            "set-hostname",
            from_fn_async(hostname::set_hostname_rpc)
                .no_display()
                .with_about("about.set-hostname")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-echoip-urls",
            from_fn_async(system::set_echoip_urls)
                .no_display()
                .with_about("about.set-echoip-urls")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-keyboard",
            from_fn_async(system::set_keyboard)
                .no_display()
                .with_about("about.set-keyboard")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-language",
            from_fn_async(system::set_language)
                .no_display()
                .with_about("about.set-language")
                .with_call_remote::<CliContext>(),
        )
}

pub fn package<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "action",
            action::action_api::<C>().with_about("about.commands-action"),
        )
        .subcommand(
            "install",
            from_fn_async(install::install)
                .with_metadata("sync_db", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "sideload",
            from_fn_async(install::sideload)
                .with_metadata("get_session", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "sideload-url",
            from_fn_async(install::sideload_url)
                .with_metadata("sync_db", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "install",
            from_fn_async_local(install::cli_install)
                .no_display()
                .with_about("about.install-package"),
        )
        .subcommand(
            "cancel-install",
            from_fn(install::cancel_install)
                .no_display()
                .with_about("about.cancel-install-package")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "uninstall",
            from_fn_async(install::uninstall)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.remove-package")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "list",
            from_fn_async(install::list)
                .with_display_serializable()
                .with_about("about.list-installed-packages")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "installed-version",
            from_fn_async(install::installed_version)
                .with_display_serializable()
                .with_about("about.display-installed-version")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "start",
            from_fn_async(control::start)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.start-service")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "stop",
            from_fn_async(control::stop)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.stop-service")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "restart",
            from_fn_async(control::restart)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.restart-service")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "rebuild",
            from_fn_async(service::rebuild)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.rebuild-service-container")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "stats",
            from_fn_async(lxc::stats)
                .with_display_serializable()
                .with_custom_display_fn(|args, res| {
                    if let Some(format) = args.params.format {
                        return display_serializable(format, res);
                    }

                    use prettytable::*;
                    let mut table = table!([
                        "Name",
                        "Container ID",
                        "Memory Usage",
                        "Memory Limit",
                        "Memory %"
                    ]);
                    for (id, stats) in res {
                        if let Some(stats) = stats {
                            table.add_row(row![
                                &*id,
                                &*stats.container_id,
                                stats.memory_usage,
                                stats.memory_limit,
                                format!(
                                    "{:.2}",
                                    stats.memory_usage.0 as f64 / stats.memory_limit.0 as f64
                                        * 100.0
                                )
                            ]);
                        } else {
                            table.add_row(row![&*id, "N/A", "0 MiB", "0 MiB", "0"]);
                        }
                    }
                    table.print_tty(false)?;
                    Ok(())
                })
                .with_about("about.list-lxc-container-info")
                .with_call_remote::<CliContext>(),
        )
        .subcommand("logs", logs::package_logs())
        .subcommand(
            "logs",
            logs::package_logs().with_about("about.display-package-logs"),
        )
        .subcommand(
            "logs",
            from_fn_async(logs::cli_logs::<RpcContext, logs::PackageIdParams>)
                .no_display()
                .with_about("about.display-package-logs"),
        )
        .subcommand(
            "backup",
            backup::package_backup::<C>().with_about("about.commands-restore-backup"),
        )
        .subcommand(
            "attach",
            from_fn_async(service::attach)
                .with_metadata("get_session", Value::Bool(true))
                .with_about("about.execute-commands-container")
                .no_cli(),
        )
        .subcommand(
            "attach",
            from_fn_async_local(service::cli_attach)
                .no_display()
                .with_about("about.execute-commands-container"),
        )
        .subcommand(
            "host",
            net::host::host_api::<C>().with_about("about.manage-network-hosts-package"),
        )
        .subcommand(
            "set-outbound-gateway",
            from_fn_async(net::gateway::set_outbound_gateway)
                .with_metadata("sync_db", Value::Bool(true))
                .no_display()
                .with_about("about.set-outbound-gateway-package")
                .with_call_remote::<CliContext>(),
        )
}
