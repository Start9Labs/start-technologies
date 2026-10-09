use std::any::TypeId;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::Arc;

use clap::{ArgMatches, Command, Parser};
use futures::Future;
use imbl_value::imbl::OrdMap;
use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use yajrc::RpcError;

#[cfg(feature = "ts")]
use crate::impl_ts_shape;
use crate::ts::HandlerTSBindings;
use crate::util::{internal_error, invalid_params, Flat};

pub mod adapters;
pub mod from_fn;
pub mod parent;

pub use adapters::*;
pub use from_fn::*;
pub use parent::*;

pub(crate) struct HandleAnyArgs<Context, Inherited> {
    pub(crate) context: Context,
    pub(crate) parent_method: VecDeque<&'static str>,
    pub(crate) method: VecDeque<&'static str>,
    pub(crate) params: Value,
    pub(crate) inherited: Inherited,
}
impl<Context: crate::Context, Inherited: Send + Sync> HandleAnyArgs<Context, Inherited> {
    fn downcast<H>(self) -> Result<HandlerArgsFor<Context, H>, imbl_value::Error>
    where
        H: HandlerTypes,
        H::InheritedParams: OrEmpty<Inherited>,
        H::Params: DeserializeOwned,
    {
        let Self {
            context,
            parent_method,
            method,
            params,
            inherited,
        } = self;
        Ok(HandlerArgs {
            context,
            parent_method,
            method,
            params: imbl_value::from_value(params.clone())?,
            inherited_params: OrEmpty::from_t(inherited),
            raw_params: params,
        })
    }
}

pub(crate) trait HandleAnyRequires: HandlerTSBindings + Send + Sync {}
impl<T: HandlerTSBindings + Send + Sync> HandleAnyRequires for T {}

#[async_trait::async_trait]
pub(crate) trait HandleAny<Context>: HandleAnyRequires {
    type Inherited: Send;
    fn handle_sync(
        &self,
        handle_args: HandleAnyArgs<Context, Self::Inherited>,
    ) -> Result<Value, RpcError>;
    async fn handle_async(
        &self,
        handle_args: HandleAnyArgs<Context, Self::Inherited>,
    ) -> Result<Value, RpcError>;
    fn metadata(&self, method: VecDeque<&'static str>) -> OrdMap<&'static str, Value>;
    fn method_from_dots(&self, method: &str) -> Option<VecDeque<&'static str>>;
    fn cli(&self) -> Option<&dyn CliBindingsAny<Context, Inherited = Self::Inherited>>;
}
#[async_trait::async_trait]
impl<Context: crate::Context, T: HandleAny<Context>> HandleAny<Context> for Arc<T> {
    type Inherited = T::Inherited;
    fn handle_sync(
        &self,
        handle_args: HandleAnyArgs<Context, Self::Inherited>,
    ) -> Result<Value, RpcError> {
        self.deref().handle_sync(handle_args)
    }
    async fn handle_async(
        &self,
        handle_args: HandleAnyArgs<Context, Self::Inherited>,
    ) -> Result<Value, RpcError> {
        self.deref().handle_async(handle_args).await
    }
    fn metadata(&self, method: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.deref().metadata(method)
    }
    fn method_from_dots(&self, method: &str) -> Option<VecDeque<&'static str>> {
        self.deref().method_from_dots(method)
    }
    fn cli(&self) -> Option<&dyn CliBindingsAny<Context, Inherited = Self::Inherited>> {
        self.deref().cli()
    }
}

