use std::any::TypeId;
use std::collections::VecDeque;
use std::fmt::Debug;

use clap::builder::{IntoResettable, StyledStr};
use clap::{CommandFactory, FromArgMatches};
use imbl_value::imbl::OrdMap;
use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::Serialize;
#[cfg(feature = "ts")]
use visit_rs::{Static, Visit};
use yajrc::RpcError;

#[cfg(feature = "ts")]
use crate::ts::{PassthroughChildrenTS, PassthroughParamsTS, PassthroughReturnTS, TSVisitor};
use crate::util::{Flat, PhantomData};
#[cfg(feature = "ts")]
use crate::PassthroughCliBindings;
use crate::{
    CallRemote, CallRemoteHandler, CliBindings, DynHandler, Handler, HandlerArgs, HandlerArgsFor,
    HandlerFor, HandlerTypes, LeafHandler, OrEmpty, PassthroughHandlerFor, PassthroughHandlerTypes,
    PrintCliResult, WithContext,
};

pub trait Adapter {
    type Inner;
    fn as_inner(&self) -> &Self::Inner;
}

pub trait HandlerExt<Context: crate::Context>: HandlerFor<Context> + Sized {
    fn no_cli(self) -> NoCli<Self>;
    fn no_display(self) -> NoDisplay<Self>;
    fn with_custom_display<C: crate::Context, P>(self, display: P) -> CustomDisplay<P, Self>
    where
        P: PrintCliResult<
            C,
            Params = Self::Params,
            InheritedParams = Self::InheritedParams,
            Ok = Self::Ok,
            Err = Self::Err,
        >;
    fn with_custom_display_fn<C: crate::Context, F>(
        self,
        display: F,
    ) -> CustomDisplayFn<F, Self, C>
    where
        F: Fn(HandlerArgsFor<C, Self>, Self::Ok) -> Result<(), Self::Err>;
    fn with_inherited<Params, InheritedParams, F>(
        self,
        f: F,
    ) -> InheritanceHandler<Params, InheritedParams, Self, F>
    where
        F: Fn(Params, InheritedParams) -> Self::InheritedParams;
    fn with_call_remote<C>(self) -> RemoteCaller<C, Context, Self>;
    fn with_about<M>(self, message: M) -> WithAbout<M, Self>
    where
        M: IntoResettable<StyledStr>;
    #[cfg(feature = "ts")]
    fn no_ts(self) -> NoTS<Self>;
    #[cfg(feature = "ts")]
    fn override_params_ts<Params>(self, params_ty: Params) -> OverrideParamsTS<Self, Params>
    where
        Params: Visit<TSVisitor>;
    #[cfg(feature = "ts")]
    fn override_return_ts<Params>(self, params_ty: Params) -> OverrideReturnTS<Self, Params>
    where
        Params: Visit<TSVisitor>;
    #[cfg(feature = "ts")]
    fn override_params_ts_as<Params>(self) -> OverrideParamsTS<Self, Static<Params>>
    where
        Static<Params>: Visit<TSVisitor>;
    #[cfg(feature = "ts")]
    fn override_return_ts_as<Params>(self) -> OverrideReturnTS<Self, Static<Params>>
    where
        Static<Params>: Visit<TSVisitor>;
}

impl<Context: crate::Context, T: HandlerFor<Context> + Sized> HandlerExt<Context> for T {
    fn no_cli(self) -> NoCli<Self> {
        NoCli(self)
    }
    fn no_display(self) -> NoDisplay<Self> {
        NoDisplay(self)
    }
    fn with_custom_display<C: crate::Context, P>(self, display: P) -> CustomDisplay<P, Self>
    where
        P: PrintCliResult<
            C,
            Params = Self::Params,
            InheritedParams = Self::InheritedParams,
            Ok = Self::Ok,
            Err = Self::Err,
        >,
    {
        CustomDisplay {
            print: display,
            handler: self,
        }
    }
    fn with_custom_display_fn<C: crate::Context, F>(self, display: F) -> CustomDisplayFn<F, Self, C>
    where
        F: Fn(HandlerArgsFor<C, Self>, Self::Ok) -> Result<(), Self::Err>,
    {
        CustomDisplayFn {
            _phantom: PhantomData::new(),
            print: display,
            handler: self,
        }
    }
    fn with_inherited<Params, InheritedParams, F>(
        self,
        f: F,
    ) -> InheritanceHandler<Params, InheritedParams, Self, F>
    where
        F: Fn(Params, InheritedParams) -> Self::InheritedParams,
    {
        InheritanceHandler {
            _phantom: PhantomData::new(),
            handler: self,
            inherit: f,
        }
    }
    fn with_call_remote<C>(self) -> RemoteCaller<C, Context, Self> {
        RemoteCaller {
            _phantom: PhantomData::new(),
            handler: self,
        }
    }

