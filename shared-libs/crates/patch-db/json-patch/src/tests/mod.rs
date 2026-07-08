#![allow(unused)]
extern crate rand;

mod util;

use serde_json::from_str;

use super::*;

#[test]
fn parse_from_value() {
    use PatchOperation::*;

    let json = json!([{"op": "add", "path": "/a/b", "value": 1}, {"op": "remove", "path": "/c"}]);
    let patch: Patch = from_value(json).unwrap();

    assert_eq!(
        patch,
        Patch(vec![
            Add(AddOperation {
                path: "/a/b".parse().unwrap(),
                value: json!(1),
            }),
            Remove(RemoveOperation {
                path: "/c".parse().unwrap(),
            }),
        ])
    );

    let _patch: Patch =
        from_str(r#"[{"op": "add", "path": "/a/b", "value": 1}, {"op": "remove", "path": "/c"}]"#)
            .unwrap();
}

#[test]
fn parse_from_string() {
    use PatchOperation::*;

    let patch: Patch =
        from_str(r#"[{"op": "add", "path": "/a/b", "value": 1}, {"op": "remove", "path": "/c"}]"#)
            .unwrap();

    assert_eq!(
        patch,
        Patch(vec![
            Add(AddOperation {
                path: "/a/b".parse().unwrap(),
                value: json!(1),
            }),
            Remove(RemoveOperation {
                path: "/c".parse().unwrap()
            }),
        ])
    );
}

#[test]
fn serialize_patch() {
    let s = r#"[{"op":"add","path":"/a/b","value":1},{"op":"remove","path":"/c"}]"#;
    let patch: Patch = from_str(s).unwrap();

    let serialized = serde_json::to_string(&patch).unwrap();
    assert_eq!(serialized, s);
}

#[test]
fn tests() {
    util::run_specs("specs/tests.json");
}

#[test]
fn spec_tests() {
    util::run_specs("specs/spec_tests.json");
}

#[test]
fn revert_tests() {
    util::run_specs("specs/revert_tests.json");
}

#[test]
fn merge_tests() {
    util::run_specs("specs/merge_tests.json");
}

#[test]
fn many_ops_no_stack_overflow() {
    let mut doc = json!({"items": {}});
    let ops: Vec<PatchOperation> = (0..10_000)
        .map(|i| {
            PatchOperation::Add(AddOperation {
                path: format!("/items/{i}").parse().unwrap(),
                value: json!(i),
            })
        })
        .collect();
    let p = Patch(ops);
    patch(&mut doc, &p).unwrap();
    assert_eq!(doc["items"].as_object().unwrap().len(), 10_000);
}

#[test]
fn many_ops_undo_on_failure() {
    let original = json!({"items": {}});
    let mut doc = original.clone();
    let mut ops: Vec<PatchOperation> = (0..1000)
        .map(|i| {
            PatchOperation::Add(AddOperation {
                path: format!("/items/{i}").parse().unwrap(),
                value: json!(i),
            })
        })
        .collect();
    // Final op fails: test against a wrong value at a valid path
    ops.push(PatchOperation::Test(TestOperation {
        path: "/items/0".parse().unwrap(),
        value: json!("wrong"),
    }));
    let p = Patch(ops);
    assert!(patch(&mut doc, &p).is_err());
    assert_eq!(
        doc, original,
        "document should be fully restored after failed patch"
    );
}
