pub mod model;
pub mod prelude;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws;
use clap::Parser;
use imbl_value::InternedString;
use itertools::Itertools;
use patch_db::json_ptr::{JsonPointer, ROOT};
use patch_db::{DiffPatch, Dump, Revision};
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{Context, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{self, UnboundedReceiver};
use tokio::sync::watch;
use tracing::instrument;
use ts_rs::TS;

use crate::context::{CliContext, RpcContext};
use crate::prelude::*;
use crate::rpc_continuations::{Guid, RpcContinuation};
use crate::util::serde::{HandlerExtSerde, apply_expr};

lazy_static::lazy_static! {
    static ref PUBLIC: JsonPointer = "/public".parse().unwrap();
}

pub trait DbAccess<T>: Sized {
    fn access<'a>(db: &'a Model<Self>) -> &'a Model<T>;
}

pub trait DbAccessMut<T>: DbAccess<T> {
    fn access_mut<'a>(db: &'a mut Model<Self>) -> &'a mut Model<T>;
}

pub trait DbAccessByKey<T>: Sized {
    type Key<'a>;
    fn access_by_key<'a>(db: &'a Model<Self>, key: Self::Key<'_>) -> Option<&'a Model<T>>;
}

pub trait DbAccessMutByKey<T>: DbAccessByKey<T> {
    fn access_mut_by_key<'a>(
        db: &'a mut Model<Self>,
        key: Self::Key<'_>,
    ) -> Option<&'a mut Model<T>>;
}

pub fn db<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "dump",
            from_fn_async(cli_dump)
                .with_display_serializable()
                .with_about("about.filter-query-db"),
        )
        .subcommand("dump", from_fn_async(dump).no_cli())
        .subcommand(
            "subscribe",
            from_fn_async(subscribe)
                .with_metadata("get_session", Value::Bool(true))
                .no_cli(),
        )
        .subcommand(
            "put",
            put::<C>().with_about("about.command-add-ui-record-db"),
        )
        .subcommand(
            "apply",
            from_fn_async(cli_apply)
                .no_display()
                .with_about("about.update-db-record"),
        )
        .subcommand("apply", from_fn_async(apply).no_cli())
}

#[derive(Deserialize, Serialize)]
#[serde(untagged)]
pub enum RevisionsRes {
    Revisions(Vec<Arc<Revision>>),
    Dump(Dump),
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct CliDumpParams {
    #[arg(
        long = "include-private",
        short = 'p',
        help = "help.arg.include-private-data"
    )]
    #[serde(default)]
    include_private: bool,
    #[arg(help = "help.arg.db-path")]
    path: Option<PathBuf>,
}

#[instrument(skip_all)]
async fn cli_dump(
    HandlerArgs {
        context,
        parent_method,
        method,
        params: CliDumpParams {
            include_private,
            path,
        },
        ..
    }: HandlerArgs<CliContext, CliDumpParams>,
) -> Result<Dump, RpcError> {
    let dump = if let Some(path) = path {
        PatchDb::open(path).await?.dump(&ROOT).await
    } else {
        let method = parent_method.into_iter().chain(method).join(".");
        from_value::<Dump>(
            context
                .call_remote::<RpcContext>(
                    &method,
                    imbl_value::json!({
                        "pointer": if include_private {
                            AsRef::<str>::as_ref(&ROOT)
                        } else {
                            AsRef::<str>::as_ref(&*PUBLIC)
                        }
                    }),
                )
                .await?,
        )?
    };

    Ok(dump)
}

#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DumpParams {
    #[ts(type = "string | null")]
    pointer: Option<JsonPointer>,
}

pub async fn dump(ctx: RpcContext, DumpParams { pointer }: DumpParams) -> Result<Dump, Error> {
    Ok(ctx.db.dump(pointer.as_ref().unwrap_or(&*PUBLIC)).await)
}

#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeParams {
    #[ts(type = "string | null")]
    pointer: Option<JsonPointer>,
    #[ts(skip)]
    #[serde(rename = "__Auth_session")]
    session: Option<InternedString>,
}

#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeRes {
    #[ts(type = "{ id: number; value: unknown }")]
    pub dump: Dump,
    pub guid: Guid,
}

