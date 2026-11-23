use std::borrow::Cow;
use std::sync::Arc;

use imbl::{OrdMap, Vector};
use yasi::InternedString;

use crate::{GenericValue, InOMap, ObjectKind, ObjectMap, OrdValue, Value};

impl<K: ObjectKind> From<serde_json::Value> for GenericValue<K> {
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Array(a) => Self::Array(a.into_iter().map(|a| a.into()).collect()),
            serde_json::Value::Bool(a) => Self::Bool(a),
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Number(a) => Self::Number(a),
            serde_json::Value::Object(a) => Self::Object(object_from_json::<K>(a)),
            serde_json::Value::String(a) => Self::String(a.into()),
        }
    }
}

impl<K: ObjectKind> From<GenericValue<K>> for serde_json::Value {
    fn from(value: GenericValue<K>) -> Self {
        match value {
            GenericValue::Array(a) => Self::Array(a.into_iter().map(|a| a.into()).collect()),
            GenericValue::Bool(a) => Self::Bool(a),
            GenericValue::Null => Self::Null,
            GenericValue::Number(a) => Self::Number(a),
            GenericValue::Object(a) => Self::Object(
                a.into_entries()
                    .map(|(a, b)| ((&*a).to_owned(), b.into()))
                    .collect(),
            ),
            GenericValue::String(a) => Self::String((&*a).to_owned()),
        }
    }
}

impl<K: ObjectKind, T: Clone + Into<GenericValue<K>>> From<&[T]> for GenericValue<K> {
    /// Convert a slice to `Value::Array`.
    fn from(value: &[T]) -> Self {
        Self::Array(value.into_iter().map(|t| t.clone().into()).collect())
    }
}

impl<K: ObjectKind> From<&str> for GenericValue<K> {
    /// Convert string slice to `Value::String`.
    fn from(value: &str) -> Self {
        GenericValue::String(Arc::new(value.into()))
    }
}

impl<K: ObjectKind, T: Into<GenericValue<K>>, const N: usize> From<[T; N]> for GenericValue<K> {
    /// Convert an array to `Value::Array`.
    fn from(value: [T; N]) -> Self {
        Self::Array(value.into_iter().map(|t| t.into()).collect())
    }
}

impl<K: ObjectKind> From<()> for GenericValue<K> {
    /// Convert `()` to `Value::Null`.
    fn from(_: ()) -> Self {
        GenericValue::Null
    }
}

impl<'a, K: ObjectKind> From<Cow<'a, str>> for GenericValue<K> {
    /// Convert copy-on-write string to `Value::String`.
    fn from(value: Cow<'a, str>) -> Self {
        GenericValue::String(Arc::new(value.into_owned()))
    }
}

impl From<serde_json::Map<String, serde_json::Value>> for InOMap<InternedString, Value> {
    fn from(value: serde_json::Map<String, serde_json::Value>) -> Self {
        object_from_json::<crate::InOrder>(value)
    }
}

impl From<InOMap<InternedString, Value>> for Value {
    fn from(value: InOMap<InternedString, Value>) -> Self {
        Self::Object(value)
    }
}

impl From<OrdMap<InternedString, OrdValue>> for OrdValue {
    fn from(value: OrdMap<InternedString, OrdValue>) -> Self {
        Self::Object(value)
    }
}

impl From<Value> for OrdValue {
    fn from(value: Value) -> Self {
        convert(value)
    }
}

impl From<OrdValue> for Value {
    fn from(value: OrdValue) -> Self {
        convert(value)
    }
}

fn object_from_json<K: ObjectKind>(map: serde_json::Map<String, serde_json::Value>) -> K::Map {
    map.into_iter()
        .map(|(k, v)| (InternedString::intern(k), v.into()))
        .collect()
}

fn convert<A: ObjectKind, B: ObjectKind>(value: GenericValue<A>) -> GenericValue<B> {
    match value {
        GenericValue::Null => GenericValue::Null,
        GenericValue::Bool(b) => GenericValue::Bool(b),
        GenericValue::Number(n) => GenericValue::Number(n),
        GenericValue::String(s) => GenericValue::String(s),
        GenericValue::Array(a) => GenericValue::Array(a.into_iter().map(convert).collect()),
        GenericValue::Object(o) => {
            GenericValue::Object(o.into_entries().map(|(k, v)| (k, convert(v))).collect())
        }
    }
}

