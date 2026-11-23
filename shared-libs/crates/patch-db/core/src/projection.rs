use imbl_value::{InOMap, Value};
use json_patch::{AddOperation, Patch, PatchOperation, ReplaceOperation};
use json_ptr::JsonPointer;
use serde::{Deserialize, Serialize};

use crate::{DiffPatch, Dump, Revision};

#[cfg(test)]
mod tests;

/// The part of a subscribed subtree a subscriber receives, applied alike to dumps and revisions.
///
/// Patterns are pointers relative to the subscribed subtree, and a `*` segment matches any
/// single key or index. Array elements must be addressed by `*`: an index segment selects a
/// position, which insertions and removals shift. Projection is a pure function of the value
/// or revision, so equal projections share one listener and no per-subscriber state.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Projection {
    /// Levels kept below the subtree; deeper objects and arrays become empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<usize>,
    /// Subtrees kept; empty keeps everything.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<JsonPointer>,
    /// Subtrees removed from what `include` keeps.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<JsonPointer>,
}

/// How a path relates to a projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    /// Not part of the projection.
    Out,
    /// Kept only as an ancestor of included paths.
    Route,
    /// Kept, minus excluded or too-deep descendants.
    In,
}

impl Projection {
    /// Orders and deduplicates patterns so equivalent projections compare equal.
    pub fn normalized(mut self) -> Self {
        self.include.sort();
        self.include.dedup();
        self.exclude.sort();
        self.exclude.dedup();
        self
    }

    pub fn is_identity(&self) -> bool {
        self.depth.is_none() && self.include.is_empty() && self.exclude.is_empty()
    }

    pub fn reach(&self, path: &[&str]) -> Reach {
        if self.depth.is_some_and(|depth| path.len() > depth) {
            return Reach::Out;
        }
        if self.exclude.iter().any(|p| covers(p, path)) {
            return Reach::Out;
        }
        if self.include.is_empty() || self.include.iter().any(|p| covers(p, path)) {
            return Reach::In;
        }
        if self.include.iter().any(|p| leads_through(p, path)) {
            return Reach::Route;
        }
        Reach::Out
    }

    /// Whether nothing below an included path can be dropped.
    fn whole_below(&self, path: &[&str]) -> bool {
        self.depth.is_none() && !self.exclude.iter().any(|p| leads_through(p, path))
    }

    /// The projected value at a path the projection reaches.
    pub fn project_value(&self, path: &mut Vec<String>, value: &Value) -> Value {
        let segments = path.iter().map(String::as_str).collect::<Vec<_>>();
        let reach = self.reach(&segments);
        if reach == Reach::In && self.whole_below(&segments) {
            return value.clone();
        }
        match value {
            Value::Object(map) => {
                let mut projected = InOMap::new();
                for (key, child) in map.iter() {
                    path.push(key.to_string());
                    if self.reach_owned(path) != Reach::Out {
                        projected.insert(key.clone(), self.project_value(path, child));
                    }
                    path.pop();
                }
                Value::Object(projected)
            }
            Value::Array(items) => {
                let mut projected = imbl_value::imbl::Vector::new();
                for (index, child) in items.iter().enumerate() {
                    path.push(index.to_string());
                    if self.reach_owned(path) != Reach::Out {
                        projected.push_back(self.project_value(path, child));
                    }
                    path.pop();
                }
                Value::Array(projected)
            }
            other => other.clone(),
        }
    }

    fn reach_owned(&self, path: &[String]) -> Reach {
        self.reach(&path.iter().map(String::as_str).collect::<Vec<_>>())
    }

    /// The projected subtree value; null when the projection excludes the subtree itself.
    pub fn project_root(&self, value: &Value) -> Value {
        if self.is_identity() {
            return value.clone();
        }
        if self.reach(&[]) == Reach::Out {
            return Value::Null;
        }
        self.project_value(&mut Vec::new(), value)
    }

    pub fn project_dump(&self, dump: Dump) -> Dump {
        if self.is_identity() {
            return dump;
        }
        Dump {
            id: dump.id,
            value: self.project_root(&dump.value),
        }
    }

    /// A revision already scoped to the subtree, restricted to the projection.
    pub fn project_revision(&self, revision: Revision) -> Revision {
        if self.is_identity() {
            return revision;
        }
        let DiffPatch(Patch(ops)) = revision.patch;
        let ops = ops
            .into_iter()
            .filter_map(|op| {
                let path = match &op {
                    PatchOperation::Add(op) => &op.path,
                    PatchOperation::Remove(op) => &op.path,
                    PatchOperation::Replace(op) => &op.path,
                    _ => return None,
                };
                let mut segments = path.iter().map(str::to_owned).collect::<Vec<_>>();
                if self.reach_owned(&segments) == Reach::Out {
                    return None;
                }
                Some(match op {
                    PatchOperation::Add(AddOperation { path, value }) => {
                        let value = self.project_value(&mut segments, &value);
                        PatchOperation::Add(AddOperation { path, value })
                    }
                    PatchOperation::Replace(ReplaceOperation { path, value }) => {
                        let value = self.project_value(&mut segments, &value);
                        PatchOperation::Replace(ReplaceOperation { path, value })
                    }
                    op => op,
                })
            })
            .collect();
        Revision {
            id: revision.id,
            patch: DiffPatch(Patch(ops)),
        }
    }
}

fn segment_matches(pattern: &str, segment: &str) -> bool {
    pattern == "*" || pattern == segment
}

/// Whether the pattern matches the path or one of its ancestors.
fn covers(pattern: &JsonPointer, path: &[&str]) -> bool {
    pattern.len() <= path.len() && pattern.iter().zip(path).all(|(p, s)| segment_matches(p, s))
}

/// Whether the pattern matches a proper descendant of the path.
fn leads_through(pattern: &JsonPointer, path: &[&str]) -> bool {
    pattern.len() > path.len() && pattern.iter().zip(path).all(|(p, s)| segment_matches(p, s))
}
