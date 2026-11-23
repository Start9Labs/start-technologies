use std::collections::{BTreeMap, BTreeSet};
use std::marker::PhantomData;
use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use imbl::OrdMap;
use imbl_value::ObjectMap;
pub use imbl_value::{GenericValue, ObjectKind, OrdValue, Value};
use patch_db::value::InternedString;
pub use patch_db::{HasModel, MutateResult, PatchDb};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::prelude::*;

pub fn to_value<T>(value: &T) -> Result<Value, Error>
where
    T: Serialize,
{
    patch_db::value::to_value(value).with_kind(ErrorKind::Serialization)
}

pub fn to_ord_value<T>(value: &T) -> Result<OrdValue, Error>
where
    T: Serialize,
{
    patch_db::value::to_ord_value(value).with_kind(ErrorKind::Serialization)
}

pub fn from_value<T>(value: GenericValue<impl ObjectKind>) -> Result<T, Error>
where
    T: DeserializeOwned,
{
    patch_db::value::from_value(value).with_kind(ErrorKind::Deserialization)
}

pub type TypedPatchDb<T> = patch_db::TypedPatchDb<T, Error>;

/// &mut Model<T> <=> &mut OrdValue
#[repr(transparent)]
#[derive(Debug)]
pub struct Model<T> {
    value: OrdValue,
    phantom: PhantomData<T>,
}
impl<T: DeserializeOwned> Model<T> {
    pub fn de(&self) -> Result<T, Error> {
        from_value(self.value.clone())
    }
}
impl<T: Serialize> Model<T> {
    pub fn new(value: &T) -> Result<Self, Error> {
        Ok(Self::from(to_ord_value(value)?))
    }
    pub fn ser(&mut self, value: &T) -> Result<(), Error> {
        self.value = to_ord_value(value)?;
        Ok(())
    }
}

impl<T> Serialize for Model<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.value.serialize(serializer)
    }
}

impl<'de, T: Serialize + Deserialize<'de>> Deserialize<'de> for Model<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        Self::new(&T::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

impl<T: Serialize + DeserializeOwned> Model<T> {
    pub fn replace(&mut self, value: &T) -> Result<T, Error> {
        let orig = self.de()?;
        self.ser(value)?;
        Ok(orig)
    }
    pub fn mutate<U>(&mut self, f: impl FnOnce(&mut T) -> Result<U, Error>) -> Result<U, Error> {
        let mut orig = self.de()?;
        let res = f(&mut orig)?;
        self.ser(&orig)?;
        Ok(res)
    }
    pub fn map_mutate(&mut self, f: impl FnOnce(T) -> Result<T, Error>) -> Result<T, Error> {
        let orig = self.de()?;
        let res = f(orig)?;
        self.ser(&res)?;
        Ok(res)
    }
}
impl<T> Clone for Model<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            phantom: PhantomData,
        }
    }
}
impl<T> From<OrdValue> for Model<T> {
    fn from(value: OrdValue) -> Self {
        Self {
            value,
            phantom: PhantomData,
        }
    }
}
impl<T> From<Model<T>> for OrdValue {
    fn from(value: Model<T>) -> Self {
        value.value
    }
}
impl<'a, T> From<&'a OrdValue> for &'a Model<T> {
    fn from(value: &'a OrdValue) -> Self {
        unsafe { std::mem::transmute(value) }
    }
}
impl<'a, T> From<&'a Model<T>> for &'a OrdValue {
    fn from(value: &'a Model<T>) -> Self {
        unsafe { std::mem::transmute(value) }
    }
}
impl<'a, T> From<&'a mut OrdValue> for &mut Model<T> {
    fn from(value: &'a mut OrdValue) -> Self {
        unsafe { std::mem::transmute(value) }
    }
}
impl<'a, T> From<&'a mut Model<T>> for &mut OrdValue {
    fn from(value: &'a mut Model<T>) -> Self {
        unsafe { std::mem::transmute(value) }
    }
}
impl<T> patch_db::Model<T> for Model<T> {
    type Model<U> = Model<U>;
}

impl<T> Model<Option<T>> {
    pub fn transpose(self) -> Option<Model<T>> {
        use patch_db::ModelExt;
        if self.value.is_null() {
            None
        } else {
            Some(self.transmute(|a| a))
        }
    }
    pub fn transpose_ref(&self) -> Option<&Model<T>> {
        use patch_db::ModelExt;
        if self.value.is_null() {
            None
        } else {
            Some(self.transmute_ref(|a| a))
        }
    }
    pub fn transpose_mut(&mut self) -> Option<&mut Model<T>> {
        use patch_db::ModelExt;
        if self.value.is_null() {
            None
        } else {
            Some(self.transmute_mut(|a| a))
        }
    }
    pub fn from_option(opt: Option<Model<T>>) -> Self {
        use patch_db::ModelExt;
        match opt {
            Some(a) => a.transmute(|a| a),
            None => Self::from_value(OrdValue::Null),
        }
    }
}

