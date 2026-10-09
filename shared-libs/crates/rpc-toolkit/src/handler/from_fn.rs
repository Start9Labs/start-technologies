use std::collections::VecDeque;
use std::fmt::Display;

use clap::{CommandFactory, FromArgMatches};
use futures::Future;
use imbl_value::imbl::OrdMap;
use imbl_value::Value;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::util::PhantomData;
use crate::{
    CliBindings, Empty, HandlerArgs, HandlerArgsFor, HandlerFor, HandlerTypes, LeafHandler,
    PrintCliResult,
};

pub struct FromFn<F, T, E, Args> {
    _phantom: PhantomData<(T, E, Args)>,
    function: F,
    blocking: bool,
    metadata: OrdMap<&'static str, Value>,
}

impl<F, T, E, Args> LeafHandler for FromFn<F, T, E, Args> {}
impl<F, T, E, Args> FromFn<F, T, E, Args> {
    pub fn with_metadata(mut self, key: &'static str, value: Value) -> Self {
        self.metadata.insert(key, value);
        self
    }
}
impl<F: Clone, T, E, Args> Clone for FromFn<F, T, E, Args> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData::new(),
            function: self.function.clone(),
            blocking: self.blocking,
            metadata: self.metadata.clone(),
        }
    }
}
impl<F, T, E, Args> std::fmt::Debug for FromFn<F, T, E, Args> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FromFn")
            .field("blocking", &self.blocking)
            .finish()
    }
}

impl<Context, F, T, E, Args> PrintCliResult<Context> for FromFn<F, T, E, Args>
where
    Context: crate::Context,
    Self: HandlerTypes,
    <Self as HandlerTypes>::Ok: Display,
{
    fn print(&self, _: HandlerArgsFor<Context, Self>, result: Self::Ok) -> Result<(), Self::Err> {
        Ok(println!("{result}"))
    }
}
impl<Context, F, T, E, Args> CliBindings<Context> for FromFn<F, T, E, Args>
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
        self.print(
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

pub fn from_fn<F, T, E, Args>(function: F) -> FromFn<F, T, E, Args>
where
    FromFn<F, T, E, Args>: HandlerTypes,
{
    FromFn {
        function,
        _phantom: PhantomData::new(),
        blocking: false,
        metadata: OrdMap::new(),
    }
}

pub fn from_fn_blocking<F, T, E, Args>(function: F) -> FromFn<F, T, E, Args>
where
    FromFn<F, T, E, Args>: HandlerTypes,
{
    FromFn {
        function,
        _phantom: PhantomData::new(),
        blocking: true,
        metadata: OrdMap::new(),
    }
}

pub struct FromFnAsync<F, Fut, T, E, Args> {
    _phantom: PhantomData<(Fut, T, E, Args)>,
    function: F,
    metadata: OrdMap<&'static str, Value>,
}

impl<F, Fut, T, E, Args> LeafHandler for FromFnAsync<F, Fut, T, E, Args> {}
impl<F, Fut, T, E, Args> FromFnAsync<F, Fut, T, E, Args> {
    pub fn with_metadata(mut self, key: &'static str, value: Value) -> Self {
        self.metadata.insert(key, value);
        self
    }
}
impl<F: Clone, Fut, T, E, Args> Clone for FromFnAsync<F, Fut, T, E, Args> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData::new(),
            function: self.function.clone(),
            metadata: self.metadata.clone(),
        }
    }
}
impl<F, Fut, T, E, Args> std::fmt::Debug for FromFnAsync<F, Fut, T, E, Args> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FromFnAsync").finish()
    }
}

