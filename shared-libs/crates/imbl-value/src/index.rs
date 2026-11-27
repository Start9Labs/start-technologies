use std::fmt::{self, Display};

use yasi::InternedString;

use crate::{GenericValue, ObjectKind, ObjectMap};

/// Used in panic messages.
struct Type<'a, K: ObjectKind>(&'a GenericValue<K>);

impl<'a, K: ObjectKind> Display for Type<'a, K> {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match *self.0 {
            GenericValue::Null => formatter.write_str("null"),
            GenericValue::Bool(_) => formatter.write_str("boolean"),
            GenericValue::Number(_) => formatter.write_str("number"),
            GenericValue::String(_) => formatter.write_str("string"),
            GenericValue::Array(_) => formatter.write_str("array"),
            GenericValue::Object(_) => formatter.write_str("object"),
        }
    }
}
pub trait Index {
    /// Return None if the key is not already in the array or object.
    #[doc(hidden)]
    fn index_into<'v, K: ObjectKind>(&self, v: &'v GenericValue<K>) -> Option<&'v GenericValue<K>>;

    /// Return None if the key is not already in the array or object.
    #[doc(hidden)]
    fn index_into_mut<'v, K: ObjectKind>(
        &self,
        v: &'v mut GenericValue<K>,
    ) -> Option<&'v mut GenericValue<K>>;

    /// Return None if the key is not already in the array or object.
    #[doc(hidden)]
    fn index_into_owned<K: ObjectKind>(&self, v: GenericValue<K>) -> Option<GenericValue<K>>;

    /// Panic if array index out of bounds. If key is not already in the object,
    /// insert it with a value of null. Panic if Value is a type that cannot be
    /// indexed into, except if Value is null then it can be treated as an empty
    /// object.
    #[doc(hidden)]
    fn index_or_insert<'v, K: ObjectKind>(
        &self,
        v: &'v mut GenericValue<K>,
    ) -> &'v mut GenericValue<K>;
}

impl Index for usize {
    fn index_into<'v, K: ObjectKind>(&self, v: &'v GenericValue<K>) -> Option<&'v GenericValue<K>> {
        match v {
            GenericValue::Array(vec) => vec.get(*self),
            _ => None,
        }
    }
    fn index_into_mut<'v, K: ObjectKind>(
        &self,
        v: &'v mut GenericValue<K>,
    ) -> Option<&'v mut GenericValue<K>> {
        match v {
            GenericValue::Array(vec) => vec.get_mut(*self),
            _ => None,
        }
    }
    fn index_into_owned<K: ObjectKind>(&self, v: GenericValue<K>) -> Option<GenericValue<K>> {
        match v {
            GenericValue::Array(mut vec) => {
                if vec.len() > *self {
                    Some(vec.remove(*self))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
    fn index_or_insert<'v, K: ObjectKind>(
        &self,
        v: &'v mut GenericValue<K>,
    ) -> &'v mut GenericValue<K> {
        match v {
            GenericValue::Array(vec) => {
                let len = vec.len();
                vec.get_mut(*self).unwrap_or_else(|| {
                    panic!(
                        "cannot access index {} of JSON array of length {}",
                        self, len
                    )
                })
            }
            _ => panic!("cannot access index {} of JSON {}", self, Type(v)),
        }
    }
}

impl Index for str {
    fn index_into<'v, K: ObjectKind>(&self, v: &'v GenericValue<K>) -> Option<&'v GenericValue<K>> {
        match v {
            GenericValue::Object(map) => map.get(self),
            _ => None,
        }
    }
    fn index_into_mut<'v, K: ObjectKind>(
        &self,
        v: &'v mut GenericValue<K>,
    ) -> Option<&'v mut GenericValue<K>> {
        match v {
            GenericValue::Object(map) => map.get_mut(self),
            _ => None,
        }
    }
    fn index_into_owned<K: ObjectKind>(&self, v: GenericValue<K>) -> Option<GenericValue<K>> {
        match v {
            GenericValue::Object(mut map) => map.remove(self),
            _ => None,
        }
    }
    fn index_or_insert<'v, K: ObjectKind>(
        &self,
        v: &'v mut GenericValue<K>,
    ) -> &'v mut GenericValue<K> {
        if let GenericValue::Null = v {
            *v = GenericValue::Object(K::Map::default());
        }
        match v {
            GenericValue::Object(map) => {
                map.get_or_insert_with(InternedString::intern(self), || GenericValue::Null)
            }
            _ => panic!("cannot access key {:?} in JSON {}", self, Type(v)),
        }
    }
}