impl<T> Model<Arc<T>> {
    pub fn deref(self) -> Model<T> {
        use patch_db::ModelExt;
        self.transmute(|a| a)
    }
    pub fn as_deref(&self) -> &Model<T> {
        use patch_db::ModelExt;
        self.transmute_ref(|a| a)
    }
    pub fn as_deref_mut(&mut self) -> &mut Model<T> {
        use patch_db::ModelExt;
        self.transmute_mut(|a| a)
    }
}

pub trait Map: DeserializeOwned + Serialize {
    type Key;
    type Value;
    fn key_str(key: &Self::Key) -> Result<impl AsRef<str>, Error>;
    fn key_string(key: &Self::Key) -> Result<InternedString, Error> {
        Ok(InternedString::intern(Self::key_str(key)?.as_ref()))
    }
}

impl<A, B> Map for BTreeMap<A, B>
where
    A: serde::Serialize + serde::de::DeserializeOwned + Ord + AsRef<str>,
    B: serde::Serialize + serde::de::DeserializeOwned,
{
    type Key = A;
    type Value = B;
    fn key_str(key: &Self::Key) -> Result<impl AsRef<str>, Error> {
        Ok(key.as_ref())
    }
}

impl<A, B> Map for BTreeMap<JsonKey<A>, B>
where
    A: serde::Serialize + serde::de::DeserializeOwned + Ord,
    B: serde::Serialize + serde::de::DeserializeOwned,
{
    type Key = JsonKey<A>;
    type Value = B;
    fn key_str(key: &Self::Key) -> Result<impl AsRef<str>, Error> {
        serde_json::to_string(&key.0).with_kind(ErrorKind::Serialization)
    }
}

impl<A, B> Map for OrdMap<A, B>
where
    A: serde::Serialize + serde::de::DeserializeOwned + Clone + Ord + AsRef<str>,
    B: serde::Serialize + serde::de::DeserializeOwned + Clone,
{
    type Key = A;
    type Value = B;
    fn key_str(key: &Self::Key) -> Result<impl AsRef<str>, Error> {
        Ok(key.as_ref())
    }
}

impl<T: Map> Model<T>
where
    T::Value: Serialize,
{
    pub fn insert_model(
        &mut self,
        key: &T::Key,
        value: Model<T::Value>,
    ) -> Result<Option<Model<T::Value>>, Error> {
        use patch_db::ModelExt;
        use serde::ser::Error;
        let v = value.into_value();
        match &mut self.value {
            OrdValue::Object(o) => {
                let prev = o.insert(T::key_string(key)?, v);
                Ok(prev.map(|v| Model::from_value(v)))
            }
            v => Err(patch_db::value::Error {
                source: patch_db::value::ErrorSource::custom(format!("expected object found {v}")),
                kind: patch_db::value::ErrorKind::Serialization,
            }
            .into()),
        }
    }
    pub fn insert(
        &mut self,
        key: &T::Key,
        value: &T::Value,
    ) -> Result<Option<Model<T::Value>>, Error> {
        self.insert_model(key, Model::new(value)?)
    }
    pub fn upsert<F>(&mut self, key: &T::Key, value: F) -> Result<&mut Model<T::Value>, Error>
    where
        F: FnOnce() -> Result<T::Value, Error>,
    {
        use serde::ser::Error;
        match &mut self.value {
            OrdValue::Object(o) => {
                use patch_db::ModelExt;
                let s = T::key_str(key)?;
                let exists = o.contains_key(s.as_ref());
                let res = self.transmute_mut(|v| {
                    use patch_db::value::index::Index;
                    s.as_ref().index_or_insert(v)
                });
                if !exists {
                    res.ser(&value()?)?;
                }
                Ok(res)
            }
            v => Err(patch_db::value::Error {
                source: patch_db::value::ErrorSource::custom(format!("expected object found {v}")),
                kind: patch_db::value::ErrorKind::Serialization,
            }
            .into()),
        }
    }
}