    fn with_about<M>(self, message: M) -> WithAbout<M, Self>
    where
        M: IntoResettable<StyledStr>,
    {
        WithAbout {
            handler: self,
            message,
        }
    }

    #[cfg(feature = "ts")]
    fn no_ts(self) -> NoTS<Self> {
        NoTS(self)
    }

    #[cfg(feature = "ts")]
    fn override_params_ts<Params>(self, params_ty: Params) -> OverrideParamsTS<Self, Params>
    where
        Params: Visit<TSVisitor>,
    {
        OverrideParamsTS {
            handler: self,
            params_ts: params_ty,
        }
    }

    #[cfg(feature = "ts")]
    fn override_return_ts<Return>(self, return_ty: Return) -> OverrideReturnTS<Self, Return>
    where
        Return: Visit<TSVisitor>,
    {
        OverrideReturnTS {
            handler: self,
            return_ty,
        }
    }

    #[cfg(feature = "ts")]
    fn override_params_ts_as<Params>(self) -> OverrideParamsTS<Self, Static<Params>>
    where
        Static<Params>: Visit<TSVisitor>,
    {
        OverrideParamsTS {
            handler: self,
            params_ts: Static::<Params>::new(),
        }
    }

    #[cfg(feature = "ts")]
    fn override_return_ts_as<Return>(self) -> OverrideReturnTS<Self, Static<Return>>
    where
        Static<Return>: Visit<TSVisitor>,
    {
        OverrideReturnTS {
            handler: self,
            return_ty: Static::<Return>::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NoCli<H>(pub H);
impl<H> Adapter for NoCli<H> {
    type Inner = H;
    fn as_inner(&self) -> &Self::Inner {
        &self.0
    }
}
impl<H: LeafHandler> LeafHandler for NoCli<H> {}
impl<H> PassthroughHandlerTypes for NoCli<H> {}
impl<H> PassthroughHandlerFor for NoCli<H> {}
impl<Context, H> CliBindings<Context> for NoCli<H>
where
    Context: crate::Context,
    H: HandlerTypes,
{
    const NO_CLI: bool = true;
    fn cli_command(&self) -> clap::Command {
        unimplemented!()
    }
    fn cli_parse(
        &self,
        _: &clap::ArgMatches,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        unimplemented!()
    }
    fn cli_display(&self, _: HandlerArgsFor<Context, Self>, _: Self::Ok) -> Result<(), Self::Err> {
        unimplemented!()
    }
}
#[cfg(feature = "ts")]
impl<H> PassthroughParamsTS for NoCli<H> {}
#[cfg(feature = "ts")]
impl<H> PassthroughReturnTS for NoCli<H> {}
#[cfg(feature = "ts")]
impl<H> PassthroughChildrenTS for NoCli<H> {}

#[derive(Debug, Clone)]
pub struct NoDisplay<H>(pub H);

impl<H> Adapter for NoDisplay<H> {
    type Inner = H;
    fn as_inner(&self) -> &Self::Inner {
        &self.0
    }
}
impl<H: LeafHandler> LeafHandler for NoDisplay<H> {}
impl<H> PassthroughHandlerTypes for NoDisplay<H> {}
impl<H> PassthroughHandlerFor for NoDisplay<H> {}
#[cfg(feature = "ts")]
impl<H> PassthroughParamsTS for NoDisplay<H> {}
#[cfg(feature = "ts")]
impl<H> PassthroughReturnTS for NoDisplay<H> {}
#[cfg(feature = "ts")]
impl<H> PassthroughChildrenTS for NoDisplay<H> {}
impl<Context, H> PrintCliResult<Context> for NoDisplay<H>
where
    Context: crate::Context,
    H: HandlerTypes,
    H::Params: FromArgMatches + CommandFactory + Serialize,
{
    fn print(&self, _: HandlerArgsFor<Context, Self>, _: Self::Ok) -> Result<(), Self::Err> {
        Ok(())
    }
}
impl<Context, H> CliBindings<Context> for NoDisplay<H>
where
    Context: crate::Context,
    Self: HandlerTypes,
    Self::Params: CommandFactory + FromArgMatches + Serialize,
    Self: PrintCliResult<Context>,
{
    fn cli_command(&self) -> clap::Command {
        Self::Params::command()
    }
    fn cli_parse(
        &self,
        matches: &clap::ArgMatches,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        Self::Params::from_arg_matches(matches).and_then(|a| {
            Ok((
                VecDeque::new(),
                imbl_value::to_value(&a)
                    .map_err(|e| clap::Error::raw(clap::error::ErrorKind::ValueValidation, e))?,
            ))
        })
    }
    fn cli_display(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        self.print(handle_args, result)
    }
}

#[derive(Clone, Debug)]
pub struct CustomDisplay<P, H> {
    print: P,
    handler: H,
}

impl<P, H> Adapter for CustomDisplay<P, H>
where
    P: Send + Sync + Clone + 'static,
{
    type Inner = H;
    fn as_inner(&self) -> &Self::Inner {
        &self.handler
    }
}
impl<P, H: LeafHandler> LeafHandler for CustomDisplay<P, H> {}
impl<P, H> PassthroughHandlerTypes for CustomDisplay<P, H> where P: Send + Sync + Clone + 'static {}
impl<P, H> PassthroughHandlerFor for CustomDisplay<P, H> where P: Send + Sync + Clone + 'static {}
#[cfg(feature = "ts")]
impl<P, H> PassthroughParamsTS for CustomDisplay<P, H> where P: Send + Sync + Clone + 'static {}
#[cfg(feature = "ts")]
impl<P, H> PassthroughReturnTS for CustomDisplay<P, H> where P: Send + Sync + Clone + 'static {}
#[cfg(feature = "ts")]
impl<P, H> PassthroughChildrenTS for CustomDisplay<P, H> where P: Send + Sync + Clone + 'static {}
impl<Context, P, H> PrintCliResult<Context> for CustomDisplay<P, H>
where
    Context: crate::Context,
    H: HandlerTypes,
    P: PrintCliResult<
            Context,
            Params = H::Params,
            InheritedParams = H::InheritedParams,
            Ok = H::Ok,
            Err = H::Err,
        > + Send
        + Sync
        + Clone
        + 'static,
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
        self.print.print(
            HandlerArgs {
                context,
                parent_method,
                method,
                params,
                inherited_params,
                raw_params,
            },
            result,
        )
    }
}
impl<Context, P, H> CliBindings<Context> for CustomDisplay<P, H>
where
    Context: crate::Context,
    Self: HandlerTypes,
    Self::Params: CommandFactory + FromArgMatches + Serialize,
    Self: PrintCliResult<Context>,
{
    fn cli_command(&self) -> clap::Command {
        Self::Params::command()
    }
    fn cli_parse(
        &self,
        matches: &clap::ArgMatches,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        Self::Params::from_arg_matches(matches).and_then(|a| {
            Ok((
                VecDeque::new(),
                imbl_value::to_value(&a)
                    .map_err(|e| clap::Error::raw(clap::error::ErrorKind::ValueValidation, e))?,
            ))
        })
    }
    fn cli_display(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        self.print(handle_args, result)
    }
}

pub struct CustomDisplayFn<F, H, Context> {
    _phantom: PhantomData<Context>,
    print: F,
    handler: H,
}

impl<F, H, Context> Adapter for CustomDisplayFn<F, H, Context>
where
    F: Send + Sync + Clone + 'static,
    Context: 'static,
{
    type Inner = H;
    fn as_inner(&self) -> &Self::Inner {
        &self.handler
    }
}

impl<F, H: LeafHandler, Context> LeafHandler for CustomDisplayFn<F, H, Context> {}

impl<Context, F: Clone, H: Clone> Clone for CustomDisplayFn<F, H, Context> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData::new(),
            print: self.print.clone(),
            handler: self.handler.clone(),
        }
    }
}
impl<Context, F: Debug, H: Debug> Debug for CustomDisplayFn<F, H, Context> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomDisplayFn")
            .field("print", &self.print)
            .field("handler", &self.handler)
            .finish()
    }
}
impl<F, H, Context> PassthroughHandlerTypes for CustomDisplayFn<F, H, Context>
where
    F: Send + Sync + Clone + 'static,
    Context: 'static,
{
}
impl<F, H, Context> PassthroughHandlerFor for CustomDisplayFn<F, H, Context>
where
    F: Send + Sync + Clone + 'static,
    Context: 'static,
{
}
#[cfg(feature = "ts")]
impl<F, H, Context> PassthroughParamsTS for CustomDisplayFn<F, H, Context>
where
    F: Send + Sync + Clone + 'static,
    Context: 'static,
{
}
#[cfg(feature = "ts")]
impl<F, H, Context> PassthroughReturnTS for CustomDisplayFn<F, H, Context>
where
    F: Send + Sync + Clone + 'static,
    Context: 'static,
{
}
#[cfg(feature = "ts")]
impl<F, H, Context> PassthroughChildrenTS for CustomDisplayFn<F, H, Context>
where
    F: Send + Sync + Clone + 'static,
    Context: 'static,
{
}
impl<F, H, Context> PrintCliResult<Context> for CustomDisplayFn<F, H, Context>
where
    Context: crate::Context,
    H: HandlerTypes,
    F: Fn(HandlerArgsFor<Context, H>, H::Ok) -> Result<(), H::Err> + Send + Sync + Clone + 'static,
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
        (self.print)(
            HandlerArgs {
                context,
                parent_method,
                method,
                params,
                inherited_params,
                raw_params,
            },
            result,
        )
    }
}
impl<Context, F, H, C> CliBindings<Context> for CustomDisplayFn<F, H, C>
where
    Context: crate::Context,
    Self: HandlerTypes,
    Self::Params: CommandFactory + FromArgMatches + Serialize,
    Self: PrintCliResult<Context>,
{
    fn cli_command(&self) -> clap::Command {
        Self::Params::command()
    }
    fn cli_parse(
        &self,
        matches: &clap::ArgMatches,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        Self::Params::from_arg_matches(matches).and_then(|a| {
            Ok((
                VecDeque::new(),
                imbl_value::to_value(&a)
                    .map_err(|e| clap::Error::raw(clap::error::ErrorKind::ValueValidation, e))?,
            ))
        })
    }
    fn cli_display(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        self.print(handle_args, result)
    }
}

