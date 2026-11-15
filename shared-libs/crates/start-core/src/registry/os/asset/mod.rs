use exver::Version;
use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};

use crate::prelude::*;
use crate::registry::RegistryDatabase;
use crate::registry::upstream::WRITES_INDEX;
use crate::rpc_continuations::Guid;

pub mod add;
pub mod get;
pub mod sign;

fn may_manage_assets(
    db: &Model<RegistryDatabase>,
    version: &Version,
    signer: &Guid,
) -> Result<bool, Error> {
    Ok(db
        .as_index()
        .as_os()
        .as_versions()
        .as_idx(version)
        .or_not_found(version)?
        .as_authorized()
        .de()?
        .contains(signer)
        || db.as_admins().de()?.contains(signer))
}

pub fn asset_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "add",
            add::add_api::<C>().with_metadata(WRITES_INDEX, Value::Bool(true)),
        )
        .subcommand(
            "add",
            from_fn_async(add::cli_add_asset)
                .no_ts()
                .no_display()
                .with_about("about.add-asset-registry"),
        )
        .subcommand(
            "remove",
            add::remove_api::<C>()
                .with_metadata(WRITES_INDEX, Value::Bool(true))
                .with_about("about.commands-remove-assets"),
        )
        .subcommand(
            "sign",
            sign::sign_api::<C>().with_metadata(WRITES_INDEX, Value::Bool(true)),
        )
        .subcommand(
            "sign",
            from_fn_async(sign::cli_sign_asset)
                .no_ts()
                .no_display()
                .with_about("about.sign-file-add-registry"),
        )
        .subcommand(
            "get",
            get::get_api::<C>().with_about("about.commands-download-assets"),
        )
}
