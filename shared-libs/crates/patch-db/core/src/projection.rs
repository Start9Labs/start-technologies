use imbl_value::imbl::OrdMap;
use imbl_value::OrdValue;
use json_patch::{AddOperation, Patch, PatchOperation, ReplaceOperation};
use json_ptr::JsonPointer;

use crate::{DiffPatch, Dump, Revision};

#[cfg(test)]
mod tests;

/// One included subtree of a [`Projection`], minus its excluded or too-deep descendants.
///
/// Patterns are pointers relative to the subscribed subtree, and a `*` segment matches any
/// single key or index; a numeric segment matches that key or array position. `depth` counts
/// levels below the subscribed subtree.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Selector {
    include: JsonPointer,
    exclude: Vec<JsonPointer>,
    depth: Option<usize>,
}

/// The part of a subscribed subtree a subscriber receives, applied alike to dumps and revisions:
/// the union of its selectors, or everything when it has none.
///
/// A projected array keeps its length, with null in place of the elements it drops, so
/// positions stay aligned with the source. Equal projections share one listener, and
/// equivalent selector sets compare equal.
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

impl Selector {
    pub fn new(include: JsonPointer, mut exclude: Vec<JsonPointer>, depth: Option<usize>) -> Self {
        exclude.sort();
        exclude.dedup();
        Self {
            include,
            exclude,
            depth,
        }
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
    pub fn new(depth: Option<usize>, include: Vec<JsonPointer>, exclude: Vec<JsonPointer>) -> Self {
        if depth.is_none() && include.is_empty() && exclude.is_empty() {
            return Self::default();
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
                .collect(),
        )
    }

    /// The union of the selectors; none keeps everything.
    pub fn union(mut selectors: Vec<Selector>) -> Self {
        selectors.sort();
        selectors.dedup();
        if selectors
            .iter()
            .any(|s| s.include.is_empty() && s.exclude.is_empty() && s.depth.is_none())
        {
            selectors.clear();
        }
        Self { selectors }
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

    /// Whether depth drops every child of the path, which then projects as an empty container.
    fn cuts_below(&self, path: &[&str]) -> bool {
        !self.is_identity()
            && self
                .selectors
                .iter()
                .filter(|s| s.reach(path) != Reach::Out)
                .all(|s| s.depth.is_some_and(|depth| depth <= path.len()))
    }

    /// The projected value at a path the projection reaches.
    pub fn project_value(&self, path: &mut Vec<String>, value: &OrdValue) -> OrdValue {
        if self.keeps_whole(&segments(path)) {
            return value.clone();
        }
        match value {
            OrdValue::Object(map) => {
                let mut projected = OrdMap::new();
                for (key, child) in map.iter() {
                    path.push(key.to_string());
                    if self.reach(&segments(path)) != Reach::Out {
                        projected.insert(key.clone(), self.project_value(path, child));
                    }
                    path.pop();
                }
                OrdValue::Object(projected)
            }
            OrdValue::Array(items) => {
                let mut projected = imbl_value::imbl::Vector::new();
                if self.cuts_below(&segments(path)) {
                    return OrdValue::Array(projected);
                }
                for (index, child) in items.iter().enumerate() {
                    path.push(index.to_string());
                    projected.push_back(if self.reach(&segments(path)) == Reach::Out {
                        OrdValue::Null
                    } else {
                        self.project_value(path, child)
                    });
                    path.pop();
                }
                OrdValue::Array(projected)
            }
            other => other.clone(),
        }
    }

    /// The projected subtree value; null when the projection excludes the subtree itself.
    pub fn project_root(&self, value: &OrdValue) -> OrdValue {
        if self.reach(&[]) == Reach::Out {
            return OrdValue::Null;
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

    /// A revision scoped to the subtree at `ptr` and restricted to the projection.
    ///
    /// An array the revision inserts into or removes from (`shifted`, from the database root)
    /// is sent whole, projected from `after`, the database after the revision, unless the
    /// projection keeps it whole; so is the subtree itself when it lies inside such an array.
    /// None when that needs `after` and it is absent.
    pub fn project_scoped(
        &self,
        ptr: &JsonPointer,
        revision: &Revision,
        shifted: &[JsonPointer],
        after: Option<&OrdValue>,
    ) -> Option<Revision> {
        if shifted
            .iter()
            .any(|array| array.len() < ptr.len() && ptr.starts_with(array))
        {
            let value = ptr.get(after?).cloned().unwrap_or(OrdValue::Null);
            return Some(Revision {
                id: revision.id,
                patch: DiffPatch(Patch(vec![PatchOperation::Replace(ReplaceOperation {
                    path: JsonPointer::default(),
                    value: self.project_root(&value),
                })])),
            });
        }
        let mut resent: Vec<JsonPointer> = shifted
            .iter()
            .filter_map(|array| array.strip_prefix(ptr).map(|rel| rel.to_owned()))
            .filter(|rel| {
                let path: Vec<&str> = rel.iter().collect();
                self.reach(&path) != Reach::Out && !self.keeps_whole(&path)
            })
            .collect();
        resent.sort_by_key(|rel| rel.len());
        let mut outermost: Vec<JsonPointer> = Vec::new();
        for rel in resent {
            if !outermost.iter().any(|outer| rel.starts_with(outer)) {
                outermost.push(rel);
            }
        }
        let resent = outermost;
        let scoped = revision.for_path(ptr);
        if resent.is_empty() {
            return Some(self.project_revision(scoped));
        }
        let after = ptr.get(after?).cloned().unwrap_or(OrdValue::Null);
        let DiffPatch(Patch(ops)) = scoped.patch;
        let last: Vec<Option<usize>> = resent
            .iter()
            .map(|rel| ops.iter().rposition(|op| op_path(op).starts_with(rel)))
            .collect();
        let mut projected = Vec::new();
        for (index, op) in ops.into_iter().enumerate() {
            match resent.iter().position(|rel| op_path(&op).starts_with(rel)) {
                Some(array) if last[array] == Some(index) => {
                    let rel = &resent[array];
                    if let Some(value) = rel.get(&after) {
                        let mut path = rel.iter().map(str::to_owned).collect();
                        projected.push(PatchOperation::Replace(ReplaceOperation {
                            path: rel.clone(),
                            value: self.project_value(&mut path, value),
                        }));
                    }
                }
                Some(_) => (),
                None => projected.extend(self.project_operation(op)),
            }
        }
        Some(Revision {
            id: revision.id,
            patch: DiffPatch(Patch(projected)),
        })
    }

    /// A revision already scoped to the subtree, restricted to the projection.
    pub fn project_revision(&self, revision: Revision) -> Revision {
        if self.is_identity() {
            return revision;
        }
        let DiffPatch(Patch(ops)) = revision.patch;
        Revision {
            id: revision.id,
            patch: DiffPatch(Patch(
                ops.into_iter()
                    .filter_map(|op| self.project_operation(op))
                    .collect(),
            )),
        }
    }

    fn project_operation(&self, op: PatchOperation) -> Option<PatchOperation> {
        if self.is_identity() {
            return Some(op);
        }
        let mut path = op_path(&op).iter().map(str::to_owned).collect::<Vec<_>>();
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
    }

    /// Operations that take a replica holding this projection of `value` to the wider projection's.
    ///
    /// They only add what the wider projection keeps and this one does not; whatever this one
    /// keeps but the wider one drops is the replica's to prune.
    pub fn extension(&self, wider: &Projection, value: &OrdValue) -> DiffPatch {
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
    value: &OrdValue,
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
        OrdValue::Object(map) => {
            for (key, child) in map.iter() {
                path.push(key.to_string());
                if wider.reach(&segments(path)) != Reach::Out {
                    extend(held, wider, path, false, child, ops);
                }
                path.pop();
            }
        }
        OrdValue::Array(items) => {
            if held.cuts_below(&here) && !wider.cuts_below(&here) {
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
fn set(path: &[String], in_array: bool, value: OrdValue) -> PatchOperation {
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

fn op_path(op: &PatchOperation) -> &JsonPointer {
    match op {
        PatchOperation::Add(op) => &op.path,
        PatchOperation::Remove(op) => &op.path,
        PatchOperation::Replace(op) => &op.path,
        _ => unreachable!("database patches contain only add, remove and replace"),
    }
}

/// The arrays a patch inserts into or removes from, read from the document after it.
pub fn shifted_arrays(patch: &DiffPatch, after: &OrdValue) -> Vec<JsonPointer> {
    let mut arrays: Vec<JsonPointer> = Vec::new();
    for op in patch.0 .0.iter() {
        if matches!(op, PatchOperation::Add(_) | PatchOperation::Remove(_)) {
            let path = op_path(op);
            if path.is_empty() {
                continue;
            }
            let parent = path.slice(..path.len() - 1).unwrap_or_default().to_owned();
            if matches!(parent.get(after), Some(OrdValue::Array(_))) && !arrays.contains(&parent) {
                arrays.push(parent);
            }
        }
    }
    arrays
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
