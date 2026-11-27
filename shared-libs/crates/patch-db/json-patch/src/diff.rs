use imbl_value::imbl::ordmap::DiffItem;
use imbl_value::OrdValue;
use json_ptr::JsonPointer;

use crate::{AddOperation, PatchOperation, RemoveOperation, ReplaceOperation};

struct PatchDiffer {
    path: JsonPointer,
    patch: super::Patch,
}

impl PatchDiffer {
    fn new() -> Self {
        Self {
            path: JsonPointer::default(),
            patch: super::Patch(Vec::new()),
        }
    }
}

/// Diff two JSON documents and generate a JSON Patch (RFC 6902).
///
/// # Example
/// Diff two JSONs:
///
/// ```rust
/// #[macro_use]
/// extern crate imbl_value;
/// extern crate json_patch;
///
/// use json_patch::{patch, diff, from_value};
///
/// # pub fn main() {
/// let left = ord_json!({
///   "title": "Goodbye!",
///   "author" : {
///     "givenName" : "John",
///     "familyName" : "Doe"
///   },
///   "tags":[ "example", "sample" ],
///   "content": "This will be unchanged"
/// });
///
/// let right = ord_json!({
///   "title": "Hello!",
///   "author" : {
///     "givenName" : "John"
///   },
///   "tags": [ "example" ],
///   "content": "This will be unchanged",
///   "phoneNumber": "+01-123-456-7890"
/// });
///
/// let p = diff(&left, &right);
/// assert_eq!(p, from_value(ord_json!([
///   { "op": "remove", "path": "/author/familyName" },
///   { "op": "add", "path": "/phoneNumber", "value": "+01-123-456-7890" },
///   { "op": "remove", "path": "/tags/1" },
///   { "op": "replace", "path": "/title", "value": "Hello!" },
/// ])).unwrap());
///
/// let mut doc = left.clone();
/// patch(&mut doc, &p).unwrap();
/// assert_eq!(doc, right);
///
/// # }
/// ```
pub fn diff(from: &OrdValue, to: &OrdValue) -> super::Patch {
    let mut differ = PatchDiffer::new();
    diff_mut(&mut differ, from, to);
    differ.patch
}

fn diff_mut(differ: &mut PatchDiffer, from: &OrdValue, to: &OrdValue) {
    match (from, to) {
        (OrdValue::Object(f), OrdValue::Object(t)) => {
            for item in f.diff(t) {
                match item {
                    DiffItem::Update {
                        old: (key, f),
                        new: (_, t),
                    } => {
                        differ.path.push_end(key);
                        diff_mut(differ, f, t);
                        differ.path.pop_end();
                    }
                    DiffItem::Remove(key, _) => {
                        differ.path.push_end(key);
                        differ.patch.0.push(PatchOperation::Remove(RemoveOperation {
                            path: differ.path.clone(),
                        }));
                        differ.path.pop_end();
                    }
                    DiffItem::Add(key, t) => {
                        differ.path.push_end(key);
                        differ.patch.0.push(PatchOperation::Add(AddOperation {
                            path: differ.path.clone(),
                            value: t.clone(),
                        }));
                        differ.path.pop_end();
                    }
                }
            }
        }
        (OrdValue::Array(f), OrdValue::Array(t)) if !f.ptr_eq(t) => {
            let mut f_idx = 0;
            let mut t_idx = 0;
            while f_idx < f.len() && t_idx < t.len() {
                if f[f_idx].ptr_eq(&t[t_idx]) {
                    f_idx += 1;
                    t_idx += 1;
                } else if t.iter().skip(t_idx + 1).any(|t| f[f_idx].ptr_eq(t)) {
                    differ.patch.0.push(PatchOperation::Add(AddOperation {
                        path: differ.path.clone().join_end_idx(t_idx),
                        value: t[t_idx].clone(),
                    }));
                    t_idx += 1;
                } else if f.iter().skip(f_idx + 1).any(|f| t[t_idx].ptr_eq(f)) {
                    differ.patch.0.push(PatchOperation::Remove(RemoveOperation {
                        path: differ.path.clone().join_end_idx(t_idx),
                    }));
                    f_idx += 1;
                } else {
                    differ.path.push_end_idx(t_idx);
                    diff_mut(differ, &f[f_idx], &t[t_idx]);
                    differ.path.pop_end();
                    f_idx += 1;
                    t_idx += 1;
                }
            }
            for _ in f_idx..f.len() {
                differ.patch.0.push(PatchOperation::Remove(RemoveOperation {
                    path: differ.path.clone().join_end_idx(t_idx),
                }));
            }
            for (t_idx, t) in t.iter().enumerate().skip(t_idx) {
                differ.patch.0.push(PatchOperation::Add(AddOperation {
                    path: differ.path.clone().join_end_idx(t_idx),
                    value: t.clone(),
                }));
            }
        }
        (f, t) if f != t => differ
            .patch
            .0
            .push(PatchOperation::Replace(ReplaceOperation {
                path: differ.path.clone(),
                value: t.clone(),
            })),
        _ => (),
    }
}

#[cfg(test)]
mod tests {
    use imbl_value::OrdValue;

