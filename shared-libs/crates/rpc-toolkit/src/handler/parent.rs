use std::collections::VecDeque;
use std::fmt::Debug;

use clap::{ArgMatches, Command, CommandFactory, FromArgMatches};
use imbl_value::imbl::OrdMap;
use imbl_value::Value;
use serde::Serialize;
use yajrc::RpcError;

use crate::util::{combine, Flat, PhantomData};
use crate::{
    CliBindings, DynHandler, Empty, HandleAny, HandleAnyArgs, Handler, HandlerArgs, HandlerArgsFor,
    HandlerFor, HandlerRequires, HandlerTypes, WithContext,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Name(pub(crate) &'static str);
impl<'a> std::borrow::Borrow<&'a str> for Name {
    fn borrow(&self) -> &&'a str {
        &self.0
    }
}

pub(crate) struct SubcommandMap<Context, Params, InheritedParams>(
    pub(crate) Option<DynHandler<Context, InheritedParams>>,
    pub(crate) OrdMap<Name, DynHandler<Context, Flat<Params, InheritedParams>>>,
);
impl<Context, Params, InheritedParams> Clone for SubcommandMap<Context, Params, InheritedParams> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), self.1.clone())
    }
}
impl<Context, Params, InheritedParams> Debug for SubcommandMap<Context, Params, InheritedParams> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut map = f.debug_map();
        if let Some(root) = &self.0 {
            #[derive(Debug)]
            struct Root;
            map.entry(&Root, root);
        }
        map.entries(self.1.iter()).finish()
    }
}
impl<Context, Params, InheritedParams> SubcommandMap<Context, Params, InheritedParams> {
    fn set_root(&mut self, handler: DynHandler<Context, InheritedParams>) {
        self.0 = Some(handler);
    }
    fn get_root<'a>(&'a self) -> Option<&'a DynHandler<Context, InheritedParams>> {
        self.0.as_ref()
    }
    fn insert(
        &mut self,
        name: &'static str,
        handler: DynHandler<Context, Flat<Params, InheritedParams>>,
    ) {
        self.1.insert(Name(name), handler);
    }
    fn get<'a>(
        &'a self,
        name: &str,
    ) -> Option<(Name, &'a DynHandler<Context, Flat<Params, InheritedParams>>)> {
        if let Some((name, handler)) = self.1.get_key_value(&name) {
            Some((*name, handler))
        } else {
            None
        }
    }
}

pub struct ParentHandler<Context, Params = Empty, InheritedParams = Empty> {
    _phantom: PhantomData<Context>,
    pub(crate) subcommands: SubcommandMap<Context, Params, InheritedParams>,
    metadata: OrdMap<&'static str, Value>,
}
impl<Context, Params, InheritedParams> ParentHandler<Context, Params, InheritedParams> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData::new(),
            subcommands: SubcommandMap(None, OrdMap::new()),
            metadata: OrdMap::new(),
        }
    }
    pub fn with_metadata(mut self, key: &'static str, value: Value) -> Self {
        self.metadata.insert(key, value);
        self
    }
}
impl<Context, Params, InheritedParams> Clone for ParentHandler<Context, Params, InheritedParams> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData::new(),
            subcommands: self.subcommands.clone(),
            metadata: self.metadata.clone(),
        }
    }
}
impl<Context, Params, InheritedParams> std::fmt::Debug
    for ParentHandler<Context, Params, InheritedParams>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ParentHandler")
            .field(&self.subcommands)
            .finish()
    }
}

