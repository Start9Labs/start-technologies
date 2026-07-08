//! A [JSON Patch (RFC 6902)](https://tools.ietf.org/html/rfc6902) and
//! [JSON Merge Patch (RFC 7396)](https://tools.ietf.org/html/rfc7396) implementation for Rust.
//!
//! # Usage
//!
//! Add this to your *Cargo.toml*:
//! ```toml
//! [dependencies]
//! json-patch = "*"
//! ```
//!
//! # Examples
//! Create and patch document using JSON Patch:
//!
//! ```rust
//! #[macro_use]
//! extern crate imbl_value;
//! extern crate json_patch;
//!
//! use json_patch::patch;
//! use serde_json::from_str;
//!
//! # pub fn main() {
//! let mut doc = json!([
//!     { "name": "Andrew" },
//!     { "name": "Maxim" }
//! ]);
//!
//! let p = from_str(r#"[
//!   { "op": "test", "path": "/0/name", "value": "Andrew" },
//!   { "op": "add", "path": "/0/happy", "value": true }
//! ]"#).unwrap();
//!
//! patch(&mut doc, &p).unwrap();
//! assert_eq!(doc, json!([
//!   { "name": "Andrew", "happy": true },
//!   { "name": "Maxim" }
//! ]));
//!
//! # }
//! ```
//!
//! Create and patch document using JSON Merge Patch:
//!
//! ```rust
//! #[macro_use]
//! extern crate imbl_value;
//! extern crate json_patch;
//!
//! use json_patch::merge;
//!
//! # pub fn main() {
//! let mut doc = json!({
//!   "title": "Goodbye!",
//!   "author" : {
//!     "givenName" : "John",
//!     "familyName" : "Doe"
//!   },
//!   "tags":[ "example", "sample" ],
//!   "content": "This will be unchanged"
//! });
//!
//! let patch = json!({
//!   "title": "Hello!",
//!   "phoneNumber": "+01-123-456-7890",
//!   "author": {
//!     "familyName": null
//!   },
//!   "tags": [ "example" ]
//! });
//!
//! merge(&mut doc, &patch);
//! assert_eq!(doc, json!({
//!   "title": "Hello!",
//!   "author" : {
//!     "givenName" : "John"
//!   },
//!   "tags": [ "example" ],
//!   "content": "This will be unchanged",
//!   "phoneNumber": "+01-123-456-7890"
//! }));
//! # }
//! ```
#![deny(warnings)]
#![warn(missing_docs)]
#[cfg_attr(test, macro_use)]
extern crate imbl_value;

use std::error::Error;
use std::{fmt, mem};

use imbl_value::{InOMap as Map, Value};
use json_ptr::{JsonPointer, SegList};
use serde::{Deserialize, Serialize};

