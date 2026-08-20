use rpc_toolkit::{Context, HandlerExt, ParentHandler, from_fn_async};

pub mod add;
pub mod get;
pub mod sign;

pub fn asset_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand("add", add::add_api::<C>())
        .subcommand(
            "add",
            from_fn_async(add::cli_add_asset)
                .no_display()
                .with_about("about.add-asset-registry"),
        )
        .subcommand(
            "remove",
            add::remove_api::<C>().with_about("about.commands-remove-assets"),
        )
        .subcommand("sign", sign::sign_api::<C>())
        .subcommand(
            "sign",
            from_fn_async(sign::cli_sign_asset)
                .no_display()
                .with_about("about.sign-file-add-registry"),
        )
        .subcommand(
            "get",
            get::get_api::<C>().with_about("about.commands-download-assets"),
        )
}
