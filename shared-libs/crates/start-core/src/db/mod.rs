pub mod model;
pub mod prelude;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws;
use clap::Parser;
use imbl_value::{InternedString, Value};
use itertools::Itertools;
use patch_db::json_ptr::{JsonPointer, ROOT};
use patch_db::{
    Catchup, DiffPatch, Dump, PatchDb, ProjectedSubscriber, Projection, Revision, Selector,
    Subscriber, Update,
};
use rpc_toolkit::yajrc::RpcError;
use rpc_toolkit::{Context, HandlerArgs, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{self, UnboundedReceiver};
use tokio::sync::watch;
use tracing::instrument;

use crate::context::{CliContext, RpcContext};
use crate::prelude::*;
use crate::rpc_continuations::{Guid, RpcContinuation, WebSocketPolicy};
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
                .no_ts()
                .with_display_serializable()
                .with_about("about.filter-query-db"),
        )
        .subcommand("dump", from_fn_async(dump).no_cli())
        .subcommand(
            "subscribe",
            from_fn_async(subscribe)
                .with_metadata("get_signer", Value::Bool(true))
                .with_metadata(crate::mcp::MCP, Value::Bool(false))
                .no_cli(),
        )
        .subcommand(
            "subscription",
            ParentHandler::<C>::new()
                .subcommand(
                    "update",
                    from_fn_async(update_subscription)
                        .with_metadata(crate::mcp::MCP, Value::Bool(false))
                        .no_cli(),
                )
                .no_cli(),
        )
        .subcommand(
            "put",
            put::<C>().with_about("about.command-add-ui-record-db"),
        )
        .subcommand(
            "apply",
            from_fn_async(cli_apply)
                .no_ts()
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

/// Narrows a dumped or subscribed subtree; omitted fields narrow nothing.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ProjectionParams {
    /// Levels kept below the pointer; deeper objects and arrays become empty.
    #[arg(long, help = "help.arg.db-depth")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<usize>,
    /// Subtrees kept, as pointers relative to the subtree where `*` matches any one key or index.
    /// Empty keeps everything. Segments that are array positions (digits or `-`) are rejected.
    #[arg(long, help = "help.arg.db-include")]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<JsonPointer>,
    /// Subtrees removed from what `include` keeps, in the same form.
    #[arg(long, help = "help.arg.db-exclude")]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<JsonPointer>,
    /// More subtrees, each with its own exclusions and depth, kept alongside the above.
    /// An explicit root inclusion in the simple form keeps the entire subtree.
    #[arg(skip)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selectors: Vec<SelectorParams>,
}

rpc_toolkit::reflect_ts!(ProjectionParams);

/// One subtree kept, with patterns and depth relative to the dumped or subscribed subtree.
#[derive(Clone, Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SelectorParams {
    pub include: JsonPointer,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<JsonPointer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<usize>,
}

rpc_toolkit::reflect_ts!(SelectorParams);

impl TryFrom<ProjectionParams> for Projection {
    type Error = Error;
    fn try_from(
        ProjectionParams {
            depth,
            include,
            exclude,
            selectors,
        }: ProjectionParams,
    ) -> Result<Self, Error> {
        let projection = || -> Result<Projection, patch_db::Error> {
            let explicit = depth.is_some() || !include.is_empty() || !exclude.is_empty();
            let simple = Projection::new(depth, include, exclude)?;
            if selectors.is_empty() {
                return Ok(simple);
            }
            let mut union = if explicit && simple.is_identity() {
                vec![Selector::new(ROOT.to_owned(), Vec::new(), None)?]
            } else {
                simple.into_selectors()
            };
            for SelectorParams {
                include,
                exclude,
                depth,
            } in selectors
            {
                union.push(Selector::new(include, exclude, depth)?);
            }
            Projection::union(union)
        };
        projection().map_err(|e| match e {
            patch_db::Error::PositionalPattern(pattern) => Error::new(
                eyre!(
                    "{}",
                    t!(
                        "db.projection-array-position",
                        pattern = pattern.to_string()
                    )
                ),
                ErrorKind::InvalidRequest,
            ),
            e => e.into(),
        })
    }
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
    #[command(flatten)]
    #[serde(flatten)]
    projection: ProjectionParams,
    #[arg(help = "help.arg.db-path")]
    path: Option<PathBuf>,
}

