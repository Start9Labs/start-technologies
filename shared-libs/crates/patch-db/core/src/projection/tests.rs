use imbl_value::{json, Value};
use json_ptr::{JsonPointer, ROOT};
use proptest::prelude::*;

use super::*;
use crate::patch::diff;

fn projection(depth: Option<usize>, include: &[&str], exclude: &[&str]) -> Projection {
    Projection {
        depth,
        include: include.iter().map(|p| p.parse().unwrap()).collect(),
        exclude: exclude.iter().map(|p| p.parse().unwrap()).collect(),
    }
}

fn projections() -> Vec<Projection> {
    vec![
        Projection::default(),
        projection(Some(0), &[], &[]),
        projection(Some(1), &[], &[]),
        projection(Some(2), &[], &[]),
        projection(None, &["/a"], &[]),
        projection(None, &["/*/b"], &[]),
        projection(None, &["/a/*/c", "/b"], &[]),
        projection(None, &[], &["/a/b"]),
        projection(None, &[], &["/*/c"]),
        projection(None, &["/a"], &["/a/*/b"]),
        projection(Some(2), &["/*/a"], &["/b"]),
        projection(None, &["/arr/*/a"], &[]),
        projection(None, &[], &["/arr/*/b"]),
        projection(None, &[], &[""]),
    ]
}

/// Projecting the new value equals applying the projected patch to the projected old value.
fn assert_commutes(projection: &Projection, old: &Value, new: &Value) {
    let revision = Revision {
        id: 1,
        patch: diff(old, new),
    };
    let mut applied = projection.project_root(old);
    let projected = projection.project_revision(revision.for_path(&ROOT));
    let before = applied.clone();
    if let Err(e) = json_patch::patch(&mut applied, &projected.patch.0) {
        panic!(
            "{:?}: {} applying {:?} to {}",
            projection, e, projected, before
        );
    }
    assert_eq!(
        applied,
        projection.project_root(new),
        "{:?}: {} -> {}",
        projection,
        old,
        new
    );
}

#[test]
fn values_keep_included_paths_minus_exclusions() {
    let value = json!({
        "a": { "x": { "b": 1, "c": 2 }, "y": { "b": 3 } },
        "b": { "a": [1, 2] },
        "arr": [{ "a": 1, "b": 2 }, { "a": 3, "b": 4 }],
    });
    assert_eq!(
        projection(None, &["/*/b"], &[]).project_root(&value),
        json!({ "a": {}, "b": {}, "arr": [] }),
    );
    assert_eq!(
        projection(None, &["/a"], &["/a/*/b"]).project_root(&value),
        json!({ "a": { "x": { "c": 2 }, "y": {} } }),
    );
    assert_eq!(
        projection(Some(1), &[], &[]).project_root(&value),
        json!({ "a": {}, "b": {}, "arr": [] }),
    );
    assert_eq!(
        projection(None, &[], &["/arr/*/b"]).project_root(&value)["arr"],
        json!([{ "a": 1 }, { "a": 3 }]),
    );
    assert_eq!(
        projection(None, &[], &[""]).project_root(&value),
        Value::Null
    );
    assert_eq!(Projection::default().project_root(&value), value);
}

#[test]
fn revisions_drop_operations_outside_the_projection() {
    let projection = projection(None, &["/a"], &["/a/*/b"]);
    let old = json!({ "a": { "x": { "b": 1 } }, "z": 1 });
    let new = json!({ "a": { "x": { "b": 2, "c": 3 } }, "z": 2 });
    let projected = projection.project_revision(Revision {
        id: 1,
        patch: diff(&old, &new),
    });
    assert_eq!(
        serde_json::to_value(&projected.patch).unwrap(),
        serde_json::json!([{ "op": "add", "path": "/a/x/c", "value": 3 }]),
    );
}

#[test]
fn equivalent_projections_share_a_key() {
    assert_eq!(
        projection(None, &["/b", "/a", "/a"], &[]).normalized(),
        projection(None, &["/a", "/b"], &[]).normalized(),
    );
}

#[test]
fn projected_revisions_track_the_projected_document() {
    let states = [
        json!(null),
        json!({}),
        json!({ "a": { "x": { "b": 1, "c": 2 } } }),
        json!({ "a": { "x": { "b": 2 }, "y": { "c": [1] } }, "b": { "a": 1 } }),
        json!({ "a": 5, "b": { "a": { "c": 1 } } }),
        json!({ "arr": [{ "a": 1, "b": 2 }] }),
        json!({ "arr": [{ "a": 0, "b": 0 }, { "a": 1, "b": 2 }, { "a": 3 }] }),
        json!({ "arr": [{ "a": 3 }] }),
        json!({ "a": [ { "b": 1 } ], "arr": {} }),
    ];
    for projection in projections() {
        for old in &states {
            for new in &states {
                assert_commutes(&projection, old, new);
            }
        }
    }
}

fn value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        (0u8..4).prop_map(|n| json!(n)),
    ];
    leaf.prop_recursive(4, 24, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4)
                .prop_map(|items| Value::Array(items.into_iter().collect())),
            prop::collection::btree_map(
                prop::sample::select(vec!["a", "b", "c", "arr"]),
                inner,
                0..4
            )
            .prop_map(|map| {
                let mut object = imbl_value::InOMap::new();
                for (key, value) in map {
                    object.insert(key.into(), value);
                }
                Value::Object(object)
            }),
        ]
    })
}

proptest! {
    #[test]
    fn projected_revisions_track_arbitrary_documents(old in value(), new in value()) {
        for projection in projections() {
            assert_commutes(&projection, &old, &new);
        }
    }
}

#[test]
fn subtree_scoping_composes_with_projection() {
    let ptr: JsonPointer = "/a".parse().unwrap();
    let projection = projection(None, &["/*/b"], &[]);
    let old = json!({ "a": { "x": { "b": 1, "c": 1 } } });
    let new = json!({ "a": { "x": { "b": 2, "c": 2 }, "y": { "b": 3 } } });
    let mut applied = projection.project_root(ptr.get(&old).unwrap());
    let projected = projection.project_revision(
        Revision {
            id: 1,
            patch: diff(&old, &new),
        }
        .for_path(&ptr),
    );
    json_patch::patch(&mut applied, &projected.patch.0).unwrap();
    assert_eq!(applied, json!({ "x": { "b": 2 }, "y": { "b": 3 } }));
}
