use std::path::Path;

use rpc_toolkit::ts::{Bindings, HandlerTSBindings, export_namespace, handler_bindings};
use start_core::context::{DiagnosticContext, InitContext, RpcContext, SetupContext};
use start_core::registry::context::RegistryContext;
use start_core::tunnel::context::TunnelContext;

/// Writes `<name>.ts` (types only) beside `<name>.schemas.ts` (runtime zod).
fn write_pair(
    directory: &Path,
    name: &str,
    bindings: Bindings,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::write(directory.join(format!("{name}.ts")), bindings.types)?;
    std::fs::write(
        directory.join(format!("{name}.schemas.ts")),
        bindings.schemas,
    )?;
    Ok(())
}

fn write_api(
    directory: &Path,
    name: &str,
    handler: impl HandlerTSBindings,
) -> Result<(), Box<dyn std::error::Error>> {
    let bindings = handler_bindings(&handler, "Api", &format!("./{name}.schemas"))?
        .ok_or("API bindings are disabled")?;
    write_pair(directory, name, bindings)
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
    write_pair(
        directory,
        "types",
        export_namespace("start_core", "", "./types.schemas")?,
    )?;
    std::fs::create_dir_all(directory.join("tunnel"))?;
    write_pair(
        &directory.join("tunnel"),
        "types",
        export_namespace("start_core", "tunnel", "./types.schemas")?,
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
        directory.join("schemas.ts"),
        "export * from './types.schemas'\nexport * as Tunnel from './tunnel/types.schemas'\nexport * as RPC from './rpc/schemas'\n",
    )?;
    std::fs::write(
        directory.join("rpc/schemas.ts"),
        "export { Api as StartOS } from './start-os.schemas'\nexport { Api as Setup } from './setup.schemas'\nexport { Api as Init } from './init.schemas'\nexport { Api as Diagnostic } from './diagnostic.schemas'\nexport { Api as Registry } from './registry.schemas'\nexport { Api as Tunnel } from './tunnel.schemas'\nexport { Api as Effects } from './effects.schemas'\n",
    )?;
    std::fs::write(
        directory.join("rpc/index.ts"),
        "export type { Api as StartOS } from './start-os'\nexport type { Api as Setup } from './setup'\nexport type { Api as Init } from './init'\nexport type { Api as Diagnostic } from './diagnostic'\nexport type { Api as Registry } from './registry'\nexport type { Api as Tunnel } from './tunnel'\nexport type { Api as Effects } from './effects'\nexport type { RpcMethod, RpcParamType, RpcReturnType } from './start-os'\n",
    )?;
    Ok(())
}
