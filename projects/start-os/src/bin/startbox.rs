use start_core::bins::MultiExecutable;

fn main() {
    start_core::bins::PRODUCT_VERSION
        .set(env!("CARGO_PKG_VERSION"))
        .ok();
    start_core::net::static_server::UI_CELL
        .set(include_dir::include_dir!(
            "$CARGO_MANIFEST_DIR/web/dist/static/ui"
        ))
        .ok();
    start_core::net::static_server::SETUP_WIZARD_CELL
        .set(include_dir::include_dir!(
            "$CARGO_MANIFEST_DIR/web/dist/static/setup-wizard"
        ))
        .ok();
    #[cfg(not(feature = "beta"))]
    start_core::db::model::public::DB_UI_SEED_CELL
        .set(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/web/patchdb-ui-seed.json"
        )))
        .ok();
    #[cfg(feature = "beta")]
    start_core::db::model::public::DB_UI_SEED_CELL
        .set(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/web/patchdb-ui-seed.beta.json"
        )))
        .ok();
    MultiExecutable::default()
        .enable_startd()
        .enable_start_cli()
        .enable_unshare_userns()
        .execute()
}