impl<Context: crate::Context, Params, InheritedParams>
    ParentHandler<Context, Params, InheritedParams>
{
    pub fn subcommand<C: crate::Context, H>(mut self, name: &'static str, handler: H) -> Self
    where
        WithContext<C, H>: Handler<Flat<Params, InheritedParams>>,
    {
        if let Some(h) = DynHandler::new(handler) {
            self.subcommands.insert(name.into(), h);
        }
        self
    }
    pub fn root_handler<C: crate::Context, H>(mut self, handler: H) -> Self
    where
        WithContext<C, H>: Handler<InheritedParams>,
        <WithContext<C, H> as Handler<InheritedParams>>::H: HandlerTypes<Params = Params>,
    {
        if let Some(h) = DynHandler::new(handler) {
            self.subcommands.set_root(h);
        }
        self
    }
}

impl<Context, Params, InheritedParams> HandlerTypes
    for ParentHandler<Context, Params, InheritedParams>
where
    Params: Send + Sync,
    InheritedParams: Send + Sync,
{
    type Params = Params;
    type InheritedParams = InheritedParams;
    type Ok = Value;
    type Err = RpcError;
}

impl<Context, Params, InheritedParams> HandlerFor<Context>
    for ParentHandler<Context, Params, InheritedParams>
where
    Self: HandlerRequires<
        Params = Params,
        InheritedParams = InheritedParams,
        Ok = Value,
        Err = RpcError,
    >,
    Context: crate::Context,
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
{
    fn handle_sync(
        &self,
        HandlerArgs {
            context,
            mut parent_method,
            mut method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let cmd = method.pop_front();
        if let Some(cmd) = cmd {
            parent_method.push_back(cmd);
            if let Some((_, sub_handler)) = &self.subcommands.get(cmd) {
                sub_handler.handle_sync(HandleAnyArgs {
                    context,
                    parent_method,
                    method,
                    params: raw_params,
                    inherited: Flat(params, inherited_params),
                })
            } else {
                Err(yajrc::METHOD_NOT_FOUND_ERROR)
            }
        } else {
            if let Some(sub_handler) = &self.subcommands.get_root() {
                sub_handler.handle_sync(HandleAnyArgs {
                    context,
                    parent_method,
                    method,
                    params: raw_params,
                    inherited: inherited_params,
                })
            } else {
                Err(yajrc::METHOD_NOT_FOUND_ERROR)
            }
        }
    }
    async fn handle_async(
        &self,
        HandlerArgs {
            context,
            mut parent_method,
            mut method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Context, Self>,
    ) -> Result<Self::Ok, Self::Err> {
        let cmd = method.pop_front();
        if let Some(cmd) = cmd {
            parent_method.push_back(cmd);
            if let Some((_, sub_handler)) = &self.subcommands.get(cmd) {
                sub_handler
                    .handle_async(HandleAnyArgs {
                        context,
                        parent_method,
                        method,
                        params: raw_params,
                        inherited: Flat(params, inherited_params),
                    })
                    .await
            } else {
                Err(yajrc::METHOD_NOT_FOUND_ERROR)
            }
        } else {
            if let Some(sub_handler) = &self.subcommands.get_root() {
                sub_handler
                    .handle_async(HandleAnyArgs {
                        context,
                        parent_method,
                        method,
                        params: raw_params,
                        inherited: inherited_params,
                    })
                    .await
            } else {
                Err(yajrc::METHOD_NOT_FOUND_ERROR)
            }
        }
    }
    fn metadata(&self, mut method: VecDeque<&'static str>) -> OrdMap<&'static str, Value> {
        let metadata = self.metadata.clone();
        if let Some(cmd) = method.pop_front() {
            if let Some((_, handler)) = self.subcommands.get(cmd) {
                handler.metadata(method).union(metadata)
            } else {
                metadata
            }
        } else {
            if let Some(handler) = self.subcommands.get_root() {
                handler.metadata(method).union(metadata)
            } else {
                metadata
            }
        }
    }
    fn method_from_dots(&self, method: &str) -> Option<VecDeque<&'static str>> {
        let (head, tail) = if method.is_empty() {
            (None, None)
        } else {
            method
                .split_once(".")
                .map(|(head, tail)| (Some(head), Some(tail)))
                .unwrap_or((Some(method), None))
        };
        if let Some(head) = head {
            let (Name(name), h) = self.subcommands.get(head)?;
            let mut res = VecDeque::new();
            res.push_back(name);
            if let Some(tail) = tail {
                res.append(&mut h.method_from_dots(tail)?);
            }
            Some(res)
        } else {
            let h = self.subcommands.get_root()?;
            let mut res = VecDeque::new();
            if let Some(tail) = tail {
                res.append(&mut h.method_from_dots(tail)?);
            }
            Some(res)
        }
    }
}

impl<Context, Params, InheritedParams> CliBindings<Context>
    for ParentHandler<Context, Params, InheritedParams>
where
    Context: crate::Context,
    Params: FromArgMatches + CommandFactory + Serialize + Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
{
    fn cli_command(&self) -> Command {
        let mut base = if let Some(cli) = &self.subcommands.0.as_ref().and_then(|h| h.cli()) {
            cli.cli_command().subcommand_required(false)
        } else {
            Params::command().subcommand_required(true)
        };
        for (name, handler) in &self.subcommands.1 {
            match (name, handler.cli()) {
                (Name(name), Some(cli)) => {
                    base = base.subcommand(cli.cli_command().name(name));
                }
                _ => (),
            }
        }
        base
    }
    fn cli_parse(
        &self,
        root_matches: &ArgMatches,
    ) -> Result<(VecDeque<&'static str>, Value), clap::Error> {
        let (name, matches) = match root_matches.subcommand() {
            Some((name, matches)) => (Some(name), matches),
            None => (None, root_matches),
        };
        if let Some(name) = name {
            let root_params = imbl_value::to_value(&Params::from_arg_matches(root_matches)?)
                .map_err(|e| clap::Error::raw(clap::error::ErrorKind::ValueValidation, e))?;
            if let Some((Name(name), cli)) = self
                .subcommands
                .get(name)
                .and_then(|(n, h)| h.cli().map(|c| (n, c)))
            {
                let (mut method, params) = cli.cli_parse(matches)?;
                method.push_front(name);

                Ok((
                    method,
                    combine(root_params, params).map_err(|e| {
                        clap::Error::raw(clap::error::ErrorKind::ArgumentConflict, e)
                    })?,
                ))
            } else {
                Ok((VecDeque::new(), root_params))
            }
        } else {
            if let Some(cli) = self.subcommands.get_root().and_then(|h| h.cli()) {
                let (method, params) = cli.cli_parse(matches)?;

                Ok((method, params))
            } else {
                let root_params = imbl_value::to_value(&Params::from_arg_matches(matches)?)
                    .map_err(|e| clap::Error::raw(clap::error::ErrorKind::ValueValidation, e))?;
                Ok((VecDeque::new(), root_params))
            }
        }
    }
    fn cli_display(
        &self,
        HandlerArgs {
            context,
            mut parent_method,
            mut method,
            params,
            inherited_params,
            raw_params,
        }: HandlerArgsFor<Context, Self>,
        result: Self::Ok,
    ) -> Result<(), Self::Err> {
        let cmd = method.pop_front();
        if let Some(cmd) = cmd {
            parent_method.push_back(cmd);
            if let Some((_, cli)) = self
                .subcommands
                .get(cmd)
                .and_then(|(n, h)| h.cli().map(|c| (n, c)))
            {
                cli.cli_display(
                    HandleAnyArgs {
                        context,
                        parent_method,
                        method,
                        params: raw_params,
                        inherited: Flat(params, inherited_params),
                    },
                    result,
                )
            } else {
                Err(yajrc::METHOD_NOT_FOUND_ERROR)
            }
        } else {
            if let Some(cli) = self.subcommands.get_root().and_then(|h| h.cli()) {
                cli.cli_display(
                    HandleAnyArgs {
                        context,
                        parent_method,
                        method,
                        params: raw_params,
                        inherited: inherited_params,
                    },
                    result,
                )
            } else {
                Err(yajrc::METHOD_NOT_FOUND_ERROR)
            }
        }
    }
}
