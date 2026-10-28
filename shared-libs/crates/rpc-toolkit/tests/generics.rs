#![cfg(feature = "ts")]

mod common;

use common::{module, typecheck, SCHEMAS};
use rpc_toolkit::reflect_ts;
use rpc_toolkit::ts::{export_namespace, Param, TSVisitor};
use serde::{Deserialize, Serialize};

/// A recursive tree.
#[derive(Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
struct Tree<T> {
    value: T,
    child_nodes: Vec<Tree<T>>,
}
reflect_ts!(generic Tree<T>);

#[derive(Serialize, Deserialize, visit_rs::VisitFields)]
struct Pair<A, B> {
    left: Tree<Vec<A>>,
    right: Option<B>,
    #[serde(default)]
    count: u32,
}
reflect_ts!(generic Pair<A, B>);

#[derive(Serialize, Deserialize, visit_rs::VisitVariants)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Source<Password> {
    Migrate { guid: String },
    Backup { password: Password },
}
reflect_ts!(generic Source<Password>);

#[derive(Serialize, Deserialize, visit_rs::VisitFields)]
struct Inner {
    #[serde(default)]
    flag: bool,
}
reflect_ts!(Inner);

#[derive(Serialize, Deserialize, visit_rs::VisitFields)]
struct Holder {
    tree: Tree<String>,
    pair: Pair<u32, Inner>,
    source: Source<Option<String>>,
}
reflect_ts!(Holder);
rpc_toolkit::ts_export!(Holder, namespaces = ["generics"]);
rpc_toolkit::ts_export!(generic Source<Password>, namespaces = ["family"]);

#[derive(Serialize, Deserialize, visit_rs::VisitFields)]
struct Shadowed<Inner> {
    value: Inner,
    other: crate::Inner,
}
reflect_ts!(generic Shadowed<Inner>);

#[allow(non_camel_case_types)]
mod literal_shadow {
    use super::*;

    #[derive(Serialize, Deserialize, visit_rs::VisitFields)]
    pub struct LiteralShadow<literal_0> {
        value: literal_0,
        #[visit(ts(type = "z.string()"))]
        text: String,
    }
    reflect_ts!(generic LiteralShadow<literal_0>);
}

fn holder() -> Holder {
    Holder {
        tree: Tree {
            value: "root".into(),
            child_nodes: vec![Tree {
                value: "leaf".into(),
                child_nodes: Vec::new(),
            }],
        },
        pair: Pair {
            left: Tree {
                value: vec![1, 2],
                child_nodes: Vec::new(),
            },
            right: Some(Inner { flag: true }),
            count: 3,
        },
        source: Source::Backup { password: None },
    }
}

#[test]
fn families_declare_once_and_instances_reference_them() {
    let bindings = module::<Holder>();
    let schemas = &bindings.schemas;
    assert!(schemas
        .contains("export const Tree = <T extends z.ZodType>(T: T) => {\n  const self = object("));
    assert!(
        schemas.contains("get \"childNodes\"(): z.ZodArray<typeof self> { return z.array(self); }")
    );
    assert!(schemas
        .contains("export const Pair = <A extends z.ZodType, B extends z.ZodType>(A: A, B: B) =>"));
    assert!(schemas.contains("\"left\": Tree(z.array(A))"));
    assert!(schemas.contains("\"count\": wire(z.int().optional(), z.int())"));
    assert!(schemas
        .contains("export const Source = <Password extends z.ZodType>(Password: Password) =>"));
    assert!(schemas.contains("\"tree\": Tree(z.string())"));
    assert!(schemas.contains("\"pair\": Pair(z.int(), Inner)"));
    assert!(schemas.contains("\"source\": Source(z.string().nullable())"));
    assert_eq!(schemas.matches("export const Tree").count(), 1);
    assert!(schemas.contains(" * A recursive tree.\n */\nexport const Tree"));
    let types = &bindings.types;
    assert!(types.contains(
        "export type Tree<T> = z.output<ReturnType<typeof schemas.Tree<z.ZodType<T, T>>>>;"
    ));
    assert!(types.contains(
        "export type TreeInput<T> = z.input<ReturnType<typeof schemas.Tree<z.ZodType<T, T>>>>;"
    ));
    assert!(types.contains("export type Pair<A, B> = z.output<ReturnType<typeof schemas.Pair<z.ZodType<A, A>, z.ZodType<B, B>>>>;"));
}

#[test]
fn generic_instances_typecheck() {
    let value = serde_json::to_string(&holder()).unwrap();
    let input = serde_json::json!({
        "tree": {"value": "root", "childNodes": []},
        "pair": {"left": {"value": [1], "childNodes": []}, "right": {}},
        "source": {"type": "migrate", "guid": "guid"},
    });
    serde_json::from_value::<Holder>(input.clone()).unwrap();
    typecheck(
        &module::<Holder>(),
        &format!(
            "const value: TestType = {value};\n\
             const input: TestTypeInput = {input};\n\
             const tree: Tree<number> = {{ value: 1, childNodes: [] }};\n\
             // @ts-expect-error\n\
             const wrong: Tree<number> = {{ value: 'one', childNodes: [] }};\n\
             // @ts-expect-error\n\
             const nested: Tree<number> = {{ value: 1, childNodes: [{{ value: 'two', childNodes: [] }}] }};\n\
             const source: Source<number> = {{ type: 'backup', password: 1 }};\n\
             // @ts-expect-error\n\
             const migrate: Source<number> = {{ type: 'migrate', password: 1 }};\n\
             const pair: PairInput<number, InnerInput> = {{ left: {{ value: [1], childNodes: [] }}, right: null }};\n\
             // @ts-expect-error\n\
             const counted: Pair<number, Inner> = {{ left: {{ value: [1], childNodes: [] }}, right: null }};\n"
        ),
    );
}

#[test]
fn exported_families_register_without_an_instance() {
    let bindings = export_namespace("generics", "family", SCHEMAS).unwrap();
    assert!(bindings
        .schemas
        .contains("export const Source = <Password extends z.ZodType>"));
    assert!(bindings.types.contains("export type Source<Password> = "));
    assert!(bindings
        .types
        .contains("export type SourceInput<Password> = "));
}

#[test]
fn parameters_cannot_shadow_declarations() {
    let mut visitor = TSVisitor::new();
    visitor.append_type::<Shadowed<u32>>();
    assert!(visitor
        .into_bindings(None, SCHEMAS)
        .unwrap_err()
        .to_string()
        .contains("Generic parameter Inner of Shadowed shadows a declaration"));
}

#[test]
fn parameters_cannot_shadow_hoisted_literals() {
    let mut visitor = TSVisitor::new();
    visitor.append_type::<literal_shadow::LiteralShadow<u32>>();
    assert!(visitor
        .into_bindings(None, SCHEMAS)
        .unwrap_err()
        .to_string()
        .contains("Generic parameter literal_0 of LiteralShadow shadows a declaration"));
}

#[test]
fn parameters_outside_their_family_fail_generation() {
    let mut visitor = TSVisitor::new();
    visitor.append_type::<Param<0>>();
    assert!(visitor
        .into_bindings(None, SCHEMAS)
        .unwrap_err()
        .to_string()
        .contains("outside its declaration"));
}
