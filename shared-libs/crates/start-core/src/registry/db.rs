use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{self, Utf8Bytes};
use clap::Parser;
use exver::Version;
use itertools::Itertools;
use patch_db::json_ptr::{JsonPointer, ROOT};
use patch_db::{Dump, JsonSubscriber};
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{Context, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::context::CliContext;
pub use crate::db::{
    ApplyWithPathParams as ApplyParams, DumpParams, SubscribeRes,
    SubscribeResult as SubscribeIndexResult,
};
use crate::prelude::*;
use crate::registry::RegistryDatabase;
use crate::registry::context::RegistryContext;
use crate::registry::device_info::WireCompatibility;
use crate::registry::mirror::{IndexMirror, index_mirror, mirror_key};
use crate::registry::package::index::ReaderParams;
use crate::rpc_continuations::{Guid, RpcContinuation, WebSocketPolicy};
use crate::util::serde::{HandlerExtSerde, apply_expr};

lazy_static::lazy_static! {
    static ref INDEX: JsonPointer = "/index".parse().unwrap();
}

pub fn db_api<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "dump",
            from_fn_async(cli_dump)
                .no_ts()
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
                .with_metadata("get_signer", Value::Bool(true))
                .with_metadata(crate::mcp::MCP, Value::Bool(false))
                .no_cli(),
        )
        .subcommand(
            "apply",
            from_fn_async(cli_apply)
                .no_ts()
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

pub async fn dump(ctx: RegistryContext, DumpParams { pointer }: DumpParams) -> Result<Dump, Error> {
    Ok(ctx
        .db
        .dump(&pointer.as_ref().map_or(ROOT, |p| p.borrowed()))
        .await)
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

async fn subscription(
    db: &PatchDb,
    pointer: Option<JsonPointer>,
) -> Result<(Dump, JsonSubscriber), Error> {
    let (dump, sub) = db
        .dump_and_sub_json(
            subscription_pointer(pointer)?,
            NonZeroUsize::new(32).unwrap(),
        )
        .await?;
    Ok((
        serde_json::from_slice(&dump).with_kind(ErrorKind::Deserialization)?,
        sub,
    ))
}

#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeIndexParams {
    pub pointer: Option<JsonPointer>,
    /// Client StartOS version. Omission preserves the canonical index format.
    pub os_version: Option<Version>,
    #[serde(flatten)]
    pub reader: ReaderParams,
}

rpc_toolkit::reflect_ts!(SubscribeIndexParams);

async fn visible_subscription(
    ctx: &RegistryContext,
    SubscribeIndexParams {
        pointer,
        os_version,
        reader,
    }: SubscribeIndexParams,
) -> Result<(Arc<IndexMirror>, Dump, JsonSubscriber), Error> {
    let pointer = subscription_pointer(pointer)?;
    let key = mirror_key(&ctx.db.peek().await, reader.signer)?;
    let wire = os_version
        .as_ref()
        .map(WireCompatibility::for_startos)
        .transpose()?
        .unwrap_or_default();
    let mirror = index_mirror(&ctx.index_mirrors, &ctx.db, key, wire, &ctx.datadir).await?;
    let (dump, sub) = subscription(&mirror.db, Some(pointer)).await?;
    Ok((mirror, dump, sub))
}

pub async fn subscribe(
    HandlerArgs {
        context,
        params,
        rpc_stream,
        ..
    }: HandlerArgs<
        RegistryContext,
        SubscribeIndexParams,
        rpc_toolkit::Empty,
        patch_db::Revision,
    >,
) -> Result<SubscribeIndexResult, Error> {
    let (mirror, dump, sub) = visible_subscription(&context, params).await?;
    if let Some(stream) = rpc_stream {
        stream_subscription(stream, mirror, sub);
        Ok(SubscribeIndexResult::Stream(dump))
    } else {
        let guid = legacy_subscription(&context, mirror, sub).await;
        Ok(SubscribeIndexResult::Continuation(SubscribeRes {
            dump,
            guid,
        }))
    }
}

fn stream_subscription(
    stream: rpc_toolkit::RpcStream<patch_db::Revision>,
    mirror: Arc<IndexMirror>,
    mut sub: JsonSubscriber,
) {
    tokio::spawn(async move {
        let result = async {
            loop {
                let revision = tokio::select! {
                    _ = stream.closed() => return Ok::<_, Error>(()),
                    result = mirror.closed() => return result,
                    revision = sub.recv() => revision?,
                };
                let Some(revision) = revision else {
                    return Ok(());
                };
                stream
                    .send(
                        serde_json::from_slice::<patch_db::Revision>(&revision)
                            .with_kind(ErrorKind::Deserialization)?,
                    )
                    .await?;
            }
        }
        .await;
        let _ = stream.close(result);
    });
}

async fn legacy_subscription(
    ctx: &RegistryContext,
    mirror: Arc<IndexMirror>,
    mut sub: JsonSubscriber,
) -> Guid {
    let guid = Guid::new();
    ctx.rpc_continuations
        .add(
            guid.clone(),
            RpcContinuation::ws(
                WebSocketPolicy::Public,
                ctx.secure_session().cloned(),
                |mut ws| async move {
                    if let Err(e) = async {
                        loop {
                            tokio::select! {
                                result = mirror.closed() => {
                                    result?;
                                    return ws.normal_close("complete").await;
                                }
                                rev = sub.recv() => {
                                    let Some(rev) = rev? else {
                                        return ws.normal_close("complete").await;
                                    };
                                    ws.send(ws::Message::Text(
                                        Utf8Bytes::try_from(rev)
                                            .with_kind(ErrorKind::Serialization)?,
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

    guid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscription_preserves_dump_and_revision_wire_format() {
        let db = PatchDb::in_memory();
        db.put(&ROOT, &imbl_value::json!({"index": {"name": "registry"}}))
            .await
            .unwrap();
        crate::sign::instance::load(&db).await.unwrap();
        let (dump, mut sub) = subscription(&db, None).await.unwrap();
        assert_eq!(dump.value, imbl_value::json!({"name": "registry"}));
        db.put(&"/index/name".parse::<JsonPointer>().unwrap(), &"renamed")
            .await
            .unwrap();
        let revision: patch_db::Revision =
            serde_json::from_slice(&sub.recv().await.unwrap().unwrap()).unwrap();
        assert_eq!(revision.id, dump.id + 1);
        let mut value = dump.value;
        patch_db::json_patch::patch(&mut value, &revision.patch).unwrap();
        assert_eq!(value, imbl_value::json!({"name": "renamed"}));
    }

    #[tokio::test]
    async fn subscription_closes_after_32_queued_revisions_and_reconnects() {
        let db = PatchDb::in_memory();
        db.put(&ROOT, &imbl_value::json!({"index": {"name": "registry"}}))
            .await
            .unwrap();
        let (_, mut sub) = subscription(&db, None).await.unwrap();
        for i in 0..32 {
            db.put(&"/index/name".parse::<JsonPointer>().unwrap(), &i)
                .await
                .unwrap();
        }
        assert!(sub.recv().await.unwrap().is_some());
        for i in 32..66 {
            db.put(&"/index/name".parse::<JsonPointer>().unwrap(), &i)
                .await
                .unwrap();
        }
        assert!(sub.recv().await.unwrap().is_none());
        let (dump, _) = subscription(&db, None).await.unwrap();
        assert_eq!(dump.value, imbl_value::json!({"name": 65}));
    }

    #[test]
    fn subscribe_params_allow_an_omitted_or_null_pointer() {
        for params in [
            serde_json::json!({}),
            serde_json::json!({ "pointer": null }),
        ] {
            let params: SubscribeIndexParams = serde_json::from_value(params).unwrap();
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
        for pointer in ["", "/", "/admins", "/serverIdentityKey", "/indexing"] {
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

pub async fn apply(
    ctx: RegistryContext,
    ApplyParams {
        expression: crate::db::ApplyParams { expr },
        ..
    }: ApplyParams,
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