impl<Context, F, Fut, T, E, Args> PrintCliResult<Context> for FromFnAsync<F, Fut, T, E, Args>
where
    Context: crate::Context,
    Self: HandlerTypes,
    <Self as HandlerTypes>::Ok: Display,
{
    fn print(&self, _: HandlerArgsFor<Context, Self>, result: Self::Ok) -> Result<(), Self::Err> {
        Ok(println!("{result}"))
    }
}
impl<Context, F, Fut, T, E, Args> CliBindings<Context> for FromFnAsync<F, Fut, T, E, Args>
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
        self.print(
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

pub fn from_fn_async<F, Fut, T, E, Args>(function: F) -> FromFnAsync<F, Fut, T, E, Args>
where
    FromFnAsync<F, Fut, T, E, Args>: HandlerTypes,
{
    FromFnAsync {
        function,
        _phantom: PhantomData::new(),
        metadata: OrdMap::new(),
    }
}

pub struct FromFnAsyncLocal<F, Fut, T, E, Args> {
    _phantom: PhantomData<(Fut, T, E, Args)>,
    function: F,
    metadata: OrdMap<&'static str, Value>,
}

impl<F, Fut, T, E, Args> LeafHandler for FromFnAsyncLocal<F, Fut, T, E, Args> {}
impl<F, Fut, T, E, Args> FromFnAsyncLocal<F, Fut, T, E, Args> {
    pub fn with_metadata(mut self, key: &'static str, value: Value) -> Self {
        self.metadata.insert(key, value);
        self
    }
}
impl<F: Clone, Fut, T, E, Args> Clone for FromFnAsyncLocal<F, Fut, T, E, Args> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData::new(),
            function: self.function.clone(),
            metadata: self.metadata.clone(),
        }
    }
}
impl<F, Fut, T, E, Args> std::fmt::Debug for FromFnAsyncLocal<F, Fut, T, E, Args> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FromFnAsyncLocal").finish()
    }
}

impl<Context, F, Fut, T, E, Args> PrintCliResult<Context> for FromFnAsyncLocal<F, Fut, T, E, Args>
where
    Context: crate::Context,
    Self: HandlerTypes,
    <Self as HandlerTypes>::Ok: Display,
{
    fn print(&self, _: HandlerArgsFor<Context, Self>, result: Self::Ok) -> Result<(), Self::Err> {
        Ok(println!("{result}"))
    }
}
impl<Context, F, Fut, T, E, Args> CliBindings<Context> for FromFnAsyncLocal<F, Fut, T, E, Args>
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
        self.print(
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

pub fn from_fn_async_local<F, Fut, T, E, Args>(function: F) -> FromFnAsyncLocal<F, Fut, T, E, Args>
where
    FromFnAsyncLocal<F, Fut, T, E, Args>: HandlerTypes,
{
    FromFnAsyncLocal {
        function,
        _phantom: PhantomData::new(),
        metadata: OrdMap::new(),
    }
}

impl<F, T, E, Context, Params, InheritedParams> HandlerTypes
    for FromFn<F, T, E, HandlerArgs<Context, Params, InheritedParams>>
where
    F: Fn(HandlerArgs<Context, Params, InheritedParams>) -> Result<T, E>
        + Send
        + Sync
        + Clone
        + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
    Context: crate::Context,
    Params: Send + Sync,
    InheritedParams: Send + Sync,
{
    type Params = Params;
    type InheritedParams = InheritedParams;
    type Ok = T;
    type Err = E;
}

impl<F, T, E, Context, Params, InheritedParams> HandlerFor<Context>
    for FromFn<F, T, E, HandlerArgs<Context, Params, InheritedParams>>
where
    Self: crate::handler::HandlerRequires<
        Params = Params,
        InheritedParams = InheritedParams,
        Ok = T,
        Err = E,
    >,
    F: Fn(HandlerArgs<Context, Params, InheritedParams>) -> Result<T, E>
        + Send
        + Sync
        + Clone
        + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
    Context: crate::Context,
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
{
    fn handle_sync(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        (self.function)(handle_args)
    }
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        if self.blocking {
            self.handle_async_with_sync_blocking(handle_args).await
        } else {
            self.handle_async_with_sync(handle_args).await
        }
    }
    fn metadata(&self, _: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
    }
}
impl<F, Fut, T, E, Context, Params, InheritedParams> HandlerTypes
    for FromFnAsync<F, Fut, T, E, HandlerArgs<Context, Params, InheritedParams>>
where
    F: Fn(HandlerArgs<Context, Params, InheritedParams>) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
    Context: crate::Context,
    Params: Send + Sync,
    InheritedParams: Send + Sync,
{
    type Params = Params;
    type InheritedParams = InheritedParams;
    type Ok = T;
    type Err = E;
}

impl<F, Fut, T, E, Context, Params, InheritedParams> HandlerFor<Context>
    for FromFnAsync<F, Fut, T, E, HandlerArgs<Context, Params, InheritedParams>>
where
    Self: crate::handler::HandlerRequires<
        Params = Params,
        InheritedParams = InheritedParams,
        Ok = T,
        Err = E,
    >,
    F: Fn(HandlerArgs<Context, Params, InheritedParams>) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
    Context: crate::Context,
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
{
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        (self.function)(handle_args).await
    }
    fn metadata(&self, _: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
    }
}

