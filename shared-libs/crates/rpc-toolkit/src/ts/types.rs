//! Zod schema modules from directional serde JSON shapes.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

pub use inventory;

/// An annotated standalone type root in a compiled crate's export namespace.
pub struct Export {
    pub module: &'static str,
    pub namespace: &'static str,
    pub name: &'static str,
    pub register: fn(&mut TSVisitor),
}
inventory::collect!(Export);

/// One generated root: the runtime schema module and its type-only projection.
///
/// `types` imports `schemas` through the specifier given at generation time, so
/// consumers that only import `types` carry no runtime code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bindings {
    pub schemas: String,
    pub types: String,
}

/// A zod expression and the TypeScript type it evaluates to.
///
/// The type annotates recursive getters, where TypeScript cannot infer a schema
/// that refers to the constant being initialized.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Expr {
    pub value: String,
    pub ty: String,
}
impl Expr {
    pub fn new(value: impl Into<String>, ty: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            ty: ty.into(),
        }
    }
    pub fn nullable(self) -> Self {
        Self::new(
            format!("{}.nullable()", self.value),
            format!("z.ZodNullable<{}>", self.ty),
        )
    }
    pub fn optional(self) -> Self {
        Self::new(
            format!("{}.optional()", self.value),
            format!("z.ZodOptional<{}>", self.ty),
        )
    }
    pub fn array(self) -> Self {
        Self::new(
            format!("z.array({})", self.value),
            format!("z.ZodArray<{}>", self.ty),
        )
    }
    pub fn record(self) -> Self {
        Self::new(
            format!("z.record(z.string(), {})", self.value),
            format!("z.ZodRecord<z.ZodString, {}>", self.ty),
        )
    }
    pub fn tuple(elements: Vec<Self>) -> Self {
        Self::new(
            format!("z.tuple([{}])", join(&elements, |e| &e.value, ", ")),
            // `Rest` must be pinned: its default widens the tuple with `...unknown[]`.
            format!("z.ZodTuple<[{}], null>", join(&elements, |e| &e.ty, ", ")),
        )
    }
    pub fn union(variants: Vec<Self>) -> Self {
        match variants.len() {
            1 => variants.into_iter().next().unwrap(),
            _ => Self::new(
                format!("z.union([{}])", join(&variants, |e| &e.value, ", ")),
                format!(
                    "z.ZodUnion<readonly [{}]>",
                    join(&variants, |e| &e.ty, ", ")
                ),
            ),
        }
    }
    pub fn intersection(self, other: Self) -> Self {
        Self::new(
            format!("z.intersection({}, {})", self.value, other.value),
            format!("z.ZodIntersection<{}, {}>", self.ty, other.ty),
        )
    }
    /// The identity codec joining a deserialization and a serialization shape.
    pub fn codec(input: Self, output: Self) -> Self {
        Self::new(
            format!("wire({}, {})", input.value, output.value),
            format!("z.ZodCodec<{}, {}>", input.ty, output.ty),
        )
    }
    pub fn literal(names: &[String]) -> Self {
        let quoted: Vec<_> = names.iter().map(|name| json_str(name)).collect();
        match quoted.as_slice() {
            [name] => Self::new(
                format!("z.literal({name})"),
                format!("z.ZodLiteral<{name}>"),
            ),
            _ => Self::new(
                format!("z.literal([{}])", quoted.join(", ")),
                format!("z.ZodLiteral<{}>", quoted.join(" | ")),
            ),
        }
    }
    pub fn enumeration(names: &[String]) -> Self {
        let quoted: Vec<_> = names.iter().map(|name| json_str(name)).collect();
        Self::new(
            format!("z.enum([{}])", quoted.join(", ")),
            format!(
                "z.ZodEnum<{{ {} }}>",
                quoted
                    .iter()
                    .map(|name| format!("{name}: {name}"))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        )
    }
    /// An object schema whose JSON Schema stays open to sibling keys, so the
    /// branches of an intersection accept each other's members; zod still
    /// strips unknown keys when parsing.
    pub fn object(shape: impl AsRef<str>, ty: impl Into<String>) -> Self {
        Self::new(format!("object({})", shape.as_ref()), ty)
    }
    /// The module's `empty` constant: any object, inferred as `{}` so
    /// intersections with it stay transparent.
    pub fn empty() -> Self {
        Self::new("empty", "typeof empty")
    }
    pub fn reference(name: &str) -> Self {
        Self::new(name, format!("typeof {name}"))
    }
    /// Appends `.meta({...})`, which leaves the schema type unchanged.
    pub fn described(mut self, id: Option<&str>, docs: &[String]) -> Self {
        self.value.push_str(&meta(id, docs));
        self
    }
}