pub(crate) trait CliBindingsAny<Context> {
    type Inherited;
    fn cli_command(&self) -> Command;
    fn cli_parse(
        &self,
        matches: &ArgMatches,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error>;
    fn cli_display(
        &self,
        handle_args: HandleAnyArgs<Context, Self::Inherited>,
        result: Value,
    ) -> Result<(), RpcError>;
}

pub trait CliBindings<Context: crate::Context>: HandlerTypes {
    const NO_CLI: bool = false;
    fn cli_command(&self) -> Command;
    fn cli_parse(
        &self,
        matches: &ArgMatches,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error>;
    fn cli_display(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err>;
}

pub trait PrintCliResult<Context: crate::Context>: HandlerTypes {
    fn print(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err>;
}

#[allow(private_interfaces)]
pub struct DynHandler<Context, Inherited>(
    pub(crate) Arc<dyn HandleAny<Context, Inherited = Inherited>>,
);
impl<Context: crate::Context, Inherited> DynHandler<Context, Inherited> {
    pub fn new<C, H>(handler: H) -> Option<Self>
    where
        C: crate::Context,
        WithContext<C, H>: Handler<Inherited>,
    {
        WithContext::<C, _>::new(handler).handler_for::<Context>()
    }
}
impl<Context, Inherited> Clone for DynHandler<Context, Inherited> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<Context, Inherited> Debug for DynHandler<Context, Inherited> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynHandler").finish()
    }
}
#[async_trait::async_trait]
impl<Context: crate::Context, Inherited: Send> HandleAny<Context>
    for DynHandler<Context, Inherited>
{
    type Inherited = Inherited;
    fn handle_sync(
        &self,
        handle_args: HandleAnyArgs<Context, Self::Inherited>,
    ) -> Result<Value, RpcError> {
        self.0.handle_sync(handle_args)
    }
    async fn handle_async(
        &self,
        handle_args: HandleAnyArgs<Context, Self::Inherited>,
    ) -> Result<Value, RpcError> {
        self.0.handle_async(handle_args).await
    }
    fn metadata(&self, method: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.0.metadata(method)
    }
    fn method_from_dots(&self, method: &str) -> Option<VecDeque<&'static str>> {
        self.0.method_from_dots(method)
    }
    fn cli(&self) -> Option<&dyn CliBindingsAny<Context, Inherited = Self::Inherited>> {
        self.0.cli()
    }
}

#[allow(type_alias_bounds)]
pub type HandlerArgsFor<Context: crate::Context, H: HandlerTypes + ?Sized> =
    HandlerArgs<Context, H::Params, H::InheritedParams>;

#[derive(Debug, Clone)]
pub struct HandlerArgs<
    Context: crate::Context,
    Params: Send + Sync = Empty,
    InheritedParams: Send + Sync = Empty,
> {
    pub context: Context,
    pub parent_method: VecDeque<&'static str>,
    pub method: VecDeque<&'static str>,
    pub params: Params,
    pub inherited_params: InheritedParams,
    pub raw_params: Value,
}

pub trait HandlerTypes {
    type Params: Send + Sync;
    type InheritedParams: Send + Sync;
    type Ok: Send + Sync;
    type Err: Send + Sync;
}

pub trait PassthroughHandlerTypes: Adapter {}
impl<T> HandlerTypes for T
where
    T: PassthroughHandlerTypes,
    T::Inner: HandlerTypes,
{
    type Params = <T::Inner as HandlerTypes>::Params;
    type InheritedParams = <T::Inner as HandlerTypes>::InheritedParams;
    type Ok = <T::Inner as HandlerTypes>::Ok;
    type Err = <T::Inner as HandlerTypes>::Err;
}

pub trait LeafHandler {}

pub trait HandlerRequires: HandlerTypes + Clone + Send + Sync + 'static {}
impl<T: HandlerTypes + Clone + Send + Sync + 'static> HandlerRequires for T {}

pub trait HandlerFor<Context: crate::Context>: HandlerRequires {
    fn handle_sync(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        if let Some(rt) = handle_args.context.runtime() {
            rt.block_on(self.handle_async(handle_args))
        } else {
            tokio::runtime::Handle::current().block_on(self.handle_async(handle_args))
        }
    }
    fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> impl Future<Output = Result<Self::Ok, Self::Err>> + Send;
    fn handle_async_with_sync<'a>(
        &'a self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> impl Future<Output = Result<Self::Ok, Self::Err>> + Send + 'a {
        async move { self.handle_sync(handle_args) }
    }
    fn handle_async_with_sync_blocking<'a>(
        &'a self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> impl Future<Output = Result<Self::Ok, Self::Err>> + Send + 'a {
        async move {
            let s = self.clone();
            if let Some(rt) = handle_args.context.runtime() {
                rt.spawn_blocking(move || s.handle_sync(handle_args)).await
            } else {
                tokio::runtime::Handle::current()
                    .spawn_blocking(move || s.handle_sync(handle_args))
                    .await
            }
            .unwrap()
        }
    }
    #[allow(unused_variables)]
    fn metadata(&self, method: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        OrdMap::new()
    }
    #[allow(unused_variables)]
    fn method_from_dots(&self, method: &str) -> Option<VecDeque<&'static str>> {
        if method.is_empty() {
            Some(VecDeque::new())
        } else {
            None
        }
    }
}
pub trait PassthroughHandlerFor: PassthroughHandlerTypes {}
impl<T, Context> HandlerFor<Context> for T
where
    T: PassthroughHandlerFor + Clone + Send + Sync + 'static,
    T::Inner: HandlerFor<Context>,
    Context: crate::Context,
{
    fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> impl Future<Output = Result<Self::Ok, Self::Err>> + Send {
        self.as_inner().handle_async(handle_args)
    }
    fn handle_async_with_sync<'a>(
        &'a self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> impl Future<Output = Result<Self::Ok, Self::Err>> + Send + 'a {
        self.as_inner().handle_async_with_sync(handle_args)
    }
    fn handle_async_with_sync_blocking<'a>(
        &'a self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> impl Future<Output = Result<Self::Ok, Self::Err>> + Send + 'a {
        self.as_inner().handle_async_with_sync_blocking(handle_args)
    }
    fn handle_sync(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.as_inner().handle_sync(handle_args)
    }
    fn metadata(&self, method: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.as_inner().metadata(method)
    }
    fn method_from_dots(&self, method: &str) -> Option<VecDeque<&'static str>> {
        self.as_inner().method_from_dots(method)
    }
}

