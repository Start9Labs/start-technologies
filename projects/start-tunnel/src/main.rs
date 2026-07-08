use start_core::bins::MultiExecutable;

fn main() {
    start_core::bins::PRODUCT_VERSION
        .set(env!("CARGO_PKG_VERSION"))
        .ok();
    start_core::tunnel::context::TUNNEL_UI_CELL
        .set(include_dir::include_dir!(
            "$CARGO_MANIFEST_DIR/web/dist/static/start-tunnel"
        ))
        .ok();
    MultiExecutable::default()
        .enable_start_tunnel()
        .enable_start_tunneld()
        .execute()
}
