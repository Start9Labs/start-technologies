//! Typed JSON-RPC method-tree modules from directional serde shapes.
//!
//! `handler_bindings` includes method inference helpers and registered definitions.
//! Implement `TS` for custom JSON representations or use `impl_ts_shape!` with
//! `visit_rs::SerdeShape`. Input aliases receive an `Input` suffix. Generation
//! errors reject unsupported shapes, anonymous recursion and conflicting names.

use std::collections::BTreeMap;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use imbl_value::imbl::{OrdMap, Vector};
use imbl_value::InternedString;
pub use visit_rs::shape::Direction;
use visit_rs::shape::{Field, SerdeShape, ShapeVisitor, Style, Tag, Variant};
use visit_rs::{Static, Visit, Visitor};

use crate::{Adapter, FromFn, FromFnAsync, FromFnAsyncLocal, HandlerTypes, ParentHandler};

/// TypeScript method-path inference helpers included in every generated module.
pub fn type_helpers() -> &'static str {
    include_str!("./type-helpers.ts")
}

/// Appends the JSON expression for the visitor's selected direction.
///
/// Nested types must use `TSVisitor::append_type` to register named definitions.
pub trait TS {
    /// Base alias name; `None` emits inline. Recursive types require a name.
    const DEFINE: Option<&str> = None;
    /// Marks nullable options whose named input fields may be omitted or flattened.
    const IS_OPTION: bool = false;
    fn visit_ts(visitor: &mut TSVisitor);
}

impl<T> Visit<TSVisitor> for Static<T>
where
    T: TS,
{
    fn visit(&self, visitor: &mut TSVisitor) -> <TSVisitor as Visitor>::Result {
        visitor.append_type::<T>();
        visitor.type_optional = T::IS_OPTION;
    }
}

/// Supplies the handler's own input shape, excluding inherited parameters.
pub trait ParamsTS {
    fn params_ts<'a>(&'a self) -> Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a>;
}
/// Supplies an output shape for callable handlers; namespaces return `None`.
pub trait ReturnTS {
    fn return_ts<'a>(&'a self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a>>;
}
/// Supplies the child method map and preserves binding opt-outs across adapters.
pub trait ChildrenTS {
    fn ts_enabled(&self) -> bool {
        true
    }
    fn children_ts<'a>(&'a self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a>>;
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
    params_ts: Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a>,
    return_ts: Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a>>,
    children: Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync + 'a>>,
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
        visitor.ts.push_str("}");
    }
}

impl<F, T, E, Args> ParamsTS for FromFn<F, T, E, Args>
where
    Self: HandlerTypes,
    Static<<Self as HandlerTypes>::Params>: Visit<TSVisitor>,
{
    fn params_ts(&self) -> Box<dyn Fn(&mut TSVisitor) + Send + Sync> {
        Box::new(|visitor| Static::<<Self as HandlerTypes>::Params>::new().visit(visitor))
    }
}
impl<F, Fut, T, E, Args> ParamsTS for FromFnAsync<F, Fut, T, E, Args>
where
    Self: HandlerTypes,
    Static<<Self as HandlerTypes>::Params>: Visit<TSVisitor>,
{
    fn params_ts(&self) -> Box<dyn Fn(&mut TSVisitor) + Send + Sync> {
        Box::new(|visitor| Static::<<Self as HandlerTypes>::Params>::new().visit(visitor))
    }
}
impl<F, Fut, T, E, Args> ParamsTS for FromFnAsyncLocal<F, Fut, T, E, Args>
where
    Self: HandlerTypes,
    Static<<Self as HandlerTypes>::Params>: Visit<TSVisitor>,
{
    fn params_ts(&self) -> Box<dyn Fn(&mut TSVisitor) + Send + Sync> {
        Box::new(|visitor| Static::<<Self as HandlerTypes>::Params>::new().visit(visitor))
    }
}
impl<Context, Params, InheritedParams> ParamsTS for ParentHandler<Context, Params, InheritedParams>
where
    Self: HandlerTypes,
    Static<<Self as HandlerTypes>::Params>: Visit<TSVisitor>,
{
    fn params_ts(&self) -> Box<dyn Fn(&mut TSVisitor) + Send + Sync> {
        Box::new(|visitor| Static::<<Self as HandlerTypes>::Params>::new().visit(visitor))
    }
}