impl Index for String {
    fn index_into<'v, K: ObjectKind>(&self, v: &'v GenericValue<K>) -> Option<&'v GenericValue<K>> {
        self[..].index_into(v)
    }
    fn index_into_mut<'v, K: ObjectKind>(
        &self,
        v: &'v mut GenericValue<K>,
    ) -> Option<&'v mut GenericValue<K>> {
        self[..].index_into_mut(v)
    }
    fn index_into_owned<K: ObjectKind>(&self, v: GenericValue<K>) -> Option<GenericValue<K>> {
        self[..].index_into_owned(v)
    }
    fn index_or_insert<'v, K: ObjectKind>(
        &self,
        v: &'v mut GenericValue<K>,
    ) -> &'v mut GenericValue<K> {
        self[..].index_or_insert(v)
    }
}

impl<'a, T> Index for &'a T
where
    T: ?Sized + Index,
{
    fn index_into<'v, K: ObjectKind>(&self, v: &'v GenericValue<K>) -> Option<&'v GenericValue<K>> {
        (**self).index_into(v)
    }
    fn index_into_mut<'v, K: ObjectKind>(
        &self,
        v: &'v mut GenericValue<K>,
    ) -> Option<&'v mut GenericValue<K>> {
        (**self).index_into_mut(v)
    }
    fn index_into_owned<K: ObjectKind>(&self, v: GenericValue<K>) -> Option<GenericValue<K>> {
        (**self).index_into_owned(v)
    }
    fn index_or_insert<'v, K: ObjectKind>(
        &self,
        v: &'v mut GenericValue<K>,
    ) -> &'v mut GenericValue<K> {
        (**self).index_or_insert(v)
    }
}

impl<I, K> std::ops::Index<I> for GenericValue<K>
where
    I: Index,
    K: ObjectKind,
{
    type Output = Self;

    /// Index into a `serde_json::Value` using the syntax `value[0]` or
    /// `value["k"]`.
    ///
    /// Returns `Value::Null` if the type of `self` does not match the type of
    /// the index, for example if the index is a string and `self` is an array
    /// or a number. Also returns `Value::Null` if the given key does not exist
    /// in the map or the given index is not within the bounds of the array.
    ///
    /// For retrieving deeply nested values, you should have a look at the
    /// `Value::pointer` method.
    ///
    /// # Examples
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let data = json!({
    ///     "x": {
    ///         "y": ["z", "zz"]
    ///     }
    /// });
    ///
    /// assert_eq!(data["x"]["y"], json!(["z", "zz"]));
    /// assert_eq!(data["x"]["y"][0], json!("z"));
    ///
    /// assert_eq!(data["a"], json!(null)); // returns null for undefined values
    /// assert_eq!(data["a"]["b"], json!(null)); // does not panic
    /// ```
    fn index(&self, index: I) -> &Self {
        index.index_into(self).unwrap_or(&GenericValue::Null)
    }
}

impl<I, K> std::ops::IndexMut<I> for GenericValue<K>
where
    I: Index,
    K: ObjectKind,
{
    /// Write into a `serde_json::Value` using the syntax `value[0] = ...` or
    /// `value["k"] = ...`.
    ///
    /// If the index is a number, the value must be an array of length bigger
    /// than the index. Indexing into a value that is not an array or an array
    /// that is too small will panic.
    ///
    /// If the index is a string, the value must be an object or null which is
    /// treated like an empty object. If the key is not already present in the
    /// object, it will be inserted with a value of null. Indexing into a value
    /// that is neither an object nor null will panic.
    ///
    /// # Examples
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let mut data = json!({ "x": 0 });
    ///
    /// // replace an existing key
    /// data["x"] = json!(1);
    ///
    /// // insert a new key
    /// data["y"] = json!([false, false, false]);
    ///
    /// // replace an array value
    /// data["y"][0] = json!(true);
    ///
    /// // inserted a deeply nested key
    /// data["a"]["b"]["c"]["d"] = json!(true);
    ///
    /// println!("{}", data);
    /// ```
    fn index_mut(&mut self, index: I) -> &mut Self {
        index.index_or_insert(self)
    }
}