impl<F, T, E> HandlerTypes for FromFn<F, T, E, ()>
where
    F: Fn() -> Result<T, E> + Send + Sync + Clone + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Empty;
    type InheritedParams = Empty;
    type Ok = T;
    type Err = E;
}

impl<Context, F, T, E> HandlerFor<Context> for FromFn<F, T, E, ()>
where
    Self: crate::handler::HandlerRequires<Ok = T, Err = E>,
    Context: crate::Context,
    F: Fn() -> Result<T, E> + Send + Sync + Clone + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    fn handle_sync(&self, _: HandlerArgsFor<Context, Self>) -> Result<Self::Ok, Self::Err> {
        (self.function)()
    }
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        if self.blocking {
            self.handle_async_with_sync_blocking(handle_args).await
        } else {
            self.handle_async_with_sync(handle_args).await
        }
    }
    fn metadata(&self, _: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
    }
}
impl<F, Fut, T, E> HandlerTypes for FromFnAsync<F, Fut, T, E, ()>
where
    F: Fn() -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Empty;
    type InheritedParams = Empty;
    type Ok = T;
    type Err = E;
}

impl<Context, F, Fut, T, E> HandlerFor<Context> for FromFnAsync<F, Fut, T, E, ()>
where
    Self: crate::handler::HandlerRequires<Ok = T, Err = E>,
    Context: crate::Context,
    F: Fn() -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    async fn handle_async(&self, _: HandlerArgsFor<Context, Self>) -> Result<Self::Ok, Self::Err> {
        (self.function)().await
    }
    fn metadata(&self, _: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
    }
}

impl<Context, F, T, E> HandlerTypes for FromFn<F, T, E, (Context,)>
where
    Context: crate::Context,
    F: Fn(Context) -> Result<T, E> + Send + Sync + Clone + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Empty;
    type InheritedParams = Empty;
    type Ok = T;
    type Err = E;
}

impl<Context, F, T, E> HandlerFor<Context> for FromFn<F, T, E, (Context,)>
where
    Self: crate::handler::HandlerRequires<Ok = T, Err = E>,
    Context: crate::Context,
    F: Fn(Context) -> Result<T, E> + Send + Sync + Clone + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    fn handle_sync(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        (self.function)(handle_args.context)
    }
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        if self.blocking {
            self.handle_async_with_sync_blocking(handle_args).await
        } else {
            self.handle_async_with_sync(handle_args).await
        }
    }
    fn metadata(&self, _: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
    }
}
impl<Context, F, Fut, T, E> HandlerTypes for FromFnAsync<F, Fut, T, E, (Context,)>
where
    Context: crate::Context,
    F: Fn(Context) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Empty;
    type InheritedParams = Empty;
    type Ok = T;
    type Err = E;
}

impl<Context, F, Fut, T, E> HandlerFor<Context> for FromFnAsync<F, Fut, T, E, (Context,)>
where
    Self: crate::handler::HandlerRequires<Ok = T, Err = E>,
    Context: crate::Context,
    F: Fn(Context) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        (self.function)(handle_args.context).await
    }
    fn metadata(&self, _: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
    }
}

impl<Context, F, T, E, Params> HandlerTypes for FromFn<F, T, E, (Context, Params)>
where
    Context: crate::Context,
    F: Fn(Context, Params) -> Result<T, E> + Send + Sync + Clone + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Params;
    type InheritedParams = Empty;
    type Ok = T;
    type Err = E;
}

impl<Context, F, T, E, Params> HandlerFor<Context> for FromFn<F, T, E, (Context, Params)>
where
    Self: crate::handler::HandlerRequires<Params = Params, Ok = T, Err = E>,
    Context: crate::Context,
    F: Fn(Context, Params) -> Result<T, E> + Send + Sync + Clone + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    fn handle_sync(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let HandlerArgs {
            context, params, ..
        } = handle_args;
        (self.function)(context, params)
    }
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        if self.blocking {
            self.handle_async_with_sync_blocking(handle_args).await
        } else {
            self.handle_async_with_sync(handle_args).await
        }
    }
    fn metadata(&self, _: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
    }
}
impl<Context, F, Fut, T, E, Params> HandlerTypes for FromFnAsync<F, Fut, T, E, (Context, Params)>
where
    Context: crate::Context,
    F: Fn(Context, Params) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Params;
    type InheritedParams = Empty;
    type Ok = T;
    type Err = E;
}

