//! TypeScript declarations from directional serde JSON shapes.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

pub use inventory;

pub use super::normalize::Direction;

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
pub use visit_rs::reflection::TypeInfo;
use visit_rs::reflection::{
    EnumKind, FieldInfo, Opaque, Position, StructKind, TypeAttribute, TypeAttributeInfo,
};
use visit_rs::{
    Named, Static, Visit, VisitFieldsStaticNamed, VisitTypeAttributes,
    VisitVariantFieldsStaticNamed, VisitVariantsStatic, Visitor,
};

pub use super::normalize::{
    declaration_name, default_name, documentation, generic_name, input_name,
};
use super::normalize::{Field, Style, Tag, Variant};

/// Appends the JSON expression for the visitor's selected direction.
///
/// Nested types must use `TSVisitor::append_type` to register named definitions.
pub trait TS {
    /// Base alias name; `None` emits inline. Recursive types require a name.
    const DEFINE: Option<&'static str> = None;
    /// Explicit input alias; `None` appends `Input` to the base alias.
    const INPUT_DEFINE: Option<&'static str> = None;
    /// Rust container doc attributes, emitted on declarations and inline expressions.
    const DOCS: &'static [&'static str] = &[];
    /// Marks nullable options whose named input fields may be omitted or flattened.
    const IS_OPTION: bool = false;
    /// Returns the output alias, including consumer metadata overrides.
    fn define_name() -> Option<String> {
        Self::DEFINE.map(str::to_owned)
    }
    /// Returns an explicit input alias; otherwise the output alias gains `Input`.
    fn input_define_name() -> Option<String> {
        Self::INPUT_DEFINE.map(str::to_owned)
    }
    /// Returns declaration documentation in source order.
    fn documentation() -> Vec<String> {
        Self::DOCS.iter().map(|doc| (*doc).to_owned()).collect()
    }
    fn visit_ts(visitor: &mut TSVisitor);
}

impl<T> Visit<TSVisitor> for Static<T>
where
    T: TS + ?Sized,
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
    fields: Vec<(Field, String, bool, bool)>,
    variants: Vec<String>,
    type_optional: bool,
    suppress_types: bool,
    pub(super) errors: Vec<String>,
    reserved: BTreeSet<String>,
    generics: Vec<&'static [&'static str]>,
}

