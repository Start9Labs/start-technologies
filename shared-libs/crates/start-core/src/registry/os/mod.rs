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
            "vulnerability",
            ParentHandler::<C>::new()
                .subcommand(
                    "add",
                    from_fn_async(crate::registry::vulnerability::add_os_vulnerability)
                        .with_metadata(crate::registry::upstream::WRITES_INDEX, Value::Bool(true))
                        .with_metadata("admin", Value::Bool(true))
                        .with_metadata("get_signer", Value::Bool(true))
                        .no_cli(),
                )
                .subcommand(
                    "add",
                    from_fn_async(crate::registry::vulnerability::cli_add_os_vulnerability)
                        .no_ts()
                        .no_display()
                        .with_about("about.add-os-vulnerability"),
                )
                .subcommand(
                    "remove",
                    from_fn_async(crate::registry::vulnerability::remove_os_vulnerability)
                        .with_metadata(crate::registry::upstream::WRITES_INDEX, Value::Bool(true))
                        .with_metadata("admin", Value::Bool(true))
                        .no_display()
                        .with_about("about.remove-os-vulnerability")
                        .with_call_remote::<CliContext>(),
                )
                .with_about("about.commands-os-vulnerabilities"),
        )
        .subcommand(
            "promote",
            from_fn_async(promote::cli_os_promote)
                .no_ts()
                .no_display()
                .with_about("about.promote-os-registry"),
        )
}