pub struct RemoteCaller<Context, RemoteContext, H> {
    _phantom: PhantomData<(Context, RemoteContext)>,
    handler: H,
}

impl<Context, RemoteContext, H: LeafHandler> LeafHandler
    for RemoteCaller<Context, RemoteContext, H>
{
}

impl<Context, RemoteContext, H: Clone> Clone for RemoteCaller<Context, RemoteContext, H> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData::new(),
            handler: self.handler.clone(),
        }
    }
}
impl<Context, RemoteContext, H: Debug> Debug for RemoteCaller<Context, RemoteContext, H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RemoteCaller").field(&self.handler).finish()
    }
}

impl<Context, H, Inherited, RemoteContext> Handler<Inherited>
    for WithContext<Context, RemoteCaller<Context, RemoteContext, H>>
where
    Context: crate::Context + CallRemote<RemoteContext>,
    RemoteContext: crate::Context,
    H: HandlerFor<RemoteContext> + CliBindings<Context> + crate::ts::HandlerTSBindings,
    H::Ok: Serialize + DeserializeOwned,
    H::Err: From<RpcError>,
    H::Params: Serialize + DeserializeOwned,
    H::InheritedParams: OrEmpty<Inherited>,
    RpcError: From<H::Err>,
    Inherited: Send + Sync + 'static,
{
    type H = H;
    fn handler_for<C: crate::Context>(self) -> Option<DynHandler<C, Inherited>> {
        if TypeId::of::<C>() == TypeId::of::<RemoteContext>() {
            DynHandler::new(self.handler.handler.no_cli())
        } else if TypeId::of::<C>() == TypeId::of::<Context>() {
            DynHandler::new(CallRemoteHandler::<Context, RemoteContext, _>::new(
                self.handler.handler,
            ))
        } else {
            None
        }
    }
}

