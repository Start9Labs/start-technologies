use startwrt::bins::MultiExecutable;

fn main() {
    MultiExecutable::default()
        .enable_daemon()
        .enable_cli()
        .enable_fdl_reset()
        .set_default("startwrt-cli")
        .execute()
}
