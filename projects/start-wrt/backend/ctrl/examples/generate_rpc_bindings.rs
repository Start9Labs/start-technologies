use rpc_toolkit::ts::handler_bindings;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("expected an output file")?;
    let bindings = handler_bindings(&startwrt::main_api::<startwrt::ServerContext>(), "Api")?
        .ok_or("API bindings are disabled")?;
    let path = std::path::PathBuf::from(path);
    std::fs::write(&path, bindings)?;
    std::fs::write(
        path.with_file_name("events.ts"),
        rpc_toolkit::ts::export_namespace("startwrt", "events")?,
    )?;
    Ok(())
}
