use std::path::Path;

use rpc_toolkit::ts::{Bindings, HandlerTSBindings, export_namespace, handler_bindings};
use start_core::context::{DiagnosticContext, InitContext, RpcContext, SetupContext};
use start_core::registry::context::RegistryContext;
use start_core::tunnel::context::TunnelContext;

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
    std::fs::write(
        directory.join("session-profile.ts"),
        format!(
            "export const SESSION_PROFILE = {} as const\n",
            serde_json::to_string_pretty(&start_core::middleware::auth::session::browser_profile())?,
        ),
    )?;
    let mut signature_profile = start_core::sign::browser_profile();
    signature_profile["http"] = start_core::middleware::auth::signature::wire_profile();
    std::fs::write(
        directory.join("signature-profile.ts"),
        format!(
            "export const SIGNATURE_PROFILE = {} as const\n",
            serde_json::to_string_pretty(&signature_profile)?,
        ),
    )?;
    write_pair(
        directory,
        "session",
        export_namespace("start_core", "session", "./session.schemas")?,
    )?;
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
    let storage = serde_json::json!({
        "sharedVolumes": start_core::shared_volumes::local_directory(start_core::PACKAGE_DATA),
        "backupTargets": start_core::backup::target::local_directory(start_core::PACKAGE_DATA),
    });
    std::fs::write(
        directory.join("storage.ts"),
        format!(
            "/** Host storage directories; each managed ID names a child directory. */\nexport const managedStorage = {storage} as const\n"
        ),
    )?;
    std::fs::write(
        directory.join("index.ts"),
        "export * from './types'\nexport { managedStorage } from './storage'\nexport * as Tunnel from './tunnel'\n",
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
        format!(
            "{}export type {{ {} }} from './start-os'\n",
            "export type { Api as StartOS } from './start-os'\nexport type { Api as Setup } from './setup'\nexport type { Api as Init } from './init'\nexport type { Api as Diagnostic } from './diagnostic'\nexport type { Api as Registry } from './registry'\nexport type { Api as Tunnel } from './tunnel'\nexport type { Api as Effects } from './effects'\n",
            rpc_toolkit::ts::type_helper_names()
                .collect::<Vec<_>>()
                .join(", "),
        ),
    )?;
    Ok(())
}
