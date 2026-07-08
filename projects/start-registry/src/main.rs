use start_core::bins::MultiExecutable;

fn main() {
    start_core::bins::PRODUCT_VERSION
        .set(env!("CARGO_PKG_VERSION"))
        .ok();
    MultiExecutable::default()
        .enable_start_registry()
        .enable_start_registryd()
        .execute()
}
