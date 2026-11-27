use imbl_value::{ord_json, OrdValue};
use json_ptr::{JsonPointer, ROOT};
use proptest::prelude::*;

use super::*;
use crate::patch::diff;

fn patterns(patterns: &[&str]) -> Vec<JsonPointer> {
    patterns.iter().map(|p| p.parse().unwrap()).collect()
}

fn projection(depth: Option<usize>, include: &[&str], exclude: &[&str]) -> Projection {
    Projection::new(depth, patterns(include), patterns(exclude))
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
        union(&[("/a", &["/a/*/b"], None), ("/a/x/b", &[], None)]),
        union(&[("/*/b", &[], None), ("/arr", &["/arr/*/b"], Some(3))]),
        union(&[("/a", &[], Some(2)), ("/b", &["/b/a"], None)]),
        projection(None, &["/arr/1"], &[]),
        projection(None, &[], &["/arr/0"]),
        union(&[("/arr/1/a", &[], None), ("/a", &[], None)]),
    ]
}

fn union(selectors: &[(&str, &[&str], Option<usize>)]) -> Projection {
    Projection::union(
        selectors
            .iter()
            .map(|(include, exclude, depth)| {
                Selector::new(include.parse().unwrap(), patterns(exclude), *depth)
            })
            .collect(),
    )
}

/// A replica re-projected to the wider projection, then extended, holds its projection.
fn assert_extends(held: &Projection, wider: &Projection, value: &OrdValue) {
    let mut replica = wider.project_root(&held.project_root(value));
    let extension = held.extension(wider, value);
    let before = replica.clone();
    if let Err(e) = json_patch::patch(&mut replica, &extension.0) {
        panic!(
            "{:?} -> {:?}: {} applying {:?} to {}",
            held, wider, e, extension, before
        );
    }
    assert_eq!(
        replica,
        wider.project_root(value),
        "{:?} -> {:?} over {}",
        held,
        wider,
        value
    );
}

/// Projecting the new value equals applying the projected patch to the projected old value.
fn assert_commutes(projection: &Projection, old: &OrdValue, new: &OrdValue) {
    let revision = Revision {
        id: 1,
        patch: diff(old, new),
    };
    let mut applied = projection.project_root(old);
    let shifted = shifted_arrays(&revision.patch, new);
    let projected = projection
        .project_scoped(&ROOT.to_owned(), &revision, &shifted, Some(new))
        .unwrap();
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
    let value = ord_json!({
        "a": { "x": { "b": 1, "c": 2 }, "y": { "b": 3 } },
        "b": { "a": [1, 2] },
        "arr": [{ "a": 1, "b": 2 }, { "a": 3, "b": 4 }],
    });
    assert_eq!(
        projection(None, &["/*/b"], &[]).project_root(&value),
        ord_json!({ "a": {}, "b": {}, "arr": [null, null] }),
    );
    assert_eq!(
        projection(None, &["/a"], &["/a/*/b"]).project_root(&value),
        ord_json!({ "a": { "x": { "c": 2 }, "y": {} } }),
    );
    assert_eq!(
        projection(Some(1), &[], &[]).project_root(&value),
        ord_json!({ "a": {}, "b": {}, "arr": [] }),
    );
    assert_eq!(
        projection(None, &[], &["/arr/*/b"]).project_root(&value)["arr"],
        ord_json!([{ "a": 1 }, { "a": 3 }]),
    );
    assert_eq!(
        projection(None, &[], &[""]).project_root(&value),
        OrdValue::Null
    );
    assert_eq!(Projection::default().project_root(&value), value);
}

#[test]
fn revisions_drop_operations_outside_the_projection() {
    let projection = projection(None, &["/a"], &["/a/*/b"]);
    let old = ord_json!({ "a": { "x": { "b": 1 } }, "z": 1 });
    let new = ord_json!({ "a": { "x": { "b": 2, "c": 3 } }, "z": 2 });
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
        projection(None, &["/b", "/a", "/a"], &[]),
        projection(None, &["/a", "/b"], &[]),
    );
}

#[test]
fn projected_revisions_track_the_projected_document() {
    let states = [
        ord_json!(null),
        ord_json!({}),
        ord_json!({ "a": { "x": { "b": 1, "c": 2 } } }),
        ord_json!({ "a": { "x": { "b": 2 }, "y": { "c": [1] } }, "b": { "a": 1 } }),
        ord_json!({ "a": 5, "b": { "a": { "c": 1 } } }),
        ord_json!({ "arr": [{ "a": 1, "b": 2 }] }),
        ord_json!({ "arr": [{ "a": 0, "b": 0 }, { "a": 1, "b": 2 }, { "a": 3 }] }),
        ord_json!({ "arr": [{ "a": 3 }] }),
        ord_json!({ "a": [ { "b": 1 } ], "arr": {} }),
    ];
    for projection in projections() {
        for old in &states {
            for new in &states {
                assert_commutes(&projection, old, new);
            }
        }
    }
}

