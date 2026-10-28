//! Zod expressions from a declaration's input and output serde plans.
//!
//! A shape renders once wherever both plans agree; a disagreement renders each
//! side and joins them with `wire`, at the narrowest level that can hold it.

use std::collections::BTreeMap;

use visit_rs::reflection::{Position, TypeAttributeInfo};

use super::normalize::{
    Direction, FieldPlan, Hints, Plan, Style, Tag, Target, Variant, VariantPlan,
};
use super::types::{jsdoc, json_str, Edge, Expr, TSVisitor};

type Callback = fn(&mut TSVisitor);
pub(super) type Storage = [(Position, Option<Callback>)];
pub(super) type Targets = [(TypeAttributeInfo, Callback)];

fn key(position: Position) -> (Option<usize>, usize) {
    (position.variant, position.field)
}

impl Target {
    fn callback(&self, targets: &Targets) -> Option<Callback> {
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
}

struct Rendered {
    expr: Expr,
    option: bool,
    recursive: bool,
    edges: Vec<Edge>,
}

struct Payload {
    expr: Expr,
    /// A newtype payload whose single field is nullable.
    nullable: bool,
    /// Holds a reference to a declaration still being rendered.
    recursive: bool,
    /// References not yet committed; an object member may still defer them.
    edges: Vec<Edge>,
}

impl Payload {
    fn plain(expr: Expr) -> Self {
        Self {
            expr,
            nullable: false,
            recursive: false,
            edges: Vec::new(),
        }
    }
}

struct Property {
    name: String,
    docs: Vec<String>,
    expr: Expr,
    recursive: bool,
}

fn compatible(input: &FieldPlan, output: &FieldPlan) -> bool {
    input.field.name == output.field.name
        && input.field.aliases.is_empty()
        && output.field.aliases.is_empty()
        && input.field.flatten == output.field.flatten
        && input.field.hints == output.field.hints
        && input.target == output.target
        && input.error == output.error
}

fn pair_fields<'a>(
    input: &'a [FieldPlan],
    output: &'a [FieldPlan],
) -> Option<Vec<(&'a FieldPlan, &'a FieldPlan)>> {
    let mut pairs: BTreeMap<_, (Option<&FieldPlan>, Option<&FieldPlan>)> = BTreeMap::new();
    for field in input {
        pairs.entry(key(field.position)).or_default().0 = Some(field);
    }
    for field in output {
        pairs.entry(key(field.position)).or_default().1 = Some(field);
    }
    pairs
        .into_values()
        .map(|pair| match pair {
            (Some(input), Some(output)) if compatible(input, output) => Some((input, output)),
            _ => None,
        })
        .collect()
}

fn pair_variants<'a>(
    input: &'a [VariantPlan],
    output: &'a [VariantPlan],
) -> Option<Vec<(&'a VariantPlan, &'a VariantPlan)>> {
    let mut pairs: BTreeMap<_, (Option<&VariantPlan>, Option<&VariantPlan>)> = BTreeMap::new();
    for variant in input {
        pairs.entry(variant.index).or_default().0 = Some(variant);
    }
    for variant in output {
        pairs.entry(variant.index).or_default().1 = Some(variant);
    }
    pairs
        .into_values()
        .map(|pair| match pair {
            (Some(input), Some(output)) if input.variant == output.variant => Some((input, output)),
            _ => None,
        })
        .collect()
}

fn property(name: &str, docs: &[String], expr: Expr) -> Property {
    Property {
        name: name.to_owned(),
        docs: docs.to_vec(),
        expr,
        recursive: false,
    }
}

/// An object schema; recursive members become annotated getters.
fn object(properties: Vec<Property>) -> Expr {
    let mut values = Vec::new();
    let mut types = Vec::new();
    for property in properties {
        let name = json_str(&property.name);
        let docs = jsdoc(&property.docs);
        let expr = property.expr.described(None, &property.docs);
        types.push(format!("{docs}{name}: {}", expr.ty));
        values.push(if property.recursive {
            format!(
                "{docs}get {name}(): {} {{ return {}; }}",
                expr.ty, expr.value
            )
        } else {
            format!("{docs}{name}: {}", expr.value)
        });
    }
    Expr::object(
        format!("{{ {} }}", values.join(", ")),
        format!("z.ZodObject<{{ {} }}>", types.join("; ")),
    )
}

/// Folds an object's own members with its flattened and alias projections.
fn assemble(properties: Vec<Property>, intersections: Vec<Expr>) -> Expr {
    let mut parts = intersections.into_iter();
    let mut expression = if properties.is_empty() {
        match parts.next() {
            Some(first) => first,
            None => Expr::empty(),
        }
    } else {
        object(properties)
    };
    for part in parts {
        expression = expression.intersection(part);
    }
    expression
}

