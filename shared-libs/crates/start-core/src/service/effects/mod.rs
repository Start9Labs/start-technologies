use std::net::Ipv4Addr;

use rpc_toolkit::{
    Context, HandlerArgs, HandlerExt, ParentHandler, from_fn, from_fn_async, from_fn_blocking,
};

use crate::prelude::*;
use crate::service::cli::ContainerCliContext;
use crate::service::effects::context::EffectContext;
use crate::util::serde::{HandlerExtSerde, StdinDeserializable};
use crate::{HOST_IP, echo};

mod action;
mod backup;
mod bind_mount;
pub mod callbacks;
pub mod context;
mod control;
pub(super) mod dependency;
mod files;
mod health;
mod init;
mod net;
pub mod notification;
pub mod plugin;
mod prelude;
mod shared_volumes;
pub mod subcontainer;
mod system;
mod version;

/// Effect parameters read from stdin in the effect's own wire format.
#[derive(serde::Deserialize, serde::Serialize, clap::Parser)]
#[group(skip)]
#[serde(transparent)]
struct StdinParams {
    #[command(flatten)]
    params: StdinDeserializable<Value>,
}

/// Fronts an effect whose parameters have no flag form.
async fn forward_stdin(
    HandlerArgs {
        context,
        parent_method,
        method,
        params: StdinParams {
            params: StdinDeserializable(params),
        },
        ..
    }: HandlerArgs<ContainerCliContext, StdinParams>,
) -> Result<Value, Error> {
    crate::context::call_fronted::<EffectContext, _>(&context, parent_method, method, &params).await
}

