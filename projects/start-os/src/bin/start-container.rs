use start_core::bins::MultiExecutable;

fn main() {
    start_core::bins::PRODUCT_VERSION
        .set(env!("CARGO_PKG_VERSION"))
        .ok();
    // unshare-userns must be a multi-call applet, not a CLI subcommand: it
    // runs `unshare(CLONE_NEWUSER)`, which the kernel rejects (EINVAL) on a
    // multi-threaded process. The CLI path isn't single-threaded — its
    // main() enables the logger, whose tracing-appender non-blocking writer
    // spawns a background thread, so a subcommand handler would already be
    // running multi-threaded. The applet entry does no such global init and
    // stays single-threaded.
    MultiExecutable::default()
        .enable_start_container()
        .enable_unshare_userns()
        .set_default("start-container")
        .execute()
}
