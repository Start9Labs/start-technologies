use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};

use crate::context::CliContext;
use crate::prelude::*;
use crate::util::serde::HandlerExtSerde;

pub const SIG_CONTEXT: &str = "startos";

pub mod asset;
pub mod index;
pub mod promote;
pub mod version;

pub fn os_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "index",
            from_fn_async(index::get_os_index)
                .with_metadata("authenticated", Value::Bool(false))
                .with_display_serializable()
                .with_about("about.list-os-versions-index")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "asset",
            asset::asset_api::<C>().with_about("about.commands-add-sign-remove-get-assets"),
        )
        .subcommand(
            "version",
            version::version_api::<C>().with_about("about.commands-add-remove-list-versions"),
        )
        .subcommand(
            "promote",
            from_fn_async(promote::cli_os_promote)
                .no_display()
                .with_about("about.promote-os-registry"),
        )
}
