use std::collections::VecDeque;
use std::ffi::OsString;

use clap::{CommandFactory, FromArgMatches};
use futures::Future;
use imbl_value::imbl::OrdMap;
use imbl_value::Value;
use reqwest::header::{ACCEPT, CONTENT_LENGTH, CONTENT_TYPE};
use reqwest::{Client, Method};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use url::Url;
use yajrc::{Id, RpcError};

#[cfg(feature = "ts")]
use crate::ts::{PassthroughChildrenTS, PassthroughParamsTS, PassthroughReturnTS};
use crate::util::{internal_error, invalid_params, parse_error, without, Flat, PhantomData};
use crate::{
    Adapter, AnyHandler, CliBindings, CliBindingsAny, Empty, HandleAny, HandleAnyArgs, HandlerArgs,
    HandlerArgsFor, HandlerFor, HandlerTypes, Name, ParentHandler, PrintCliResult,
};

type GenericRpcMethod<'a> = yajrc::GenericRpcMethod<&'a str, Value, Value>;
type RpcRequest<'a> = yajrc::RpcRequest<GenericRpcMethod<'a>>;
type RpcResponse<'a> = yajrc::RpcResponse<GenericRpcMethod<'static>>;

pub struct CliApp<Context: crate::Context + Clone, Config: CommandFactory + FromArgMatches> {
    _phantom: PhantomData<(Context, Config)>,
    make_ctx: Box<dyn FnOnce(Config) -> Result<Context, RpcError> + Send + Sync>,
    root_handler: ParentHandler<Context>,
    mut_cmd: Option<Box<dyn FnOnce(clap::Command) -> clap::Command + Send + Sync>>,
}
impl<Context: crate::Context + Clone, Config: CommandFactory + FromArgMatches>
    CliApp<Context, Config>
{
    pub fn new<MakeCtx: FnOnce(Config) -> Result<Context, RpcError> + Send + Sync + 'static>(
        make_ctx: MakeCtx,
        root_handler: ParentHandler<Context>,
    ) -> Self {
        Self {
            _phantom: PhantomData::new(),
            make_ctx: Box::new(make_ctx),
            root_handler,
            mut_cmd: None,
        }
    }
    pub fn mutate_command(
        mut self,
        f: impl FnOnce(clap::Command) -> clap::Command + Send + Sync + 'static,
    ) -> Self {
        self.mut_cmd = if let Some(prev) = self.mut_cmd.take() {
            Some(Box::new(|cmd| f(prev(cmd))))
        } else {
            Some(Box::new(f))
        };
        self
    }
    fn command(&mut self) -> clap::Command {
        let mut cmd = Config::command();
        for (name, handler) in &self.root_handler.subcommands.1 {
            if let (Name(name), Some(cli)) = (name, handler.cli()) {
                cmd = cmd.subcommand(cli.cli_command().name(name));
            }
        }
        if let Some(f) = self.mut_cmd.take() {
            cmd = f(cmd);
        }
        cmd
    }
    pub fn into_command(mut self) -> clap::Command {
        self.command()
    }
    pub fn run(mut self, args: impl IntoIterator<Item = OsString>) -> Result<(), RpcError> {
        let cmd = self.command();
        let matches = cmd.get_matches_from(args);
        let config = Config::from_arg_matches(&matches)?;
        let ctx = (self.make_ctx)(config)?;
        let root_handler = AnyHandler::new(self.root_handler);
        let (method, params) = root_handler.cli_parse(&matches)?;
        let res = root_handler.handle_sync(HandleAnyArgs {
            context: ctx.clone(),
            parent_method: VecDeque::new(),
            method: method.clone(),
            params: params.clone(),
            inherited: crate::Empty {},
        })?;
        root_handler.cli_display(
            HandleAnyArgs {
                context: ctx,
                parent_method: VecDeque::new(),
                method,
                params,
                inherited: crate::Empty {},
            },
            res,
        )?;
        Ok(())
    }
}

