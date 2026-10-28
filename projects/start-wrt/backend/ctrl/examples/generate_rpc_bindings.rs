use rpc_toolkit::ts::handler_bindings;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("expected an output file")?;
    let path = std::path::PathBuf::from(path);
    let bindings = handler_bindings(
        &startwrt::main_api::<startwrt::ServerContext>(),
        "Api",
        "./bindings.schemas",
    )?
    .ok_or("API bindings are disabled")?;
    std::fs::write(&path, bindings.types)?;
    std::fs::write(path.with_file_name("bindings.schemas.ts"), bindings.schemas)?;
    let events = rpc_toolkit::ts::export_namespace("startwrt", "events", "./events.schemas")?;
    std::fs::write(path.with_file_name("events.ts"), events.types)?;
    std::fs::write(path.with_file_name("events.schemas.ts"), events.schemas)?;
    Ok(())
}