#[derive(Debug, Clone)]
struct Definition {
    docs: Vec<String>,
    owner: (&'static str, Direction),
    params: &'static [&'static str],
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
            suppress_types: false,
            errors: Vec::new(),
            reserved: BTreeSet::new(),
            generics: Vec::new(),
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
        if self.suppress_types {
            return;
        }
        let owner = (std::any::type_name::<T>(), self.direction);
        if let Some((name, _)) = self
            .definitions
            .iter()
            .find(|(_, definition)| definition.owner == owner && definition.params.is_empty())
        {
            self.ts.push_str(name);
        } else if let Some(define) = T::define_name() {
            self.declare::<T>(&define);
        } else if self.active.contains(&owner) {
            self.errors
                .push(format!("Recursive inline type {} requires DEFINE", owner.0));
        } else {
            self.active.push(owner);
            let docs = T::documentation();
            if !docs.is_empty() {
                // Prettier requires inline JSDoc to end on the type's line for stability.
                self.ts.push_str(jsdoc(&docs).trim_matches('\n'));
                self.ts.push(' ');
            }
            T::visit_ts(self);
            self.active.pop();
        }
    }

    /// Registers a named concrete type, including recursive generic instances.
    pub fn declare<T: TS + ?Sized>(&mut self, name: &str) {
        let name = match self.direction {
            Direction::Input => T::input_define_name().unwrap_or_else(|| format!("{name}Input")),
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
                docs: T::documentation(),
                owner,
                params: &[],
                expression: None,
            },
        );
        let expression = self.capture(T::visit_ts);
        self.definitions.get_mut(&name).unwrap().expression = Some(expression);
    }

    /// Appends a generic instance, registering its family from `D`, which
    /// substitutes `Param<N>` for each named parameter.
    pub fn append_generic<D>(&mut self, params: &'static [&'static str], args: &[fn(&mut Self)])
    where
        D: TypeInfo + VisitTypeAttributes<MetadataCollector>,
        D::Kind: ShapeKind<D>,
    {
        if self.suppress_types {
            return;
        }
        let args: Vec<_> = args.iter().map(|arg| self.capture(arg)).collect();
        if args.len() != params.len() {
            self.error(format!(
                "Generic {} expects {} arguments",
                D::DECLARATION.name,
                params.len()
            ));
        }
        self.declare_generic::<D>(params);
        self.ts.push('<');
        self.ts.push_str(&args.join(","));
        self.ts.push('>');
    }

    /// Registers a generic family by name, appending only that name.
    pub fn declare_generic<D>(&mut self, params: &'static [&'static str])
    where
        D: TypeInfo + VisitTypeAttributes<MetadataCollector>,
        D::Kind: ShapeKind<D>,
    {
        let base = generic_name(&D::DECLARATION);
        let name = match self.direction {
            Direction::Input => input_name(&D::DECLARATION)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{base}Input")),
            Direction::Output => base.to_owned(),
        };
        let owner = (std::any::type_name::<D>(), self.direction);
        if !valid_identifier(&name) || self.reserved.contains(&name) {
            self.error(format!("Invalid TypeScript definition name: {name}"));
            return;
        }
        if let Some(param) = params.iter().find(|param| !valid_parameter(param)) {
            self.error(format!("Invalid TypeScript generic parameter: {param}"));
            return;
        }
        self.ts.push_str(&name);
        if let Some(existing) = self.definitions.get(&name) {
            if existing.owner != owner {
                self.error(format!("Conflicting TypeScript definition: {name}"));
            }
            return;
        }
        self.definitions.insert(
            name.clone(),
            Definition {
                docs: documentation(&D::DECLARATION),
                owner,
                params,
                expression: None,
            },
        );
        self.generics.push(params);
        let expression = self.capture(visit_shape::<D>);
        self.generics.pop();
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
        let optional = std::mem::replace(&mut self.type_optional, false);
        visit(self);
        let expression = self.ts.split_off(start);
        self.type_optional = optional;
        expression
    }

    fn payload(&mut self, style: Style, fields: impl FnOnce(&mut Self)) -> String {
        let saved = std::mem::take(&mut self.fields);
        fields(self);
        let fields = std::mem::replace(&mut self.fields, saved);
        let fields: Vec<_> = fields
            .into_iter()
            .filter_map(|(field, ty, option, omitted)| {
                if omitted {
                    if style != Style::Struct {
                        self.errors.push("Omission requires a named field".into());
                    }
                    None
                } else {
                    Some((field, ty, option))
                }
            })
            .collect();
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
                    .map(|(field, ty, _)| format!("{}{ty}", jsdoc(&field.docs)))
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
                    let ty = if optional { format!("({ty})?") } else { ty };
                    format!("{}{ty}", jsdoc(&f.docs))
                }).collect();
                format!("[{}]", elements.join(","))
            }
            Style::Struct => {
                let mut object = String::from("{");
                let mut intersections = Vec::new();
                for (field, ty, option) in fields {
                    if field.flatten {
                        intersections.push(if option {
                            format!("{}Partial<Exclude<({ty}),null>>", jsdoc(&field.docs))
                        } else {
                            format!("{}({ty})", jsdoc(&field.docs))
                        });
                        continue;
                    }
                    let optional = field.optional
                        || (self.direction == Direction::Input && option && field.option_default);
                    let mut names = vec![field.name.clone()];
                    names.extend(field.aliases.iter().cloned());
                    names.sort_unstable();
                    names.dedup();
                    if names.len() > 1 && !optional {
                        intersections.push(format!(
                            "({})",
                            names
                                .iter()
                                .map(|name| format!(
                                    "{{{}{}:({ty})}}",
                                    jsdoc(&field.docs),
                                    json_str(name)
                                ))
                                .collect::<Vec<_>>()
                                .join("|")
                        ));
                    }
                    for name in &names {
                        object.push_str(&jsdoc(&field.docs));
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
    pub fn into_declarations(mut self) -> Result<String, BindingError> {
        for (name, definition) in &self.definitions {
            for param in definition.params {
                if self.definitions.contains_key(*param) || self.reserved.contains(*param) {
                    self.errors.push(format!(
                        "Generic parameter {param} of {name} shadows a declaration"
                    ));
                }
            }
        }
        if !self.errors.is_empty() {
            return Err(BindingError(self.errors.join("\n")));
        }
        Ok(self
            .definitions
            .into_iter()
            .map(|(name, definition)| {
                let params = if definition.params.is_empty() {
                    String::new()
                } else {
                    format!("<{}>", definition.params.join(","))
                };
                format!(
                    "{}export type {name}{params} = {};\n",
                    jsdoc(&definition.docs),
                    definition.expression.unwrap()
                )
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
                docs: Vec::new(),
                owner: ("", self.direction),
                params: &[],
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

fn valid_parameter(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'A'..=b'Z' | b'a'..=b'z' | b'_'))
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn jsdoc(docs: &[String]) -> String {
    if docs.is_empty() {
        return String::new();
    }
    // TypeScript attaches JSDoc only after a line break.
    let mut comment = String::from("\n/**\n");
    for doc in docs {
        for line in doc.split('\n') {
            comment.push_str(" *");
            let line = line.strip_prefix(' ').unwrap_or(line);
            if !line.is_empty() {
                comment.push(' ');
                comment.push_str(&line.replace("*/", "*\\/"));
            }
            comment.push('\n');
        }
    }
    comment.push_str(" */\n");
    comment
}

fn json_str(value: &str) -> String {
    serde_json::to_string(value).unwrap()
}

impl Visitor for TSVisitor {
    type Result = ();
}

impl TSVisitor {
    /// Records an error that fails `into_declarations`.
    pub fn error(&mut self, reason: impl Into<String>) {
        self.errors.push(reason.into());
    }
    pub(super) fn structure(
        &mut self,
        style: Style,
        tag: Option<(String, String)>,
        fields: impl FnOnce(&mut Self),
    ) {
        let payload = self.payload(style, fields);
        if let Some((tag, name)) = tag {
            // Serde ignores a struct's tag when deserializing.
            let optional = if self.direction == Direction::Input {
                "?"
            } else {
                ""
            };
            self.ts.push_str(&format!(
                "{{{}{optional}:({})}}&",
                json_str(&tag),
                json_str(&name)
            ));
        }
        self.ts.push_str(&payload);
    }

    pub(super) fn enumeration(&mut self, variants: impl FnOnce(&mut Self)) {
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

    pub(super) fn variant(&mut self, variant: Variant, fields: impl FnOnce(&mut Self)) {
        if variant.custom || variant.other {
            self.errors.push(format!(
                "Custom variant {} requires a TypeScript override",
                variant.name
            ));
        }
        let payload = self.payload(variant.style, fields);
        let mut names = vec![variant.name.clone()];
        names.extend(variant.aliases.iter().cloned());
        names.sort_unstable();
        names.dedup();
        let names = names
            .into_iter()
            .map(|name| json_str(&name))
            .collect::<Vec<_>>()
            .join("|");
        let unit = variant.style == Style::Unit;
        let docs = jsdoc(&variant.docs);
        let ty = match variant.tag {
            Tag::External if unit => format!("{docs}{names}"),
            Tag::External => {
                let mut variants = Vec::new();
                let mut names = vec![variant.name.clone()];
                names.extend(variant.aliases.iter().cloned());
                names.sort_unstable();
                names.dedup();
                for name in names {
                    variants.push(format!("{{{docs}{}:({payload})}}", json_str(&name)));
                }
                format!("({})", variants.join("|"))
            }
            Tag::Internal(tag) => {
                let tagged = format!("{{{docs}{}:({names})}}", json_str(&tag));
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
                    format!("{{{docs}{}:({names})}}", json_str(&tag))
                } else {
                    format!(
                        "{{{docs}{}:({names});{}:({payload})}}",
                        json_str(&tag),
                        json_str(&content)
                    )
                }
            }
            Tag::Untagged => format!("{docs}{payload}"),
        };
        self.variants.push(ty);
    }

    pub(super) fn render_field(&mut self, field: Field, callback: Option<fn(&mut Self)>) {
        let hints = &field.hints;
        self.errors.extend(hints.errors.clone());
        if hints.skip {
            self.fields.push((field, String::new(), false, true));
            return;
        }
        let Some(callback) = callback else {
            self.error(format!("Opaque field {} requires a typed optionality fact from storage or a selected wire target", field.name));
            return;
        };
        let mut optional = false;
        let ty = self.capture(|visitor| {
            if let Some(literal) = &hints.literal {
                let saved = visitor.suppress_types;
                visitor.suppress_types = true;
                callback(visitor);
                visitor.suppress_types = saved;
                optional = visitor.type_optional;
                visitor.ts.push_str(literal);
            } else {
                callback(visitor);
                optional = visitor.type_optional;
            }
        });
        self.fields.push((field, ty, optional, false));
    }
}

type TypeCallback = fn(&mut TSVisitor);
type StorageCallbacks = Vec<(Position, Option<TypeCallback>)>;

fn type_callback<T: TS + ?Sized>(visitor: &mut TSVisitor) {
    Static::<T>::new().visit(visitor);
}

#[doc(hidden)]
pub struct StorageCollector;
impl Visitor for StorageCollector {
    type Result = Option<TypeCallback>;
}
impl<T: TS + ?Sized> Visit<StorageCollector> for Static<T> {
    fn visit(&self, _: &mut StorageCollector) -> Option<TypeCallback> {
        Some(type_callback::<T>)
    }
}
impl<T: ?Sized> Visit<StorageCollector> for Static<Opaque<T>> {
    fn visit(&self, _: &mut StorageCollector) -> Option<TypeCallback> {
        None
    }
}
impl<T: ?Sized> Visit<StorageCollector> for Named<'_, Static<T>>
where
    Static<T>: Visit<StorageCollector>,
{
    fn visit(&self, visitor: &mut StorageCollector) -> Option<TypeCallback> {
        self.value.visit(visitor)
    }
}

#[doc(hidden)]
#[derive(Default)]
pub struct MetadataCollector {
    info: Option<TypeAttributeInfo>,
}
impl Visitor for MetadataCollector {
    type Result = (TypeAttributeInfo, TypeCallback);
}
impl<T: TS + ?Sized> Visit<MetadataCollector> for Static<T> {
    fn visit(&self, visitor: &mut MetadataCollector) -> (TypeAttributeInfo, TypeCallback) {
        (
            visitor
                .info
                .expect("selected metadata must carry its source coordinate"),
            type_callback::<T>,
        )
    }
}
impl<T: ?Sized> Visit<MetadataCollector> for TypeAttribute<T>
where
    Static<T>: Visit<MetadataCollector>,
{
    fn visit(&self, visitor: &mut MetadataCollector) -> (TypeAttributeInfo, TypeCallback) {
        let previous = visitor.info.replace(self.info);
        let result = self.marker.visit(visitor);
        visitor.info = previous;
        result
    }
}

fn pair_fields(fields: &[FieldInfo], callbacks: Vec<Option<TypeCallback>>) -> StorageCallbacks {
    fields
        .iter()
        .map(|field| {
            (
                field.position,
                field.visit_index.and_then(|index| callbacks[index]),
            )
        })
        .collect()
}

#[doc(hidden)]
#[derive(Default)]
pub struct EnumCollector {
    index: usize,
}
impl Visitor for EnumCollector {
    type Result = StorageCallbacks;
}
impl<T> Visit<EnumCollector> for visit_rs::Variant<'_, Static<T>>
where
    T: TypeInfo + VisitVariantFieldsStaticNamed<StorageCollector>,
{
    fn visit(&self, visitor: &mut EnumCollector) -> StorageCallbacks {
        let index = visitor.index;
        visitor.index += 1;
        let callbacks =
            T::visit_variant_fields_static_named(&self.info, &mut StorageCollector).collect();
        pair_fields(T::DECLARATION.variants[index].fields, callbacks)
    }
}

#[doc(hidden)]
pub trait ShapeKind<T: TypeInfo> {
    fn storage() -> StorageCallbacks;
}
impl<T: TypeInfo + VisitFieldsStaticNamed<StorageCollector>> ShapeKind<T> for StructKind {
    fn storage() -> StorageCallbacks {
        pair_fields(
            T::DECLARATION.fields,
            T::visit_fields_static_named(&mut StorageCollector).collect(),
        )
    }
}
impl<T: TypeInfo + VisitVariantsStatic<EnumCollector>> ShapeKind<T> for EnumKind {
    fn storage() -> StorageCallbacks {
        T::visit_variants_static(&mut EnumCollector::default())
            .flatten()
            .collect()
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

/// Stands in for a generic family's `N`th parameter while its declaration renders.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Param<const N: usize>;
impl<const N: usize> TS for Param<N> {
    fn visit_ts(visitor: &mut TSVisitor) {
        match visitor.generics.last().and_then(|params| params.get(N)) {
            Some(param) => visitor.ts.push_str(param),
            None => visitor.error(format!("Generic parameter {N} outside its declaration")),
        }
    }
}
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

/// Renders existing field/variant callbacks through the RPC-owned serde plan.
///
/// Selected metadata requires target support independently of storage visitors.
///
/// ```compile_fail
/// use rpc_toolkit::ts::{visit_shape, TSVisitor};
/// struct Unsupported;
/// #[derive(visit_rs::VisitFields)]
/// #[visit(wire = "Unsupported", type_attributes(visit::wire))]
/// struct Root { value: u32 }
/// visit_shape::<Root>(&mut TSVisitor::new());
/// ```
///
/// Opacity does not exempt selected targets from consumer support.
///
/// ```compile_fail
/// struct Unsupported;
/// #[derive(visit_rs::VisitFields)]
/// #[visit(opaque, wire = "Unsupported", type_attributes(visit::wire))]
/// struct Root(Unsupported);
/// rpc_toolkit::reflect_ts!(Root);
/// ```
pub fn visit_shape<T>(visitor: &mut TSVisitor)
where
    T: TypeInfo + VisitTypeAttributes<MetadataCollector>,
    T::Kind: ShapeKind<T>,
{
    let children = <T::Kind as ShapeKind<T>>::storage();
    let targets: Vec<_> = T::visit_type_attributes(&mut MetadataCollector::default()).collect();
    match super::normalize::normalize(&T::DECLARATION, visitor.direction) {
        Ok(plan) => plan.render(visitor, &children, &targets),
        Err(error) => visitor.error(error),
    }
}

/// Bridges reflected declaration identity, docs and wire shape to TypeScript.
///
/// `generic Name<A, B>` declares one TypeScript generic for every instance.
#[macro_export]
macro_rules! reflect_ts {
    (generic $name:ident < $($param:ident),+ $(,)? > $(where [$($bounds:tt)*])?) => {
        impl<$($param),+> $crate::ts::TS for $name<$($param),+>
        where $($param: $crate::ts::TS,)+ $($($bounds)*)?
        {
            fn visit_ts(visitor: &mut $crate::ts::TSVisitor) {
                visitor.append_generic::<$crate::__ts_generic_family!($name [] [] $($param)+)>(
                    &[$(stringify!($param)),+],
                    &[$($crate::ts::TSVisitor::append_type::<$param>),+],
                );
            }
        }
    };
    ($ty:ty) => { $crate::reflect_ts!(impl [] for $ty where []); };
    (impl [$($generic:tt)*] for $ty:ty $(where [$($bounds:tt)*])?) => {
        impl<$($generic)*> $crate::ts::TS for $ty where $($($bounds)*)? {
            const DEFINE: Option<&'static str> = $crate::ts::default_name(&<Self as $crate::ts::TypeInfo>::DECLARATION);
            const INPUT_DEFINE: Option<&'static str> = $crate::ts::input_name(&<Self as $crate::ts::TypeInfo>::DECLARATION);
            const DOCS: &'static [&'static str] = <Self as $crate::ts::TypeInfo>::DECLARATION.docs;
            fn visit_ts(visitor: &mut $crate::ts::TSVisitor) { $crate::ts::visit_shape::<Self>(visitor); }
        }
    };
}

/// Bridges a reflected serde shape with manually selected declaration identity.
#[macro_export]
macro_rules! impl_ts_shape {
    ($ty:ty $({ define: $name:expr $(, input_define: $input:expr)? $(,)? })?) => {
        $crate::impl_ts_shape!(impl [] for $ty where [] $({ define: $name $(, input_define: $input)? })?);
    };
    (impl [$($generic:tt)*] for $ty:ty $(where [$($bounds:tt)*])? $({ define: $name:expr $(, input_define: $input:expr)? $(,)? })?) => {
        impl<$($generic)*> $crate::ts::TS for $ty where $($($bounds)*)? {
            $(const DEFINE: Option<&'static str> = Some($name);
              $(const INPUT_DEFINE: Option<&'static str> = Some($input);)?)?
            const DOCS: &'static [&'static str] = <Self as $crate::ts::TypeInfo>::DECLARATION.docs;
            fn visit_ts(visitor: &mut $crate::ts::TSVisitor) { $crate::ts::visit_shape::<Self>(visitor); }
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __ts_generic_family {
    ($name:ident [$($done:ty),*] [$($index:tt)*]) => { $name<$($done),*> };
    ($name:ident [$($done:ty),*] [$($index:tt)*] $param:ident $($rest:ident)*) => {
        $crate::__ts_generic_family!(
            $name [$($done,)* $crate::ts::Param<{ 0 $($index)* }>] [$($index)* + 1] $($rest)*
        )
    };
}

/// Registers a typed root beside its owner; unnamed registrations use `TS::DEFINE`.
#[macro_export]
macro_rules! ts_export {
    (generic $name:ident < $($param:ident),+ $(,)? >, namespaces = [$($namespace:expr),* $(,)?]) => {
        const _: () = {
            type Family = $crate::__ts_generic_family!($name [] [] $($param)+);
            const PARAMS: &[&str] = &[$(stringify!($param)),+];
            $( $crate::ts::inventory::submit! {
                $crate::ts::Export {
                    module: module_path!(), namespace: $namespace,
                    name: $crate::ts::generic_name(&<Family as $crate::ts::TypeInfo>::DECLARATION),
                    register: |visitor| visitor.declare_generic::<Family>(PARAMS),
                }
            } )*
        };
    };
    ($ty:ty, namespaces = [$($namespace:expr),* $(,)?]) => {
        $( $crate::ts::inventory::submit! {
            $crate::ts::Export {
                module: module_path!(), namespace: $namespace,
                name: <$ty as $crate::ts::TS>::DEFINE.expect("export root requires DEFINE"),
                register: |visitor| visitor.append_type::<$ty>(),
            }
        } )*
    };
    ($ty:ty, name = $name:expr, namespaces = [$($namespace:expr),* $(,)?]) => { $(
        $crate::ts::inventory::submit! {
            $crate::ts::Export {
                module: module_path!(), namespace: $namespace, name: $name,
                register: |visitor| visitor.declare::<$ty>($name),
            }
        }
    )* };
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

impl_ts!(imbl_value::Value => "unknown");
impl_ts_map!(imbl_value::InOMap<K,V> where [K: Eq + Clone, V: Clone]);
impl_ts!(yasi::InternedString => "string");
#[cfg(feature = "exver")]
impl_ts!(exver::Version, exver::ExtendedVersion, exver::VersionRange => "string");
#[cfg(feature = "patch-db")]
crate::reflect_ts!(patch_db::Dump);