/// Representation of JSON Patch (list of patch operations)
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Patch(pub Vec<PatchOperation>);
impl Patch {
    /// Prepend a path to a patch.
    /// This is useful if you run a diff on a JSON document that is a small member of a larger document
    pub fn prepend<S: AsRef<str>, V: SegList>(&mut self, ptr: &JsonPointer<S, V>) {
        for op in self.0.iter_mut() {
            match op {
                PatchOperation::Add(ref mut op) => {
                    op.path.prepend(ptr);
                }
                PatchOperation::Remove(ref mut op) => {
                    op.path.prepend(ptr);
                }
                PatchOperation::Replace(ref mut op) => {
                    op.path.prepend(ptr);
                }
                PatchOperation::Move(ref mut op) => {
                    op.path.prepend(ptr);
                    op.from.prepend(ptr);
                }
                PatchOperation::Copy(ref mut op) => {
                    op.path.prepend(ptr);
                    op.from.prepend(ptr);
                }
                PatchOperation::Test(ref mut op) => {
                    op.path.prepend(ptr);
                }
            }
        }
    }
    /// Checks whether or not the data at a path could be affected by a patch
    pub fn affects_path<S: AsRef<str>, V: SegList>(&self, ptr: &JsonPointer<S, V>) -> bool {
        for op in self.0.iter() {
            match op {
                PatchOperation::Add(ref op) => {
                    if op.path.starts_with(ptr) || ptr.starts_with(&op.path) {
                        return true;
                    }
                }
                PatchOperation::Remove(ref op) => {
                    if op.path.starts_with(ptr) || ptr.starts_with(&op.path) {
                        return true;
                    }
                }
                PatchOperation::Replace(ref op) => {
                    if op.path.starts_with(ptr) || ptr.starts_with(&op.path) {
                        return true;
                    }
                }
                PatchOperation::Move(ref op) => {
                    if op.path.starts_with(ptr)
                        || ptr.starts_with(&op.path)
                        || op.from.starts_with(ptr)
                        || ptr.starts_with(&op.from)
                    {
                        return true;
                    }
                }
                PatchOperation::Copy(ref op) => {
                    if op.path.starts_with(ptr) || ptr.starts_with(&op.path) {
                        return true;
                    }
                }
                PatchOperation::Test(_) => {}
            }
        }
        false
    }
    /// Returns whether the patch is empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// JSON Patch 'add' operation representation
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AddOperation {
    /// JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
    /// within the target document where the operation is performed.
    pub path: JsonPointer<String>,
    /// Value to add to the target location.
    pub value: Value,
}

/// JSON Patch 'remove' operation representation
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct RemoveOperation {
    /// JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
    /// within the target document where the operation is performed.
    pub path: JsonPointer<String>,
}

/// JSON Patch 'replace' operation representation
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ReplaceOperation {
    /// JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
    /// within the target document where the operation is performed.
    pub path: JsonPointer<String>,
    /// Value to replace with.
    pub value: Value,
}

/// JSON Patch 'move' operation representation
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct MoveOperation {
    /// JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
    /// to move value from.
    pub from: JsonPointer<String>,
    /// JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
    /// within the target document where the operation is performed.
    pub path: JsonPointer<String>,
}

/// JSON Patch 'copy' operation representation
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct CopyOperation {
    /// JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
    /// to copy value from.
    pub from: JsonPointer<String>,
    /// JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
    /// within the target document where the operation is performed.
    pub path: JsonPointer<String>,
}

/// JSON Patch 'test' operation representation
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TestOperation {
    /// JSON-Pointer value [RFC6901](https://tools.ietf.org/html/rfc6901) that references a location
    /// within the target document where the operation is performed.
    pub path: JsonPointer<String>,
    /// Value to test against.
    pub value: Value,
}

/// JSON Patch single patch operation
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "op")]
#[serde(rename_all = "lowercase")]
pub enum PatchOperation {
    /// 'add' operation
    Add(AddOperation),
    /// 'remove' operation
    Remove(RemoveOperation),
    /// 'replace' operation
    Replace(ReplaceOperation),
    /// 'move' operation
    Move(MoveOperation),
    /// 'copy' operation
    Copy(CopyOperation),
    /// 'test' operation
    Test(TestOperation),
}

/// This type represents all possible errors that can occur when applying JSON patch
#[derive(Debug)]
pub enum PatchError {
    /// One of the pointers in the patch is invalid
    InvalidPointer,

    /// 'test' operation failed
    TestFailed,
}

impl Error for PatchError {}

impl fmt::Display for PatchError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            PatchError::InvalidPointer => write!(fmt, "invalid pointer"),
            PatchError::TestFailed => write!(fmt, "test failed"),
        }
    }
}

fn add<S: AsRef<str>, V: SegList>(
    doc: &mut Value,
    path: &JsonPointer<S, V>,
    value: Value,
) -> Result<Option<Value>, PatchError> {
    path.insert(doc, value, false)
        .map_err(|_| PatchError::InvalidPointer)
}

fn remove<S: AsRef<str>, V: SegList>(
    doc: &mut Value,
    path: &JsonPointer<S, V>,
    allow_last: bool,
) -> Result<Value, PatchError> {
    path.remove(doc, allow_last)
        .ok_or(PatchError::InvalidPointer)
}

fn replace<S: AsRef<str>, V: SegList>(
    doc: &mut Value,
    path: &JsonPointer<S, V>,
    value: Value,
) -> Result<Value, PatchError> {
    if let Some(target) = path.get_mut(doc) {
        Ok(mem::replace(target, value))
    } else {
        Ok(add(doc, path, value)?.unwrap_or_default())
    }
}

fn mov<S0: AsRef<str>, S1: AsRef<str>>(
    doc: &mut Value,
    from: &JsonPointer<S0>,
    path: &JsonPointer<S1>,
    allow_last: bool,
) -> Result<Option<Value>, PatchError> {
    if path == from {
        return Ok(None);
    }
    // Check we are not moving inside own child
    if path.starts_with(from) || from.is_empty() {
        return Err(PatchError::InvalidPointer);
    }
    let val = remove(doc, from, allow_last)?;
    add(doc, path, val)
}