fn variant_names(variant: &Variant) -> Vec<String> {
    let mut names = vec![variant.name.clone()];
    names.extend(variant.aliases.iter().cloned());
    names.sort_unstable();
    names.dedup();
    names
}

fn only_units(variants: &[&Variant]) -> bool {
    variants
        .iter()
        .all(|variant| variant.style == Style::Unit && variant.tag == Tag::External)
}

impl TSVisitor {
    pub(super) fn render_plans(
        &mut self,
        input: Plan,
        output: Plan,
        storage: &Storage,
        targets: &Targets,
    ) {
        self.errors.extend(input.hints.errors.iter().cloned());
        if input.hints.skip {
            self.error("TypeScript skip requires a named field");
        }
        if let Some(literal) = &input.hints.literal {
            let literal = self.literal(literal);
            self.push(literal);
            return;
        }
        let expression = self.joint(&input, &output, storage, targets);
        self.push(expression);
    }

    fn required(&mut self, target: &Target, targets: &Targets) -> Option<Callback> {
        let callback = target.callback(targets);
        if callback.is_none() {
            self.error(format!(
                "Missing selected type metadata callback at {:?} for {}; add a local visit(type_attributes(...)) selector",
                target.position,
                target.path.join("::")
            ));
        }
        callback
    }

    fn joint(&mut self, input: &Plan, output: &Plan, storage: &Storage, targets: &Targets) -> Expr {
        if input.target.is_some() || output.target.is_some() {
            if input.target != output.target {
                return self.split(input, output, storage, targets);
            }
            return match self.required(input.target.as_ref().unwrap(), targets) {
                Some(callback) => self.capture(callback),
                None => Expr::default(),
            };
        }
        match (input.style, output.style) {
            (Some(style), Some(_)) => {
                let tag = input
                    .tag
                    .as_ref()
                    .map(|(tag, name)| self.tag_object(tag, name, None));
                let payload = match self.payload_joint(
                    style,
                    &input.fields,
                    &output.fields,
                    storage,
                    targets,
                ) {
                    Some(payload) => self.settle(payload, false),
                    None => {
                        let i = self.payload_side(
                            style,
                            &input.fields,
                            Direction::Input,
                            storage,
                            targets,
                        );
                        let i = self.settle(i, false);
                        let o = self.payload_side(
                            style,
                            &output.fields,
                            Direction::Output,
                            storage,
                            targets,
                        );
                        let o = self.settle(o, false);
                        self.wired(i, o)
                    }
                };
                tagged(tag, payload)
            }
            (None, None) => {
                self.enumeration_joint(&input.variants, &output.variants, storage, targets)
            }
            _ => self.split(input, output, storage, targets),
        }
    }

    fn split(&mut self, input: &Plan, output: &Plan, storage: &Storage, targets: &Targets) -> Expr {
        let i = self.side(input, Direction::Input, storage, targets);
        let o = self.side(output, Direction::Output, storage, targets);
        self.wired(i, o)
    }

    fn side(
        &mut self,
        plan: &Plan,
        direction: Direction,
        storage: &Storage,
        targets: &Targets,
    ) -> Expr {
        if let Some(target) = &plan.target {
            return match self.required(target, targets) {
                Some(callback) => self.capture(callback),
                None => Expr::default(),
            };
        }
        match plan.style {
            Some(style) => {
                let tag = plan
                    .tag
                    .as_ref()
                    .map(|(tag, name)| self.tag_object(tag, name, Some(direction)));
                let payload = self.payload_side(style, &plan.fields, direction, storage, targets);
                let payload = self.settle(payload, false);
                tagged(tag, payload)
            }
            None => self.enumeration_side(&plan.variants, direction, storage, targets),
        }
    }

    /// The tag object of an internally tagged struct; serde ignores the tag on input.
    fn tag_object(&mut self, tag: &str, name: &str, direction: Option<Direction>) -> Expr {
        let literal = Expr::literal(&[name.to_owned()]);
        let value = match direction {
            None => self.wired(literal.clone().optional(), literal),
            Some(Direction::Input) => literal.optional(),
            Some(Direction::Output) => literal,
        };
        object(vec![property(tag, &[], value)])
    }

