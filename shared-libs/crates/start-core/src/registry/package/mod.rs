use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async, from_fn_async_local};

use crate::context::CliContext;
use crate::prelude::*;
use crate::util::serde::HandlerExtSerde;

pub mod add;
pub mod category;
pub mod get;
pub mod icon;
pub mod index;
pub mod promote;
pub mod signer;

pub fn package_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "index",
            from_fn_async(index::get_package_index)
                .with_metadata("authenticated", Value::Bool(false))
                .with_display_serializable()
                .with_about("about.list-packages-categories")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "add",
            from_fn_async(add::add_package)
                .with_metadata("get_signer", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "add",
            from_fn_async(add::cli_add_package)
                .no_display()
                .with_about("about.add-package-registry"),
        )
        .subcommand(
            "add-mirror",
            from_fn_async(add::add_mirror)
                .with_metadata("get_signer", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "add-mirror",
            from_fn_async(add::cli_add_mirror)
                .no_display()
                .with_about("about.add-mirror-s9pk"),
        )
        .subcommand(
            "remove",
            from_fn_async(add::remove_package)
                .with_metadata("get_signer", Value::Bool(true))
                .with_custom_display_fn(|args, changed| {
                    if !changed {
                        tracing::warn!(
                            "{}",
                            t!(
                                "registry.package.remove-not-exist",
                                id = args.params.id,
                                version = args
                                    .params
                                    .version
                                    .as_ref()
                                    .map_or(String::new(), |v| format!("@{v}")),
                                sighash = args
                                    .params
                                    .sighash
                                    .map_or(String::new(), |h| format!("#{h}"))
                            )
                        );
                    }
                    Ok(())
                })
                .with_about("about.remove-package-registry")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove-mirror",
            from_fn_async(add::remove_mirror)
                .with_metadata("get_signer", Value::Bool(true))
                .no_display()
                .with_about("about.remove-mirror-package")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "signer",
            signer::signer_api::<C>().with_about("about.add-remove-list-package-signers"),
        )
        .subcommand(
            "get",
            from_fn_async(get::get_package)
                .with_metadata("authenticated", Value::Bool(false))
                .with_metadata("get_device_info", Value::Bool(true))
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| {
                    get::display_package_info(handle.params, result)
                })
                .with_about("about.list-installation-candidates")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "download",
            from_fn_async_local(get::cli_download)
                .no_display()
                .with_about("about.download-s9pk"),
        )
        .subcommand(
            "promote",
            from_fn_async(promote::cli_promote)
                .no_display()
                .with_about("about.promote-package-registry"),
        )
        .subcommand(
            "category",
            category::category_api::<C>().with_about("about.update-categories-registry"),
        )
}