pub(crate) struct DbSubscriber {
    pub(crate) rev: u64,
    pub(crate) sub: UnboundedReceiver<Revision>,
    pub(crate) sync_db: watch::Receiver<u64>,
}
impl DbSubscriber {
    pub(crate) async fn recv(&mut self) -> Option<Revision> {
        loop {
            tokio::select! {
                rev = self.sub.recv() => {
                    if let Some(rev) = rev.as_ref() {
                        self.rev = rev.id;
                    }
                    return rev
                }
                _ = self.sync_db.changed() => {
                    let id = *self.sync_db.borrow();
                    if id > self.rev {
                        match self.sub.try_recv() {
                            Ok(rev) => {
                                self.rev = rev.id;
                                return Some(rev)
                            }
                            Err(mpsc::error::TryRecvError::Disconnected) => {
                                return None
                            }
                            Err(mpsc::error::TryRecvError::Empty) => {
                                return Some(Revision { id, patch: DiffPatch::default() })
                            }
                        }
                    }
                }
            }
        }
    }
}

pub async fn subscribe(
    ctx: RpcContext,
    SubscribeParams { pointer, session }: SubscribeParams,
) -> Result<SubscribeRes, Error> {
    let (dump, sub) = ctx
        .db
        .dump_and_sub(pointer.unwrap_or_else(|| PUBLIC.clone()))
        .await;
    let mut sub = DbSubscriber {
        rev: dump.id,
        sub,
        sync_db: ctx.sync_db.subscribe(),
    };
    let guid = Guid::new();
    ctx.rpc_continuations
        .add(
            guid.clone(),
            RpcContinuation::ws_authed(
                &ctx,
                session,
                |mut ws| async move {
                    if let Err(e) = async {
                        loop {
                            tokio::select! {
                                rev = sub.recv() => {
                                    if let Some(rev) = rev {
                                        ws.send(ws::Message::Text(
                                            serde_json::to_string(&rev)
                                                .with_kind(ErrorKind::Serialization)?
                                                .into(),
                                        ))
                                        .await
                                        .with_kind(ErrorKind::Network)?;
                                    } else {
                                        return ws.normal_close("complete").await;
                                    }
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
                            tracing::error!("Error in db websocket: {e}");
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

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct CliApplyParams {
    #[arg(long, help = "help.arg.allow-model-mismatch")]
    allow_model_mismatch: bool,
    #[arg(help = "help.arg.db-apply-expr")]
    expr: String,
    #[arg(help = "help.arg.db-path")]
    path: Option<PathBuf>,
}

#[instrument(skip_all)]
async fn cli_apply(
    HandlerArgs {
        context,
        parent_method,
        method,
        params:
            CliApplyParams {
                allow_model_mismatch,
                expr,
                path,
            },
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

                let value = if allow_model_mismatch {
                    serde_json::from_value::<Value>(res.clone().into()).with_ctx(|_| {
                        (
                            crate::ErrorKind::Deserialization,
                            "result does not match database model",
                        )
                    })?
                } else {
                    to_value(
                        &serde_json::from_value::<model::Database>(res.clone().into()).with_ctx(
                            |_| {
                                (
                                    crate::ErrorKind::Deserialization,
                                    "result does not match database model",
                                )
                            },
                        )?,
                    )?
                };
                Ok::<_, Error>((value, ()))
            })
            .await
            .result?;
    } else {
        let method = parent_method.into_iter().chain(method).join(".");
        context
            .call_remote::<RpcContext>(&method, imbl_value::json!({ "expr": expr }))
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
}

pub async fn apply(ctx: RpcContext, ApplyParams { expr }: ApplyParams) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            let res = apply_expr(
                serde_json::to_value(patch_db::Value::from(db.clone()))
                    .with_kind(ErrorKind::Deserialization)?
                    .into(),
                &expr,
            )?;

            db.ser(
                &serde_json::from_value::<model::Database>(res.clone().into()).with_ctx(|_| {
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

pub fn put<C: Context>() -> ParentHandler<C> {
    ParentHandler::new().subcommand(
        "ui",
        from_fn_async(ui)
            .with_display_serializable()
            .with_about("about.add-path-value-db")
            .with_call_remote::<CliContext>(),
    )
}
#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct UiParams {
    #[arg(help = "help.arg.json-pointer")]
    #[ts(type = "string")]
    pointer: JsonPointer,
    #[arg(help = "help.arg.json-value")]
    #[ts(type = "any")]
    value: Value,
}

// #[command(display(display_serializable))]
#[instrument(skip_all)]
pub async fn ui(ctx: RpcContext, UiParams { pointer, value, .. }: UiParams) -> Result<(), Error> {
    let ptr = "/public/ui"
        .parse::<JsonPointer>()
        .with_kind(ErrorKind::Database)?
        + &pointer;
    ctx.db.put(&ptr, &value).await?;
    Ok(())
}
