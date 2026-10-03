//! TypeScript declarations from directional serde JSON shapes.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

pub use inventory;
pub use visit_rs_derive::TS;

pub use crate::shape::Direction;

/// An annotated standalone type root in a compiled crate's export namespace.
pub struct Export {
    pub module: &'static str,
    pub namespace: &'static str,
    pub name: &'static str,
    pub register: fn(&mut TSVisitor),
}
inventory::collect!(Export);

/// Collects annotated roots and their dependencies into one checked declaration module.
pub fn export_namespace(crate_name: &str, namespace: &str) -> Result<String, BindingError> {
    let prefix = format!("{crate_name}::");
    let mut exports: Vec<_> = inventory::iter::<Export>
        .into_iter()
        .filter(|export| {
            (export.module == crate_name || export.module.starts_with(&prefix))
                && export.namespace == namespace
        })
        .collect();
    exports.sort_by_key(|export| (export.name, export.module));
    let mut visitor = TSVisitor::new();
    for direction in [Direction::Input, Direction::Output] {
        visitor.with_direction(direction, |visitor| {
            for export in &exports {
                (export.register)(visitor);
            }
        });
    }
    visitor.into_declarations()
}
use crate::shape::{Field, SerdeShape, ShapeVisitor, Style, Tag, Variant};
use crate::{Static, Visit, Visitor};

/// Appends the JSON expression for the visitor's selected direction.
///
/// Nested types must use `TSVisitor::append_type` to register named definitions.
pub trait TS {
    /// Base alias name; `None` emits inline. Recursive types require a name.
    const DEFINE: Option<&'static str> = None;
    /// Explicit input alias; `None` appends `Input` to the base alias.
    const INPUT_DEFINE: Option<&'static str> = None;
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

/// Accumulates one root expression, its named definitions and generation errors.
#[derive(Debug, Clone)]
pub struct TSVisitor {
    definitions: BTreeMap<String, Definition>,
    /// Current expression buffer for custom `TS` implementations.
    pub ts: String,
    direction: Direction,
    active: Vec<(&'static str, Direction)>,
    fields: Vec<(Field, String, bool)>,
    variants: Vec<String>,
    type_optional: bool,
    errors: Vec<String>,
    reserved: BTreeSet<String>,
}

#[derive(Debug, Clone)]
struct Definition {
    owner: (&'static str, Direction),
    expression: Option<String>,
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
            active: Vec::new(),
            fields: Vec::new(),
            variants: Vec::new(),
            type_optional: false,
            errors: Vec::new(),
            reserved: BTreeSet::new(),
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
        if let Some((name, _)) = self
            .definitions
            .iter()
            .find(|(_, definition)| definition.owner == owner)
        {
            self.ts.push_str(name);
        } else if let Some(define) = T::DEFINE {
            self.declare::<T>(define);
        } else if self.active.contains(&owner) {
            self.errors
                .push(format!("Recursive inline type {} requires DEFINE", owner.0));
        } else {
            self.active.push(owner);
            T::visit_ts(self);
            self.active.pop();
        }
    }

    /// Registers a named concrete type, including recursive generic instances.
    pub fn declare<T: TS + ?Sized>(&mut self, name: &str) {
        let name = match self.direction {
            Direction::Input => T::INPUT_DEFINE
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{name}Input")),
            Direction::Output => name.to_owned(),
        };
        let owner = (std::any::type_name::<T>(), self.direction);
        if !valid_identifier(&name) || self.reserved.contains(&name) {
            self.errors
                .push(format!("Invalid TypeScript definition name: {name}"));
            return;
        }
        self.ts.push_str(&name);
        if let Some(existing) = self.definitions.get(&name) {
            if existing.owner != owner {
                self.errors
                    .push(format!("Conflicting TypeScript definition: {name}"));
            }
            return;
        }
        self.definitions.insert(
            name.clone(),
            Definition {
                owner,
                expression: None,
            },
        );
        let expression = self.capture(T::visit_ts);
        self.definitions.get_mut(&name).unwrap().expression = Some(expression);
    }

