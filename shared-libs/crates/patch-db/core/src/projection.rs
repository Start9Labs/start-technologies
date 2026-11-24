use imbl_value::{InOMap, Value};
use json_patch::{AddOperation, Patch, PatchOperation, ReplaceOperation};
use json_ptr::JsonPointer;

use crate::{DiffPatch, Dump, Error, Revision};

#[cfg(test)]
mod tests;

/// One included subtree of a [`Projection`], minus its excluded or too-deep descendants.
///
/// Patterns are pointers relative to the subscribed subtree, and a `*` segment matches any
/// single key or index. `depth` counts levels below the subscribed subtree.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Selector {
    include: JsonPointer,
    exclude: Vec<JsonPointer>,
    depth: Option<usize>,
}

/// The part of a subscribed subtree a subscriber receives, applied alike to dumps and revisions:
/// the union of its selectors, or everything when it has none.
///
/// Constructors reject pattern segments that are array positions (digits or `-`): insertions
/// and removals shift positions, which projecting a revision on its own cannot follow.
/// Projection is a pure function of the value or revision, so equal projections share one
/// listener. Equivalent selector sets compare equal.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Projection {
    selectors: Vec<Selector>,
}

/// How a path relates to a projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reach {
    /// Not part of the projection.
    Out,
    /// Kept only as an ancestor of included paths.
    Route,
    /// Kept, minus excluded or too-deep descendants.
    In,
}

/// Whether a pattern segment would select an array position.
pub fn is_positional(segment: &str) -> bool {
    segment == "-" || (!segment.is_empty() && segment.bytes().all(|b| b.is_ascii_digit()))
}

fn reject_positional<'a>(patterns: impl IntoIterator<Item = &'a JsonPointer>) -> Result<(), Error> {
    match patterns.into_iter().find(|p| p.iter().any(is_positional)) {
        Some(pattern) => Err(Error::PositionalPattern(pattern.clone())),
        None => Ok(()),
    }
}

impl Selector {
    pub fn new(
        include: JsonPointer,
        mut exclude: Vec<JsonPointer>,
        depth: Option<usize>,
    ) -> Result<Self, Error> {
        reject_positional(std::iter::once(&include).chain(&exclude))?;
        exclude.sort();
        exclude.dedup();
        Ok(Self {
            include,
            exclude,
            depth,
        })
    }

    fn reach(&self, path: &[&str]) -> Reach {
        if self.depth.is_some_and(|depth| path.len() > depth) {
            return Reach::Out;
        }
        if self.exclude.iter().any(|p| covers(p, path)) {
            return Reach::Out;
        }
        if covers(&self.include, path) {
            return Reach::In;
        }
        if leads_through(&self.include, path) {
            return Reach::Route;
        }
        Reach::Out
    }

    /// Whether this selector keeps the path and everything below it.
    fn keeps_whole(&self, path: &[&str]) -> bool {
        self.depth.is_none()
            && covers(&self.include, path)
            && !self
                .exclude
                .iter()
                .any(|p| covers(p, path) || leads_through(p, path))
    }
}

impl Projection {
    /// Keeps `depth` levels and the `include` subtrees minus the `exclude` subtrees; an empty
    /// `include` keeps everything.
    pub fn new(
        depth: Option<usize>,
        include: Vec<JsonPointer>,
        exclude: Vec<JsonPointer>,
    ) -> Result<Self, Error> {
        if depth.is_none() && include.is_empty() && exclude.is_empty() {
            return Ok(Self::default());
        }
        let include = if include.is_empty() {
            vec![JsonPointer::default()]
        } else {
            include
        };
        Self::union(
            include
                .into_iter()
                .map(|include| Selector::new(include, exclude.clone(), depth))
                .collect::<Result<_, _>>()?,
        )
    }

    /// The union of the selectors; none keeps everything.
    pub fn union(mut selectors: Vec<Selector>) -> Result<Self, Error> {
        selectors.sort();
        selectors.dedup();
        if selectors
            .iter()
            .any(|s| s.include.is_empty() && s.exclude.is_empty() && s.depth.is_none())
        {
            selectors.clear();
        }
        Ok(Self { selectors })
    }

    pub fn is_identity(&self) -> bool {
        self.selectors.is_empty()
    }

    pub fn into_selectors(self) -> Vec<Selector> {
        self.selectors
    }

