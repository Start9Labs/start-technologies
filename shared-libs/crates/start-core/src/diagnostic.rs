use std::sync::Arc;

use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{
    CallRemoteHandler, Context, Empty, HandlerExt, ParentHandler, from_fn, from_fn_async,
};

use crate::context::{CliContext, DiagnosticContext, RpcContext};
use crate::disk::repair;
use crate::init::SYSTEM_REBUILD_PATH;
use crate::prelude::*;
use crate::shutdown::Shutdown;
use crate::update::diagnostic as update;
use crate::util::io::delete_file;

pub fn diagnostic<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "error",
            from_fn(error)
                .with_about("about.display-diagnostic-error")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "logs",
            crate::system::logs::<DiagnosticContext>(
                crate::rpc_continuations::WebSocketPolicy::Public,
            )
            .with_about("about.display-os-logs"),
        )
        .subcommand(
            "logs",
            from_fn_async(crate::logs::cli_logs::<DiagnosticContext, Empty>)
                .no_ts()
                .no_display()
                .with_about("about.display-os-logs"),
        )
        .subcommand(
            "kernel-logs",
            crate::system::kernel_logs::<DiagnosticContext>(
                crate::rpc_continuations::WebSocketPolicy::Public,
            )
            .with_about("about.display-kernel-logs"),
        )
        .subcommand(
            "kernel-logs",
            from_fn_async(crate::logs::cli_logs::<DiagnosticContext, Empty>)
                .no_ts()
                .no_display()
                .with_about("about.display-kernel-logs"),
        )
        .subcommand(
            "restart",
            from_fn(restart)
                .no_display()
                .with_about("about.restart-server")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "disk",
            disk::<C>().with_about("about.command-remove-disk-filesystem"),
        )
        .subcommand(
            "rebuild",
            from_fn_async(rebuild)
                .no_display()
                .with_about("about.teardown-rebuild-containers")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "update",
            from_fn_async(update::update_system)
                .with_metadata("get_local_auth", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "update",
            from_fn_async(update::cli_update_system)
                .no_display()
                .with_about("about.check-update-startos"),
        )
}

pub fn error(ctx: DiagnosticContext) -> Result<Arc<RpcError>, Error> {
    Ok(ctx.error.clone())
}

pub fn restart(ctx: DiagnosticContext) -> Result<(), Error> {
    ctx.shutdown
        .send(Shutdown {
            disk_guid: ctx.disk_guid.clone(),
            restart: true,
        })
        .map_err(|_| eyre!("receiver dropped"))
        .log_err();
    Ok(())
}
pub async fn rebuild(ctx: DiagnosticContext) -> Result<(), Error> {
    tokio::fs::write(SYSTEM_REBUILD_PATH, b"").await?;
    restart(ctx)
}

pub fn disk<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "forget",
            from_fn_async(forget_disk::<DiagnosticContext>).no_cli(),
        )
        .subcommand("forget", from_fn_async(forget_disk::<RpcContext>).no_cli())
        .subcommand(
            "forget",
            CallRemoteHandler::<CliContext, _, _>::new(
                from_fn_async(forget_disk::<RpcContext>).no_display(),
            )
            .no_display()
            .with_about("about.remove-disk-filesystem"),
        )
        .subcommand(
            "repair",
            from_fn_async(|_: DiagnosticContext| repair()).no_cli(),
        )
        .subcommand("repair", from_fn_async(|_: RpcContext| repair()).no_cli())
        .subcommand(
            "repair",
            CallRemoteHandler::<CliContext, _, _>::new(
                from_fn_async(|_: RpcContext| repair()).no_display(),
            )
            .no_display()
            .with_about("about.repair-disk-corruption"),
        )
}

pub async fn forget_disk<C: Context>(_: C) -> Result<(), Error> {
    delete_file("/media/startos/config/overlay/etc/hostname").await?;
    crate::context::startup::forget_identity().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use rpc_toolkit::HandlerFor;

    use super::*;
    use crate::context::{InitContext, SetupContext};
    use crate::net::static_server::UiContext;

    #[test]
    fn destructive_disk_commands_are_context_scoped() {
        let init = InitContext::api();
        let setup = SetupContext::api();
        let diagnostic = DiagnosticContext::api();
        let runtime = RpcContext::api();
        let cli = crate::main_api::<CliContext>();

        for (method, diagnostic_available) in [
            ("diagnostic.disk.forget", true),
            ("diagnostic.disk.repair", true),
            ("disk.repair", false),
        ] {
            assert!(init.method_from_dots(method).is_none(), "{method} in init");
            assert!(
                setup.method_from_dots(method).is_none(),
                "{method} in setup"
            );
            assert_eq!(
                diagnostic.method_from_dots(method).is_some(),
                diagnostic_available,
                "{method} in diagnostic",
            );
            let runtime_method = runtime
                .method_from_dots(method)
                .unwrap_or_else(|| panic!("{method} missing from runtime"));
            assert_ne!(
                runtime.metadata(runtime_method).get("authenticated"),
                Some(&Value::Bool(false)),
                "{method} must require runtime authentication",
            );
            assert!(
                cli.method_from_dots(method).is_some(),
                "{method} missing from CLI",
            );
        }
    }
}
