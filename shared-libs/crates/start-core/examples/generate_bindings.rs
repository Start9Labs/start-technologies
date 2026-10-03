use std::path::Path;

use rpc_toolkit::ts::{HandlerTSBindings, handler_bindings};
use start_core::context::{DiagnosticContext, InitContext, RpcContext, SetupContext};
use start_core::registry::context::RegistryContext;
use start_core::tunnel::context::TunnelContext;

fn write_api(
    directory: &Path,
    name: &str,
    handler: impl HandlerTSBindings,
) -> Result<(), Box<dyn std::error::Error>> {
    let bindings = handler_bindings(&handler, "Api")?.ok_or("API bindings are disabled")?;
    std::fs::write(directory.join(format!("{name}.ts")), bindings)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::args()
        .nth(1)
        .ok_or("expected an output directory")?;
    let directory = Path::new(&directory);
    std::fs::create_dir_all(directory.join("rpc"))?;
    let rpc = directory.join("rpc");
    write_api(&rpc, "start-os", start_core::main_api::<RpcContext>())?;
    write_api(&rpc, "setup", start_core::main_api::<SetupContext>())?;
    write_api(&rpc, "init", start_core::main_api::<InitContext>())?;
    write_api(
        &rpc,
        "diagnostic",
        start_core::main_api::<DiagnosticContext>(),
    )?;
    write_api(
        &rpc,
        "registry",
        start_core::registry::registry_api::<RegistryContext>(),
    )?;
    write_api(
        &rpc,
        "tunnel",
        start_core::tunnel::api::tunnel_api::<TunnelContext>(),
    )?;
    write_api(
        &rpc,
        "effects",
        start_core::service::effects::handler::<start_core::service::effects::context::EffectContext>(
        ),
    )?;
    std::fs::write(
        directory.join("types.ts"),
        visit_rs::ts::export_namespace("start_core", "")?,
    )?;
    std::fs::create_dir_all(directory.join("tunnel"))?;
    std::fs::write(
        directory.join("tunnel/types.ts"),
        visit_rs::ts::export_namespace("start_core", "tunnel")?,
    )?;
    std::fs::write(
        directory.join("tunnel/index.ts"),
        "export * from './types'\n",
    )?;
    std::fs::write(
        directory.join("index.ts"),
        "export * from './types'\nexport * as Tunnel from './tunnel'\n",
    )?;
    std::fs::write(
        directory.join("rpc/index.ts"),
        "export type { Api as StartOS } from './start-os'\nexport type { Api as Setup } from './setup'\nexport type { Api as Init } from './init'\nexport type { Api as Diagnostic } from './diagnostic'\nexport type { Api as Registry } from './registry'\nexport type { Api as Tunnel } from './tunnel'\nexport type { Api as Effects } from './effects'\nexport type { RpcMethod, RpcParamType, RpcReturnType } from './start-os'\n",
    )?;
    Ok(())
}