#[instrument(skip_all)]
async fn cli_dump(
    HandlerArgs {
        context,
        parent_method,
        method,
        params:
            CliDumpParams {
                include_private,
                projection,
                path,
            },
        ..
    }: HandlerArgs<CliContext, CliDumpParams>,
) -> Result<Dump, RpcError> {
    let dump = if let Some(path) = path {
        PatchDb::open(path)
            .await?
            .dump_projected(&ROOT, &projection.try_into()?)
            .await
    } else {
        let method = parent_method.into_iter().chain(method).join(".");
        let pointer = if include_private {
            ROOT.to_owned()
        } else {
            PUBLIC.clone()
        };
        from_value::<Dump>(
            context
                .call_remote::<RpcContext>(
                    &method,
                    to_value(&DumpParams {
                        pointer: Some(pointer),
                        projection,
                    })?,
                )
                .await?,
        )?
    };

    Ok(dump)
}

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct DumpParams {
    pub pointer: Option<JsonPointer>,
    #[serde(flatten)]
    pub projection: ProjectionParams,
}

rpc_toolkit::reflect_ts!(DumpParams);

pub async fn dump(
    ctx: RpcContext,
    DumpParams {
        pointer,
        projection,
    }: DumpParams,
) -> Result<Dump, Error> {
    Ok(ctx
        .db
        .dump_projected(
            pointer.as_ref().unwrap_or(&*PUBLIC),
            &projection.try_into()?,
        )
        .await)
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct DbCursor {
    pub epoch: InternedString,
    pub id: u64,
}

rpc_toolkit::reflect_ts!(DbCursor);

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeParams {
    pub pointer: Option<JsonPointer>,
    /// The subscriber's last revision; resumes only within the epoch that reported it.
    pub since: Option<DbCursor>,
    #[serde(flatten)]
    pub projection: ProjectionParams,
    #[visit(ts(skip))]
    #[serde(rename = "__Auth_signer")]
    pub signer: Option<InternedString>,
}

rpc_toolkit::reflect_ts!(SubscribeParams);

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeRes {
    pub dump: Dump,
    pub guid: Guid,
}

rpc_toolkit::reflect_ts!(SubscribeRes);

/// A dump when `revisions` is null; otherwise the revisions since the cursor, current as of `id`.
#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeStart {
    pub epoch: InternedString,
    pub id: u64,
    pub value: Option<Value>,
    pub revisions: Option<Vec<Revision>>,
    /// Names the stream to `db.subscription.update`; absent where its projection is fixed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscription: Option<Guid>,
}

rpc_toolkit::reflect_ts!(SubscribeStart);

pub(crate) async fn subscribe_start(
    db: &PatchDb,
    pointer: JsonPointer,
    projection: Projection,
    since: Option<&DbCursor>,
) -> (SubscribeStart, Subscriber) {
    let (epoch, catchup, sub) = db
        .catch_up_and_sub_projected(
            pointer,
            projection,
            since.map(|since| (&*since.epoch, since.id)),
        )
        .await;
    (subscribe_start_from(epoch, catchup), sub)
}

/// Opted-in rpcstream requests receive the dump directly; other callers receive a GUID.
#[derive(Deserialize, Serialize, VisitVariants)]
#[serde(untagged)]
pub enum SubscribeResult {
    Continuation(SubscribeRes),
    Stream(Dump),
}

rpc_toolkit::reflect_ts!(SubscribeResult);