impl<K: ObjectKind> From<serde_json::Map<String, serde_json::Value>> for GenericValue<K> {
    fn from(value: serde_json::Map<String, serde_json::Value>) -> Self {
        Self::Object(object_from_json::<K>(value))
    }
}

impl<K: ObjectKind> From<serde_json::Number> for GenericValue<K> {
    /// Convert `serde_json::Number` to `Value::Number`.
    fn from(value: serde_json::Number) -> Self {
        Self::Number(value)
    }
}

impl<K: ObjectKind, T: Into<GenericValue<K>>> From<Option<T>> for GenericValue<K> {
    /// Convert `Option` to `Value::Null` if none, or a corresponding `Value` if some
    fn from(value: Option<T>) -> Self {
        match value {
            None => GenericValue::Null,
            Some(a) => a.into(),
        }
    }
}

impl<K: ObjectKind> From<String> for GenericValue<K> {
    /// Convert `String` to `Value::String`.
    fn from(value: String) -> Self {
        GenericValue::String(Arc::new(value))
    }
}

impl<K: ObjectKind> From<Arc<String>> for GenericValue<K> {
    /// Convert `Arc<String>` to `Value::String`.
    fn from(value: Arc<String>) -> Self {
        GenericValue::String(value)
    }
}

impl<K: ObjectKind, T: Into<GenericValue<K>>> From<Vec<T>> for GenericValue<K> {
    /// Convert a `Vec` to `Value::Array`.
    fn from(value: Vec<T>) -> Self {
        GenericValue::Array(value.into_iter().map(|v| v.into()).collect())
    }
}

impl<K: ObjectKind, T: Clone + Into<GenericValue<K>>> From<Vector<T>> for GenericValue<K> {
    /// Convert a `Vector` to `Value::Array`.
    fn from(value: Vector<T>) -> Self {
        GenericValue::Array(value.into_iter().map(|v| v.into()).collect())
    }
}

impl<K: ObjectKind> From<bool> for GenericValue<K> {
    /// Convert a `boolean` to `Value::Bool`.
    fn from(value: bool) -> Self {
        GenericValue::Bool(value)
    }
}

impl<K: ObjectKind> From<f32> for GenericValue<K> {
    /// Convert 32-bit floating point number to `Value::Number`, or `Value::Null` if infinite or NaN.
    fn from(value: f32) -> Self {
        serde_json::Number::from_f64(value as f64).into()
    }
}
impl<K: ObjectKind> From<f64> for GenericValue<K> {
    /// Convert 64-bit floating point number to `Value::Number`, or `Value::Null` if infinite or NaN.
    fn from(value: f64) -> Self {
        serde_json::Number::from_f64(value).into()
    }
}
impl<K: ObjectKind> From<i8> for GenericValue<K> {
    fn from(value: i8) -> Self {
        Self::Number(value.into())
    }
}
impl<K: ObjectKind> From<i16> for GenericValue<K> {
    fn from(value: i16) -> Self {
        Self::Number(value.into())
    }
}
impl<K: ObjectKind> From<i32> for GenericValue<K> {
    fn from(value: i32) -> Self {
        Self::Number(value.into())
    }
}
impl<K: ObjectKind> From<i64> for GenericValue<K> {
    fn from(value: i64) -> Self {
        Self::Number(value.into())
    }
}
impl<K: ObjectKind> From<isize> for GenericValue<K> {
    fn from(value: isize) -> Self {
        Self::Number(value.into())
    }
}
impl<K: ObjectKind> From<u8> for GenericValue<K> {
    fn from(value: u8) -> Self {
        Self::Number(value.into())
    }
}
impl<K: ObjectKind> From<u16> for GenericValue<K> {
    fn from(value: u16) -> Self {
        Self::Number(value.into())
    }
}
impl<K: ObjectKind> From<u32> for GenericValue<K> {
    fn from(value: u32) -> Self {
        Self::Number(value.into())
    }
}
impl<K: ObjectKind> From<u64> for GenericValue<K> {
    fn from(value: u64) -> Self {
        Self::Number(value.into())
    }
}
impl<K: ObjectKind> From<usize> for GenericValue<K> {
    fn from(value: usize) -> Self {
        Self::Number(value.into())
    }
}
