use std::collections::BTreeSet;
use std::ops::Deref;

use imbl_value::Value;
use json_patch::{AddOperation, Patch, PatchOperation, RemoveOperation, ReplaceOperation};
use json_ptr::{JsonPointer, SegList};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Revision {
    pub id: u64,
    pub patch: DiffPatch,
}
impl Revision {
    pub fn for_path<S: AsRef<str>, V: SegList>(&self, ptr: &JsonPointer<S, V>) -> Revision {
        Self {
            id: self.id,
            patch: self.patch.for_path(ptr),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "ts", derive(visit_rs::TS))]
pub struct Dump {
    pub id: u64,
    pub value: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiffPatch(pub(crate) Patch);
impl DiffPatch {
    pub fn prepend<S: AsRef<str>, V: SegList>(&mut self, ptr: &JsonPointer<S, V>) {
        self.0.prepend(ptr)
    }

    pub fn add(value: Value) -> Self {
        DiffPatch(Patch(vec![PatchOperation::Add(AddOperation {
            path: JsonPointer::default(),
            value,
        })]))
    }

    pub fn append(&mut self, other: DiffPatch) {
        (self.0).0.extend((other.0).0)
    }

    // safe to assume dictionary style symantics for arrays since patches will always be rebased before being applied
    pub fn for_path<S: AsRef<str>, V: SegList>(&self, ptr: &JsonPointer<S, V>) -> DiffPatch {
        let DiffPatch(Patch(ops)) = self;
        DiffPatch(Patch(
            ops.iter()
                .filter_map(|op| match op {
                    PatchOperation::Add(op) => {
                        if let Some(tail) = op.path.strip_prefix(ptr) {
                            Some(PatchOperation::Add(AddOperation {
                                path: tail.to_owned(),
                                value: op.value.clone(),
                            }))
                        } else if let Some(tail) = ptr.strip_prefix(&op.path) {
                            Some(PatchOperation::Add(AddOperation {
                                path: Default::default(),
                                value: tail.get(&op.value).cloned().unwrap_or_default(),
                            }))
                        } else {
                            None
                        }
                    }
                    PatchOperation::Replace(op) => {
                        if let Some(tail) = op.path.strip_prefix(ptr) {
                            Some(PatchOperation::Replace(ReplaceOperation {
                                path: tail.to_owned(),
                                value: op.value.clone(),
                            }))
                        } else if let Some(tail) = ptr.strip_prefix(&op.path) {
                            Some(PatchOperation::Replace(ReplaceOperation {
                                path: Default::default(),
                                value: tail.get(&op.value).cloned().unwrap_or_default(),
                            }))
                        } else {
                            None
                        }
                    }
                    PatchOperation::Remove(op) => {
                        if ptr.starts_with(&op.path) {
                            Some(PatchOperation::Replace(ReplaceOperation {
                                path: Default::default(),
                                value: Default::default(),
                            }))
                        } else if let Some(tail) = op.path.strip_prefix(ptr) {
                            Some(PatchOperation::Remove(RemoveOperation {
                                path: tail.to_owned(),
                            }))
                        } else {
                            None
                        }
                    }
                    _ => unreachable!(),
                })
                .collect(),
        ))
    }

    pub fn rebase(&mut self, onto: &DiffPatch) {
        let DiffPatch(Patch(ops)) = self;
        let DiffPatch(Patch(onto_ops)) = onto;
        for onto_op in onto_ops {
            if let PatchOperation::Add(onto_op) = onto_op {
                let arr_path_idx = onto_op.path.len() - 1;
                if let Some(onto_idx) = onto_op
                    .path
                    .get_segment(arr_path_idx)
                    .and_then(|seg| seg.parse::<usize>().ok())
                {
                    let prefix = onto_op.path.slice(..arr_path_idx).unwrap_or_default();
                    for op in ops.iter_mut() {
                        let path = match op {
                            PatchOperation::Add(op) => &mut op.path,
                            PatchOperation::Replace(op) => &mut op.path,
                            PatchOperation::Remove(op) => &mut op.path,
                            _ => unreachable!(),
                        };
                        if path.starts_with(&prefix) {
                            if let Some(idx) = path
                                .get_segment(arr_path_idx)
                                .and_then(|seg| seg.parse::<usize>().ok())
                            {
                                if idx >= onto_idx {
                                    let mut new_path = prefix.clone().to_owned();
                                    new_path.push_end_idx(idx + 1);
                                    if let Some(tail) = path.slice(arr_path_idx + 1..) {
                                        new_path.append(&tail);
                                    }
                                    *path = new_path;
                                }
                            }
                        }
                    }
                }
            } else if let PatchOperation::Remove(onto_op) = onto_op {
                let arr_path_idx = onto_op.path.len() - 1;
                if let Some(onto_idx) = onto_op
                    .path
                    .get_segment(arr_path_idx)
                    .and_then(|seg| seg.parse::<usize>().ok())
                {
                    let prefix = onto_op.path.slice(..arr_path_idx).unwrap_or_default();
                    for op in ops.iter_mut() {
                        let path = match op {
                            PatchOperation::Add(op) => &mut op.path,
                            PatchOperation::Replace(op) => &mut op.path,
                            PatchOperation::Remove(op) => &mut op.path,
                            _ => unreachable!(),
                        };
                        if path.starts_with(&prefix) {
                            if let Some(idx) = path
                                .get_segment(arr_path_idx)
                                .and_then(|seg| seg.parse::<usize>().ok())
                            {
                                if idx > onto_idx {
                                    let mut new_path = prefix.clone().to_owned();
                                    new_path.push_end_idx(idx - 1);
                                    if let Some(tail) = path.slice(arr_path_idx + 1..) {
                                        new_path.append(&tail);
                                    }
                                    *path = new_path;
                                } else if idx == onto_idx {
                                    let new_op = match &*op {
                                        PatchOperation::Replace(r) => {
                                            Some(PatchOperation::Add(AddOperation {
                                                path: r.path.clone(),
                                                value: r.value.clone(),
                                            }))
                                        }
                                        _ => None,
                                    };
                                    if let Some(new_op) = new_op {
                                        *op = new_op;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn exists(&self) -> Option<bool> {
        let mut res = None;
        for op in &(self.0).0 {
            match op {
                PatchOperation::Add(a) => {
                    if a.path.is_empty() {
                        res = Some(!a.value.is_null());
                    }
                }
                PatchOperation::Replace(a) => {
                    if a.path.is_empty() {
                        res = Some(!a.value.is_null())
                    }
                }
                PatchOperation::Remove(a) => {
                    if a.path.is_empty() {
                        res = Some(false)
                    }
                }
                _ => unreachable!(),
            }
        }
        res
    }

    pub fn keys(&self, mut keys: BTreeSet<String>) -> BTreeSet<String> {
        for op in &(self.0).0 {
            match op {
                PatchOperation::Add(a) => {
                    if a.path.len() == 1 {
                        keys.insert(a.path.get_segment(0).unwrap().to_owned());
                    }
                }
                PatchOperation::Replace(_) => (),
                PatchOperation::Remove(a) => {
                    if a.path.len() == 1 {
                        keys.remove(a.path.get_segment(0).unwrap());
                    }
                }
                _ => unreachable!(),
            }
        }
        keys
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
impl Default for DiffPatch {
    fn default() -> Self {
        DiffPatch(Patch(Vec::default()))
    }
}
impl Deref for DiffPatch {
    type Target = Patch;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub fn diff(left: &Value, right: &Value) -> DiffPatch {
    DiffPatch(json_patch::diff(left, right))
}
