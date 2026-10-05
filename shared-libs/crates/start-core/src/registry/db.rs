use std::path::PathBuf;
use std::time::Duration;

use axum::extract::ws;
use clap::Parser;
use itertools::Itertools;
use patch_db::Dump;
use patch_db::json_ptr::{JsonPointer, ROOT};
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{Context, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use ts_rs::TS;

use crate::context::CliContext;
use crate::db::SubscribeRes;
use crate::prelude::*;
use crate::registry::RegistryDatabase;
use crate::registry::context::RegistryContext;
use crate::rpc_continuations::{Guid, RpcContinuation};
use crate::util::serde::{HandlerExtSerde, apply_expr};

lazy_static::lazy_static! {
    static ref INDEX: JsonPointer = "/index".parse().unwrap();
}

pub fn db_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "dump",
            from_fn_async(cli_dump)
                .with_display_serializable()
                .with_about("about.filter-query-db"),
        )
        .subcommand(
            "dump",
            from_fn_async(dump)
                .with_metadata("admin", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "subscribe",
            from_fn_async(subscribe)
                .with_metadata("authenticated", Value::Bool(false))
                .no_cli(),
        )
        .subcommand(
            "apply",
            from_fn_async(cli_apply)
                .no_display()
                .with_about("about.update-db-record"),
        )
        .subcommand(
            "apply",
            from_fn_async(apply)
                .with_metadata("admin", Value::Bool(true))
                .no_cli(),
        )
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct CliDumpParams {
    #[arg(long = "pointer", short = 'p', help = "help.arg.db-pointer")]
    pointer: Option<JsonPointer>,
    #[arg(help = "help.arg.database-path")]
    path: Option<PathBuf>,
}

#[instrument(skip_all)]
async fn cli_dump(
    HandlerArgs {
        context,
        parent_method,
        method,
        params: CliDumpParams { pointer, path },
        ..
    }: HandlerArgs<CliContext, CliDumpParams>,
) -> Result<Dump, RpcError> {
    let dump = if let Some(path) = path {
        PatchDb::open(path).await?.dump(&ROOT).await
    } else {
        let method = parent_method.into_iter().chain(method).join(".");
        from_value::<Dump>(
            context
                .call_remote::<RegistryContext>(&method, imbl_value::json!({ "pointer": pointer }))
                .await?,
        )?
    };

    Ok(dump)
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct DumpParams {
    #[arg(long = "pointer", short = 'p', help = "help.arg.db-pointer")]
    #[ts(type = "string | null")]
    pointer: Option<JsonPointer>,
}

pub async fn dump(ctx: RegistryContext, DumpParams { pointer }: DumpParams) -> Result<Dump, Error> {
    Ok(ctx
        .db
        .dump(&pointer.as_ref().map_or(ROOT, |p| p.borrowed()))
        .await)
}

#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeParams {
    #[ts(type = "string | null")]
    pointer: Option<JsonPointer>,
}

fn subscription_pointer(pointer: Option<JsonPointer>) -> Result<JsonPointer, Error> {
    let pointer = pointer.unwrap_or_else(|| INDEX.clone());
    ensure_code!(
        pointer.starts_with(&*INDEX),
        ErrorKind::InvalidRequest,
        "{}",
        rust_i18n::t!("registry.db.pointer-outside-index")
    );
    Ok(pointer)
}

pub async fn subscribe(
    ctx: RegistryContext,
    SubscribeParams { pointer }: SubscribeParams,
) -> Result<SubscribeRes, Error> {
    let (dump, mut sub) = ctx.db.dump_and_sub(subscription_pointer(pointer)?).await;
    let guid = Guid::new();
    ctx.rpc_continuations
        .add(
            guid.clone(),
            RpcContinuation::ws(
                |mut ws| async move {
                    if let Err(e) = async {
                        loop {
                            tokio::select! {
                                rev = sub.recv() => {
                                    let Some(rev) = rev else {
                                        return ws.normal_close("complete").await;
                                    };
                                    ws.send(ws::Message::Text(
                                        serde_json::to_string(&rev)
                                            .with_kind(ErrorKind::Serialization)?
                                            .into(),
                                    ))
                                    .await
                                    .with_kind(ErrorKind::Network)?;
                                }
                                msg = ws.recv() => {
                                    if msg.transpose().with_kind(ErrorKind::Network)?.is_none() {
                                        return Ok(())
                                    }
                                }
                            }
                        }
                    }
                    .await
                    {
                        if !crate::util::net::is_ws_reset_without_close(&e) {
                            tracing::error!("Error in registry db websocket: {e}");
                            tracing::debug!("{e:?}");
                        }
                    }
                },
                Duration::from_secs(30),
            ),
        )
        .await;

    Ok(SubscribeRes { dump, guid })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscribe_params_allow_an_omitted_or_null_pointer() {
        for params in [
            serde_json::json!({}),
            serde_json::json!({ "pointer": null }),
        ] {
            let params: SubscribeParams = serde_json::from_value(params).unwrap();
            assert!(params.pointer.is_none());
        }
    }

    #[test]
    fn subscription_pointer_defaults_to_index() {
        assert_eq!(subscription_pointer(None).unwrap(), INDEX.clone());
    }

    #[test]
    fn subscription_pointer_accepts_index_descendants() {
        for pointer in ["/index", "/index/package", "/index/os/versions"] {
            let pointer: JsonPointer = pointer.parse().unwrap();
            assert_eq!(
                subscription_pointer(Some(pointer.clone())).unwrap(),
                pointer
            );
        }
    }

    #[test]
    fn subscription_pointer_rejects_paths_outside_index() {
        for pointer in ["", "/", "/admins", "/indexing"] {
            let err = subscription_pointer(Some(pointer.parse().unwrap())).unwrap_err();
            assert!(matches!(err.kind, ErrorKind::InvalidRequest));
        }
    }
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct CliApplyParams {
    #[arg(help = "help.arg.db-apply-expr")]
    expr: String,
    #[arg(help = "help.arg.database-path")]
    path: Option<PathBuf>,
}

#[instrument(skip_all)]
async fn cli_apply(
    HandlerArgs {
        context,
        parent_method,
        method,
        params: CliApplyParams { expr, path },
        ..
    }: HandlerArgs<CliContext, CliApplyParams>,
) -> Result<(), RpcError> {
    if let Some(path) = path {
        PatchDb::open(path)
            .await?
            .apply_function(|db| {
                let res = apply_expr(
                    serde_json::to_value(patch_db::Value::from(db))
                        .with_kind(ErrorKind::Deserialization)?
                        .into(),
                    &expr,
                )?;

                Ok::<_, Error>((
                    to_value(
                        &serde_json::from_value::<RegistryDatabase>(res.clone().into()).with_ctx(
                            |_| {
                                (
                                    crate::ErrorKind::Deserialization,
                                    "result does not match database model",
                                )
                            },
                        )?,
                    )?,
                    (),
                ))
            })
            .await
            .result?;
    } else {
        let method = parent_method.into_iter().chain(method).join(".");
        context
            .call_remote::<RegistryContext>(&method, imbl_value::json!({ "expr": expr }))
            .await?;
    }

    Ok(())
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ApplyParams {
    #[arg(help = "help.arg.db-apply-expr")]
    expr: String,
    #[arg(help = "help.arg.database-path")]
    path: Option<PathBuf>,
}

pub async fn apply(
    ctx: RegistryContext,
    ApplyParams { expr, .. }: ApplyParams,
) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            let res = apply_expr(
                serde_json::to_value(patch_db::Value::from(db.clone()))
                    .with_kind(ErrorKind::Deserialization)?
                    .into(),
                &expr,
            )?;

            db.ser(
                &serde_json::from_value::<RegistryDatabase>(res.clone().into()).with_ctx(|_| {
                    (
                        crate::ErrorKind::Deserialization,
                        "result does not match database model",
                    )
                })?,
            )
        })
        .await
        .result
}