fn copy<S0: AsRef<str>, S1: AsRef<str>>(
    doc: &mut Value,
    from: &JsonPointer<S0>,
    path: &JsonPointer<S1>,
) -> Result<Option<Value>, PatchError> {
    let source = from.get(doc).ok_or(PatchError::InvalidPointer)?.clone();
    add(doc, path, source)
}

fn test<S: AsRef<str>, V: SegList>(
    doc: &Value,
    path: &JsonPointer<S, V>,
    expected: &Value,
) -> Result<(), PatchError> {
    let target = path.get(doc).ok_or(PatchError::InvalidPointer)?;
    if *target == *expected {
        Ok(())
    } else {
        Err(PatchError::TestFailed)
    }
}

/// Create JSON Patch from JSON Value
/// # Examples
///
/// Create patch from `imbl_value::Value`:
///
/// ```rust
/// #[macro_use]
/// extern crate imbl_value;
/// extern crate json_patch;
///
/// use json_patch::{Patch, from_value};
///
/// # pub fn main() {
/// let patch_value = json!([
///   { "op": "test", "path": "/0/name", "value": "Andrew" },
///   { "op": "add", "path": "/0/happy", "value": true }
/// ]);
/// let patch: Patch = from_value(patch_value).unwrap();
/// # }
/// ```
///
/// Create patch from string:
///
/// ```rust
/// #[macro_use]
/// extern crate serde_json;
/// extern crate json_patch;
///
/// use json_patch::Patch;
/// use serde_json::from_str;
///
/// # pub fn main() {
/// let patch_str = r#"[
///   { "op": "test", "path": "/0/name", "value": "Andrew" },
///   { "op": "add", "path": "/0/happy", "value": true }
/// ]"#;
/// let patch: Patch = from_str(patch_str).unwrap();
/// # }
/// ```
pub fn from_value(value: Value) -> Result<Patch, imbl_value::Error> {
    let patch = imbl_value::from_value::<Vec<PatchOperation>>(value)?;
    Ok(Patch(patch))
}

/// Patch provided JSON document (given as `imbl_value::Value`) in-place. If any of the patch is
/// failed, all previous operations are reverted. In case of internal error resulting in panic,
/// document might be left in inconsistent state.
///
/// # Example
/// Create and patch document:
///
/// ```rust
/// #[macro_use]
/// extern crate imbl_value;
/// extern crate json_patch;
///
/// use json_patch::patch;
/// use serde_json::from_str;
///
/// # pub fn main() {
/// let mut doc = json!([
///     { "name": "Andrew" },
///     { "name": "Maxim" }
/// ]);
///
/// let p = from_str(r#"[
///   { "op": "test", "path": "/0/name", "value": "Andrew" },
///   { "op": "add", "path": "/0/happy", "value": true }
/// ]"#).unwrap();
///
/// patch(&mut doc, &p).unwrap();
/// assert_eq!(doc, json!([
///   { "name": "Andrew", "happy": true },
///   { "name": "Maxim" }
/// ]));
///
/// # }
/// ```
pub fn patch<'a>(doc: &mut Value, patch: &'a Patch) -> Result<Undo<'a>, PatchError> {
    let mut undo = Undo(Vec::with_capacity(patch.0.len()));
    apply_patches(doc, &patch.0, &mut undo).map(|_| undo)
}

/// Object that can be used to undo a patch if successful
pub struct Undo<'a>(Vec<Box<dyn FnOnce(&mut Value) + Send + Sync + 'a>>);
impl<'a> Undo<'a> {
    /// Apply the undo to the document
    pub fn apply(mut self, doc: &mut Value) {
        while let Some(undo) = self.0.pop() {
            undo(doc)
        }
    }
}