    #[test]
    pub fn replace_all() {
        let left = ord_json!({"title": "Hello!"});
        let p = super::diff(&left, &OrdValue::Null);
        assert_eq!(
            p,
            imbl_value::from_value(ord_json!([
                { "op": "replace", "path": "", "value": null },
            ]))
            .unwrap()
        );
    }

    #[test]
    pub fn add_all() {
        let right = ord_json!({"title": "Hello!"});
        let p = super::diff(&OrdValue::Null, &right);
        assert_eq!(
            p,
            imbl_value::from_value(ord_json!([
                { "op": "replace", "path": "", "value": { "title": "Hello!" } },
            ]))
            .unwrap()
        );
    }

    #[test]
    pub fn remove_all() {
        let left = ord_json!(["hello", "bye"]);
        let right = ord_json!([]);
        let p = super::diff(&left, &right);
        assert_eq!(
            p,
            imbl_value::from_value(ord_json!([
                { "op": "remove", "path": "/0" },
                { "op": "remove", "path": "/0" },
            ]))
            .unwrap()
        );
    }

    #[test]
    pub fn remove_tail() {
        let left = ord_json!(["hello", "bye", "hi"]);
        let right = ord_json!(["hello"]);
        let p = super::diff(&left, &right);
        assert_eq!(
            p,
            imbl_value::from_value(ord_json!([
                { "op": "remove", "path": "/1" },
                { "op": "remove", "path": "/1" },
            ]))
            .unwrap()
        );
    }
    #[test]
    pub fn replace_object() {
        let left = ord_json!(["hello", "bye"]);
        let right = ord_json!({"hello": "bye"});
        let p = super::diff(&left, &right);
        assert_eq!(
            p,
            imbl_value::from_value(ord_json!([
                { "op": "replace", "path": "", "value": &right },
            ]))
            .unwrap()
        );
    }

    #[test]
    fn escape_json_keys() {
        let mut left = ord_json!({
            "/slashed/path": 1
        });
        let right = ord_json!({
            "/slashed/path": 2,
        });
        let patch = super::diff(&left, &right);

        eprintln!("{:?}", patch);

        crate::patch(&mut left, &patch).unwrap();
        assert_eq!(left, right);
    }

    fn shift_patch(from: OrdValue, to: OrdValue) -> crate::Patch {
        let mut applied = from.clone();
        let patch = super::diff(&from, &to);
        crate::patch(&mut applied, &patch).unwrap();
        assert_eq!(applied, to);
        patch
    }

    #[test]
    fn a_window_shifted_by_a_prepend_is_one_add_and_one_remove() {
        let from = ord_json!([{ "t": 3 }, { "t": 2 }, { "t": 1 }]);
        let mut items = from.as_array().unwrap().clone();
        items.push_front(ord_json!({ "t": 4 }));
        items.pop_back();
        assert_eq!(
            shift_patch(from, OrdValue::Array(items)),
            imbl_value::from_value(ord_json!([
                { "op": "add", "path": "/0", "value": { "t": 4 } },
                { "op": "remove", "path": "/3" },
            ]))
            .unwrap()
        );
    }

    #[test]
    fn a_window_shifted_by_an_append_is_one_remove_and_one_add() {
        let from = ord_json!([{ "t": 1 }, { "t": 2 }, { "t": 3 }]);
        let mut items = from.as_array().unwrap().clone();
        items.pop_front();
        items.push_back(ord_json!({ "t": 4 }));
        assert_eq!(
            shift_patch(from, OrdValue::Array(items)),
            imbl_value::from_value(ord_json!([
                { "op": "remove", "path": "/0" },
                { "op": "add", "path": "/2", "value": { "t": 4 } },
            ]))
            .unwrap()
        );
    }

    #[test]
    fn swapped_elements_still_patch_to_the_target() {
        let from = ord_json!([{ "a": 1 }, { "b": 2 }, { "c": 3 }]);
        let items = from.as_array().unwrap();
        let swapped = OrdValue::Array(
            vec![items[2].clone(), items[1].clone(), items[0].clone()]
                .into_iter()
                .collect(),
        );
        shift_patch(from, swapped);
    }

    proptest::proptest! {
        #[test]
        fn shared_elements_reordered_inserted_and_removed_patch_to_the_target(
            from in proptest::collection::vec(proptest::prelude::any::<OrdValue>(), 0..12),
            edits in proptest::collection::vec((0usize..3, 0usize..16, proptest::prelude::any::<OrdValue>()), 0..8),
        ) {
            let from: imbl_value::imbl::Vector<OrdValue> = from.into_iter().collect();
            let mut to = from.clone();
            for (kind, at, value) in edits {
                match kind {
                    0 => to.insert(at.min(to.len()), value),
                    1 if !to.is_empty() => { to.remove(at % to.len()); }
                    _ if !to.is_empty() => {
                        let moved = to.remove(at % to.len());
                        to.push_front(moved);
                    }
                    _ => {}
                }
            }
            shift_patch(OrdValue::Array(from), OrdValue::Array(to));
        }

        #[test]
        fn test_diff(mut from: OrdValue, to: OrdValue) {
            let patch = super::diff(&from, &to);
            crate::patch(&mut from, &patch).unwrap();
            assert_eq!(from, to);
        }
    }
}
