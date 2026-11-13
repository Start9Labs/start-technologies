use start_core::bins::MultiExecutable;

// musl's malloc fragments under startd's large, varied responses and rarely returns
// memory to the OS, so startd's RSS grows until it is OOM-killed. mimalloc does not.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    start_core::bins::PRODUCT_VERSION
        .set(start_core::bins::startos_version())
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
    start_core::db::model::public::DB_UI_SEED_CELL
        .set(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/web/patchdb-ui-seed.json"
        )))
        .ok();
    MultiExecutable::default()
        .enable_startd()
        .enable_start_cli()
        .enable_unshare_userns()
        .execute()
}