    fn field_callback(
        &mut self,
        plan: &FieldPlan,
        storage: &Storage,
        targets: &Targets,
    ) -> Result<Option<Callback>, ()> {
        if let Some(error) = &plan.error {
            self.error(error.clone());
            return Err(());
        }
        if plan.field.hints.skip {
            return Ok(None);
        }
        if let Some(target) = &plan.target {
            return self.required(target, targets).map(Some).ok_or(());
        }
        match storage
            .iter()
            .find(|(position, _)| *position == plan.position)
            .and_then(|(_, callback)| *callback)
        {
            Some(callback) => Ok(Some(callback)),
            None => {
                self.error(format!(
                    "Opaque field {} requires a typed optionality fact from storage or a selected wire target",
                    plan.field.name
                ));
                Err(())
            }
        }
    }

    fn render_type(&mut self, hints: &Hints, callback: Callback) -> Rendered {
        self.errors.extend(hints.errors.iter().cloned());
        let (expr, option, recursive, edges) = match &hints.literal {
            Some(literal) => {
                let (_, option, recursive, edges) =
                    self.capture_field(|visitor| visitor.with_suppressed_types(callback));
                (self.literal(literal), option, recursive, edges)
            }
            None => self.capture_field(callback),
        };
        Rendered {
            expr,
            option,
            recursive,
            edges,
        }
    }

    fn optionality(&mut self, expr: Expr, input: bool, output: bool) -> Expr {
        match (input, output) {
            (false, false) => expr,
            (true, true) => expr.optional(),
            (true, false) => self.wired(expr.clone().optional(), expr),
            (false, true) => self.wired(expr.clone(), expr.optional()),
        }
    }

    fn flattened(rendered: Rendered, docs: &[String]) -> Expr {
        let expr = if rendered.option {
            Expr::union(vec![rendered.expr, Expr::empty()])
        } else {
            rendered.expr
        };
        expr.described(None, docs)
    }