pub trait CallRemote<RemoteContext, Extra = Empty>: crate::Context {
    fn call_remote(
        &self,
        method: &str,
        metadata: OrdMap<&'static str, Value>,
        params: Value,
        extra: Extra,
    ) -> impl Future<Output = Result<Value, RpcError>> + Send;
}

pub async fn call_remote_http(
    client: &Client,
    url: Url,
    method: &str,
    params: Value,
) -> Result<Value, RpcError> {
    let rpc_req = RpcRequest {
        id: Some(Id::Number(0.into())),
        method: GenericRpcMethod::new(method),
        params,
    };
    let mut req = client.request(Method::POST, url);
    let body;
    #[cfg(feature = "cbor")]
    {
        req = req.header(CONTENT_TYPE, "application/cbor");
        req = req.header(ACCEPT, "application/cbor, application/json");
        body = serde_cbor::to_vec(&rpc_req)?;
    }
    #[cfg(not(feature = "cbor"))]
    {
        req = req.header(CONTENT_TYPE, "application/json");
        req = req.header(ACCEPT, "application/json");
        body = serde_json::to_vec(&rpc_req)?;
    }
    let res = req
        .header(CONTENT_LENGTH, body.len())
        .body(body)
        .send()
        .await?;

    match res
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    {
        Some("application/json") => {
            serde_json::from_slice::<RpcResponse>(&*res.bytes().await.map_err(internal_error)?)
                .map_err(parse_error)?
                .result
        }
        #[cfg(feature = "cbor")]
        Some("application/cbor") => {
            serde_cbor::from_slice::<RpcResponse>(&*res.bytes().await.map_err(internal_error)?)
                .map_err(parse_error)?
                .result
        }
        _ => Err(internal_error("missing content type")),
    }
}

pub async fn call_remote_socket(
    connection: impl AsyncRead + AsyncWrite,
    method: &str,
    params: Value,
) -> Result<Value, RpcError> {
    let rpc_req = RpcRequest {
        id: Some(Id::Number(0.into())),
        method: GenericRpcMethod::new(method),
        params,
    };
    let conn = connection;
    tokio::pin!(conn);
    let mut buf = serde_json::to_vec(&rpc_req).map_err(|e| RpcError {
        data: Some(e.to_string().into()),
        ..yajrc::INTERNAL_ERROR
    })?;
    buf.push(b'\n');
    conn.write_all(&buf).await.map_err(|e| RpcError {
        data: Some(e.to_string().into()),
        ..yajrc::INTERNAL_ERROR
    })?;
    let mut line = String::new();
    BufReader::new(conn).read_line(&mut line).await?;
    serde_json::from_str::<RpcResponse>(&line)
        .map_err(parse_error)?
        .result
}

pub struct CallRemoteHandler<Context, RemoteContext, RemoteHandler, Extra = Empty> {
    _phantom: PhantomData<(Context, RemoteContext, Extra)>,
    handler: RemoteHandler,
}
impl<Context, RemoteContext, RemoteHandler, Extra>
    CallRemoteHandler<Context, RemoteContext, RemoteHandler, Extra>
{
    pub fn new(handler: RemoteHandler) -> Self {
        Self {
            _phantom: PhantomData::new(),
            handler: handler,
        }
    }
}
impl<Context, RemoteContext, RemoteHandler: Clone, Extra> Clone
    for CallRemoteHandler<Context, RemoteContext, RemoteHandler, Extra>
{
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData::new(),
            handler: self.handler.clone(),
        }
    }
}
impl<Context, RemoteHandler, Extra> std::fmt::Debug
    for CallRemoteHandler<Context, RemoteHandler, Extra>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CallRemoteHandler").finish()
    }
}

impl<Context, RemoteContext, RemoteHandler, Extra> HandlerTypes
    for CallRemoteHandler<Context, RemoteContext, RemoteHandler, Extra>
