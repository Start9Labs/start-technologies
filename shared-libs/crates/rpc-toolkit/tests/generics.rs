#![cfg(feature = "ts")]

use std::path::Path;
use std::process::Command;

use rpc_toolkit::reflect_ts;
use rpc_toolkit::ts::{export_namespace, Direction, Param, TSVisitor, TS};
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

fn module<T: TS>(direction: Direction) -> String {
    let mut visitor = TSVisitor::new();
    visitor.with_direction(direction, |v| v.append_type::<T>());
    visitor.into_module("TestType").unwrap()
}

fn typecheck(module: &str, assertions: &str) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("fixture.ts");
    std::fs::write(&file, format!("{module}\n{assertions}")).unwrap();
    let tsc =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../node_modules/typescript/bin/tsc");
    assert!(
        tsc.exists(),
        "Run npm ci at the monorepo root before testing the ts feature"
    );
    let output = Command::new("node")
        .current_dir(dir.path())
        .arg(tsc)
        .args([
            "--strict",
            "--exactOptionalPropertyTypes",
            "--noEmit",
            "--skipLibCheck",
            "--target",
            "ES2020",
        ])
        .arg(file)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
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
    let output = module::<Holder>(Direction::Output);
    assert!(output.contains("export type Tree<T> = "));
    assert!(output.contains("\"childNodes\":((Tree<T>)[])"));
    assert!(output.contains("export type Pair<A,B> = "));
    assert!(output.contains("\"left\":(Tree<(A)[]>)"));
    assert!(output.contains("export type Source<Password> = "));
    assert!(output.contains("\"tree\":(Tree<string>)"));
    assert!(output.contains("\"pair\":(Pair<number,Inner>)"));
    assert!(output.contains("\"source\":(Source<(string|null)>)"));
    assert_eq!(output.matches("export type Tree<").count(), 1);
    assert!(output.contains(" * A recursive tree.\n */\nexport type Tree<T>"));

    let input = module::<Holder>(Direction::Input);
    assert!(input.contains("export type TreeInput<T> = "));
    assert!(input.contains("\"pair\":(PairInput<number,InnerInput>)"));
    assert!(input.contains("\"count\"?:(number)"));
}

#[test]
fn generic_instances_typecheck() {
    let value = serde_json::to_string(&holder()).unwrap();
    typecheck(
        &module::<Holder>(Direction::Output),
        &format!(
            "const value: TestType = {value};\n\
             const tree: Tree<number> = {{ value: 1, childNodes: [] }};\n\
             // @ts-expect-error\n\
             const wrong: Tree<number> = {{ value: 'one', childNodes: [] }};\n\
             // @ts-expect-error\n\
             const nested: Tree<number> = {{ value: 1, childNodes: [{{ value: 'two', childNodes: [] }}] }};\n\
             const source: Source<number> = {{ type: 'backup', password: 1 }};\n\
             // @ts-expect-error\n\
             const migrate: Source<number> = {{ type: 'migrate', password: 1 }};\n"
        ),
    );
    let input = serde_json::json!({
        "tree": {"value": "root", "childNodes": []},
        "pair": {"left": {"value": [1], "childNodes": []}, "right": {}},
        "source": {"type": "migrate", "guid": "guid"},
    });
    serde_json::from_value::<Holder>(input.clone()).unwrap();
    typecheck(
        &module::<Holder>(Direction::Input),
        &format!("const input: TestType = {input};\n"),
    );
}

#[test]
fn exported_families_register_without_an_instance() {
    let output = export_namespace("generics", "family").unwrap();
    assert!(output.contains("export type Source<Password> = "));
    assert!(output.contains("export type SourceInput<Password> = "));
}

#[test]
fn parameters_cannot_shadow_declarations() {
    let mut visitor = TSVisitor::new();
    visitor.append_type::<Shadowed<u32>>();
    assert!(visitor
        .into_declarations()
        .unwrap_err()
        .to_string()
        .contains("Generic parameter Inner of Shadowed shadows a declaration"));
}

#[test]
fn parameters_outside_their_family_fail_generation() {
    let mut visitor = TSVisitor::new();
    visitor.append_type::<Param<0>>();
    assert!(visitor
        .into_declarations()
        .unwrap_err()
        .to_string()
        .contains("outside its declaration"));
}