impl<T: Map> Model<T>
where
    T::Key: FromStr + Ord + Clone,
    Error: From<<T::Key as FromStr>::Err>,
{
    pub fn keys(&self) -> Result<BTreeSet<T::Key>, Error> {
        use serde::de::Error;
        match &self.value {
            OrdValue::Object(o) => o
                .keys()
                .cloned()
                .map(|k| Ok(T::Key::from_str(&*k)?))
                .collect(),
            v => Err(patch_db::value::Error {
                source: patch_db::value::ErrorSource::custom(format!("expected object found {v}")),
                kind: patch_db::value::ErrorKind::Deserialization,
            }
            .into()),
        }
    }

    pub fn into_entries(self) -> Result<Vec<(T::Key, Model<T::Value>)>, Error> {
        use patch_db::ModelExt;
        use serde::de::Error;
        match self.value {
            OrdValue::Object(o) => o
                .into_iter()
                .map(|(k, v)| Ok((T::Key::from_str(&*k)?, Model::from_value(v))))
                .collect(),
            v => Err(patch_db::value::Error {
                source: patch_db::value::ErrorSource::custom(format!("expected object found {v}")),
                kind: patch_db::value::ErrorKind::Deserialization,
            }
            .into()),
        }
    }
    pub fn as_entries(&self) -> Result<Vec<(T::Key, &Model<T::Value>)>, Error> {
        use patch_db::ModelExt;
        use serde::de::Error;
        match &self.value {
            OrdValue::Object(o) => o
                .iter()
                .map(|(k, v)| Ok((T::Key::from_str(&**k)?, Model::value_as(v))))
                .collect(),
            v => Err(patch_db::value::Error {
                source: patch_db::value::ErrorSource::custom(format!("expected object found {v}")),
                kind: patch_db::value::ErrorKind::Deserialization,
            }
            .into()),
        }
    }
    pub fn as_entries_mut(&mut self) -> Result<Vec<(T::Key, &mut Model<T::Value>)>, Error> {
        use patch_db::ModelExt;
        use serde::de::Error;
        match &mut self.value {
            OrdValue::Object(o) => o
                .entries_mut()
                .map(|(k, v)| Ok((T::Key::from_str(&**k)?, Model::value_as_mut(v))))
                .collect(),
            v => Err(patch_db::value::Error {
                source: patch_db::value::ErrorSource::custom(format!("expected object found {v}")),
                kind: patch_db::value::ErrorKind::Deserialization,
            }
            .into()),
        }
    }
}
impl<T: Map> Model<T> {
    pub fn contains_key(&self, key: &T::Key) -> Result<bool, Error> {
        use serde::de::Error;
        let s = T::key_str(key)?;
        match &self.value {
            OrdValue::Object(o) => Ok(o.contains_key(s.as_ref())),
            v => Err(patch_db::value::Error {
                source: patch_db::value::ErrorSource::custom(format!("expected object found {v}")),
                kind: patch_db::value::ErrorKind::Deserialization,
            }
            .into()),
        }
    }
    pub fn into_idx(self, key: &T::Key) -> Option<Model<T::Value>> {
        use patch_db::ModelExt;
        let s = T::key_str(key).ok()?;
        match &self.value {
            OrdValue::Object(o) if o.contains_key(s.as_ref()) => Some(self.transmute(|v| {
                use patch_db::value::index::Index;
                s.as_ref().index_into_owned(v).unwrap()
            })),
            _ => None,
        }
    }
    pub fn as_idx<'a>(&'a self, key: &T::Key) -> Option<&'a Model<T::Value>> {
        use patch_db::ModelExt;
        let s = T::key_str(key).ok()?;
        match &self.value {
            OrdValue::Object(o) if o.contains_key(s.as_ref()) => Some(self.transmute_ref(|v| {
                use patch_db::value::index::Index;
                s.as_ref().index_into(v).unwrap()
            })),
            _ => None,
        }
    }
    pub fn as_idx_mut<'a>(&'a mut self, key: &T::Key) -> Option<&'a mut Model<T::Value>> {
        use patch_db::ModelExt;
        let s = T::key_str(key).ok()?;
        match &mut self.value {
            OrdValue::Object(o) if o.contains_key(s.as_ref()) => Some(self.transmute_mut(|v| {
                use patch_db::value::index::Index;
                s.as_ref().index_or_insert(v)
            })),
            _ => None,
        }
    }
    pub fn remove(&mut self, key: &T::Key) -> Result<Option<Model<T::Value>>, Error> {
        use serde::ser::Error;
        match &mut self.value {
            OrdValue::Object(o) => {
                let v = o.remove(T::key_str(key)?.as_ref());
                Ok(v.map(patch_db::ModelExt::from_value))
            }
            v => Err(patch_db::value::Error {
                source: patch_db::value::ErrorSource::custom(format!("expected object found {v}")),
                kind: patch_db::value::ErrorKind::Serialization,
            }
            .into()),
        }
    }
}