    /// Appends the intersection of two object projections.
    pub fn intersection(&mut self, left: impl FnOnce(&mut Self), right: impl FnOnce(&mut Self)) {
        self.ts.push('(');
        left(self);
        self.ts.push_str(")&(");
        right(self);
        self.ts.push(')');
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
                    let optional = field.optional
                        || (self.direction == Direction::Input && option && field.option_default);
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

    /// Reserves names supplied by the module's other declarations.
    pub fn reserve(&mut self, names: impl IntoIterator<Item = impl AsRef<str>>) {
        self.reserved
            .extend(names.into_iter().map(|name| name.as_ref().to_owned()));
    }

    /// Emits registered type declarations, rejecting accumulated generation errors.
    pub fn into_declarations(self) -> Result<String, BindingError> {
        if !self.errors.is_empty() {
            return Err(BindingError(self.errors.join("\n")));
        }
        Ok(self
            .definitions
            .into_iter()
            .map(|(name, definition)| {
                format!("export type {name} = {};\n", definition.expression.unwrap())
            })
            .collect())
    }

    /// Emits registered declarations and a named root expression.
    pub fn into_module(mut self, root_name: &str) -> Result<String, BindingError> {
        if !valid_identifier(root_name)
            || self.reserved.contains(root_name)
            || self.definitions.contains_key(root_name)
            || self.ts.is_empty()
        {
            self.errors
                .push(format!("Invalid or conflicting root name: {root_name}"));
        }
        self.definitions.insert(
            root_name.to_owned(),
            Definition {
                owner: ("", self.direction),
                expression: Some(std::mem::take(&mut self.ts)),
            },
        );
        self.into_declarations()
    }
}

fn valid_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'A'..=b'Z'))
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !matches!(name, "Partial" | "Exclude")
}

fn json_str(value: &str) -> String {
    serde_json::to_string(value).unwrap()
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
                    format!(
                        "(({tagged}&Exclude<({payload}),null>)|(null extends ({payload})?{tagged}:never))"
                    )
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
impl_ts!(String, str => "string");
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
impl_ts!(Unknown,serde_json::Value => "unknown");
#[cfg(feature = "cbor")]
impl_ts!(serde_cbor::Value => "unknown");
impl_ts!(Never => "never");
impl_ts!(() => "null");
impl_ts!(serde_json::Number => "number");
#[cfg(feature = "url")]
impl_ts!(url::Url => "string");
#[cfg(feature = "ipnet")]
impl_ts!(ipnet::IpNet, ipnet::Ipv4Net, ipnet::Ipv6Net => "string");
#[cfg(feature = "josekit")]
impl_ts!(josekit::jwk::Jwk => "{[key:string]:unknown}");
#[cfg(feature = "yajrc")]
impl_ts!(yajrc::RpcError => "{code:number;message:string;data?:unknown}");
#[cfg(feature = "chrono")]
impl<T: chrono::TimeZone> TS for chrono::DateTime<T> {
    fn visit_ts(visitor: &mut TSVisitor) {
        visitor.ts.push_str("string");
    }
}
impl_ts!(std::path::Path, std::path::PathBuf,
    std::net::Ipv4Addr, std::net::Ipv6Addr, std::net::IpAddr,
    std::net::SocketAddr, std::net::SocketAddrV4, std::net::SocketAddrV6 => "string");

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
impl_ts_map!(std::collections::HashMap<K,V>, imbl::HashMap<K,V>, BTreeMap<K,V>, imbl::OrdMap<K,V>);

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
impl_ts_array!(
    Vec<T>,
    imbl::Vector<T>,
    [T],
    std::collections::BTreeSet<T>,
    std::collections::HashSet<T>,
    std::collections::VecDeque<T>
);

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
