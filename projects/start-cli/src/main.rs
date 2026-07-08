use start_core::bins::MultiExecutable;
use start_core::s9pk::v2::pack::PREFER_DOCKER;

fn main() {
    start_core::bins::PRODUCT_VERSION
        .set(env!("CARGO_PKG_VERSION"))
        .ok();
    if !std::env::var("STARTOS_USE_PODMAN").map_or(false, |v| {
        let v = v.trim();
        if ["1", "true", "y", "yes"].into_iter().any(|x| v == x) {
            true
        } else if ["0", "false", "n", "no"].into_iter().any(|x| v == x) {
            false
        } else {
            tracing::warn!("Unknown value for STARTOS_USE_PODMAN: {v}");
            false
        }
    }) {
        PREFER_DOCKER.set(true).ok();
    }
    MultiExecutable::default()
        .enable_start_cli()
        .set_default("start-cli")
        .execute()
}
