use std::collections::BTreeSet;

use rpc_toolkit::{Context, Empty, HandlerExt, ParentHandler, from_fn_async};
use url::Url;

use crate::context::{CliContext, RpcContext};
use crate::middleware::auth::signature::SigningContext;
use crate::prelude::*;
use crate::registry::context::RegistryUrlParams;
use crate::sign::AnyVerifyingKey;
use crate::util::serde::{HandlerExtSerde, WithIoFormat, display_serializable};

pub fn registry_signing_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "add",
            from_fn_async(add_signed_registry)
                .no_display()
                .with_about("about.add-signed-registry")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "remove",
            from_fn_async(remove_signed_registry)
                .no_display()
                .with_about("about.remove-signed-registry")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "list",
            from_fn_async(list_signed_registries)
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| {
                    display_signed_registries(handle.params, result)
                })
                .with_about("about.list-signed-registries")
                .with_call_remote::<CliContext>(),
        )
        .subcommand(
            "key",
            from_fn_async(registry_signing_key)
                .with_display_serializable()
                .with_custom_display_fn(|handle, key| {
                    if let Some(format) = handle.params.format {
                        return display_serializable(format, key);
                    }
                    println!("{key}");
                    Ok(())
                })
                .with_about("about.registry-signing-key")
                .with_call_remote::<CliContext>(),
        )
}

pub async fn add_signed_registry(
    ctx: RpcContext,
    RegistryUrlParams { registry }: RegistryUrlParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_signed_registries_mut()
                .mutate(|r| {
                    r.insert(registry);
                    Ok(())
                })
        })
        .await
        .result
}

pub async fn remove_signed_registry(
    ctx: RpcContext,
    RegistryUrlParams { registry }: RegistryUrlParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_server_info_mut()
                .as_signed_registries_mut()
                .mutate(|r| {
                    r.remove(&registry);
                    Ok(())
                })
        })
        .await
        .result
}

pub async fn list_signed_registries(ctx: RpcContext) -> Result<BTreeSet<Url>, Error> {
    ctx.db
        .peek()
        .await
        .as_public()
        .as_server_info()
        .as_signed_registries()
        .de()
}

fn display_signed_registries(
    params: WithIoFormat<Empty>,
    registries: BTreeSet<Url>,
) -> Result<(), Error> {
    if let Some(format) = params.format {
        return display_serializable(format, registries);
    }
    for registry in registries {
        println!("{registry}");
    }
    Ok(())
}

/// The key a registry admin registers as a signer to recognize this server.
pub async fn registry_signing_key(ctx: RpcContext) -> Result<AnyVerifyingKey, Error> {
    Ok(ctx.signing_key()?.verifying_key())
}