pub struct InheritanceHandler<Params, InheritedParams, H, F> {
    _phantom: PhantomData<(Params, InheritedParams)>,
    handler: H,
    inherit: F,
}

impl<Params, InheritedParams, H, F> Adapter for InheritanceHandler<Params, InheritedParams, H, F>
where
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    F: Send + Sync + Clone + 'static,
{
    type Inner = H;
    fn as_inner(&self) -> &Self::Inner {
        &self.handler
    }
}

impl<Params, InheritedParams, H: LeafHandler, F> LeafHandler
    for InheritanceHandler<Params, InheritedParams, H, F>
{
}

impl<Params, InheritedParams, H: Clone, F: Clone> Clone
    for InheritanceHandler<Params, InheritedParams, H, F>
{
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData::new(),
            handler: self.handler.clone(),
            inherit: self.inherit.clone(),
        }
    }
}
impl<Params, InheritedParams, H: std::fmt::Debug, F> std::fmt::Debug
    for InheritanceHandler<Params, InheritedParams, H, F>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("InheritanceHandler")
            .field(&self.handler)
            .finish()
    }
}
impl<Params, InheritedParams, H, F> HandlerTypes
    for InheritanceHandler<Params, InheritedParams, H, F>
where
    H: HandlerTypes,
    Params: Send + Sync,
    InheritedParams: Send + Sync,
{
    type Params = H::Params;
    type InheritedParams = Flat<Params, InheritedParams>;
    type Ok = H::Ok;
    type Err = H::Err;
}