pub trait PassthroughCliBindings: PassthroughHandlerTypes {}
impl<T, Context> CliBindings<Context> for T
where
    T: PassthroughCliBindings,
    T::Inner: CliBindings<Context>,
    Context: crate::Context,
{
    const NO_CLI: bool = <T::Inner as CliBindings<Context>>::NO_CLI;
    fn cli_command(&self) -> Command {
        self.as_inner().cli_command()
    }
    fn cli_parse(
        &self,
        matches: &ArgMatches,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        self.as_inner().cli_parse(matches)
    }
    fn cli_display(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        self.as_inner().cli_display(handle_args, result)
    }
}

pub trait Handler<Inherited> {
    type H: HandlerTypes;
    fn handler_for<C: crate::Context>(self) -> Option<DynHandler<C, Inherited>>;
}

pub struct WithContext<Context, H> {
    _phantom: PhantomData<Context>,
    handler: H,
}
impl<Context, H> WithContext<Context, H> {
    pub fn new(handler: H) -> Self {
        Self {
            _phantom: PhantomData,
            handler,
        }
    }
}

impl<Context, Inherited, H> Handler<Inherited> for WithContext<Context, H>
where
    Context: crate::Context,
    H: HandlerFor<Context> + CliBindings<Context> + HandlerTSBindings,
    H::Ok: Serialize + DeserializeOwned,
    H::Params: DeserializeOwned,
    H::InheritedParams: OrEmpty<Inherited>,
    RpcError: From<H::Err>,
    Inherited: Send + Sync + 'static,
{
    type H = H;
    fn handler_for<C: crate::Context>(self) -> Option<DynHandler<C, Inherited>> {
        if TypeId::of::<Context>() == TypeId::of::<C>() {
            Some(unsafe {
                std::mem::transmute::<DynHandler<Context, Inherited>, DynHandler<C, Inherited>>(
                    DynHandler(Arc::new(AnyHandler::new(self.handler))),
                )
            })
        } else {
            None
        }
    }
}

pub(crate) struct AnyHandler<Context, Inherited, H> {
    _phantom: PhantomData<(Context, Inherited)>,
    handler: H,
}
impl<Context, Inherited, H> AnyHandler<Context, Inherited, H> {
    pub(crate) fn new(handler: H) -> Self {
        Self {
            _phantom: PhantomData,
            handler,
        }
    }
}
impl<Context, Inherited, H: std::fmt::Debug> std::fmt::Debug for AnyHandler<Context, Inherited, H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnyHandler")
            .field("handler", &self.handler)
            .finish()
    }
}