// Apply patches while tracking all the changes being made so they can be reverted back in case
// subsequent patches fail. Uses heap allocated closures to keep the state.
fn apply_patches<'a>(
    doc: &mut Value,
    patches: &'a [PatchOperation],
    undo: &mut Undo<'a>,
) -> Result<(), PatchError> {
    let initial_len = undo.0.len();
    for patch in patches {
        let res = match *patch {
            PatchOperation::Add(ref op) => add(doc, &op.path, op.value.clone()).map(|prev| {
                undo.0.push(Box::new(move |doc| {
                    match prev {
                        None => {
                            remove(doc, &op.path, true).unwrap();
                        }
                        Some(v) => {
                            add(doc, &op.path, v).unwrap().unwrap();
                        }
                    };
                }));
            }),
            PatchOperation::Remove(ref op) => remove(doc, &op.path, false).map(|prev| {
                undo.0.push(Box::new(move |doc| {
                    assert!(add(doc, &op.path, prev).unwrap().is_none());
                }));
            }),
            PatchOperation::Replace(ref op) => {
                replace(doc, &op.path, op.value.clone()).map(|prev| {
                    undo.0.push(Box::new(move |doc| {
                        replace(doc, &op.path, prev).unwrap();
                    }));
                })
            }
            PatchOperation::Move(ref op) => mov(doc, &op.from, &op.path, false).map(|prev| {
                undo.0.push(Box::new(move |doc| {
                    mov(doc, &op.path, &op.from, true).unwrap();
                    if let Some(prev) = prev {
                        assert!(add(doc, &op.path, prev).unwrap().is_none());
                    }
                }));
            }),
            PatchOperation::Copy(ref op) => copy(doc, &op.from, &op.path).map(|prev| {
                undo.0.push(Box::new(move |doc| {
                    match prev {
                        None => {
                            remove(doc, &op.path, true).unwrap();
                        }
                        Some(v) => {
                            add(doc, &op.path, v).unwrap().unwrap();
                        }
                    };
                }));
            }),
            PatchOperation::Test(ref op) => test(doc, &op.path, &op.value).map(|()| {
                undo.0.push(Box::new(move |_| ()));
            }),
        };
        if let Err(e) = res {
            while undo.0.len() > initial_len {
                undo.0.pop().unwrap()(doc);
            }
            return Err(e);
        }
    }
    Ok(())
}

/// Patch provided JSON document (given as `imbl_value::Value`) in place.
/// Operations are applied in unsafe manner. If any of the operations fails, all previous
/// operations are not reverted.
pub fn patch_unsafe(doc: &mut Value, patch: &Patch) -> Result<(), PatchError> {
    for op in &patch.0 {
        match *op {
            PatchOperation::Add(ref op) => {
                add(doc, &op.path, op.value.clone())?;
            }
            PatchOperation::Remove(ref op) => {
                remove(doc, &op.path, false)?;
            }
            PatchOperation::Replace(ref op) => {
                replace(doc, &op.path, op.value.clone())?;
            }
            PatchOperation::Move(ref op) => {
                mov(doc, &op.from, &op.path, false)?;
            }
            PatchOperation::Copy(ref op) => {
                copy(doc, &op.from, &op.path)?;
            }
            PatchOperation::Test(ref op) => {
                test(doc, &op.path, &op.value)?;
            }
        };
    }
    Ok(())
}

/// Patch provided JSON document (given as `imbl_value::Value`) in place with JSON Merge Patch
/// (RFC 7396).
///
/// # Example
/// Create and patch document:
///
/// ```rust
/// #[macro_use]
/// extern crate imbl_value;
/// extern crate json_patch;
///
/// use json_patch::merge;
///
/// # pub fn main() {
/// let mut doc = json!({
///   "title": "Goodbye!",
///   "author" : {
///     "givenName" : "John",
///     "familyName" : "Doe"
///   },
///   "tags":[ "example", "sample" ],
///   "content": "This will be unchanged"
/// });
///
/// let patch = json!({
///   "title": "Hello!",
///   "phoneNumber": "+01-123-456-7890",
///   "author": {
///     "familyName": null
///   },
///   "tags": [ "example" ]
/// });
///
/// merge(&mut doc, &patch);
/// assert_eq!(doc, json!({
///   "title": "Hello!",
///   "author" : {
///     "givenName" : "John"
///   },
///   "tags": [ "example" ],
///   "content": "This will be unchanged",
///   "phoneNumber": "+01-123-456-7890"
/// }));
/// # }
/// ```
pub fn merge(doc: &mut Value, patch: &Value) {
    if !patch.is_object() {
        *doc = patch.clone();
        return;
    }

    if !doc.is_object() {
        *doc = Value::Object(Map::new());
    }
    let map = doc.as_object_mut().unwrap();
    for (key, value) in patch.as_object().unwrap() {
        if value.is_null() {
            map.remove(&*key);
        } else {
            merge(map.entry(key.clone()).or_insert(Value::Null), value);
        }
    }
}

#[cfg(feature = "diff")]
mod diff;

#[cfg(feature = "diff")]
pub use self::diff::diff;

#[cfg(test)]
mod tests;