fn value() -> impl Strategy<Value = OrdValue> {
    let leaf = prop_oneof![
        Just(OrdValue::Null),
        any::<bool>().prop_map(OrdValue::Bool),
        (0u8..4).prop_map(|n| ord_json!(n)),
    ];
    leaf.prop_recursive(4, 24, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4)
                .prop_map(|items| OrdValue::Array(items.into_iter().collect())),
            prop::collection::btree_map(
                prop::sample::select(vec!["a", "b", "c", "arr"]),
                inner,
                0..4
            )
            .prop_map(|map| {
                let mut object = imbl_value::imbl::OrdMap::new();
                for (key, value) in map {
                    object.insert(key.into(), value);
                }
                OrdValue::Object(object)
            }),
        ]
    })
}

#[test]
fn extensions_add_exactly_what_a_wider_projection_keeps() {
    let value = ord_json!({
        "a": { "x": { "b": 1, "c": 2 }, "y": { "b": 3 } },
        "b": { "a": [1, 2] },
        "arr": [{ "a": 1, "b": 2 }, { "a": 3, "b": 4 }],
    });
    for held in projections() {
        for wider in projections() {
            assert_extends(&held, &wider, &value);
        }
    }
    let narrow = projection(None, &["/a"], &["/a/*/b"]);
    let wide = projection(None, &["/a"], &[]);
    assert_eq!(
        serde_json::to_value(&narrow.extension(&wide, &value)).unwrap(),
        serde_json::json!([
            { "op": "add", "path": "/a/x/b", "value": 1 },
            { "op": "add", "path": "/a/y/b", "value": 3 },
        ]),
    );
}

proptest! {
    #[test]
    fn extensions_track_arbitrary_documents(value in value()) {
        for held in projections() {
            for wider in projections() {
                assert_extends(&held, &wider, &value);
            }
        }
    }

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
    let old = ord_json!({ "a": { "x": { "b": 1, "c": 1 } } });
    let new = ord_json!({ "a": { "x": { "b": 2, "c": 2 }, "y": { "b": 3 } } });
    let mut applied = projection.project_root(ptr.get(&old).unwrap());
    let projected = projection.project_revision(
        Revision {
            id: 1,
            patch: diff(&old, &new),
        }
        .for_path(&ptr),
    );
    json_patch::patch(&mut applied, &projected.patch.0).unwrap();
    assert_eq!(applied, ord_json!({ "x": { "b": 2 }, "y": { "b": 3 } }));
}

#[test]
fn positions_stay_aligned_and_shifted_arrays_are_resent() {
    let projection = projection(None, &["/arr/1", "/bindings/80"], &[]);
    let old = ord_json!({ "arr": ["a", "b", "c"], "bindings": { "80": 1, "443": 2 } });
    assert_eq!(
        projection.project_root(&old),
        ord_json!({ "arr": [null, "b", null], "bindings": { "80": 1 } }),
    );
    for patch in [
        ord_json!([{ "op": "add", "path": "/arr/0", "value": "x" }]),
        ord_json!([{ "op": "remove", "path": "/arr/0" }]),
        ord_json!([
            { "op": "add", "path": "/arr/3", "value": "d" },
            { "op": "replace", "path": "/arr/1", "value": "B" },
            { "op": "remove", "path": "/arr/0" },
        ]),
    ] {
        let patch: DiffPatch = serde_json::from_value(patch.into()).unwrap();
        let mut new = old.clone();
        json_patch::patch(&mut new, &patch.0).unwrap();
        let revision = Revision { id: 2, patch };
        let shifted = shifted_arrays(&revision.patch, &new);
        assert_eq!(shifted, vec!["/arr".parse::<JsonPointer>().unwrap()]);
        let projected = projection
            .project_scoped(&ROOT.to_owned(), &revision, &shifted, Some(&new))
            .unwrap();
        let mut applied = projection.project_root(&old);
        json_patch::patch(&mut applied, &projected.patch.0).unwrap();
        assert_eq!(applied, projection.project_root(&new));
        assert!(projection
            .project_scoped(&ROOT.to_owned(), &revision, &shifted, None)
            .is_none());
    }
}

#[test]
fn a_subtree_inside_a_shifted_array_follows_its_position() {
    let ptr: JsonPointer = "/arr/1".parse().unwrap();
    let old = ord_json!({ "arr": ["a", "b"] });
    let patch: DiffPatch =
        serde_json::from_value(ord_json!([{ "op": "add", "path": "/arr/0", "value": "x" }]).into())
            .unwrap();
    let mut new = old.clone();
    json_patch::patch(&mut new, &patch.0).unwrap();
    let revision = Revision { id: 2, patch };
    let shifted = shifted_arrays(&revision.patch, &new);
    let projected = Projection::default()
        .project_scoped(&ptr, &revision, &shifted, Some(&new))
        .unwrap();
    let mut applied = ptr.get(&old).unwrap().clone();
    json_patch::patch(&mut applied, &projected.patch.0).unwrap();
    assert_eq!(applied, ord_json!("a"));
}