impl<Context, F, Fut, T, E, Params> HandlerFor<Context>
    for FromFnAsync<F, Fut, T, E, (Context, Params)>
where
    Self: crate::handler::HandlerRequires<Params = Params, Ok = T, Err = E>,
    Context: crate::Context,
    F: Fn(Context, Params) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let HandlerArgs {
            context, params, ..
        } = handle_args;
        (self.function)(context, params).await
    }
    fn metadata(&self, _: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
    }
}

impl<Context, F, T, E, Params, InheritedParams> HandlerTypes
    for FromFn<F, T, E, (Context, Params, InheritedParams)>
where
    Context: crate::Context,
    F: Fn(Context, Params, InheritedParams) -> Result<T, E> + Send + Sync + Clone + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Params;
    type InheritedParams = InheritedParams;
    type Ok = T;
    type Err = E;
}

impl<Context, F, T, E, Params, InheritedParams> HandlerFor<Context>
    for FromFn<F, T, E, (Context, Params, InheritedParams)>
where
    Self: crate::handler::HandlerRequires<
        Params = Params,
        InheritedParams = InheritedParams,
        Ok = T,
        Err = E,
    >,
    Context: crate::Context,
    F: Fn(Context, Params, InheritedParams) -> Result<T, E> + Send + Sync + Clone + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    fn handle_sync(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let HandlerArgs {
            context,
            params,
            inherited_params,
            ..
        } = handle_args;
        (self.function)(context, params, inherited_params)
    }
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        if self.blocking {
            self.handle_async_with_sync_blocking(handle_args).await
        } else {
            self.handle_async_with_sync(handle_args).await
        }
    }
    fn metadata(&self, _: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
    }
}
impl<Context, F, Fut, T, E, Params, InheritedParams> HandlerTypes
    for FromFnAsync<F, Fut, T, E, (Context, Params, InheritedParams)>
where
    Context: crate::Context,
    F: Fn(Context, Params, InheritedParams) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Params;
    type InheritedParams = InheritedParams;
    type Ok = T;
    type Err = E;
}

impl<Context, F, Fut, T, E, Params, InheritedParams> HandlerFor<Context>
    for FromFnAsync<F, Fut, T, E, (Context, Params, InheritedParams)>
where
    Self: crate::handler::HandlerRequires<
        Params = Params,
        InheritedParams = InheritedParams,
        Ok = T,
        Err = E,
    >,
    Context: crate::Context,
    F: Fn(Context, Params, InheritedParams) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let HandlerArgs {
            context,
            params,
            inherited_params,
            ..
        } = handle_args;
        (self.function)(context, params, inherited_params).await
    }
    fn metadata(&self, _: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
    }
}

impl<F, Fut, T, E, Context, Params, InheritedParams> HandlerTypes
    for FromFnAsyncLocal<F, Fut, T, E, HandlerArgs<Context, Params, InheritedParams>>
where
    F: Fn(HandlerArgs<Context, Params, InheritedParams>) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
    Context: crate::Context,
    Params: Send + Sync,
    InheritedParams: Send + Sync,
{
    type Params = Params;
    type InheritedParams = InheritedParams;
    type Ok = T;
    type Err = E;
}

impl<F, Fut, T, E, Context, Params, InheritedParams> HandlerFor<Context>
    for FromFnAsyncLocal<F, Fut, T, E, HandlerArgs<Context, Params, InheritedParams>>
where
    Self: crate::handler::HandlerRequires<
        Params = Params,
        InheritedParams = InheritedParams,
        Ok = T,
        Err = E,
    >,
    F: Fn(HandlerArgs<Context, Params, InheritedParams>) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
    Context: crate::Context,
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
{
    fn handle_sync(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let local = tokio::task::LocalSet::new();
        if let Some(rt) = handle_args.context.runtime() {
            local.block_on(&*rt, (self.function)(handle_args))
        } else {
            tokio::runtime::Handle::current()
                .block_on(local.run_until((self.function)(handle_args)))
        }
    }
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handle_async_with_sync_blocking(handle_args).await
    }
    fn metadata(&self, _: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
    }
}

impl<F, Fut, T, E> HandlerTypes for FromFnAsyncLocal<F, Fut, T, E, ()>
where
    F: Fn() -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Empty;
    type InheritedParams = Empty;
    type Ok = T;
    type Err = E;
}