where
    RemoteHandler: HandlerTypes,
    RemoteHandler::Params: Serialize,
    RemoteHandler::Ok: DeserializeOwned,
    RemoteHandler::Err: From<RpcError>,
    Extra: Send + Sync + 'static,
{
    type Params = Flat<RemoteHandler::Params, Extra>;
    type InheritedParams = RemoteHandler::InheritedParams;
    type Ok = RemoteHandler::Ok;
    type Err = RemoteHandler::Err;
}

impl<Context, RemoteContext, RemoteHandler, Extra> Adapter
    for CallRemoteHandler<Context, RemoteContext, RemoteHandler, Extra>
{
    type Inner = RemoteHandler;
    fn as_inner(&self) -> &Self::Inner {
        &self.handler
    }
}
#[cfg(feature = "ts")]
impl<C, R, H, E> PassthroughParamsTS for CallRemoteHandler<C, R, H, E> {}
#[cfg(feature = "ts")]
impl<C, R, H, E> PassthroughReturnTS for CallRemoteHandler<C, R, H, E> {}
#[cfg(feature = "ts")]
impl<C, R, H, E> PassthroughChildrenTS for CallRemoteHandler<C, R, H, E> {}

impl<Context, RemoteContext, RemoteHandler, Extra> HandlerFor<Context>
    for CallRemoteHandler<Context, RemoteContext, RemoteHandler, Extra>
where
    Context: CallRemote<RemoteContext, Extra>,
    RemoteContext: crate::Context,
    RemoteHandler: HandlerFor<RemoteContext>,
    RemoteHandler::Params: Serialize,
    RemoteHandler::Ok: DeserializeOwned,
    RemoteHandler::Err: From<RpcError>,
    Extra: Serialize + Send + Sync + 'static,
{
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let full_method = handle_args
            .parent_method
            .into_iter()
            .chain(handle_args.method.clone())
            .collect::<Vec<_>>();
        match handle_args
            .context
            .call_remote(
                &full_method.join("."),
                self.handler.metadata(handle_args.method),
                without(handle_args.raw_params.clone(), &handle_args.params.1)
                    .map_err(invalid_params)?,
                handle_args.params.1,
            )
            .await
        {
            Ok(a) => imbl_value::from_value(a)
                .map_err(internal_error)
                .map_err(Self::Err::from),
            Err(e) => Err(Self::Err::from(e)),
        }
    }
    fn metadata(&self, method: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.handler.metadata(method)
    }
    fn method_from_dots(&self, method: &str) -> Option<VecDeque<&'static str>> {
        self.handler.method_from_dots(method)
    }
}
impl<Context, RemoteContext, RemoteHandler, Extra> PrintCliResult<Context>
    for CallRemoteHandler<Context, RemoteContext, RemoteHandler, Extra>
where
    Context: CallRemote<RemoteContext>,
    RemoteHandler: PrintCliResult<Context>,
    RemoteHandler::Params: Serialize,
    RemoteHandler::Ok: DeserializeOwned,
    RemoteHandler::Err: From<RpcError>,
    Extra: Send + Sync + 'static,
{
    fn print(
        &self,
        HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        self.handler.print(
            HandlerArgs {
                context,
                parent_method,
                method,
                params: params.0,
                inherited_params,
                raw_params,
            },
            result,
        )
    }
}
impl<Context, RemoteContext, RemoteHandler, Extra> CliBindings<Context>
    for CallRemoteHandler<Context, RemoteContext, RemoteHandler, Extra>
where
    Context: crate::Context,
    RemoteHandler: CliBindings<Context>,
    RemoteHandler::Params: Serialize,
    RemoteHandler::Ok: DeserializeOwned,
    RemoteHandler::Err: From<RpcError>,
    Extra: Send + Sync + 'static,
{
    fn cli_command(&self) -> clap::Command {
        self.handler.cli_command()
    }
    fn cli_parse(
        &self,
        matches: &clap::ArgMatches,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        self.handler.cli_parse(matches)
    }
    fn cli_display(
        &self,
        HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        self.handler.cli_display(
            HandlerArgs {
                context,
                parent_method,
                method,
                params: params.0,
                inherited_params,
                raw_params,
            },
            result,
        )
    }
}