impl<C, I, H> Adapter for AnyHandler<C, I, H> {
    type Inner = H;
    fn as_inner(&self) -> &H {
        &self.handler
    }
}
#[cfg(feature = "ts")]
impl<C, I, H> crate::ts::PassthroughParamsTS for AnyHandler<C, I, H> {}
#[cfg(feature = "ts")]
impl<C, I, H> crate::ts::PassthroughReturnTS for AnyHandler<C, I, H> {}
#[cfg(feature = "ts")]
impl<C, I, H> crate::ts::PassthroughChildrenTS for AnyHandler<C, I, H> {}

#[async_trait::async_trait]
impl<Context, Inherited, H> HandleAny<Context> for AnyHandler<Context, Inherited, H>
where
    Context: crate::Context,
    H: HandlerFor<Context> + CliBindings<Context> + HandlerTSBindings,
    H::Params: DeserializeOwned,
    H::Ok: Serialize + DeserializeOwned,
    H::InheritedParams: OrEmpty<Inherited>,
    RpcError: From<H::Err>,
    Inherited: Send + Sync,
{
    type Inherited = Inherited;
    fn handle_sync(
        &self,
        handle_args: HandleAnyArgs<Context, Self::Inherited>,
    ) -> Result<Value, RpcError> {
        imbl_value::to_value(
            &self
                .handler
                .handle_sync(handle_args.downcast::<H>().map_err(invalid_params)?)?,
        )
        .map_err(internal_error)
    }
    async fn handle_async(
        &self,
        handle_args: HandleAnyArgs<Context, Self::Inherited>,
    ) -> Result<Value, RpcError> {
        imbl_value::to_value(
            &self
                .handler
                .handle_async(handle_args.downcast::<H>().map_err(invalid_params)?)
                .await?,
        )
        .map_err(internal_error)
    }
    fn metadata(&self, method: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.handler.metadata(method)
    }
    fn method_from_dots(&self, method: &str) -> Option<VecDeque<&'static str>> {
        self.handler.method_from_dots(method)
    }
    fn cli(&self) -> Option<&dyn CliBindingsAny<Context, Inherited = Self::Inherited>> {
        if H::NO_CLI {
            None
        } else {
            Some(self)
        }
    }
}

impl<Context, Inherited, H> CliBindingsAny<Context> for AnyHandler<Context, Inherited, H>
where
    Context: crate::Context,
    H: CliBindings<Context>,
    H::Params: DeserializeOwned,
    H::Ok: Serialize + DeserializeOwned,
    RpcError: From<H::Err>,
    H::InheritedParams: OrEmpty<Inherited>,
    Inherited: Send + Sync,
{
    type Inherited = Inherited;
    fn cli_command(&self) -> Command {
        self.handler.cli_command()
    }
    fn cli_parse(
        &self,
        matches: &ArgMatches,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        self.handler.cli_parse(matches)
    }
    fn cli_display(
        &self,
        handle_args: HandleAnyArgs<Context, Self::Inherited>,
        result: Value,
    ) -> Result<(), RpcError> {
        self.handler
            .cli_display(
                handle_args.downcast::<H>().map_err(invalid_params)?,
                imbl_value::from_value(result).map_err(internal_error)?,
            )
            .map_err(RpcError::from)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Parser)]
#[cfg_attr(feature = "ts", derive(visit_rs::VisitFields))]
#[group(skip)]
pub struct Empty {}
#[cfg(feature = "ts")]
impl_ts_shape!(Empty);

pub trait OrEmpty<T> {
    fn from_t(t: T) -> Self;
}
impl<T> OrEmpty<T> for T {
    fn from_t(t: T) -> Self {
        t
    }
}
impl<A, B> OrEmpty<Flat<A, B>> for Empty {
    fn from_t(_: Flat<A, B>) -> Self {
        Empty {}
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Parser)]
#[group(skip)]
pub enum Never {}