    fn render_fields<'a>(
        &mut self,
        style: Style,
        fields: impl Iterator<Item = &'a FieldPlan>,
        storage: &Storage,
        targets: &Targets,
    ) -> Vec<(&'a FieldPlan, Rendered)> {
        let mut rendered = Vec::new();
        for plan in fields {
            let Ok(callback) = self.field_callback(plan, storage, targets) else {
                continue;
            };
            let Some(callback) = callback else {
                if style != Style::Struct {
                    self.error("Omission requires a named field");
                }
                continue;
            };
            let field = self.render_type(&plan.field.hints, callback);
            rendered.push((plan, field));
        }
        rendered
    }

    /// Renders both plans as one payload; `None` where a field differs beyond optionality.
    fn payload_joint(
        &mut self,
        style: Style,
        input: &[FieldPlan],
        output: &[FieldPlan],
        storage: &Storage,
        targets: &Targets,
    ) -> Option<Payload> {
        let pairs = pair_fields(input, output)?;
        let outputs: BTreeMap<_, _> = pairs.iter().map(|(i, o)| (key(i.position), *o)).collect();
        let mut rendered: Vec<_> = self
            .render_fields(style, pairs.iter().map(|(i, _)| *i), storage, targets)
            .into_iter()
            .map(|(plan, field)| {
                let in_opt = plan.field.optional
                    || (style == Style::Struct && field.option && plan.field.option_default);
                let out_opt = outputs[&key(plan.position)].field.optional;
                (plan, field, in_opt, out_opt)
            })
            .collect();
        Some(match style {
            Style::Unit => Payload::plain(Expr::new("z.null()", "z.ZodNull")),
            Style::Newtype if rendered.is_empty() => Payload::plain(self.wired(
                Expr::new("z.unknown()", "z.ZodUnknown"),
                Expr::new("z.null()", "z.ZodNull"),
            )),
            Style::Newtype => {
                if rendered.len() != 1 {
                    self.error("A newtype must have one visible field");
                }
                let (plan, field, _, _) = rendered.into_iter().next().unwrap();
                Payload {
                    nullable: field.option || field.expr.value == "z.null()",
                    recursive: field.recursive,
                    edges: field.edges,
                    expr: field.expr.described(None, &plan.field.docs),
                }
            }
            Style::Tuple => {
                if rendered.iter().any(|(_, _, i, o)| i != o) {
                    return None;
                }
                let recursive = rendered.iter().any(|(_, field, _, _)| field.recursive);
                let edges = rendered
                    .iter_mut()
                    .flat_map(|(_, field, _, _)| std::mem::take(&mut field.edges))
                    .collect();
                let expr = self.tuple(
                    rendered
                        .into_iter()
                        .map(|(plan, field, optional, _)| (plan, field, optional)),
                    Direction::Input,
                );
                Payload {
                    expr,
                    nullable: false,
                    recursive,
                    edges,
                }
            }
            Style::Struct => {
                let mut properties = Vec::new();
                let mut intersections = Vec::new();
                for (plan, mut field, in_opt, out_opt) in rendered {
                    let recursive = field.recursive && !plan.field.flatten;
                    self.commit_edges(std::mem::take(&mut field.edges), recursive, field.recursive);
                    if plan.field.flatten {
                        intersections.push(Self::flattened(field, &plan.field.docs));
                        continue;
                    }
                    let expr = self.optionality(field.expr, in_opt, out_opt);
                    properties.push(Property {
                        recursive,
                        ..property(&plan.field.name, &plan.field.docs, expr)
                    });
                }
                Payload::plain(assemble(properties, intersections))
            }
        })
    }

    fn tuple<'a>(
        &mut self,
        elements: impl Iterator<Item = (&'a FieldPlan, Rendered, bool)>,
        direction: Direction,
    ) -> Expr {
        let mut optional_seen = false;
        let mut rendered = Vec::new();
        for (plan, field, optional) in elements {
            if direction == Direction::Output && optional {
                self.error("Conditional tuple fields require a TypeScript override");
            }
            if optional_seen && !optional {
                self.error(
                    "A required tuple field after an optional field requires a TypeScript override",
                );
            }
            optional_seen |= optional;
            let expr = if optional {
                field.expr.optional()
            } else {
                field.expr
            };
            rendered.push(expr.described(None, &plan.field.docs));
        }
        Expr::tuple(rendered)
    }

    fn payload_side(
        &mut self,
        style: Style,
        fields: &[FieldPlan],
        direction: Direction,
        storage: &Storage,
        targets: &Targets,
    ) -> Payload {
        let mut rendered: Vec<_> = self
            .render_fields(style, fields.iter(), storage, targets)
            .into_iter()
            .map(|(plan, field)| {
                let optional = plan.field.optional
                    || (direction == Direction::Input
                        && style == Style::Struct
                        && field.option
                        && plan.field.option_default);
                (plan, field, optional)
            })
            .collect();
        match style {
            Style::Unit => Payload::plain(Expr::new("z.null()", "z.ZodNull")),
            Style::Newtype if rendered.is_empty() => Payload::plain(match direction {
                Direction::Input => Expr::new("z.unknown()", "z.ZodUnknown"),
                Direction::Output => Expr::new("z.null()", "z.ZodNull"),
            }),
            Style::Newtype => {
                if rendered.len() != 1 {
                    self.error("A newtype must have one visible field");
                }
                let (plan, field, _) = rendered.into_iter().next().unwrap();
                Payload {
                    nullable: field.option || field.expr.value == "z.null()",
                    recursive: field.recursive,
                    edges: field.edges,
                    expr: field.expr.described(None, &plan.field.docs),
                }
            }
            Style::Tuple => {
                let recursive = rendered.iter().any(|(_, field, _)| field.recursive);
                let edges = rendered
                    .iter_mut()
                    .flat_map(|(_, field, _)| std::mem::take(&mut field.edges))
                    .collect();
                Payload {
                    expr: self.tuple(rendered.into_iter(), direction),
                    nullable: false,
                    recursive,
                    edges,
                }
            }
            Style::Struct => {
                let mut properties = Vec::new();
                let mut intersections = Vec::new();
                for (plan, mut field, optional) in rendered {
                    let recursive = field.recursive && !plan.field.flatten;
                    self.commit_edges(std::mem::take(&mut field.edges), recursive, field.recursive);
                    if plan.field.flatten {
                        intersections.push(Self::flattened(field, &plan.field.docs));
                        continue;
                    }
                    let mut names = vec![plan.field.name.clone()];
                    names.extend(plan.field.aliases.iter().cloned());
                    names.sort_unstable();
                    names.dedup();
                    if names.len() > 1 && !optional {
                        intersections.push(Expr::union(
                            names
                                .iter()
                                .map(|name| object(vec![property(name, &[], field.expr.clone())]))
                                .collect(),
                        ));
                    }
                    let expr = if optional || names.len() > 1 {
                        field.expr.optional()
                    } else {
                        field.expr
                    };
                    for name in &names {
                        properties.push(Property {
                            recursive: field.recursive,
                            ..property(name, &plan.field.docs, expr.clone())
                        });
                    }
                }
                Payload::plain(assemble(properties, intersections))
            }
        }
    }

    /// Commits a payload's references, deferred where an object member holds it.
    fn settle(&mut self, payload: Payload, deferred: bool) -> Expr {
        self.commit_edges(payload.edges, deferred, payload.recursive);
        payload.expr
    }

    fn enumeration_joint(
        &mut self,
        input: &[VariantPlan],
        output: &[VariantPlan],
        storage: &Storage,
        targets: &Targets,
    ) -> Expr {
        let Some(pairs) = pair_variants(input, output) else {
            let i = self.enumeration_side(input, Direction::Input, storage, targets);
            let o = self.enumeration_side(output, Direction::Output, storage, targets);
            return self.wired(i, o);
        };
        if pairs.is_empty() {
            return Expr::new("z.never()", "z.ZodNever");
        }
        if only_units(&pairs.iter().map(|(i, _)| &i.variant).collect::<Vec<_>>()) {
            return Expr::enumeration(
                &pairs
                    .iter()
                    .map(|(i, _)| i.variant.name.clone())
                    .collect::<Vec<_>>(),
            );
        }
        let mut variants = Vec::new();
        for (i, o) in pairs {
            self.check_variant(&i.variant);
            let payload =
                match self.payload_joint(i.variant.style, &i.fields, &o.fields, storage, targets) {
                    Some(payload) => payload,
                    None => {
                        let a = self.payload_side(
                            i.variant.style,
                            &i.fields,
                            Direction::Input,
                            storage,
                            targets,
                        );
                        let b = self.payload_side(
                            i.variant.style,
                            &o.fields,
                            Direction::Output,
                            storage,
                            targets,
                        );
                        let mut edges = a.edges;
                        edges.extend(b.edges);
                        Payload {
                            expr: self.wired(a.expr, b.expr),
                            nullable: a.nullable || b.nullable,
                            recursive: a.recursive || b.recursive,
                            edges,
                        }
                    }
                };
            variants.push(self.wrap_variant(&i.variant, &[i.variant.name.clone()], payload));
        }
        Expr::union(variants)
    }

    fn enumeration_side(
        &mut self,
        variants: &[VariantPlan],
        direction: Direction,
        storage: &Storage,
        targets: &Targets,
    ) -> Expr {
        if variants.is_empty() {
            return Expr::new("z.never()", "z.ZodNever");
        }
        if only_units(&variants.iter().map(|v| &v.variant).collect::<Vec<_>>()) {
            return Expr::enumeration(
                &variants
                    .iter()
                    .flat_map(|v| variant_names(&v.variant))
                    .collect::<Vec<_>>(),
            );
        }
        let mut rendered = Vec::new();
        for plan in variants {
            self.check_variant(&plan.variant);
            let payload = self.payload_side(
                plan.variant.style,
                &plan.fields,
                direction,
                storage,
                targets,
            );
            rendered.push(self.wrap_variant(&plan.variant, &variant_names(&plan.variant), payload));
        }
        Expr::union(rendered)
    }

    fn check_variant(&mut self, variant: &Variant) {
        if variant.custom || variant.other {
            self.error(format!(
                "Custom variant {} requires a TypeScript override",
                variant.name
            ));
        }
    }
}