#[cfg(feature = "ts")]
impl<Params, InheritedParams, H, F> PassthroughParamsTS
    for InheritanceHandler<Params, InheritedParams, H, F>
where
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    F: Send + Sync + Clone + 'static,
{
}
#[cfg(feature = "ts")]
impl<Params, InheritedParams, H, F> PassthroughReturnTS
    for InheritanceHandler<Params, InheritedParams, H, F>
where
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    F: Send + Sync + Clone + 'static,
{
}
#[cfg(feature = "ts")]
impl<Params, InheritedParams, H, F> PassthroughChildrenTS
    for InheritanceHandler<Params, InheritedParams, H, F>
where
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    F: Send + Sync + Clone + 'static,
{
}

impl<Context, Params, InheritedParams, H, F> HandlerFor<Context>
    for InheritanceHandler<Params, InheritedParams, H, F>
where
    Context: crate::Context,
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    H: HandlerFor<Context>,
    F: Fn(Params, InheritedParams) -> H::InheritedParams + Send + Sync + Clone + 'static,
{
    fn handle_sync(
        &self,
        HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handler.handle_sync(HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params: (self.inherit)(inherited_params.0, inherited_params.1),
            raw_params,
        })
    }
    async fn handle_async(
        &self,
        HandlerArgs {
            context,
            parent_method,
            method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handler
            .handle_async(HandlerArgs {
                context,
                parent_method,
                method,
                params,
                inherited_params: (self.inherit)(inherited_params.0, inherited_params.1),
                raw_params,
            })
            .await
    }
    fn metadata(&self, method: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.handler.metadata(method)
    }
    fn method_from_dots(&self, method: &str) -> Option<VecDeque<&'static str>> {
        self.handler.method_from_dots(method)
    }
}

impl<Context, Params, InheritedParams, H, F> CliBindings<Context>
    for InheritanceHandler<Params, InheritedParams, H, F>
where
    Context: crate::Context,
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    H: CliBindings<Context>,
    F: Fn(Params, InheritedParams) -> H::InheritedParams + Send + Sync + Clone + 'static,
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
                params,
                inherited_params: (self.inherit)(inherited_params.0, inherited_params.1),
                raw_params,
            },
            result,
        )
    }
}

#[derive(Debug, Clone)]
pub struct WithAbout<M, H> {
    handler: H,
    message: M,
}

impl<M, H> Adapter for WithAbout<M, H>
where
    M: Clone + Send + Sync + 'static,
{
    type Inner = H;
    fn as_inner(&self) -> &Self::Inner {
        &self.handler
    }
}

impl<M, H: LeafHandler> LeafHandler for WithAbout<M, H> {}
impl<M, H> PassthroughHandlerTypes for WithAbout<M, H> where M: Clone + Send + Sync + 'static {}
impl<M, H> PassthroughHandlerFor for WithAbout<M, H> where M: Clone + Send + Sync + 'static {}
#[cfg(feature = "ts")]
impl<M, H> PassthroughParamsTS for WithAbout<M, H> where M: Clone + Send + Sync + 'static {}
#[cfg(feature = "ts")]
impl<M, H> PassthroughReturnTS for WithAbout<M, H> where M: Clone + Send + Sync + 'static {}
#[cfg(feature = "ts")]
impl<M, H> PassthroughChildrenTS for WithAbout<M, H> where M: Clone + Send + Sync + 'static {}
impl<Context, M, H> CliBindings<Context> for WithAbout<M, H>
where
    Context: crate::Context,
    H: CliBindings<Context>,
    M: IntoResettable<StyledStr> + Clone + Send + Sync + 'static,
{
    fn cli_command(&self) -> clap::Command {
        self.handler.cli_command().about(self.message.clone())
    }
    fn cli_parse(
        &self,
        arg_matches: &clap::ArgMatches,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        self.handler.cli_parse(arg_matches)
    }
    fn cli_display(
        &self,
        handler: HandlerArgsFor<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        self.handler.cli_display(handler, result)
    }
}