fn join<'a, T>(items: &'a [T], part: impl Fn(&'a T) -> &'a str, separator: &str) -> String {
    items.iter().map(part).collect::<Vec<_>>().join(separator)
}

/// Collects annotated roots and their dependencies into one checked module pair.
pub fn export_namespace(
    crate_name: &str,
    namespace: &str,
    schemas_specifier: &str,
) -> Result<Bindings, BindingError> {
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
    for export in &exports {
        (export.register)(&mut visitor);
        visitor.ts.clear();
    }
    visitor.into_bindings(None, schemas_specifier)
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
    declaration_name, default_name, documentation, generic_name, input_name, Direction,
};

/// Appends the zod expression for a type's JSON shape.
///
/// Implementations append through `TSVisitor::leaf`, `append_type` and the
/// composition helpers so the schema type stays paired with the value.
/// Where deserialization and serialization shapes differ, the expression wraps
/// both sides with `TSVisitor::wire`; `z.input` and `z.output` select them.
pub trait TS {
    /// Base schema name; `None` emits inline. Recursive types require a name.
    const DEFINE: Option<&'static str> = None;
    /// Explicit input type alias; `None` appends `Input` to the base name.
    const INPUT_DEFINE: Option<&'static str> = None;
    /// Rust container doc attributes, emitted on declarations and inline expressions.
    const DOCS: &'static [&'static str] = &[];
    /// Marks nullable options whose named input fields may be omitted or flattened.
    const IS_OPTION: bool = false;
    /// Returns the schema name, including consumer metadata overrides.
    fn define_name() -> Option<String> {
        Self::DEFINE.map(str::to_owned)
    }
    /// Returns an explicit input alias; otherwise the schema name gains `Input`.
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
    /// Current expression buffer; `ty` holds its schema type in parallel.
    pub ts: String,
    pub ty: String,
    active: Vec<&'static str>,
    type_optional: bool,
    suppress_types: bool,
    pending_reference: bool,
    pub(super) errors: Vec<String>,
    reserved: BTreeSet<String>,
    generics: Vec<(&'static str, &'static [&'static str])>,
    uses_wire: bool,
    types_prelude: String,
    /// Definition references collected per enclosing declaration or member.
    frames: Vec<Vec<Edge>>,
    /// Wire direction of the surrounding handler position; `None` reaches both.
    context: Option<Direction>,
    /// Directions each declaration is referenced from, before closure.
    reach: BTreeMap<String, Reach>,
    /// Literal expressions hoisted into module constants, by text.
    literals: BTreeMap<String, usize>,
    /// Folded parameter expressions of the enclosing handlers, innermost last.
    inherited_params: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Reach {
    input: bool,
    output: bool,
}

/// A referenced definition name; deferred where a getter or factory body holds it.
pub(super) type Edge = (String, bool);

#[derive(Debug, Clone)]
struct Definition {
    docs: Vec<String>,
    owner: &'static str,
    input_name: Option<String>,
    params: &'static [&'static str],
    expression: Option<Expr>,
    edges: Vec<Edge>,
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

/// Identifiers the schema module prelude and generated code own.
const RESERVED: &[&str] = &["z", "wire", "object", "empty", "schemas", "self"];

const WIRE_PRELUDE: &str = "const wire = <A extends z.ZodType, B extends z.ZodType>(input: A, output: B) =>\n  z.codec(input, output, { decode: (v) => v as never, encode: (v) => v as never });\n";
/// Marks a schema constructor call as droppable when its result is unused, so
/// bundlers shake schemas a consumer never imports. esbuild applies it to the
/// whole member chain that follows.
pub const PURE: &str = "/* @__PURE__ */";
const OBJECT_PRELUDE: &str = "const object = <Shape extends z.core.$ZodLooseShape>(shape: Shape) =>\n  z.object(shape).meta({ additionalProperties: true });\n";
/// A cast expression would need parentheses, which hide a purity annotation
/// from bundlers once TypeScript erases the cast; a typed constant does not.
const EMPTY_PRELUDE: &str = "const empty: z.ZodType<{}, {}> = /* @__PURE__ */ object({});\n";

impl TSVisitor {
    /// Starts with an empty expression/definition registry.
    pub fn new() -> Self {
        Self {
            definitions: BTreeMap::new(),
            ts: String::new(),
            ty: String::new(),
            active: Vec::new(),
            type_optional: false,
            suppress_types: false,
            pending_reference: false,
            errors: Vec::new(),
            reserved: RESERVED.iter().map(|name| (*name).to_owned()).collect(),
            generics: Vec::new(),
            uses_wire: false,
            types_prelude: String::new(),
            frames: Vec::new(),
            context: None,
            reach: BTreeMap::new(),
            literals: BTreeMap::new(),
            inherited_params: Vec::new(),
        }
    }

    /// References a literal zod expression through a module constant, so its
    /// schema type is `typeof` that constant rather than a guess.
    ///
    /// The expression must be closed: it may use `z` and nothing declared in
    /// the module.
    pub fn literal(&mut self, expression: &str) -> Expr {
        let next = self.literals.len();
        let index = *self.literals.entry(expression.to_owned()).or_insert(next);
        let name = format!("literal_{index}");
        self.reserved.insert(name.clone());
        Expr::new(name.clone(), format!("typeof {name}"))
    }

    /// Renders under one wire direction, so declarations reached only there
    /// project only that direction's type alias.
    pub fn in_direction<R>(
        &mut self,
        direction: Direction,
        visit: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let previous = self.context.replace(direction);
        let result = visit(self);
        self.context = previous;
        result
    }

    /// The parameters every handler under the current tree position accepts
    /// besides its own, already folded; `None` where no ancestor declares any.
    pub fn inherited_params(&self) -> Option<&str> {
        self.inherited_params
            .last()
            .map(String::as_str)
            .filter(|params| *params != "empty")
    }

    /// Runs the children of a handler whose full parameters are `params`.
    pub fn with_inherited_params<R>(
        &mut self,
        params: String,
        visit: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.inherited_params.push(params);
        let result = visit(self);
        self.inherited_params.pop();
        result
    }

    /// Records a reference to a declaration; one whose eager references reach
    /// a declaration still being rendered makes the enclosing member a getter.
    fn reference(&mut self, name: &str) {
        if self.reaches_pending(name) {
            self.pending_reference = true;
        }
        let reach = self.reach.entry(name.to_owned()).or_default();
        reach.input |= self.context != Some(Direction::Output);
        reach.output |= self.context != Some(Direction::Input);
        if let Some(frame) = self.frames.last_mut() {
            frame.push((name.to_owned(), false));
        }
    }

    /// Closes reachability over references, so a declaration projects every
    /// direction any of its referrers is reached from.
    fn reachability(&self) -> BTreeMap<String, Reach> {
        let mut reach = self.reach.clone();
        let mut changed = true;
        while changed {
            changed = false;
            for (name, definition) in &self.definitions {
                let Some(from) = reach.get(name).copied() else {
                    continue;
                };
                for (target, _) in &definition.edges {
                    let to = reach.entry(target.clone()).or_default();
                    let merged = Reach {
                        input: to.input || from.input,
                        output: to.output || from.output,
                    };
                    if merged != *to {
                        *to = merged;
                        changed = true;
                    }
                }
            }
        }
        reach
    }

    fn reaches_pending(&self, name: &str) -> bool {
        let mut seen = BTreeSet::new();
        let mut stack = vec![name.to_owned()];
        while let Some(name) = stack.pop() {
            let Some(definition) = self.definitions.get(&name) else {
                continue;
            };
            if definition.expression.is_none() {
                return true;
            }
            if !seen.insert(name) {
                continue;
            }
            stack.extend(
                definition
                    .edges
                    .iter()
                    .filter(|(_, deferred)| !deferred)
                    .map(|(target, _)| target.clone()),
            );
        }
        false
    }

    /// Appends helper declarations to the type module after its imports.
    pub fn types_prelude(&mut self, text: &str) {
        self.types_prelude.push_str(text);
    }

    /// Appends an inline expression or registers and references a named definition.
    pub fn append_type<T: TS + ?Sized>(&mut self) {
        if self.suppress_types {
            return;
        }
        let owner = std::any::type_name::<T>();
        if let Some((name, _)) = self
            .definitions
            .iter()
            .find(|(_, definition)| definition.owner == owner && definition.params.is_empty())
        {
            let name = name.clone();
            self.reference(&name);
            self.push(Expr::reference(&name));
        } else if let Some(define) = T::define_name() {
            self.declare::<T>(&define);
        } else if self.active.contains(&owner) {
            self.errors
                .push(format!("Recursive inline type {owner} requires DEFINE"));
        } else {
            self.active.push(owner);
            let expression = self.capture(T::visit_ts);
            self.active.pop();
            self.push(expression.described(None, &T::documentation()));
        }
    }

    /// Appends a composed expression.
    pub fn push(&mut self, expression: Expr) {
        self.ts.push_str(&expression.value);
        self.ty.push_str(&expression.ty);
    }

    /// Appends a primitive schema and its type.
    pub fn leaf(&mut self, value: &str, ty: &str) {
        self.ts.push_str(value);
        self.ty.push_str(ty);
    }

    /// Registers a named concrete type, including recursive generic instances.
    pub fn declare<T: TS + ?Sized>(&mut self, name: &str) {
        let owner = std::any::type_name::<T>();
        if !valid_identifier(name) || self.reserved.contains(name) {
            self.errors
                .push(format!("Invalid TypeScript definition name: {name}"));
            return;
        }
        self.reference(name);
        self.push(Expr::reference(name));
        if let Some(existing) = self.definitions.get(name) {
            if existing.owner != owner {
                self.errors
                    .push(format!("Conflicting TypeScript definition: {name}"));
            }
            return;
        }
        self.definitions.insert(
            name.to_owned(),
            Definition {
                docs: T::documentation(),
                owner,
                input_name: T::input_define_name(),
                params: &[],
                expression: None,
                edges: Vec::new(),
            },
        );
        let top_level = self.frames.is_empty();
        self.frames.push(Vec::new());
        let expression = self.capture(T::visit_ts);
        let edges = self.frames.pop().unwrap();
        if top_level {
            self.pending_reference = false;
        }
        let definition = self.definitions.get_mut(name).unwrap();
        definition.expression = Some(expression);
        definition.edges = edges;
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
        let owner = std::any::type_name::<D>();
        if self.generics.last().map(|(family, _)| *family) == Some(owner)
            && args
                .iter()
                .map(|arg| arg.value.as_str())
                .eq(params.iter().copied())
        {
            self.pending_reference = true;
            self.push(Expr::reference("self"));
            return;
        }
        let name = generic_name(&D::DECLARATION);
        self.declare_generic::<D>(params);
        self.push(Expr::new(
            format!("{name}({})", join(&args, |arg| &arg.value, ", ")),
            format!(
                "ReturnType<typeof {name}<{}>>",
                join(&args, |arg| &arg.ty, ", ")
            ),
        ));
    }

    /// Registers a generic family by name without appending anything.
    pub fn declare_generic<D>(&mut self, params: &'static [&'static str])
    where
        D: TypeInfo + VisitTypeAttributes<MetadataCollector>,
        D::Kind: ShapeKind<D>,
    {
        let name = generic_name(&D::DECLARATION);
        let owner = std::any::type_name::<D>();
        if !valid_identifier(name) || self.reserved.contains(name) {
            self.error(format!("Invalid TypeScript definition name: {name}"));
            return;
        }
        self.reference(name);
        if let Some(param) = params
            .iter()
            .find(|param| !valid_parameter(param) || self.reserved.contains(**param))
        {
            self.error(format!("Invalid TypeScript generic parameter: {param}"));
            return;
        }
        if let Some(existing) = self.definitions.get(name) {
            if existing.owner != owner {
                self.error(format!("Conflicting TypeScript definition: {name}"));
            }
            return;
        }
        self.definitions.insert(
            name.to_owned(),
            Definition {
                docs: documentation(&D::DECLARATION),
                owner,
                input_name: input_name(&D::DECLARATION).map(str::to_owned),
                params,
                expression: None,
                edges: Vec::new(),
            },
        );
        self.generics.push((owner, params));
        let top_level = self.frames.is_empty();
        self.frames.push(Vec::new());
        let expression = self.capture(visit_shape::<D>);
        let edges = self.frames.pop().unwrap();
        if top_level {
            self.pending_reference = false;
        }
        self.generics.pop();
        let definition = self.definitions.get_mut(name).unwrap();
        definition.expression = Some(expression);
        // A factory body runs at call time, after every declaration.
        definition.edges = edges.into_iter().map(|(name, _)| (name, true)).collect();
    }

    /// Appends the intersection of two object projections.
    pub fn intersection(&mut self, left: impl FnOnce(&mut Self), right: impl FnOnce(&mut Self)) {
        let left = self.capture(left);
        let right = self.capture(right);
        self.push(left.intersection(right));
    }

    /// Appends a shape whose deserialization and serialization forms differ.
    ///
    /// Identical renderings collapse to one expression.
    pub fn wire(&mut self, input: impl FnOnce(&mut Self), output: impl FnOnce(&mut Self)) {
        let input = self.capture(input);
        let output = self.capture(output);
        let expression = self.wired(input, output);
        self.push(expression);
    }

    pub(super) fn wired(&mut self, input: Expr, output: Expr) -> Expr {
        if input.value == output.value {
            input
        } else {
            self.uses_wire = true;
            Expr::codec(input, output)
        }
    }

    /// Renders an expression into a fresh buffer, leaving the current one intact.
    pub fn capture(&mut self, visit: impl FnOnce(&mut Self)) -> Expr {
        let value = self.ts.len();
        let ty = self.ty.len();
        let optional = std::mem::replace(&mut self.type_optional, false);
        visit(self);
        let expression = Expr {
            value: self.ts.split_off(value),
            ty: self.ty.split_off(ty),
        };
        self.type_optional = optional;
        expression
    }

    /// Renders a member type, reporting typed optionality, whether it refers to
    /// a declaration still being rendered, and the references it made, which
    /// the caller commits once it knows whether a getter defers them.
    pub(super) fn capture_field(
        &mut self,
        visit: impl FnOnce(&mut Self),
    ) -> (Expr, bool, bool, Vec<Edge>) {
        let pending = std::mem::replace(&mut self.pending_reference, false);
        self.frames.push(Vec::new());
        let mut optional = false;
        let expression = self.capture(|visitor| {
            visit(visitor);
            optional = visitor.type_optional;
        });
        let edges = self.frames.pop().unwrap();
        let recursive = std::mem::replace(&mut self.pending_reference, pending);
        (expression, optional, recursive, edges)
    }

    /// Records a member's references in the enclosing declaration.
    ///
    /// A recursive member that no getter defers re-raises the pending
    /// reference, so an enclosing object member becomes the getter instead.
    pub(super) fn commit_edges(&mut self, edges: Vec<Edge>, deferred: bool, recursive: bool) {
        if let Some(frame) = self.frames.last_mut() {
            frame.extend(
                edges
                    .into_iter()
                    .map(|(name, held)| (name, held || deferred)),
            );
        }
        if recursive && !deferred {
            self.pending_reference = true;
        }
    }

    /// Declaration names in an order every eager reference precedes its use.
    fn emission_order(&mut self) -> Vec<String> {
        let mut remaining: BTreeMap<&str, BTreeSet<&str>> = self
            .definitions
            .iter()
            .map(|(name, definition)| {
                (
                    name.as_str(),
                    definition
                        .edges
                        .iter()
                        .filter(|(target, deferred)| !deferred && target != name)
                        .map(|(target, _)| target.as_str())
                        .collect(),
                )
            })
            .collect();
        for (name, definition) in &self.definitions {
            if definition
                .edges
                .iter()
                .any(|(target, deferred)| !deferred && target == name)
            {
                self.errors.push(format!(
                    "Recursive reference to {name} outside an object member requires a wrapper struct or an override"
                ));
            }
        }
        let mut order = Vec::new();
        while !remaining.is_empty() {
            let ready: Vec<&str> = remaining
                .iter()
                .filter(|(_, deps)| deps.iter().all(|dep| !remaining.contains_key(dep)))
                .map(|(name, _)| *name)
                .collect();
            if ready.is_empty() {
                let mut cycle: Vec<_> = remaining.keys().map(|name| (*name).to_owned()).collect();
                cycle.sort();
                self.errors.push(format!(
                    "Recursive references outside object members among {} require a wrapper struct or an override",
                    cycle.join(", ")
                ));
                break;
            }
            for name in ready {
                remaining.remove(name);
                order.push(name.to_owned());
            }
        }
        order
    }

    pub(super) fn with_suppressed_types(&mut self, visit: impl FnOnce(&mut Self)) {
        let saved = std::mem::replace(&mut self.suppress_types, true);
        visit(self);
        self.suppress_types = saved;
    }

    /// Reserves names supplied by the module's other declarations.
    pub fn reserve(&mut self, names: impl IntoIterator<Item = impl AsRef<str>>) {
        self.reserved
            .extend(names.into_iter().map(|name| name.as_ref().to_owned()));
    }

    /// Records an error that fails `into_bindings`.
    pub fn error(&mut self, reason: impl Into<String>) {
        self.errors.push(reason.into());
    }

    /// Emits registered declarations and an optional named root, rejecting
    /// accumulated generation errors.
    ///
    /// The root is the current expression buffer; a root name makes the schema
    /// module export it and the type module project it with `RpcSchemaTree`.
    pub fn into_bindings(
        mut self,
        root_name: Option<&str>,
        schemas_specifier: &str,
    ) -> Result<Bindings, BindingError> {
        if let Some(root) = root_name {
            if !valid_identifier(root)
                || self.reserved.contains(root)
                || self.definitions.contains_key(root)
                || self.ts.is_empty()
            {
                self.errors
                    .push(format!("Invalid or conflicting root name: {root}"));
            }
        }
        let reach = self.reachability();
        let mut aliases = BTreeMap::new();
        for (name, definition) in &self.definitions {
            for param in definition.params {
                if self.definitions.contains_key(*param) || self.reserved.contains(*param) {
                    self.errors.push(format!(
                        "Generic parameter {param} of {name} shadows a declaration"
                    ));
                }
            }
            let alias = definition
                .input_name
                .clone()
                .unwrap_or_else(|| format!("{name}Input"));
            let projected = reach.get(name).copied().unwrap_or_default();
            if projected.input
                && (!valid_identifier(&alias)
                    || self.reserved.contains(&alias)
                    || self
                        .definitions
                        .get(&alias)
                        .is_some_and(|_| reach.get(&alias).is_some_and(|r| r.output))
                    || root_name == Some(alias.as_str())
                    || aliases.insert(alias.clone(), name.clone()).is_some())
            {
                self.errors
                    .push(format!("Conflicting TypeScript definition: {alias}"));
            }
        }
        let order = self.emission_order();
        if !self.errors.is_empty() {
            let mut seen = BTreeSet::new();
            self.errors.retain(|error| seen.insert(error.clone()));
            return Err(BindingError(self.errors.join("\n")));
        }
        let mut schemas = String::from("import { z } from 'zod'\n");
        if self.uses_wire {
            schemas.push_str(WIRE_PRELUDE);
        }
        let uses = |needle: &str| {
            self.definitions
                .values()
                .filter_map(|definition| definition.expression.as_ref())
                .any(|expression| expression.value.contains(needle))
                || self.ts.contains(needle)
        };
        let uses_empty = uses("empty");
        if uses_empty || uses("object(") {
            schemas.push_str(OBJECT_PRELUDE);
        }
        if uses_empty {
            schemas.push_str(EMPTY_PRELUDE);
        }
        let mut literals: Vec<_> = self.literals.iter().collect();
        literals.sort_by_key(|(_, index)| **index);
        for (expression, index) in literals {
            schemas.push_str(&format!("const literal_{index} = {PURE} {expression};\n"));
        }
        let mut types = format!(
            "import type {{ z }} from 'zod'\nimport type * as schemas from {}\n{}",
            json_str(schemas_specifier),
            self.types_prelude
        );
        for name in &order {
            let definition = &self.definitions[name];
            let docs = jsdoc(&definition.docs);
            let expression = definition.expression.as_ref().unwrap();
            let projected = reach.get(name).copied().unwrap_or_default();
            let alias = aliases
                .iter()
                .find(|(_, owner)| *owner == name)
                .map(|(alias, _)| alias.as_str());
            if definition.params.is_empty() {
                // The annotation carries member JSDoc through declaration
                // emit, which drops comments from an inferred shape.
                schemas.push_str(&format!(
                    "{docs}export const {name}: {} = {PURE} {}{};\n",
                    expression.ty,
                    expression.value,
                    meta(Some(name), &definition.docs)
                ));
                if projected.output {
                    types.push_str(&format!(
                        "{docs}export type {name} = z.output<typeof schemas.{name}>;\n"
                    ));
                }
                if let Some(alias) = alias {
                    types.push_str(&format!(
                        "{docs}export type {alias} = z.input<typeof schemas.{name}>;\n"
                    ));
                }
            } else {
                let params = definition.params;
                let schema_params = params
                    .iter()
                    .map(|param| format!("{param} extends z.ZodType"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let schema_args = params
                    .iter()
                    .map(|param| format!("{param}: {param}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                schemas.push_str(&format!(
                    "{docs}export const {name} = <{schema_params}>({schema_args}) => {{\n  const self = {};\n  return self;\n}};\n",
                    expression.value
                ));
                let type_params = params.join(", ");
                let type_args = params
                    .iter()
                    .map(|param| format!("z.ZodType<{param}, {param}>"))
                    .collect::<Vec<_>>()
                    .join(", ");
                if projected.output {
                    types.push_str(&format!(
                        "{docs}export type {name}<{type_params}> = z.output<ReturnType<typeof schemas.{name}<{type_args}>>>;\n"
                    ));
                }
                if let Some(alias) = alias {
                    types.push_str(&format!(
                        "{docs}export type {alias}<{type_params}> = z.input<ReturnType<typeof schemas.{name}<{type_args}>>>;\n"
                    ));
                }
            }
        }
        if let Some(root) = root_name {
            schemas.push_str(&format!("export const {root} = {};\n", self.ts));
            types.push_str(&format!(
                "export type {root} = RpcSchemaTree<typeof schemas.{root}>;\n"
            ));
        }
        Ok(Bindings { schemas, types })
    }
}

fn valid_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'A'..=b'Z'))
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn valid_parameter(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'A'..=b'Z' | b'a'..=b'z' | b'_'))
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// A JSDoc block ending in a line break, or nothing.
pub(super) fn jsdoc(docs: &[String]) -> String {
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

/// A `.meta({...})` suffix carrying the JSON Schema id and description, or nothing.
///
/// The object opens on its own line: prettier keeps such an object expanded,
/// which keeps a member chain holding a long description stable across passes.
pub(super) fn meta(id: Option<&str>, docs: &[String]) -> String {
    let mut entries = Vec::new();
    if let Some(id) = id {
        entries.push(format!("id: {}", json_str(id)));
    }
    let description = docs
        .iter()
        .flat_map(|doc| doc.split('\n'))
        .map(|line| line.strip_prefix(' ').unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n");
    if !docs.is_empty() {
        entries.push(format!("description: {}", json_str(&description)));
    }
    if entries.is_empty() {
        String::new()
    } else {
        format!(".meta({{\n{} }})", entries.join(", "))
    }
}

pub(super) fn json_str(value: &str) -> String {
    serde_json::to_string(value).unwrap()
}

impl Visitor for TSVisitor {
    type Result = ();
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

/// A literal zod expression the caller owns, hoisted into a module constant.
pub struct LiteralTS(pub std::borrow::Cow<'static, str>);
impl Visit<TSVisitor> for LiteralTS {
    fn visit(&self, visitor: &mut TSVisitor) {
        let literal = visitor.literal(&self.0);
        visitor.push(literal);
        visitor.type_optional = false;
    }
}

/// Stands in for a generic family's `N`th parameter while its declaration renders.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Param<const N: usize>;
impl<const N: usize> TS for Param<N> {
    fn visit_ts(visitor: &mut TSVisitor) {
        match visitor
            .generics
            .last()
            .and_then(|(_, params)| params.get(N))
        {
            Some(param) => visitor.leaf(param, param),
            None => visitor.error(format!("Generic parameter {N} outside its declaration")),
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub enum Never {}

/// Implements `TS` with a fixed zod expression and its schema type.
#[macro_export]
macro_rules! impl_ts {
    ($($ty:ty),+ => $value:expr, $schema:expr) => { $(
        impl $crate::ts::TS for $ty {
            fn visit_ts(visitor: &mut $crate::ts::TSVisitor) { visitor.leaf($value, $schema); }
        }
    )+ };
}

impl_ts!(bool => "z.boolean()", "z.ZodBoolean");
impl_ts!(String, str, char => "z.string()", "z.ZodString");
impl_ts!(usize,u8,u16,u32,u64,u128,isize,i8,i16,i32,i64,i128 => "z.int()", "z.ZodInt");
macro_rules! float {
    ($($ty:ty),*) => { $(impl TS for $ty {
        fn visit_ts(v: &mut TSVisitor) {
            v.wire(
                |v| v.leaf("z.number()", "z.ZodNumber"),
                |v| v.leaf("z.number().nullable()", "z.ZodNullable<z.ZodNumber>"),
            );
        }
    })* };
}
float!(f32, f64);
impl_ts!(serde_json::Value => "z.unknown()", "z.ZodUnknown");
#[cfg(feature = "cbor")]
impl_ts!(serde_cbor::Value => "z.unknown()", "z.ZodUnknown");
impl_ts!(Never => "z.never()", "z.ZodNever");
impl_ts!(() => "z.null()", "z.ZodNull");
impl_ts!(serde_json::Number => "z.number()", "z.ZodNumber");
#[cfg(feature = "url")]
impl_ts!(url::Url => "z.string()", "z.ZodString");
#[cfg(feature = "ipnet")]
impl_ts!(ipnet::IpNet, ipnet::Ipv4Net, ipnet::Ipv6Net => "z.string()", "z.ZodString");
#[cfg(feature = "isocountry")]
impl_ts!(isocountry::CountryCode => "z.string()", "z.ZodString");
#[cfg(feature = "josekit")]
impl_ts!(josekit::jwk::Jwk => "z.record(z.string(), z.unknown())", "z.ZodRecord<z.ZodString, z.ZodUnknown>");
#[cfg(feature = "yajrc")]
impl TS for yajrc::RpcError {
    fn visit_ts(visitor: &mut TSVisitor) {
        visitor.push(Expr::object(
            "{ code: z.number(), message: z.string(), data: z.unknown().optional() }",
            "z.ZodObject<{ code: z.ZodNumber; message: z.ZodString; data: z.ZodOptional<z.ZodUnknown> }>",
        ));
    }
}
#[cfg(feature = "yajrc")]
impl TS for crate::RpcRequest {
    fn visit_ts(visitor: &mut TSVisitor) {
        visitor.push(Expr::object(
            "{ id: z.union([z.string(), z.number()]).nullable().optional(), method: z.string(), params: z.unknown() }",
            "z.ZodObject<{ id: z.ZodOptional<z.ZodNullable<z.ZodUnion<readonly [z.ZodString, z.ZodNumber]>>>; method: z.ZodString; params: z.ZodUnknown }>",
        ));
    }
}
#[cfg(feature = "chrono")]
impl<T: chrono::TimeZone> TS for chrono::DateTime<T> {
    fn visit_ts(visitor: &mut TSVisitor) {
        visitor.leaf("z.string()", "z.ZodString");
    }
}
impl_ts!(std::path::Path, std::path::PathBuf,
    std::net::Ipv4Addr, std::net::Ipv6Addr, std::net::IpAddr,
    std::net::SocketAddr, std::net::SocketAddrV4, std::net::SocketAddrV6 => "z.string()", "z.ZodString");

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
    let input = super::normalize::normalize(&T::DECLARATION, Direction::Input);
    let output = super::normalize::normalize(&T::DECLARATION, Direction::Output);
    match (input, output) {
        (Ok(input), Ok(output)) => visitor.render_plans(input, output, &children, &targets),
        (Err(error), _) | (_, Err(error)) => visitor.error(error),
    }
}

/// Bridges reflected declaration identity, docs and wire shape to a zod schema.
///
/// `generic Name<A, B>` declares one schema factory for every instance.
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
                let value = visitor.capture(|visitor| visitor.append_type::<V>());
                visitor.push(value.record());
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
                let element = visitor.capture(|visitor| visitor.append_type::<T>());
                visitor.push(element.array());
            }
        }
    )+ };
}
impl_ts_array!(
    Vec<T>,
    imbl::Vector<T>,
    imbl::OrdSet<T>,
    imbl::HashSet<T>,
    [T],
    std::collections::BTreeSet<T>,
    std::collections::HashSet<T>,
    std::collections::VecDeque<T>
);

impl<T: TS, const N: usize> TS for [T; N] {
    fn visit_ts(v: &mut TSVisitor) {
        let element = v.capture(|v| v.append_type::<T>());
        v.push(Expr::tuple(vec![element; N]));
    }
}
macro_rules! tuple {
    ($($ty:ident),+) => {
        impl<$($ty: TS),+> TS for ($($ty,)+) {
            fn visit_ts(v: &mut TSVisitor) {
                let elements = vec![$(v.capture(|v| v.append_type::<$ty>())),+];
                v.push(Expr::tuple(elements));
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
        let inner = visitor.capture(|visitor| visitor.append_type::<T>());
        visitor.push(inner.nullable());
    }
}

impl_ts!(imbl_value::Value => "z.unknown()", "z.ZodUnknown");
impl_ts_map!(imbl_value::InOMap<K,V> where [K: Eq + Clone, V: Clone]);
impl_ts!(yasi::InternedString => "z.string()", "z.ZodString");
#[cfg(feature = "exver")]
impl_ts!(exver::Version, exver::ExtendedVersion, exver::VersionRange => "z.string()", "z.ZodString");
#[cfg(feature = "patch-db")]
crate::reflect_ts!(patch_db::Dump);
#[cfg(feature = "patch-db")]
impl<S: AsRef<str>, V: patch_db::json_ptr::SegList> TS for patch_db::json_ptr::JsonPointer<S, V> {
    fn visit_ts(visitor: &mut TSVisitor) {
        visitor.leaf("z.string()", "z.ZodString");
    }
}