fn tagged(tag: Option<Expr>, payload: Expr) -> Expr {
    match tag {
        Some(tag) => tag.intersection(payload),
        None => payload,
    }
}

impl TSVisitor {
    fn wrap_variant(&mut self, variant: &Variant, names: &[String], payload: Payload) -> Expr {
        let docs = &variant.docs;
        let unit = variant.style == Style::Unit;
        let literal = Expr::literal(names);
        let member = |name: &str, docs: &[String], payload: &Payload| Property {
            recursive: payload.recursive,
            ..property(name, docs, payload.expr.clone())
        };
        match &variant.tag {
            Tag::External if unit => literal.described(None, docs),
            Tag::External => {
                let union = Expr::union(
                    names
                        .iter()
                        .map(|name| object(vec![member(name, docs, &payload)]))
                        .collect(),
                )
                .described(None, docs);
                let deferred = payload.recursive;
                self.settle(payload, deferred);
                union
            }
            Tag::Internal(tag) => {
                let tagged = object(vec![property(tag, docs, literal)]);
                if unit {
                    tagged
                } else if variant.style == Style::Newtype && payload.nullable {
                    let payload = self.settle(payload, false);
                    Expr::union(vec![tagged.clone().intersection(payload), tagged])
                } else {
                    let payload = self.settle(payload, false);
                    tagged.intersection(payload)
                }
            }
            Tag::Adjacent(tag, content) => {
                if unit {
                    object(vec![property(tag, docs, literal)])
                } else {
                    let object = object(vec![
                        property(tag, docs, literal),
                        member(content, &[], &payload),
                    ]);
                    let deferred = payload.recursive;
                    self.settle(payload, deferred);
                    object
                }
            }
            Tag::Untagged => {
                let expr = self.settle(payload, false);
                expr.described(None, docs)
            }
        }
    }
}
