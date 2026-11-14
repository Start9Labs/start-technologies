use start_core::bins::MultiExecutable;

// Rust allocations use mimalloc; musl's allocator serializes them on one lock.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    start_core::bins::PRODUCT_VERSION
        .set(env!("CARGO_PKG_VERSION"))
        .ok();
    MultiExecutable::default()
        .enable_start_registry()
        .enable_start_registryd()
        .execute()
}