pub fn handler<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "git-info",
            from_fn(|_: C| crate::version::git_info()).with_about("about.container-git-info"),
        )
        .subcommand(
            "echo",
            from_fn(echo::<EffectContext>)
                .with_about("about.container-echo")
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "prepare-upload",
            from_fn_async(files::prepare_upload).no_cli(),
        )
        .subcommand(
            "prepare-download",
            from_fn_async(files::prepare_download).no_cli(),
        )
        .subcommand(
            "resolve-upload",
            from_fn_async(files::resolve_upload).no_cli(),
        )
        .subcommand(
            "get-upload-limits",
            from_fn_async(files::get_upload_limits).no_cli(),
        )
        // action
        .subcommand(
            "action",
            action::action_api::<C>().with_about("about.container-commands-action"),
        )
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
                .with_about("about.container-rebuild")
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "restart",
            from_fn_async(control::restart)
                .no_display()
                .with_about("about.container-restart")
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "shutdown",
            from_fn_async(control::shutdown)
                .no_display()
                .with_about("about.container-shutdown")
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "set-main-status",
            from_fn_async(control::set_main_status)
                .no_display()
                .with_about("about.container-set-main-status")
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "get-status",
            from_fn_async(control::get_status)
                .with_display_serializable()
                .with_about("about.container-get-status")
                .with_call_remote::<ContainerCliContext>(),
        )
        // dependency
        .subcommand(
            "set-dependencies",
            from_fn_async(dependency::set_dependencies)
                .no_display()
                .with_about("about.container-set-dependencies")
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "get-dependencies",
            from_fn_async(dependency::get_dependencies)
                .with_display_serializable()
                .with_about("about.container-get-dependencies")
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "check-dependencies",
            from_fn_async(dependency::check_dependencies)
                .with_display_serializable()
                .with_about("about.container-check-dependencies")
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "get-shared-volumes",
            from_fn_async(shared_volumes::get_shared_volumes)
                .with_display_serializable()
                .with_about("about.container-get-shared-volumes")
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "mount-shared-volume",
            from_fn_async(shared_volumes::mount_shared_volume)
                .with_display_serializable()
                .with_about("about.container-mount-shared-volume")
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand("mount", from_fn_async(dependency::mount).no_cli())
        .subcommand(
            "mount",
            from_fn_async(forward_stdin)
                .no_ts()
                .no_display()
                .with_about("about.container-mount"),
        )
        .subcommand(
            "bind-mount",
            from_fn_async(bind_mount::bind_mount)
                .no_display()
                .with_about("about.container-bind-mount"),
        )
        .subcommand(
            "get-installed-packages",
            from_fn_async(dependency::get_installed_packages)
                .with_display_serializable()
                .with_about("about.container-get-installed-packages")
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "get-service-manifest",
            from_fn_async(dependency::get_service_manifest)
                .with_display_serializable()
                .with_about("about.container-get-service-manifest")
                .with_call_remote::<ContainerCliContext>(),
        )
        // backup
        .subcommand(
            "set-backup-progress",
            from_fn_async(backup::set_backup_progress).no_cli(),
        )
        .subcommand(
            "set-backup-progress",
            from_fn_async(forward_stdin)
                .no_ts()
                .no_display()
                .with_about("about.container-set-backup-progress"),
        )
        // init
        .subcommand(
            "set-init-progress",
            from_fn_async(init::set_init_progress).no_cli(),
        )
        .subcommand(
            "set-init-progress",
            from_fn_async(forward_stdin)
                .no_ts()
                .no_display()
                .with_about("about.container-set-init-progress"),
        )
        // health
        .subcommand("set-health", from_fn_async(health::set_health).no_cli())
        .subcommand(
            "set-health",
            from_fn_async(forward_stdin)
                .no_ts()
                .no_display()
                .with_about("about.container-set-health"),
        )
        .subcommand("clear-health", from_fn_async(health::clear_health).no_cli())
        // notification
        .subcommand(
            "notification",
            ParentHandler::<C>::new()
                .subcommand(
                    "create",
                    from_fn_async(notification::create)
                        .no_display()
                        .with_about("about.container-notification-create")
                        .with_call_remote::<ContainerCliContext>(),
                )
                .with_about("about.container-commands-notification"),
        )
        // subcontainer
        .subcommand(
            "subcontainer",
            ParentHandler::<C>::new()
                .subcommand(
                    "launch",
                    from_fn_blocking(subcontainer::launch)
                        .no_ts()
                        .no_display()
                        .with_about("about.container-subcontainer-launch"),
                )
                .subcommand(
                    "launch-init",
                    from_fn_blocking(subcontainer::launch_init)
                        .no_ts()
                        .no_display()
                        .with_about("about.container-subcontainer-launch-init"),
                )
                .subcommand(
                    "exec",
                    from_fn_blocking(subcontainer::exec)
                        .no_ts()
                        .no_display()
                        .with_about("about.container-subcontainer-exec"),
                )
                .subcommand(
                    "exec-command",
                    from_fn_blocking(subcontainer::exec_command)
                        .no_ts()
                        .no_display()
                        .with_about("about.container-subcontainer-exec-command"),
                )
                .subcommand(
                    "create-fs",
                    from_fn_async(subcontainer::create_subcontainer_fs)
                        .with_custom_display_fn(|_, (path, _)| Ok(println!("{}", path.display())))
                        .with_about("about.container-subcontainer-create-fs")
                        .with_call_remote::<ContainerCliContext>(),
                )
                .subcommand(
                    "destroy-fs",
                    from_fn_async(subcontainer::destroy_subcontainer_fs)
                        .no_display()
                        .with_about("about.container-subcontainer-destroy-fs")
                        .with_call_remote::<ContainerCliContext>(),
                )
                .with_about("about.container-commands-subcontainer"),
        )
        .subcommand(
            "pipe-wrap",
            from_fn_blocking(subcontainer::pipe_wrap)
                .no_ts()
                .no_display()
                .with_about("about.container-pipe-wrap"),
        )
        // net
        .subcommand("bind", from_fn_async(net::bind::bind).no_cli())
        .subcommand(
            "bind",
            from_fn_async(forward_stdin)
                .no_ts()
                .no_display()
                .with_about("about.container-bind"),
        )
        .subcommand("bind-range", from_fn_async(net::bind::bind_range).no_cli())
        .subcommand(
            "bind-range",
            from_fn_async(forward_stdin)
                .no_ts()
                .no_display()
                .with_about("about.container-bind-range"),
        )
        .subcommand(
            "get-service-port-forward",
            from_fn_async(net::bind::get_service_port_forward)
                .with_display_serializable()
                .with_about("about.container-get-service-port-forward")
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "clear-bindings",
            from_fn_async(net::bind::clear_bindings)
                .no_display()
                .with_about("about.container-clear-bindings")
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "retire-host",
            from_fn_async(net::bind::retire_host)
                .with_display_serializable()
                .with_about("about.container-retire-host")
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "retire-binding",
            from_fn_async(net::bind::retire_binding)
                .with_display_serializable()
                .with_about("about.container-retire-binding")
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "get-host-info",
            from_fn_async(net::host::get_host_info)
                .with_display_serializable()
                .with_about("about.container-get-host-info")
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "get-container-ip",
            from_fn_async(net::info::get_container_ip)
                .with_display_serializable()
                .with_about("about.container-get-container-ip")
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "get-outbound-gateway",
            from_fn_async(net::info::get_outbound_gateway)
                .with_display_serializable()
                .with_about("about.container-get-outbound-gateway")
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "get-os-ip",
            from_fn(|_: C| Ok::<_, Error>(Ipv4Addr::from(HOST_IP)))
                .with_about("about.container-get-os-ip"),
        )
        .subcommand(
            "export-service-interface",
            from_fn_async(net::interface::export_service_interface).no_cli(),
        )
        .subcommand(
            "export-service-interface",
            from_fn_async(forward_stdin)
                .no_ts()
                .no_display()
                .with_about("about.container-export-service-interface"),
        )
        .subcommand(
            "export-range-service-interface",
            from_fn_async(net::interface::export_range_service_interface).no_cli(),
        )
        .subcommand(
            "export-range-service-interface",
            from_fn_async(forward_stdin)
                .no_ts()
                .no_display()
                .with_about("about.container-export-range-service-interface"),
        )
        .subcommand(
            "get-service-interface",
            from_fn_async(net::interface::get_service_interface)
                .with_display_serializable()
                .with_about("about.container-get-service-interface")
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "list-service-interfaces",
            from_fn_async(net::interface::list_service_interfaces)
                .with_display_serializable()
                .with_about("about.container-list-service-interfaces")
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "clear-service-interfaces",
            from_fn_async(net::interface::clear_service_interfaces)
                .no_display()
                .with_about("about.container-clear-service-interfaces")
                .with_call_remote::<ContainerCliContext>(),
        )
        .subcommand(
            "get-ssl-certificate",
            from_fn_async(net::ssl::get_ssl_certificate).no_cli(),
        )
        .subcommand(
            "get-ssl-certificate",
            from_fn_async(forward_stdin)
                .no_ts()
                .with_display_serializable()
                .with_about("about.container-get-ssl-certificate"),
        )
        .subcommand("get-ssl-key", from_fn_async(net::ssl::get_ssl_key).no_cli())
        .subcommand(
            "get-ssl-key",
            from_fn_async(forward_stdin)
                .no_ts()
                .with_display_serializable()
                .with_about("about.container-get-ssl-key"),
        )
        // plugin
        .subcommand(
            "plugin",
            ParentHandler::<C>::new()
                .subcommand(
                    "url",
                    ParentHandler::<C>::new()
                        .subcommand("register", from_fn_async(net::plugin::register).no_cli())
                        .subcommand(
                            "register",
                            from_fn_async(forward_stdin)
                                .no_ts()
                                .no_display()
                                .with_about("about.container-plugin-url-register"),
                        )
                        .subcommand(
                            "export-url",
                            from_fn_async(net::plugin::export_url).no_cli(),
                        )
                        .subcommand(
                            "export-url",
                            from_fn_async(forward_stdin)
                                .no_ts()
                                .no_display()
                                .with_about("about.container-plugin-url-export-url"),
                        )
                        .subcommand(
                            "clear-urls",
                            from_fn_async(net::plugin::clear_urls).no_cli(),
                        )
                        .subcommand(
                            "clear-urls",
                            from_fn_async(forward_stdin)
                                .no_ts()
                                .no_display()
                                .with_about("about.container-plugin-url-clear-urls"),
                        )
                        .with_about("about.container-commands-plugin-url"),
                )
                .with_about("about.container-commands-plugin"),
        )
        .subcommand(
            "set-data-version",
            from_fn_async(version::set_data_version)
                .no_display()
                .with_about("about.container-set-data-version")
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
                .with_about("about.container-get-data-version")
                .with_call_remote::<ContainerCliContext>(),
        )
        // system
        .subcommand(
            "get-system-smtp",
            from_fn_async(system::get_system_smtp)
                .with_display_serializable()
                .with_about("about.container-get-system-smtp")
                .with_call_remote::<ContainerCliContext>(),
        )
}