impl<T: Map> Model<T>
where
    T::Key: FromStr,
    Error: From<<T::Key as FromStr>::Err>,
{
    /// Retains only the elements specified by the predicate.
    /// The predicate can mutate the values and returns whether to keep each entry.
    pub fn retain<F>(&mut self, mut f: F) -> Result<(), Error>
    where
        F: FnMut(&T::Key, &mut Model<T::Value>) -> Result<bool, Error>,
    {
        let mut to_remove = Vec::new();

        match &mut self.value {
            OrdValue::Object(o) => {
                for (k, v) in o.entries_mut() {
                    let key = T::Key::from_str(&**k)?;
                    if !f(&key, patch_db::ModelExt::value_as_mut(v))? {
                        to_remove.push(k.clone());
                    }
                }
            }
            v => {
                use serde::de::Error;
                return Err(patch_db::value::Error {
                    source: patch_db::value::ErrorSource::custom(format!(
                        "expected object found {v}"
                    )),
                    kind: patch_db::value::ErrorKind::Deserialization,
                }
                .into());
            }
        }

        // Remove entries that didn't pass the filter
        if let OrdValue::Object(o) = &mut self.value {
            for k in to_remove {
                o.remove(&k);
            }
        }

        Ok(())
    }
}

/// The `index` that places a new entry after every indexed entry.
pub fn next_index(indices: impl IntoIterator<Item = Option<u32>>) -> u32 {
    indices
        .into_iter()
        .flatten()
        .max()
        .map_or(0, |last| last + 1)
}

/// Seeds missing positions in stored order before subsequent additions receive an index.
pub(crate) fn populate_indices<'a>(
    entries: impl IntoIterator<Item = &'a mut Value>,
) -> Result<(), Error> {
    let entries = entries
        .into_iter()
        .map(|entry| Ok((from_value::<Option<u32>>(entry["index"].clone())?, entry)))
        .collect::<Result<Vec<_>, Error>>()?;
    let mut next = next_index(entries.iter().map(|(index, _)| *index));
    for (index, entry) in entries {
        if index.is_none() {
            entry["index"] = to_value(&next)?;
            next += 1;
        }
    }
    Ok(())
}

#[repr(transparent)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct JsonKey<T>(pub T);
impl<T> From<T> for JsonKey<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}
impl<T> JsonKey<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }
    pub fn unwrap(self) -> T {
        self.0
    }
    pub fn new_ref(value: &T) -> &Self {
        unsafe { std::mem::transmute(value) }
    }
    pub fn new_mut(value: &mut T) -> &mut Self {
        unsafe { std::mem::transmute(value) }
    }
}
impl<T> std::ops::Deref for JsonKey<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T> std::ops::DerefMut for JsonKey<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl<T: DeserializeOwned> FromStr for JsonKey<T> {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s).with_kind(ErrorKind::Deserialization)
    }
}
impl<T: Serialize> Serialize for JsonKey<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::Error;
        serde_json::to_string(&self.0)
            .map_err(S::Error::custom)?
            .serialize(serializer)
    }
}
// { "foo": "bar" } -> "{ \"foo\": \"bar\" }"
impl<'de, T: DeserializeOwned> Deserialize<'de> for JsonKey<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        let string = String::deserialize(deserializer)?;
        Ok(Self(
            serde_json::from_str(&string).map_err(D::Error::custom)?,
        ))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, VisitFields)]
pub struct WithTimeData<T> {
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub value: T,
}
rpc_toolkit::reflect_ts!(impl [T] for WithTimeData<T> where [T: rpc_toolkit::ts::TS]);

impl<T> WithTimeData<T> {
    pub fn new(value: T) -> Self {
        let now = Utc::now();
        Self {
            created_at: now,
            updated_at: now,
            value,
        }
    }
}
impl<T> std::ops::Deref for WithTimeData<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}
impl<T> std::ops::DerefMut for WithTimeData<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.updated_at = Utc::now();
        &mut self.value
    }
}

#[test]
fn next_index_follows_the_highest_assigned_index() {
    assert_eq!(next_index([]), 0);
    assert_eq!(next_index([None, None]), 0);
    assert_eq!(next_index([Some(3), None, Some(1)]), 4);
}

#[test]
fn populating_indices_keeps_assigned_positions() {
    use imbl_value::json;

    let mut entries = [json!({ "index": 3 }), json!({}), json!({ "index": 1 })];
    populate_indices(entries.iter_mut()).unwrap();
    assert_eq!(
        entries,
        [json!({ "index": 3 }), json!({ "index": 4 }), json!({ "index": 1 })]
    );
    let populated = entries.clone();
    populate_indices(entries.iter_mut()).unwrap();
    assert_eq!(entries, populated);
}
