use serde_derive_internals::{ast, attr, Ctxt, Derive};
use syn::{Attribute, Meta};
use visit_rs::reflection::{
    DeclarationInfo, GenericParameter, MetadataPosition, Position, TypeAttributeInfo,
};

use super::types::TSVisitor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Direction {
    Input,
    Output,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Style {
    Struct,
    Tuple,
    Newtype,
    Unit,
}
#[derive(Debug, Clone)]
pub(super) enum Tag {
    External,
    Internal(String),
    Adjacent(String, String),
    Untagged,
}
#[derive(Debug, Clone, Default)]
pub(super) struct Hints {
    pub literal: Option<String>,
    pub skip: bool,
    pub errors: Vec<String>,
}
#[derive(Debug, Clone)]
pub(super) struct Field {
    pub docs: Vec<String>,
    pub name: String,
    pub aliases: Vec<String>,
    pub optional: bool,
    pub option_default: bool,
    pub flatten: bool,
    pub hints: Hints,
}
#[derive(Debug, Clone)]
pub(super) struct Variant {
    pub docs: Vec<String>,
    pub name: String,
    pub aliases: Vec<String>,
    pub style: Style,
    pub tag: Tag,
    pub custom: bool,
    pub other: bool,
}
#[derive(Clone)]
struct Target {
    position: MetadataPosition,
    path: Vec<String>,
}
struct FieldPlan {
    position: Position,
    field: Field,
    target: Option<Target>,
    error: Option<String>,
}
struct VariantPlan {
    variant: Variant,
    fields: Vec<FieldPlan>,
}
pub(super) struct Plan {
    hints: Hints,
    target: Option<Target>,
    style: Option<Style>,
    tag: Option<(String, String)>,
    fields: Vec<FieldPlan>,
    variants: Vec<VariantPlan>,
}

use visit_rs::metadata::{AttributeMeta, MetaValue};

const fn same(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    if left.len() != right.len() {
        return false;
    }
    let mut i = 0;
    while i < left.len() {
        if left[i] != right[i] {
            return false;
        }
        i += 1;
    }
    true
}

const fn setting(metadata: &'static [AttributeMeta], key: &str) -> Option<&'static str> {
    let mut i = 0;
    while i < metadata.len() {
        if let AttributeMeta::List { path, items } = &metadata[i] {
            if same(path, "visit") {
                let mut j = 0;
                while j < items.len() {
                    if let AttributeMeta::List { path, items } = &items[j] {
                        if same(path, "ts") {
                            let mut k = 0;
                            while k < items.len() {
                                if let AttributeMeta::NameValue {
                                    path,
                                    value: MetaValue::Str(value),
                                    ..
                                } = &items[k]
                                {
                                    if same(path, key) {
                                        return Some(value);
                                    }
                                }
                                k += 1;
                            }
                        }
                    }
                    j += 1;
                }
            }
        }
        i += 1;
    }
    None
}

const fn rename_value(meta: &'static AttributeMeta) -> Option<&'static str> {
    match meta {
        AttributeMeta::NameValue {
            value: MetaValue::Str(value),
            ..
        } => Some(value),
        AttributeMeta::List { items, .. } => {
            let mut serialize = None;
            let mut deserialize = None;
            let mut i = 0;
            while i < items.len() {
                if let AttributeMeta::NameValue {
                    path,
                    value: MetaValue::Str(value),
                    ..
                } = &items[i]
                {
                    if same(path, "serialize") {
                        serialize = Some(*value);
                    } else if same(path, "deserialize") {
                        deserialize = Some(*value);
                    }
                }
                i += 1;
            }
            match serialize {
                Some(value) => Some(value),
                None => deserialize,
            }
        }
        _ => None,
    }
}

const fn legacy_name(info: &DeclarationInfo) -> &'static str {
    let mut i = 0;
    while i < info.metadata.len() {
        if let AttributeMeta::List { path, items } = &info.metadata[i] {
            if same(path, "visit") || same(path, "serde") {
                let mut j = 0;
                while j < items.len() {
                    let path = match &items[j] {
                        AttributeMeta::NameValue { path, .. }
                        | AttributeMeta::List { path, .. } => *path,
                        _ => "",
                    };
                    if same(path, "rename") {
                        if let Some(value) = rename_value(&items[j]) {
                            return value;
                        }
                    }
                    j += 1;
                }
            }
        }
        i += 1;
    }
    info.name
}

pub const fn default_name(info: &DeclarationInfo) -> Option<&'static str> {
    let mut i = 0;
    while i < info.parameters.len() {
        if !matches!(info.parameters[i], GenericParameter::Lifetime { .. }) {
            return None;
        }
        i += 1;
    }
    match setting(info.metadata, "rename") {
        Some(name) => Some(name),
        None => Some(legacy_name(info)),
    }
}
pub const fn input_name(info: &DeclarationInfo) -> Option<&'static str> {
    setting(info.metadata, "input_rename")
}
pub fn declaration_name(info: &DeclarationInfo, input: bool) -> Option<String> {
    if input {
        input_name(info)
    } else {
        default_name(info)
    }
    .map(str::to_owned)
}
pub fn documentation(info: &DeclarationInfo) -> Vec<String> {
    info.docs.iter().map(|doc| (*doc).to_owned()).collect()
}
fn docs(attrs: &[Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter_map(|a| match &a.meta {
            Meta::NameValue(n) if n.path.is_ident("doc") => literal(&n.value),
            _ => None,
        })
        .collect()
}
fn literal(value: &syn::Expr) -> Option<String> {
    match value {
        syn::Expr::Lit(l) => match &l.lit {
            syn::Lit::Str(s) => Some(s.value()),
            _ => None,
        },
        _ => None,
    }
}
fn nested_meta(input: syn::parse::ParseStream) -> syn::Result<Meta> {
    use syn::ext::IdentExt;
    let mut path = syn::Path::from(input.call(syn::Ident::parse_any)?);
    while input.peek(syn::Token![::]) {
        input.parse::<syn::Token![::]>()?;
        path.segments
            .push(syn::PathSegment::from(input.call(syn::Ident::parse_any)?));
    }
    if input.peek(syn::Token![=]) {
        Ok(Meta::NameValue(syn::MetaNameValue {
            path,
            eq_token: input.parse()?,
            value: input.parse()?,
        }))
    } else if input.peek(syn::token::Paren) {
        let content;
        let delimiter = syn::MacroDelimiter::Paren(syn::parenthesized!(content in input));
        Ok(Meta::List(syn::MetaList {
            path,
            delimiter,
            tokens: content.parse()?,
        }))
    } else {
        Ok(Meta::Path(path))
    }
}
fn children(list: &syn::MetaList) -> syn::Result<Vec<Meta>> {
    list.parse_args_with(|input: syn::parse::ParseStream| {
        syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated_with(
            input,
            nested_meta,
        )
    })
    .map(|items| items.into_iter().collect())
}
fn hints(attrs: &[Attribute]) -> Hints {
    let mut result = Hints::default();
    let mut seen = std::collections::BTreeSet::new();
    for attribute in attrs.iter().filter(|a| a.path().is_ident("visit")) {
        let Meta::List(list) = &attribute.meta else {
            continue;
        };
        let items = match children(list) {
            Ok(items) => items,
            Err(error) => {
                if list.tokens.clone().into_iter().any(|token| {
                    matches!(
                        token.to_string().as_str(),
                        "ts" | "wire" | "input_wire" | "output_wire"
                    )
                }) {
                    result
                        .errors
                        .push(format!("Malformed consumer metadata: {error}"));
                }
                continue;
            }
        };
        for item in items.iter().filter(|m| m.path().is_ident("ts")) {
            let Meta::List(list) = item else {
                result
                    .errors
                    .push("TypeScript hints require a ts(...) list".into());
                continue;
            };
            let items = match children(list) {
                Ok(items) => items,
                Err(error) => {
                    result.errors.push(error.to_string());
                    continue;
                }
            };
            for item in items {
                let key = item
                    .path()
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::");
                let valid = match &item {
                    Meta::Path(_) if key == "skip" => {
                        result.skip = true;
                        true
                    }
                    Meta::NameValue(n)
                        if matches!(key.as_str(), "type" | "rename" | "input_rename") =>
                    {
                        if let Some(value) = literal(&n.value) {
                            match key.as_str() {
                                "type" => result.literal = Some(value),
                                _ => {}
                            }
                            true
                        } else {
                            false
                        }
                    }
                    _ => false,
                };
                if !valid {
                    result
                        .errors
                        .push(format!("Invalid TypeScript hint: {key}"));
                }
                if !seen.insert(key.clone()) {
                    result
                        .errors
                        .push(format!("Duplicate TypeScript hint: {key}"));
                }
            }
        }
    }
    result
}
fn selected(
    attrs: &[Attribute],
    position: Position,
    field: bool,
    path: &[&str],
) -> Result<Option<Target>, String> {
    fn walk(
        meta: &Meta,
        prefix: &[String],
        occurrence: &mut usize,
        wanted: &[&str],
        matches: &mut Vec<(usize, Vec<String>)>,
    ) -> Result<(), String> {
        let mut full = prefix.to_vec();
        full.extend(meta.path().segments.iter().map(|s| s.ident.to_string()));
        let ordinal = *occurrence;
        *occurrence += 1;
        if full.iter().map(String::as_str).eq(wanted.iter().copied()) {
            let Meta::NameValue(n) = meta else {
                return Err("expected a wire type string".into());
            };
            let value = literal(&n.value).ok_or("expected a wire type string")?;
            syn::parse_str::<syn::Type>(&value).map_err(|e| e.to_string())?;
            matches.push((ordinal, full.clone()));
        }
        if let Meta::List(list) = meta {
            if let Ok(items) = children(list) {
                for item in items {
                    walk(&item, &full, occurrence, wanted, matches)?;
                }
            }
        }
        Ok(())
    }
    let mut found = None;
    for (attribute, attr) in attrs.iter().enumerate() {
        let mut matches = Vec::new();
        walk(&attr.meta, &[], &mut 0, path, &mut matches)?;
        for (occurrence, path) in matches {
            let target = Target {
                position: MetadataPosition {
                    variant: position.variant,
                    field: if field { Some(position.field) } else { None },
                    attribute,
                    occurrence,
                },
                path,
            };
            if found.replace(target).is_some() {
                return Err("duplicate wire type override".into());
            }
        }
    }
    Ok(found)
}
fn wire(
    attrs: &[Attribute],
    position: Position,
    field: bool,
    input: bool,
) -> Result<Option<Target>, String> {
    let common = selected(attrs, position, field, &["visit", "wire"])?;
    let directional = selected(
        attrs,
        position,
        field,
        &["visit", if input { "input_wire" } else { "output_wire" }],
    )?;
    Ok(directional.or(common))
}
fn style(style: ast::Style) -> Style {
    match style {
        ast::Style::Struct => Style::Struct,
        ast::Style::Tuple => Style::Tuple,
        ast::Style::Newtype => Style::Newtype,
        ast::Style::Unit => Style::Unit,
    }
}
fn tag(tag: &attr::TagType) -> Tag {
    match tag {
        attr::TagType::External => Tag::External,
        attr::TagType::Internal { tag } => Tag::Internal(tag.clone()),
        attr::TagType::Adjacent { tag, content } => Tag::Adjacent(tag.clone(), content.clone()),
        attr::TagType::None => Tag::Untagged,
    }
}
fn fields(
    fs: &[ast::Field],
    variant: Option<usize>,
    input: bool,
    transparent: bool,
    default: bool,
    style: ast::Style,
) -> Result<Vec<FieldPlan>, String> {
    let mut plans = Vec::new();
    for (index, f) in fs.iter().enumerate() {
        let a = &f.attrs;
        if (!matches!(style, ast::Style::Newtype)
            && if input {
                a.skip_deserializing()
            } else {
                a.skip_serializing()
            })
            || (transparent && !a.transparent())
        {
            continue;
        }
        let position = Position {
            variant,
            field: index,
        };
        let target = wire(&f.original.attrs, position, true, input)?;
        let name = if input {
            a.name().deserialize_name()
        } else {
            a.name().serialize_name()
        }
        .value
        .clone();
        let custom = if input {
            a.deserialize_with().is_some()
        } else {
            a.serialize_with().is_some()
        };
        let error = if custom && target.is_none() {
            Some(format!(
                "Custom serde field {name} requires a wire type override"
            ))
        } else {
            None
        };
        plans.push(FieldPlan {
            position,
            target,
            error,
            field: Field {
                docs: docs(&f.original.attrs),
                name,
                aliases: if input {
                    a.aliases().iter().map(|a| a.value.clone()).collect()
                } else {
                    Vec::new()
                },
                optional: if input {
                    default || !a.default().is_none()
                } else {
                    a.skip_serializing_if().is_some()
                },
                option_default: a.deserialize_with().is_none(),
                flatten: a.flatten(),
                hints: hints(&f.original.attrs),
            },
        });
    }
    Ok(plans)
}
pub(super) fn normalize(info: &DeclarationInfo, direction: Direction) -> Result<Plan, String> {
    let input = direction == Direction::Input;
    let source: syn::DeriveInput = syn::parse_str(info.source).map_err(|e| e.to_string())?;
    let hints = hints(&source.attrs);
    let mut plan = Plan {
        hints,
        target: None,
        style: None,
        tag: None,
        fields: Vec::new(),
        variants: Vec::new(),
    };
    if plan.hints.literal.is_some() {
        return Ok(plan);
    }
    let root = Position {
        variant: None,
        field: 0,
    };
    if let Some(target) = wire(&source.attrs, root, false, input)? {
        plan.target = Some(target);
        return Ok(plan);
    }
    let cx = Ctxt::new();
    let container = ast::Container::from_ast(
        &cx,
        &source,
        if input {
            Derive::Deserialize
        } else {
            Derive::Serialize
        },
        &syn::parse_quote!(__serde),
    );
    cx.check().map_err(|e| e.to_string())?;
    let container = container.ok_or("expected a struct or enum")?;
    if (input && !matches!(container.attrs.identifier(), attr::Identifier::No))
        || container.attrs.remote().is_some()
    {
        return Err(
            "Serde identifier and remote derives require an explicit shape override".into(),
        );
    }
    let conversion = if input {
        if container.attrs.type_from().is_some() {
            Some("from")
        } else if container.attrs.type_try_from().is_some() {
            Some("try_from")
        } else {
            None
        }
    } else if container.attrs.type_into().is_some() {
        Some("into")
    } else {
        None
    };
    if let Some(key) = conversion {
        plan.target = selected(&source.attrs, root, false, &["serde", key])?;
        return Ok(plan);
    }
    match &container.data {
        ast::Data::Struct(s, fs) => {
            plan.tag = match container.attrs.tag() {
                attr::TagType::External => None,
                attr::TagType::Internal { tag } => Some((
                    tag.clone(),
                    container.attrs.name().serialize_name().value.clone(),
                )),
                _ => return Err("Unsupported struct tagging".into()),
            };
            plan.style = Some(if container.attrs.transparent() {
                Style::Newtype
            } else {
                style(*s)
            });
            plan.fields = fields(
                fs,
                None,
                input,
                container.attrs.transparent(),
                !container.attrs.default().is_none(),
                *s,
            )?;
        }
        ast::Data::Enum(vs) => {
            for (index, v) in vs.iter().enumerate() {
                let a = &v.attrs;
                if if input {
                    a.skip_deserializing()
                } else {
                    a.skip_serializing()
                } {
                    continue;
                }
                let s = if matches!(v.style, ast::Style::Newtype)
                    && if input {
                        v.fields[0].attrs.skip_deserializing()
                    } else {
                        v.fields[0].attrs.skip_serializing()
                    } {
                    ast::Style::Unit
                } else {
                    v.style
                };
                let custom = if input {
                    a.deserialize_with().is_some()
                } else {
                    a.serialize_with().is_some()
                };
                let fs = if custom {
                    Vec::new()
                } else {
                    fields(&v.fields, Some(index), input, false, false, s)?
                };
                plan.variants.push(VariantPlan {
                    variant: Variant {
                        docs: docs(&v.original.attrs),
                        name: if input {
                            a.name().deserialize_name()
                        } else {
                            a.name().serialize_name()
                        }
                        .value
                        .clone(),
                        aliases: if input {
                            a.aliases().iter().map(|a| a.value.clone()).collect()
                        } else {
                            Vec::new()
                        },
                        style: style(s),
                        tag: if a.untagged() {
                            Tag::Untagged
                        } else {
                            tag(container.attrs.tag())
                        },
                        custom,
                        other: input && a.other(),
                    },
                    fields: fs,
                });
            }
        }
    }
    Ok(plan)
}
impl Target {
    fn callback(
        &self,
        targets: &[(TypeAttributeInfo, fn(&mut TSVisitor))],
    ) -> Option<fn(&mut TSVisitor)> {
        targets
            .iter()
            .find(|(info, _)| {
                info.position == self.position
                    && info
                        .path
                        .iter()
                        .copied()
                        .eq(self.path.iter().map(String::as_str))
            })
            .map(|(_, callback)| *callback)
    }
    fn required(
        &self,
        visitor: &mut TSVisitor,
        targets: &[(TypeAttributeInfo, fn(&mut TSVisitor))],
    ) -> Option<fn(&mut TSVisitor)> {
        let callback = self.callback(targets);
        if callback.is_none() {
            visitor.error(format!("Missing selected type metadata callback at {:?} for {}; add a local visit(type_attributes(...)) selector", self.position, self.path.join("::")));
        }
        callback
    }
}
impl FieldPlan {
    fn render(
        self,
        visitor: &mut TSVisitor,
        storage: &[(Position, Option<fn(&mut TSVisitor)>)],
        targets: &[(TypeAttributeInfo, fn(&mut TSVisitor))],
    ) {
        if let Some(error) = self.error {
            visitor.error(error);
            return;
        }
        let callback = if self.field.hints.skip {
            None
        } else if let Some(target) = self.target {
            target.required(visitor, targets)
        } else {
            storage
                .iter()
                .find(|(position, _)| *position == self.position)
                .and_then(|(_, callback)| *callback)
        };
        visitor.render_field(self.field, callback);
    }
}
impl Plan {
    pub(super) fn render(
        self,
        visitor: &mut TSVisitor,
        storage: &[(Position, Option<fn(&mut TSVisitor)>)],
        targets: &[(TypeAttributeInfo, fn(&mut TSVisitor))],
    ) {
        let Plan {
            hints,
            target,
            style,
            tag,
            fields,
            variants,
        } = self;
        visitor.errors.extend(hints.errors);
        if hints.skip {
            visitor.error("TypeScript skip requires a named field");
        }
        if let Some(literal) = hints.literal {
            visitor.ts.push_str(&literal);
        } else if let Some(target) = target {
            if let Some(callback) = target.required(visitor, targets) {
                callback(visitor);
            }
        } else if let Some(style) = style {
            visitor.structure(style, tag, |visitor| {
                for field in fields {
                    field.render(visitor, storage, targets);
                }
            });
        } else {
            visitor.enumeration(|visitor| {
                for VariantPlan { variant, fields } in variants {
                    visitor.variant(variant, |visitor| {
                        for field in fields {
                            field.render(visitor, storage, targets);
                        }
                    });
                }
            });
        }
    }
}