#[cfg(feature = "ts")]
pub use ts::*;

#[cfg(feature = "ts")]
mod ts {
    use super::*;
    use crate::ts::{ChildrenTS, ParamsTS, ReturnTS};

    #[derive(Debug, Clone)]
    pub struct NoTS<H>(pub H);
    impl<H> Adapter for NoTS<H> {
        type Inner = H;
        fn as_inner(&self) -> &Self::Inner {
            &self.0
        }
    }
    impl<H: LeafHandler> LeafHandler for NoTS<H> {}
    impl<H> PassthroughHandlerTypes for NoTS<H> {}
    impl<H> PassthroughHandlerFor for NoTS<H> {}
    impl<H> PassthroughCliBindings for NoTS<H> {}
    impl<H> ParamsTS for NoTS<H> {
        fn params_ts(&self) -> Box<dyn Fn(&mut TSVisitor) + Send + Sync> {
            Box::new(|v| v.ts.push_str("unknown"))
        }
    }
    impl<H> ReturnTS for NoTS<H> {
        fn return_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync>> {
            Some(Box::new(|v| v.ts.push_str("unknown")))
        }
    }
    impl<H> ChildrenTS for NoTS<H> {
        fn ts_enabled(&self) -> bool {
            false
        }
        fn children_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync>> {
            None
        }
    }

    #[derive(Clone, Debug)]
    pub struct OverrideParamsTS<H, P> {
        pub(super) handler: H,
        pub(super) params_ts: P,
    }
    impl<H, P> Adapter for OverrideParamsTS<H, P> {
        type Inner = H;
        fn as_inner(&self) -> &Self::Inner {
            &self.handler
        }
    }
    impl<H: LeafHandler, P> LeafHandler for OverrideParamsTS<H, P> {}
    impl<H, P> PassthroughHandlerTypes for OverrideParamsTS<H, P> {}
    impl<H, P> PassthroughHandlerFor for OverrideParamsTS<H, P> {}
    impl<H, P> PassthroughCliBindings for OverrideParamsTS<H, P> {}
    impl<H, P> ParamsTS for OverrideParamsTS<H, P>
    where
        H: Send + Sync,
        P: Visit<TSVisitor> + Send + Sync,
    {
        fn params_ts<'a>(&'a self) -> Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a> {
            Box::new(move |visitor| self.params_ts.visit(visitor))
        }
    }
    impl<H, P> PassthroughReturnTS for OverrideParamsTS<H, P> where P: Clone + Send + Sync + 'static {}
    impl<H, P> PassthroughChildrenTS for OverrideParamsTS<H, P> where P: Clone + Send + Sync + 'static {}

    #[derive(Clone, Debug)]
    pub struct OverrideReturnTS<H, R> {
        pub(super) handler: H,
        pub(super) return_ty: R,
    }
    impl<H, R> Adapter for OverrideReturnTS<H, R> {
        type Inner = H;
        fn as_inner(&self) -> &Self::Inner {
            &self.handler
        }
    }
    impl<H: LeafHandler, R> LeafHandler for OverrideReturnTS<H, R> {}
    impl<H, R> PassthroughHandlerTypes for OverrideReturnTS<H, R> {}
    impl<H, R> PassthroughHandlerFor for OverrideReturnTS<H, R> {}
    impl<H, R> PassthroughCliBindings for OverrideReturnTS<H, R> {}
    impl<H, R> PassthroughParamsTS for OverrideReturnTS<H, R> {}
    impl<H, R> ReturnTS for OverrideReturnTS<H, R>
    where
        H: Send + Sync,
        R: Visit<TSVisitor> + Send + Sync,
    {
        fn return_ts<'a>(&'a self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a>> {
            Some(Box::new(move |visitor| self.return_ty.visit(visitor)))
        }
    }
    impl<H, R> PassthroughChildrenTS for OverrideReturnTS<H, R> {}
}
