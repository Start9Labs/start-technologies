use std::net::Ipv4Addr;

use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn, from_fn_async, from_fn_blocking};

use crate::prelude::*;
use crate::service::cli::ContainerCliContext;
use crate::service::effects::context::EffectContext;
use crate::{HOST_IP, echo};

mod action;
mod backup;
mod bind_mount;
pub mod callbacks;
pub mod context;
mod control;
mod dependency;
mod health;
mod init;
mod net;
pub mod notification;
pub mod plugin;
mod prelude;
pub mod subcontainer;
mod system;
mod version;

pub fn handler<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand("git-info", from_fn(|_: C| crate::version::git_info()))
        .subcommand(
            "echo",
            from_fn(echo::<EffectContext>).with_call_remote::<ContainerCliContext>(),
        )
        // action
        .subcommand("action", action::action_api::<C>())
        // callbacks
        .subcommand(
            "clear-callbacks",
            from_fn(callbacks::clear_callbacks).no_cli(),
        )
        // control
        .subcommand(
            "rebuild",
            from_fn_async(control::rebuild)
                .no_display()
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "restart",
            from_fn_async(control::restart)
                .no_display()
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "shutdown",
            from_fn_async(control::shutdown)
                .no_display()
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "set-main-status",
            from_fn_async(control::set_main_status)
                .no_display()
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "get-status",
            from_fn_async(control::get_status)
                .no_display()
                .with_call_remote::<ContainerCliContext>(),
        )
        // dependency
        .subcommand(
            "set-dependencies",
            from_fn_async(dependency::set_dependencies)
                .no_display()
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "get-dependencies",
            from_fn_async(dependency::get_dependencies)
                .no_display()
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "check-dependencies",
            from_fn_async(dependency::check_dependencies)
                .no_display()
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand("mount", from_fn_async(dependency::mount).no_cli())
        .subcommand(
            "bind-mount",
            from_fn_async(bind_mount::bind_mount::<C>).no_display(),
        )
        .subcommand(
            "get-installed-packages",
            from_fn_async(dependency::get_installed_packages).no_cli(),
        )
        .subcommand(
            "get-service-manifest",
            from_fn_async(dependency::get_service_manifest).no_cli(),
        )
        // backup
        .subcommand(
            "set-backup-progress",
            from_fn_async(backup::set_backup_progress).no_cli(),
        )
        // init
        .subcommand(
            "set-init-progress",
            from_fn_async(init::set_init_progress).no_cli(),
        )
        // health
        .subcommand("set-health", from_fn_async(health::set_health).no_cli())
        // notification
        .subcommand(
            "notification",
            ParentHandler::<C>::new()
                .subcommand("create", from_fn_async(notification::create).no_cli()),
        )
        // subcontainer
        .subcommand(
            "subcontainer",
            ParentHandler::<C>::new()
                .subcommand(
                    "launch",
                    from_fn_blocking(subcontainer::launch).no_display(),
                )
                .subcommand(
                    "launch-init",
                    from_fn_blocking(subcontainer::launch_init).no_display(),
                )
                .subcommand("exec", from_fn_blocking(subcontainer::exec).no_display())
                .subcommand(
                    "exec-command",
                    from_fn_blocking(subcontainer::exec_command).no_display(),
                )
                .subcommand(
                    "create-fs",
                    from_fn_async(subcontainer::create_subcontainer_fs)
                        .with_custom_display_fn(|_, (path, _)| Ok(println!("{}", path.display())))
                        .with_call_remote::<ContainerCliContext>(),
                )
                .subcommand(
                    "destroy-fs",
                    from_fn_async(subcontainer::destroy_subcontainer_fs)
                        .no_display()
                        .with_call_remote::<ContainerCliContext>(),
                ),
        )
        .subcommand(
            "pipe-wrap",
            from_fn_blocking(subcontainer::pipe_wrap).no_display(),
        )
        // net
        .subcommand("bind", from_fn_async(net::bind::bind).no_cli())
        .subcommand("bind-range", from_fn_async(net::bind::bind_range).no_cli())
        .subcommand(
            "get-service-port-forward",
            from_fn_async(net::bind::get_service_port_forward).no_cli(),
        )
        .subcommand(
            "clear-bindings",
            from_fn_async(net::bind::clear_bindings).no_cli(),
        )
        .subcommand(
            "get-host-info",
            from_fn_async(net::host::get_host_info).no_cli(),
        )
        .subcommand(
            "get-container-ip",
            from_fn_async(net::info::get_container_ip).no_cli(),
        )
        .subcommand(
            "get-outbound-gateway",
            from_fn_async(net::info::get_outbound_gateway).no_cli(),
        )
        .subcommand(
            "get-os-ip",
            from_fn(|_: C| Ok::<_, Error>(Ipv4Addr::from(HOST_IP))),
        )
        .subcommand(
            "export-service-interface",
            from_fn_async(net::interface::export_service_interface).no_cli(),
        )
        .subcommand(
            "export-range-service-interface",
            from_fn_async(net::interface::export_range_service_interface).no_cli(),
        )
        .subcommand(
            "get-service-interface",
            from_fn_async(net::interface::get_service_interface).no_cli(),
        )
        .subcommand(
            "list-service-interfaces",
            from_fn_async(net::interface::list_service_interfaces).no_cli(),
        )
        .subcommand(
            "clear-service-interfaces",
            from_fn_async(net::interface::clear_service_interfaces).no_cli(),
        )
        .subcommand(
            "get-ssl-certificate",
            from_fn_async(net::ssl::get_ssl_certificate).no_cli(),
        )
        .subcommand("get-ssl-key", from_fn_async(net::ssl::get_ssl_key).no_cli())
        // plugin
        .subcommand(
            "plugin",
            ParentHandler::<C>::new().subcommand(
                "url",
                ParentHandler::<C>::new()
                    .subcommand("register", from_fn_async(net::plugin::register).no_cli())
                    .subcommand(
                        "export-url",
                        from_fn_async(net::plugin::export_url).no_cli(),
                    )
                    .subcommand(
                        "clear-urls",
                        from_fn_async(net::plugin::clear_urls).no_cli(),
                    ),
            ),
        )
        .subcommand(
            "set-data-version",
            from_fn_async(version::set_data_version)
                .no_display()
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "get-data-version",
            from_fn_async(version::get_data_version)
                .with_custom_display_fn(|_, v| {
                    if let Some(v) = v {
                        println!("{v}")
                    } else {
                        println!("N/A")
                    }
                    Ok(())
                })
                .with_call_remote::<ContainerCliContext>(),
        )
        // system
        .subcommand(
            "get-system-smtp",
            from_fn_async(system::get_system_smtp).no_cli(),
        )
}