impl<Context, F, Fut, T, E> HandlerFor<Context> for FromFnAsyncLocal<F, Fut, T, E, ()>
where
    Self: crate::handler::HandlerRequires<Ok = T, Err = E>,
    Context: crate::Context,
    F: Fn() -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    fn handle_sync(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let local = tokio::task::LocalSet::new();
        if let Some(rt) = handle_args.context.runtime() {
            local.block_on(&*rt, (self.function)())
        } else {
            tokio::runtime::Handle::current().block_on(local.run_until((self.function)()))
        }
    }
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handle_async_with_sync_blocking(handle_args).await
    }
    fn metadata(&self, _: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
    }
}

impl<Context, F, Fut, T, E> HandlerTypes for FromFnAsyncLocal<F, Fut, T, E, (Context,)>
where
    Context: crate::Context,
    F: Fn(Context) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Empty;
    type InheritedParams = Empty;
    type Ok = T;
    type Err = E;
}

impl<Context, F, Fut, T, E> HandlerFor<Context> for FromFnAsyncLocal<F, Fut, T, E, (Context,)>
where
    Self: crate::handler::HandlerRequires<Ok = T, Err = E>,
    Context: crate::Context,
    F: Fn(Context) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    fn handle_sync(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let local = tokio::task::LocalSet::new();
        if let Some(rt) = handle_args.context.runtime() {
            local.block_on(&*rt, (self.function)(handle_args.context))
        } else {
            tokio::runtime::Handle::current()
                .block_on(local.run_until((self.function)(handle_args.context)))
        }
    }
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handle_async_with_sync_blocking(handle_args).await
    }
    fn metadata(&self, _: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
    }
}

impl<Context, F, Fut, T, E, Params> HandlerTypes
    for FromFnAsyncLocal<F, Fut, T, E, (Context, Params)>
where
    Context: crate::Context,
    F: Fn(Context, Params) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Params;
    type InheritedParams = Empty;
    type Ok = T;
    type Err = E;
}

impl<Context, F, Fut, T, E, Params> HandlerFor<Context>
    for FromFnAsyncLocal<F, Fut, T, E, (Context, Params)>
where
    Self: crate::handler::HandlerRequires<Params = Params, Ok = T, Err = E>,
    Context: crate::Context,
    F: Fn(Context, Params) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    fn handle_sync(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let local = tokio::task::LocalSet::new();
        if let Some(rt) = handle_args.context.runtime() {
            local.block_on(
                &*rt,
                (self.function)(handle_args.context, handle_args.params),
            )
        } else {
            tokio::runtime::Handle::current()
                .block_on(local.run_until((self.function)(handle_args.context, handle_args.params)))
        }
    }
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handle_async_with_sync_blocking(handle_args).await
    }
    fn metadata(&self, _: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
    }
}

impl<Context, F, Fut, T, E, Params, InheritedParams> HandlerTypes
    for FromFnAsyncLocal<F, Fut, T, E, (Context, Params, InheritedParams)>
where
    Context: crate::Context,
    F: Fn(Context, Params, InheritedParams) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    type Params = Params;
    type InheritedParams = InheritedParams;
    type Ok = T;
    type Err = E;
}

impl<Context, F, Fut, T, E, Params, InheritedParams> HandlerFor<Context>
    for FromFnAsyncLocal<F, Fut, T, E, (Context, Params, InheritedParams)>
where
    Self: crate::handler::HandlerRequires<
        Params = Params,
        InheritedParams = InheritedParams,
        Ok = T,
        Err = E,
    >,
    Context: crate::Context,
    F: Fn(Context, Params, InheritedParams) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T, E>> + 'static,
    Params: DeserializeOwned + Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    fn handle_sync(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let local = tokio::task::LocalSet::new();
        if let Some(rt) = handle_args.context.runtime() {
            local.block_on(
                &*rt,
                (self.function)(
                    handle_args.context,
                    handle_args.params,
                    handle_args.inherited_params,
                ),
            )
        } else {
            tokio::runtime::Handle::current().block_on(local.run_until((self.function)(
                handle_args.context,
                handle_args.params,
                handle_args.inherited_params,
            )))
        }
    }
    async fn handle_async(
        &self,
        handle_args: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        self.handle_async_with_sync_blocking(handle_args).await
    }
    fn metadata(&self, _: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        self.metadata.clone()
    }
}