impl<F, T, E, Args> ReturnTS for FromFn<F, T, E, Args>
where
    Self: HandlerTypes,
    Static<<Self as HandlerTypes>::Ok>: Visit<TSVisitor>,
{
    fn return_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync>> {
        Some(Box::new(|visitor| {
            Static::<<Self as HandlerTypes>::Ok>::new().visit(visitor)
        }))
    }
}
impl<F, Fut, T, E, Args> ReturnTS for FromFnAsync<F, Fut, T, E, Args>
where
    Self: HandlerTypes,
    Static<<Self as HandlerTypes>::Ok>: Visit<TSVisitor>,
{
    fn return_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync>> {
        Some(Box::new(|visitor| {
            Static::<<Self as HandlerTypes>::Ok>::new().visit(visitor)
        }))
    }
}
impl<F, Fut, T, E, Args> ReturnTS for FromFnAsyncLocal<F, Fut, T, E, Args>
where
    Self: HandlerTypes,
    Static<<Self as HandlerTypes>::Ok>: Visit<TSVisitor>,
{
    fn return_ts(&self) -> Option<Box<dyn Fn(&mut TSVisitor) + Send + Sync>> {
        Some(Box::new(|visitor| {
            Static::<<Self as HandlerTypes>::Ok>::new().visit(visitor)
        }))
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

/// Accumulates one root expression, its named definitions and generation errors.
#[derive(Debug, Clone)]
pub struct TSVisitor {
    definitions: BTreeMap<String, String>,
    /// Current expression buffer for custom `TS` implementations.
    pub ts: String,
    direction: Direction,
    owners: BTreeMap<String, (&'static str, Direction)>,
    active: Vec<(&'static str, Direction)>,
    fields: Vec<(Field, String, bool)>,
    variants: Vec<String>,
    type_optional: bool,
    errors: Vec<String>,
}

impl Default for TSVisitor {
    fn default() -> Self {
        Self::new()
    }
}

/// A rejected JSON shape, recursive inline type or declaration-name collision.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct BindingError(String);

impl TSVisitor {
    /// Starts with output traversal and an empty expression/definition registry.
    pub fn new() -> Self {
        Self {
            definitions: BTreeMap::new(),
            ts: String::new(),
            direction: Direction::Output,
            owners: BTreeMap::new(),
            active: Vec::new(),
            fields: Vec::new(),
            variants: Vec::new(),
            type_optional: false,
            errors: Vec::new(),
        }
    }

    /// Selects a direction for the callback, restoring it afterward.
    pub fn with_direction(&mut self, direction: Direction, visit: impl FnOnce(&mut Self)) {
        let previous = std::mem::replace(&mut self.direction, direction);
        visit(self);
        self.direction = previous;
    }

    /// Wire direction selected for the expression being visited.
    pub fn direction(&self) -> Direction {
        self.direction
    }

    /// Appends an inline expression or registers and references a named definition.
    pub fn append_type<T: TS + ?Sized>(&mut self) {
        let owner = (std::any::type_name::<T>(), self.direction);
        if let Some(define) = T::DEFINE {
            let name = match self.direction {
                Direction::Input => format!("{define}Input"),
                Direction::Output => define.to_owned(),
            };
            if !valid_identifier(&name) {
                self.errors
                    .push(format!("Invalid TypeScript definition name: {name}"));
                return;
            }
            self.ts.push_str(&name);
            if let Some(existing) = self.owners.get(&name) {
                if *existing != owner {
                    self.errors
                        .push(format!("Conflicting TypeScript definition: {name}"));
                }
                return;
            }
            self.owners.insert(name.clone(), owner);
            let start = self.ts.len();
            T::visit_ts(self);
            let definition = self.ts.split_off(start);
            self.definitions.insert(name, definition);
        } else if self.active.contains(&owner) {
            self.errors
                .push(format!("Recursive inline type {} requires DEFINE", owner.0));
        } else {
            self.active.push(owner);
            T::visit_ts(self);
            self.active.pop();
        }
    }

    fn capture(&mut self, visit: impl FnOnce(&mut Self)) -> String {
        let start = self.ts.len();
        visit(self);
        self.ts.split_off(start)
    }

    fn payload(&mut self, style: Style, fields: impl FnOnce(&mut Self)) -> String {
        let saved = std::mem::take(&mut self.fields);
        fields(self);
        let fields = std::mem::replace(&mut self.fields, saved);
        match style {
            Style::Unit => "null".into(),
            Style::Newtype if fields.is_empty() => match self.direction {
                Direction::Input => "unknown".into(),
                Direction::Output => "null".into(),
            },
            Style::Newtype => {
                if fields.len() != 1 {
                    self.errors
                        .push("A newtype must have one visible field".into());
                }
                fields
                    .into_iter()
                    .next()
                    .map(|(_, ty, _)| ty)
                    .unwrap_or_else(|| "never".into())
            }
            Style::Tuple => {
                if self.direction == Direction::Output && fields.iter().any(|(f, _, _)| f.optional)
                {
                    self.errors
                        .push("Conditional tuple fields require a TypeScript override".into());
                }
                let mut optional_seen = false;
                let elements: Vec<_> = fields.into_iter().map(|(f, ty, _)| {
                    let optional = self.direction == Direction::Input && f.optional;
                    if optional_seen && !optional {
                        self.errors.push("A required tuple field after an optional field requires a TypeScript override".into());
                    }
                    optional_seen |= optional;
                    if optional { format!("({ty})?") } else { ty }
                }).collect();
                format!("[{}]", elements.join(","))
            }
            Style::Struct => {
                let mut object = String::from("{");
                let mut intersections = Vec::new();
                for (field, ty, option) in fields {
                    if field.flatten {
                        intersections.push(if option {
                            format!("Partial<Exclude<({ty}),null>>")
                        } else {
                            format!("({ty})")
                        });
                        continue;
                    }
                    let optional = field.optional || (self.direction == Direction::Input && option);
                    let mut names = vec![field.name];
                    names.extend(field.aliases.iter().copied());
                    names.sort_unstable();
                    names.dedup();
                    if names.len() > 1 && !optional {
                        intersections.push(format!(
                            "({})",
                            names
                                .iter()
                                .map(|name| format!("{{{}:({ty})}}", json_str(name)))
                                .collect::<Vec<_>>()
                                .join("|")
                        ));
                    }
                    for name in &names {
                        object.push_str(&format!(
                            "{}{}:({ty});",
                            json_str(name),
                            if optional || names.len() > 1 { "?" } else { "" }
                        ));
                    }
                }
                object.push('}');
                for inner in intersections {
                    object.push('&');
                    object.push_str(&inner);
                }
                object
            }
        }
    }

    /// Emits helpers, definitions and the root alias, or accumulated generation errors.
    pub fn into_module(self, root_name: &str) -> Result<String, BindingError> {
        if !self.errors.is_empty() {
            return Err(BindingError(self.errors.join("\n")));
        }
        if !valid_identifier(root_name) || self.definitions.contains_key(root_name) {
            return Err(BindingError(format!(
                "Invalid or conflicting root name: {root_name}"
            )));
        }
        let mut module = type_helpers().to_owned();
        module.push('\n');
        for (name, ty) in self.definitions {
            module.push_str(&format!("export type {name} = {ty};\n"));
        }
        module.push_str(&format!("export type {root_name} = {};\n", self.ts));
        Ok(module)
    }
}

fn valid_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'A'..=b'Z'))
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !matches!(
            name,
            "RpcHandler" | "RpcParamType" | "RpcReturnType" | "Partial" | "Exclude"
        )
}

fn json_str(value: &str) -> String {
    serde_json::to_string(value).unwrap()
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
    ts.visit(&mut visitor);
    visitor.into_module(root_name).map(Some)
}

impl Visitor for TSVisitor {
    type Result = ();
}

impl ShapeVisitor for TSVisitor {
    fn unsupported(&mut self, reason: &'static str) {
        self.errors.push(reason.to_owned());
    }
    fn structure(&mut self, style: Style, fields: impl FnOnce(&mut Self)) {
        let payload = self.payload(style, fields);
        self.ts.push_str(&payload);
    }

    fn enumeration(&mut self, variants: impl FnOnce(&mut Self)) {
        let saved = std::mem::take(&mut self.variants);
        variants(self);
        let variants = std::mem::replace(&mut self.variants, saved);
        self.ts
            .push_str(if variants.is_empty() { "never" } else { "(" });
        if !variants.is_empty() {
            self.ts.push_str(&variants.join("|"));
            self.ts.push(')');
        }
    }

    fn variant(&mut self, variant: Variant, fields: impl FnOnce(&mut Self)) {
        if variant.custom || variant.other {
            self.errors.push(format!(
                "Custom variant {} requires a TypeScript override",
                variant.name
            ));
        }
        let payload = self.payload(variant.style, fields);
        let mut names = vec![variant.name];
        names.extend(variant.aliases.iter().copied());
        names.sort_unstable();
        names.dedup();
        let names = names
            .into_iter()
            .map(json_str)
            .collect::<Vec<_>>()
            .join("|");
        let unit = variant.style == Style::Unit;
        let ty = match variant.tag {
            Tag::External if unit => names,
            Tag::External => {
                let mut variants = Vec::new();
                let mut names = vec![variant.name];
                names.extend(variant.aliases.iter().copied());
                names.sort_unstable();
                names.dedup();
                for name in names {
                    variants.push(format!("{{{}:({payload})}}", json_str(name)));
                }
                format!("({})", variants.join("|"))
            }
            Tag::Internal(tag) => {
                let tagged = format!("{{{}:({names})}}", json_str(tag));
                if unit {
                    tagged
                } else if variant.style == Style::Newtype {
                    format!("(({tagged}&Exclude<({payload}),null>)|(null extends ({payload})?{tagged}:never))")
                } else {
                    format!("{tagged}&({payload})")
                }
            }
            Tag::Adjacent(tag, content) => {
                if unit {
                    format!("{{{}:({names})}}", json_str(tag))
                } else {
                    format!(
                        "{{{}:({names});{}:({payload})}}",
                        json_str(tag),
                        json_str(content)
                    )
                }
            }
            Tag::Untagged => payload,
        };
        self.variants.push(ty);
    }

    fn field<T: ?Sized>(&mut self, field: Field)
    where
        Static<T>: Visit<Self>,
    {
        let ty = self.capture(|visitor| Static::<T>::new().visit(visitor));
        self.fields.push((field, ty, self.type_optional));
    }
}

pub struct LiteralTS(pub std::borrow::Cow<'static, str>);
impl Visit<TSVisitor> for LiteralTS {
    fn visit(&self, visitor: &mut TSVisitor) {
        visitor.ts.push_str(&self.0);
        visitor.type_optional = false;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Unknown;
#[derive(Debug, Clone, Copy)]
pub enum Never {}

#[macro_export]
macro_rules! impl_ts {
    ($($ty:ty),+ => $ts:expr) => { $(
        impl $crate::ts::TS for $ty {
            fn visit_ts(visitor: &mut $crate::ts::TSVisitor) { visitor.ts.push_str($ts); }
        }
    )+ };
}

impl_ts!(bool => "boolean");
impl_ts!(String, str, InternedString => "string");
impl_ts!(usize,u8,u16,u32,u64,u128,isize,i8,i16,i32,i64,i128 => "number");
impl_ts!(char => "string");
macro_rules! float {
    ($($ty:ty),*) => { $(impl TS for $ty {
        fn visit_ts(v: &mut TSVisitor) {
            v.ts.push_str(match v.direction {
                Direction::Input => "number",
                Direction::Output => "(number|null)",
            });
        }
    })* };
}
float!(f32, f64);
impl_ts!(Unknown,imbl_value::Value,serde_json::Value => "unknown");
#[cfg(feature = "cbor")]
impl_ts!(serde_cbor::Value => "unknown");
impl_ts!(Never => "never");
impl_ts!(() => "null");

pub fn visit_shape<T: SerdeShape<TSVisitor>>(visitor: &mut TSVisitor) {
    T::visit_shape(visitor, visitor.direction);
}

#[macro_export]
macro_rules! impl_ts_shape {
    ($ty:ty $({ define: $name:expr })?) => {
        impl $crate::ts::TS for $ty {
            $(const DEFINE: Option<&str> = Some($name);)?
            fn visit_ts(visitor: &mut $crate::ts::TSVisitor) { $crate::ts::visit_shape::<Self>(visitor); }
        }
    };
}

#[macro_export]
macro_rules! impl_ts_map {
    ($($ty:ty $(where [$($bounds:tt)*])?),+ $(,)?) => { $(
        impl<K, V> $crate::ts::TS for $ty
        where V: $crate::ts::TS, $($($bounds)*,)? {
            fn visit_ts(visitor: &mut $crate::ts::TSVisitor) {
                visitor.ts.push_str("{[key:string]:");
                visitor.append_type::<V>();
                visitor.ts.push('}');
            }
        }
    )+ };
}
impl_ts_map!(std::collections::HashMap<K,V>, imbl_value::imbl::HashMap<K,V>,
    imbl_value::InOMap<K,V> where [K: Eq + Clone, V: Clone], BTreeMap<K,V>, OrdMap<K,V>);

#[macro_export]
macro_rules! impl_ts_array {
    ($($ty:ty $(where [$($bounds:tt)*])?),+ $(,)?) => { $(
        impl<T> $crate::ts::TS for $ty
        where T: $crate::ts::TS, $($($bounds)*,)? {
            fn visit_ts(visitor: &mut $crate::ts::TSVisitor) {
                visitor.ts.push('('); visitor.append_type::<T>(); visitor.ts.push_str(")[]");
            }
        }
    )+ };
}
impl_ts_array!(Vec<T>, Vector<T>, [T]);

impl<T: TS, const N: usize> TS for [T; N] {
    fn visit_ts(v: &mut TSVisitor) {
        let ty = v.capture(|v| v.append_type::<T>());
        v.ts.push_str(&format!("[{}]", vec![ty; N].join(",")));
    }
}
macro_rules! tuple {
    ($($ty:ident),+) => {
        impl<$($ty: TS),+> TS for ($($ty,)+) {
            fn visit_ts(v: &mut TSVisitor) {
                let elements = [$(v.capture(|v| v.append_type::<$ty>())),+];
                v.ts.push_str(&format!("[{}]", elements.join(",")));
            }
        }
    };
}
tuple!(A);
tuple!(A, B);
tuple!(A, B, C);
tuple!(A, B, C, D);
tuple!(A, B, C, D, E);
tuple!(A, B, C, D, E, F);
tuple!(A, B, C, D, E, F, G);
tuple!(A, B, C, D, E, F, G, H);
tuple!(A, B, C, D, E, F, G, H, I);
tuple!(A, B, C, D, E, F, G, H, I, J);
tuple!(A, B, C, D, E, F, G, H, I, J, K);
tuple!(A, B, C, D, E, F, G, H, I, J, K, L);
tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M);
tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N);
tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);

macro_rules! pointer {
    ($($ty:ident),*) => { $(
        impl<T: TS + ?Sized> TS for $ty<T> {
            const IS_OPTION: bool = T::IS_OPTION;
            fn visit_ts(visitor: &mut TSVisitor) { visitor.append_type::<T>(); }
        }
    )* };
}
pointer!(Box, Arc, Rc);
impl<T: TS + ?Sized> TS for &T {
    const IS_OPTION: bool = T::IS_OPTION;
    fn visit_ts(v: &mut TSVisitor) {
        v.append_type::<T>();
    }
}
impl<T: TS + std::borrow::ToOwned + ?Sized> TS for std::borrow::Cow<'_, T> {
    const IS_OPTION: bool = T::IS_OPTION;
    fn visit_ts(v: &mut TSVisitor) {
        v.append_type::<T>();
    }
}

impl<T: TS> TS for Option<T> {
    const IS_OPTION: bool = true;
    fn visit_ts(visitor: &mut TSVisitor) {
        visitor.ts.push('(');
        visitor.append_type::<T>();
        visitor.ts.push_str("|null)");
    }
}
impl<A: TS, B: TS> TS for crate::util::Flat<A, B> {
    fn visit_ts(visitor: &mut TSVisitor) {
        visitor.ts.push('(');
        visitor.append_type::<A>();
        visitor.ts.push_str(")&(");
        visitor.append_type::<B>();
        visitor.ts.push(')');
    }
}
