//! Typed JSON-RPC method-tree modules from directional serde shapes.

use std::ops::Deref;
use std::sync::Arc;

pub use visit_rs::ts::*;
pub use visit_rs::{impl_ts, impl_ts_array, impl_ts_map, impl_ts_shape};
use visit_rs::{Static, Visit, Visitor};

use crate::{Adapter, FromFn, FromFnAsync, FromFnAsyncLocal, HandlerTypes, ParentHandler};

/// TypeScript method-path inference helpers included in every generated module.
pub fn type_helpers() -> &'static str {
    include_str!("./type-helpers.ts")
}

/// Appends a binding expression in the visitor's selected direction.
pub type TSWriter<'a> = Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a>;

pub fn intersection_writer<'a>(first: TSWriter<'a>, second: TSWriter<'a>) -> TSWriter<'a> {
    Box::new(move |visitor| visitor.intersection(&first, &second))
}

pub fn type_writer<T>() -> TSWriter<'static>
where
    Static<T>: Visit<TSVisitor>,
{
    Box::new(|visitor| Static::<T>::new().visit(visitor))
}

/// Supplies the handler's own input shape, excluding inherited parameters.
pub trait ParamsTS {
    fn params_ts<'a>(&'a self) -> TSWriter<'a>;
}
/// Supplies an output shape for callable handlers; namespaces return `None`.
pub trait ReturnTS {
    fn return_ts<'a>(&'a self) -> Option<TSWriter<'a>>;
}
/// Supplies the child method map and preserves binding opt-outs across adapters.
pub trait ChildrenTS {
    fn ts_enabled(&self) -> bool {
        true
    }
    fn children_ts<'a>(&'a self) -> Option<TSWriter<'a>>;
}

/// Composes parameter, result and child bindings unless the handler opts out.
pub trait HandlerTSBindings: ParamsTS + ReturnTS + ChildrenTS {
    fn get_ts<'a>(&'a self) -> Option<HandlerTS<'a>>;
}
impl<T: ParamsTS + ReturnTS + ChildrenTS> HandlerTSBindings for T {
    fn get_ts<'a>(&'a self) -> Option<HandlerTS<'a>> {
        self.ts_enabled().then(|| HandlerTS::new(self))
    }
}
impl<T: ParamsTS> ParamsTS for Arc<T> {
    fn params_ts(&self) -> Box<dyn Fn(&mut TSVisitor) + Send + Sync + '_> {
        self.deref().params_ts()
    }
}
impl<T: ReturnTS> ReturnTS for Arc<T> {
    fn return_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync + '_>> {
        self.deref().return_ts()
    }
}
impl<T: ChildrenTS> ChildrenTS for Arc<T> {
    fn ts_enabled(&self) -> bool {
        self.deref().ts_enabled()
    }
    fn children_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync + '_>> {
        self.deref().children_ts()
    }
}
impl<C, I> ParamsTS for crate::DynHandler<C, I> {
    fn params_ts(&self) -> Box<dyn Fn(&mut TSVisitor) + Send + Sync + '_> {
        self.0.params_ts()
    }
}
impl<C, I> ReturnTS for crate::DynHandler<C, I> {
    fn return_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync + '_>> {
        self.0.return_ts()
    }
}
impl<C, I> ChildrenTS for crate::DynHandler<C, I> {
    fn ts_enabled(&self) -> bool {
        self.0.ts_enabled()
    }
    fn children_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync + '_>> {
        self.0.children_ts()
    }
}