    pub fn reach(&self, path: &[&str]) -> Reach {
        if self.is_identity() {
            return Reach::In;
        }
        self.selectors
            .iter()
            .map(|s| s.reach(path))
            .max()
            .unwrap_or(Reach::Out)
    }

    fn keeps_whole(&self, path: &[&str]) -> bool {
        self.is_identity() || self.selectors.iter().any(|s| s.keeps_whole(path))
    }

    /// The projected value at a path the projection reaches.
    pub fn project_value(&self, path: &mut Vec<String>, value: &Value) -> Value {
        if self.keeps_whole(&segments(path)) {
            return value.clone();
        }
        match value {
            Value::Object(map) => {
                let mut projected = InOMap::new();
                for (key, child) in map.iter() {
                    path.push(key.to_string());
                    if self.reach(&segments(path)) != Reach::Out {
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
                    if self.reach(&segments(path)) != Reach::Out {
                        projected.push_back(self.project_value(path, child));
                    }
                    path.pop();
                }
                Value::Array(projected)
            }
            other => other.clone(),
        }
    }

    /// The projected subtree value; null when the projection excludes the subtree itself.
    pub fn project_root(&self, value: &Value) -> Value {
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
                let mut path = path.iter().map(str::to_owned).collect::<Vec<_>>();
                if self.reach(&segments(&path)) == Reach::Out {
                    return None;
                }
                Some(match op {
                    PatchOperation::Add(AddOperation { path: ptr, value }) => {
                        let value = self.project_value(&mut path, &value);
                        PatchOperation::Add(AddOperation { path: ptr, value })
                    }
                    PatchOperation::Replace(ReplaceOperation { path: ptr, value }) => {
                        let value = self.project_value(&mut path, &value);
                        PatchOperation::Replace(ReplaceOperation { path: ptr, value })
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

    /// Operations that take a replica holding this projection of `value` to the wider projection's.
    ///
    /// They only add what the wider projection keeps and this one does not; whatever this one
    /// keeps but the wider one drops is the replica's to prune.
    pub fn extension(&self, wider: &Projection, value: &Value) -> DiffPatch {
        let mut ops = Vec::new();
        if wider.reach(&[]) != Reach::Out {
            extend(self, wider, &mut Vec::new(), false, value, &mut ops);
        }
        DiffPatch(Patch(ops))
    }
}

/// Whether the replica lacks data the wider projection keeps at the path.
fn gains(held: &Projection, wider: &Projection, path: &[&str]) -> bool {
    let (before, after) = (held.reach(path), wider.reach(path));
    before == Reach::Out && after != Reach::Out || after == Reach::In && before != Reach::In
}

fn extend(
    held: &Projection,
    wider: &Projection,
    path: &mut Vec<String>,
    in_array: bool,
    value: &Value,
    ops: &mut Vec<PatchOperation>,
) {
    let here = segments(path);
    if gains(held, wider, &here) {
        let value = wider.project_value(path, value);
        ops.push(set(path, in_array, value));
        return;
    }
    if held.keeps_whole(&here) {
        return;
    }
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter() {
                path.push(key.to_string());
                if wider.reach(&segments(path)) != Reach::Out {
                    extend(held, wider, path, false, child, ops);
                }
                path.pop();
            }
        }
        Value::Array(items) => {
            let shifted = (0..items.len()).any(|index| {
                path.push(index.to_string());
                let shifted = held.reach(&segments(path)) == Reach::Out
                    && wider.reach(&segments(path)) != Reach::Out;
                path.pop();
                shifted
            });
            if shifted {
                let value = wider.project_value(path, value);
                ops.push(set(path, in_array, value));
                return;
            }
            for (index, child) in items.iter().enumerate() {
                path.push(index.to_string());
                if wider.reach(&segments(path)) != Reach::Out {
                    extend(held, wider, path, true, child, ops);
                }
                path.pop();
            }
        }
        _ => (),
    }
}

/// Sets a path whose parent the replica already holds: `replace` at an array element or the
/// root, `add` for an object member, which also replaces.
fn set(path: &[String], in_array: bool, value: Value) -> PatchOperation {
    let mut ptr = JsonPointer::default();
    for segment in path {
        ptr.push_end(segment);
    }
    if in_array || path.is_empty() {
        PatchOperation::Replace(ReplaceOperation { path: ptr, value })
    } else {
        PatchOperation::Add(AddOperation { path: ptr, value })
    }
}

fn segments(path: &[String]) -> Vec<&str> {
    path.iter().map(String::as_str).collect()
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