/// Opted-in rpcstream requests receive the stream start directly; other callers receive a GUID.
#[derive(Deserialize, Serialize, VisitVariants)]
#[serde(untagged)]
pub enum SubscribeStartResult {
    Continuation(SubscribeRes),
    Stream(SubscribeStart),
}

rpc_toolkit::reflect_ts!(SubscribeStartResult);

/// A stream item that carries a revision id, which a sync fence can stand in for.
trait Sequenced: Sized {
    fn id(&self) -> u64;
    fn fence(id: u64) -> Self;
}

impl Sequenced for Revision {
    fn id(&self) -> u64 {
        self.id
    }
    fn fence(id: u64) -> Self {
        Revision {
            id,
            patch: DiffPatch::default(),
        }
    }
}

impl Sequenced for Update {
    fn id(&self) -> u64 {
        match self {
            Update::Revision(revision) | Update::Extend(revision) => revision.id,
        }
    }
    fn fence(id: u64) -> Self {
        Update::Revision(Revision::fence(id))
    }
}

struct DbSubscriber<T> {
    rev: u64,
    sub: UnboundedReceiver<T>,
    sync_db: watch::Receiver<u64>,
}
impl<T: Sequenced> DbSubscriber<T> {
    async fn recv(&mut self) -> Option<T> {
        loop {
            tokio::select! {
                item = self.sub.recv() => {
                    if let Some(item) = item.as_ref() {
                        self.rev = self.rev.max(item.id());
                    }
                    return item
                }
                changed = self.sync_db.changed() => {
                    if changed.is_err() {
                        return self.sub.recv().await;
                    }
                    let id = *self.sync_db.borrow();
                    if id > self.rev {
                        match self.sub.try_recv() {
                            Ok(item) => {
                                self.rev = self.rev.max(item.id());
                                return Some(item)
                            }
                            Err(mpsc::error::TryRecvError::Disconnected) => {
                                return None
                            }
                            Err(mpsc::error::TryRecvError::Empty) => {
                                return Some(T::fence(id))
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Unregisters a stream from `db.subscription.update` when the stream ends.
struct RegisteredSubscription {
    ctx: RpcContext,
    guid: Guid,
}
impl Drop for RegisteredSubscription {
    fn drop(&mut self) {
        self.ctx.db_subscriptions.mutate(|subscriptions| {
            subscriptions.remove(&self.guid);
        });
    }
}

/// A projected revision; with `extend`, what the latest `db.subscription.update` added,
/// current as of `id`, sent once per update before any later revision.
#[derive(Debug, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionUpdate {
    pub id: u64,
    pub patch: DiffPatch,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub extend: bool,
}

rpc_toolkit::reflect_ts!(SubscriptionUpdate);

impl From<Update> for SubscriptionUpdate {
    fn from(update: Update) -> Self {
        match update {
            Update::Revision(Revision { id, patch }) => Self {
                id,
                patch,
                extend: false,
            },
            Update::Extend(Revision { id, patch }) => Self {
                id,
                patch,
                extend: true,
            },
        }
    }
}

fn subscribe_start_from(epoch: InternedString, catchup: Catchup) -> SubscribeStart {
    match catchup {
        Catchup::Dump(Dump { id, value }) => SubscribeStart {
            epoch,
            id,
            value: Some(value),
            revisions: None,
            subscription: None,
        },
        Catchup::Replay { id, revisions } => SubscribeStart {
            epoch,
            id,
            value: None,
            revisions: Some(revisions),
            subscription: None,
        },
    }
}

pub async fn subscribe(
    HandlerArgs {
        context: ctx,
        params:
            SubscribeParams {
                pointer,
                since,
                projection,
                signer,
            },
        rpc_stream,
        ..
    }: HandlerArgs<RpcContext, SubscribeParams, rpc_toolkit::Empty, SubscriptionUpdate>,
) -> Result<SubscribeStartResult, Error> {
    let pointer = pointer.unwrap_or_else(|| PUBLIC.clone());
    if let Some(stream) = rpc_stream {
        let (epoch, catchup, ProjectedSubscriber { id, receiver }) = ctx
            .db
            .catch_up_and_sub_mutable(
                pointer,
                projection.try_into()?,
                since.as_ref().map(|since| (&*since.epoch, since.id)),
            )
            .await;
        let mut start = subscribe_start_from(epoch, catchup);
        let guid = Guid::new();
        ctx.db_subscriptions.mutate(|subscriptions| {
            subscriptions.insert(guid.clone(), id);
        });
        start.subscription = Some(guid.clone());
        let sub = DbSubscriber {
            rev: start.id,
            sub: receiver,
            sync_db: ctx.sync_db.subscribe(),
        };
        let registered = RegisteredSubscription {
            ctx: ctx.clone(),
            guid,
        };
        crate::auth::continuation_subscription(&ctx, signer)
            .await?
            .forward_stream(
                stream,
                futures::stream::unfold((sub, registered), |(mut sub, registered)| async move {
                    sub.recv()
                        .await
                        .map(|update| (Ok(SubscriptionUpdate::from(update)), (sub, registered)))
                }),
            );
        return Ok(SubscribeStartResult::Stream(start));
    }
    let (dump, sub) = ctx
        .db
        .dump_and_sub_projected(pointer, projection.try_into()?)
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
                crate::auth::continuation_subscription(&ctx, signer).await?,
                WebSocketPolicy::Authenticated,
                ctx.secure_session().cloned(),
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

    Ok(SubscribeStartResult::Continuation(SubscribeRes {
        dump,
        guid,
    }))
}

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSubscriptionParams {
    /// The `subscription` of the stream's start.
    pub subscription: Guid,
    /// The stream's new projection, replacing the previous one.
    #[serde(flatten)]
    pub projection: ProjectionParams,
}

rpc_toolkit::reflect_ts!(UpdateSubscriptionParams);

/// Changes a `db.subscribe` stream's projection; the stream receives what it adds first.
pub async fn update_subscription(
    ctx: RpcContext,
    UpdateSubscriptionParams {
        subscription,
        projection,
    }: UpdateSubscriptionParams,
) -> Result<(), Error> {
    let projection = projection.try_into()?;
    let id = ctx
        .db_subscriptions
        .peek(|subscriptions| subscriptions.get(&subscription).copied());
    if let Some(id) = id {
        if ctx.db.reproject(id, projection).await {
            return Ok(());
        }
    }
    ctx.db_subscriptions.mutate(|subscriptions| {
        subscriptions.remove(&subscription);
    });
    Err(Error::new(
        eyre!(
            "{}",
            t!(
                "db.subscription-not-found",
                subscription = subscription.as_ref()
            )
        ),
        ErrorKind::NotFound,
    ))
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

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ApplyParams {
    #[arg(help = "help.arg.db-apply-expr")]
    pub expr: String,
}

rpc_toolkit::reflect_ts!(ApplyParams);

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct ApplyWithPathParams {
    #[serde(flatten)]
    pub expression: ApplyParams,
    pub path: Option<PathBuf>,
}

rpc_toolkit::reflect_ts!(ApplyWithPathParams);

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
#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct UiParams {
    #[arg(help = "help.arg.json-pointer")]
    pointer: JsonPointer,
    #[arg(help = "help.arg.json-value")]
    value: Value,
}

rpc_toolkit::reflect_ts!(UiParams);

#[cfg(test)]
mod subscription_tests;

#[instrument(skip_all)]
pub async fn ui(ctx: RpcContext, UiParams { pointer, value, .. }: UiParams) -> Result<(), Error> {
    let mut ptr = "/public/ui"
        .parse::<JsonPointer>()
        .with_kind(ErrorKind::Database)?;
    ptr.append(&pointer);
    ctx.db.put(&ptr, &value).await?;
    Ok(())
}
