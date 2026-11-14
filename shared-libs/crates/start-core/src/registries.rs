use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};

use crate::context::CliContext;
use crate::prelude::*;
use crate::trusted_keys::connection::{connect, list, refresh, remove, set_os_registry};
use crate::util::serde::HandlerExtSerde;

pub fn api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "list",
            from_fn_async(list)
                .with_display_serializable()
                .with_about("about.list-registries")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "add",
            from_fn_async(connect)
                .with_metadata("sync_db", Value::Bool(true))
                .with_display_serializable()
                .with_about("about.add-registry")
                .with_call_remote::<CliContext>(),
        )
        .subcommand("refresh", from_fn_async(refresh).no_cli())
        .subcommand(
            "remove",
            from_fn_async(remove)
                .with_metadata("sync_db", Value::Bool(true))
                .with_display_serializable()
                .with_about("about.remove-registry")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "set-os",
            from_fn_async(set_os_registry)
                .with_metadata("sync_db", Value::Bool(true))
                .with_display_serializable()
                .with_about("about.set-os-registry")
                .with_call_remote::<CliContext>(),
        )
}