/// Borrowed writers for one handler's method-tree node.
pub struct HandlerTS<'a> {
    params_ts: TSWriter<'a>,
    return_ts: Option<TSWriter<'a>>,
    children: Option<TSWriter<'a>>,
}
impl<'a> HandlerTS<'a> {
    pub fn new<H>(handler: &'a H) -> Self
    where
        H: ParamsTS + ReturnTS + ChildrenTS,
    {
        Self {
            params_ts: handler.params_ts(),
            return_ts: handler.return_ts(),
            children: handler.children_ts(),
        }
    }
}
impl<'a> Visit<TSVisitor> for HandlerTS<'a> {
    fn visit(&self, visitor: &mut TSVisitor) -> <TSVisitor as Visitor>::Result {
        visitor.ts.push_str("{_PARAMS:");
        visitor.with_direction(Direction::Input, |visitor| (self.params_ts)(visitor));
        if let Some(return_ty) = &self.return_ts {
            visitor.ts.push_str(";_RETURN:");
            visitor.with_direction(Direction::Output, |visitor| return_ty(visitor));
        }
        if let Some(children) = &self.children {
            visitor.ts.push_str(";_CHILDREN:");
            children(visitor);
        }
        visitor.ts.push('}');
    }
}

impl<F, T, E, Args> ParamsTS for FromFn<F, T, E, Args>
where
    Self: HandlerTypes,
    Static<<Self as HandlerTypes>::Params>: Visit<TSVisitor>,
{
    fn params_ts(&self) -> Box<dyn Fn(&mut TSVisitor) + Send + Sync> {
        type_writer::<<Self as HandlerTypes>::Params>()
    }
}
impl<F, Fut, T, E, Args> ParamsTS for FromFnAsync<F, Fut, T, E, Args>
where
    Self: HandlerTypes,
    Static<<Self as HandlerTypes>::Params>: Visit<TSVisitor>,
{
    fn params_ts(&self) -> Box<dyn Fn(&mut TSVisitor) + Send + Sync> {
        type_writer::<<Self as HandlerTypes>::Params>()
    }
}
impl<F, Fut, T, E, Args> ParamsTS for FromFnAsyncLocal<F, Fut, T, E, Args>
where
    Self: HandlerTypes,
    Static<<Self as HandlerTypes>::Params>: Visit<TSVisitor>,
{
    fn params_ts(&self) -> Box<dyn Fn(&mut TSVisitor) + Send + Sync> {
        type_writer::<<Self as HandlerTypes>::Params>()
    }
}
impl<Context, Params, InheritedParams> ParamsTS for ParentHandler<Context, Params, InheritedParams>
where
    Self: HandlerTypes,
    Static<<Self as HandlerTypes>::Params>: Visit<TSVisitor>,
{
    fn params_ts(&self) -> Box<dyn Fn(&mut TSVisitor) + Send + Sync> {
        type_writer::<<Self as HandlerTypes>::Params>()
    }
}

impl<F, T, E, Args> ReturnTS for FromFn<F, T, E, Args>
where
    Self: HandlerTypes,
    Static<<Self as HandlerTypes>::Ok>: Visit<TSVisitor>,
{
    fn return_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync>> {
        Some(type_writer::<<Self as HandlerTypes>::Ok>())
    }
}
impl<F, Fut, T, E, Args> ReturnTS for FromFnAsync<F, Fut, T, E, Args>
where
    Self: HandlerTypes,
    Static<<Self as HandlerTypes>::Ok>: Visit<TSVisitor>,
{
    fn return_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync>> {
        Some(type_writer::<<Self as HandlerTypes>::Ok>())
    }
}
impl<F, Fut, T, E, Args> ReturnTS for FromFnAsyncLocal<F, Fut, T, E, Args>
where
    Self: HandlerTypes,
    Static<<Self as HandlerTypes>::Ok>: Visit<TSVisitor>,
{
    fn return_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync>> {
        Some(type_writer::<<Self as HandlerTypes>::Ok>())
    }
}
impl<Context, Params, InheritedParams> ReturnTS
    for ParentHandler<Context, Params, InheritedParams>
{
    fn return_ts<'a>(&'a self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a>> {
        match self.subcommands.0.as_ref()?.get_ts() {
            Some(ts) => ts.return_ts,
            None => Some(Box::new(|v| v.ts.push_str("unknown"))),
        }
    }
}

impl<F, T, E, Args> ChildrenTS for FromFn<F, T, E, Args> {
    fn children_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync>> {
        None
    }
}
impl<F, Fut, T, E, Args> ChildrenTS for FromFnAsync<F, Fut, T, E, Args> {
    fn children_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync>> {
        None
    }
}
impl<F, Fut, T, E, Args> ChildrenTS for FromFnAsyncLocal<F, Fut, T, E, Args> {
    fn children_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync>> {
        None
    }
}
impl<Context, Params, InheritedParams> ChildrenTS
    for ParentHandler<Context, Params, InheritedParams>
where
    Context: crate::Context,
    Params: Send + Sync + 'static,
    InheritedParams: Send + Sync + 'static,
{
    fn children_ts<'a>(&'a self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a>> {
        use std::fmt::Write;

        Some(Box::new(move |visitor| {
            visitor.ts.push('{');
            for (name, handler) in &self.subcommands.1 {
                write!(
                    &mut visitor.ts,
                    "{}:",
                    serde_json::to_string(&name.0).unwrap()
                )
                .ok();
                if let Some(ts) = handler.0.get_ts() {
                    ts.visit(visitor);
                } else {
                    visitor.ts.push_str("{_PARAMS:unknown;_RETURN:unknown}");
                }
                visitor.ts.push(';');
            }
            visitor.ts.push('}');
        }))
    }
}

/// Forwards an adapter's parameter bindings unchanged.
pub trait PassthroughParamsTS: Adapter {}
impl<T: PassthroughParamsTS> ParamsTS for T
where
    T::Inner: ParamsTS,
{
    fn params_ts<'a>(&'a self) -> Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a> {
        self.as_inner().params_ts()
    }
}

/// Forwards an adapter's result bindings unchanged.
pub trait PassthroughReturnTS: Adapter {}
impl<T: PassthroughReturnTS> ReturnTS for T
where
    T::Inner: ReturnTS,
{
    fn return_ts<'a>(&'a self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a>> {
        self.as_inner().return_ts()
    }
}

/// Forwards an adapter's child bindings and binding opt-out unchanged.
pub trait PassthroughChildrenTS: Adapter {}
impl<T: PassthroughChildrenTS> ChildrenTS for T
where
    T::Inner: ChildrenTS,
{
    fn ts_enabled(&self) -> bool {
        self.as_inner().ts_enabled()
    }
    fn children_ts<'a>(&'a self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a>> {
        self.as_inner().children_ts()
    }
}

/// Generates a module for the handler tree; an opted-out root returns `None`.
pub fn handler_bindings<H: HandlerTSBindings>(
    handler: &H,
    root_name: &str,
) -> Result<Option<String>, BindingError> {
    let Some(ts) = handler.get_ts() else {
        return Ok(None);
    };
    let mut visitor = TSVisitor::new();
    visitor.reserve(type_helpers().lines().filter_map(|line| {
        line.strip_prefix("export type ").and_then(|declaration| {
            declaration
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .next()
        })
    }));
    ts.visit(&mut visitor);
    visitor
        .into_module(root_name)
        .map(|module| Some(format!("{}\n{module}", type_helpers())))
}

impl<A: TS, B: TS> TS for crate::util::Flat<A, B> {
    fn visit_ts(visitor: &mut TSVisitor) {
        visitor.intersection(TSVisitor::append_type::<A>, TSVisitor::append_type::<B>);
    }
}
