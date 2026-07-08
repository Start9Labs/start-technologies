use std::cmp::Ordering;
use std::fmt::{self, Debug, Display};
use std::hash::{Hash, Hasher};
use std::io;
use std::sync::Arc;

use imbl::Vector;
use index::Index;
use serde::de::DeserializeOwned;
use serde::Serialize;

pub mod de;
mod from;
pub mod in_order_map;
pub mod index;
pub mod macros;
pub mod ser;

#[cfg(feature = "arbitrary")]
mod arbitrary;

#[cfg(feature = "ts-rs")]
mod ts_rs;

pub use imbl;
pub use in_order_map::InOMap;
pub use serde_json::{Error as ErrorSource, Number};
pub use yasi::InternedString;

pub const NULL: Value = Value::Null;

#[derive(Debug)]
pub enum ErrorKind {
    Serialization,
    Deserialization,
}

#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
    pub source: ErrorSource,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} Error: {}", self.kind, self.source)
    }
}
impl std::error::Error for Error {}

/// See the [`serde_json::value` module documentation](self) for usage examples.
#[derive(Clone)]
pub enum Value {
    /// Represents a JSON null value.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let v = json!(null);
    /// ```
    Null,

    /// Represents a JSON boolean.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let v = json!(true);
    /// ```
    Bool(bool),

    /// Represents a JSON number, whether integer or floating point.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let v = json!(12.5);
    /// ```
    Number(Number),

    /// Represents a JSON string.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let v = json!("a string");
    /// ```
    String(Arc<String>),

    /// Represents a JSON array.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let v = json!(["an", "array"]);
    /// ```
    Array(Vector<Value>),

    /// Represents a JSON object.
    ///
    /// By default the map is backed by a BTreeMap. Enable the `preserve_order`
    /// feature of serde_json to use IndexMap instead, which preserves
    /// entries in the order they are inserted into the map. In particular, this
    /// allows JSON data to be deserialized into a Value and serialized to a
    /// string while retaining the order of map keys in the input.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let v = json!({ "an": "object" });
    /// ```
    Object(InOMap<InternedString, Value>),
}

impl Debug for Value {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Null => formatter.write_str("Null"),
            Value::Bool(boolean) => write!(formatter, "Bool({})", boolean),
            Value::Number(number) => Debug::fmt(number, formatter),
            Value::String(string) => write!(formatter, "String({:?})", string),
            Value::Array(vec) => {
                formatter.write_str("Array ")?;
                Debug::fmt(vec, formatter)
            }
            Value::Object(map) => {
                formatter.write_str("Object ")?;
                Debug::fmt(map, formatter)
            }
        }
    }
}

impl Display for Value {
    /// Display a JSON value as a string.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let json = json!({ "city": "London", "street": "10 Downing Street" });
    ///
    /// // Compact format:
    /// //
    /// // {"city":"London","street":"10 Downing Street"}
    /// let compact = format!("{}", json);
    /// assert_eq!(compact,
    ///     "{\"city\":\"London\",\"street\":\"10 Downing Street\"}");
    ///
    /// // Pretty format:
    /// //
    /// // {
    /// //   "city": "London",
    /// //   "street": "10 Downing Street"
    /// // }
    /// let pretty = format!("{:#}", json);
    /// assert_eq!(pretty,
    ///     "{\n  \"city\": \"London\",\n  \"street\": \"10 Downing Street\"\n}");
    /// ```
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        struct WriterFormatter<'a, 'b: 'a> {
            inner: &'a mut fmt::Formatter<'b>,
        }

        impl<'a, 'b> io::Write for WriterFormatter<'a, 'b> {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                // Safety: the serializer below only emits valid utf8 when using
                // the default formatter.
                let s = unsafe { String::from_utf8_unchecked(buf.to_owned()) };
                self.inner.write_str(&s).map_err(io_error)?;
                Ok(buf.len())
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        fn io_error(_: fmt::Error) -> io::Error {
            // Error value does not matter because Display impl just maps it
            // back to fmt::Error.
            io::Error::new(io::ErrorKind::Other, "fmt error")
        }

        let alternate = f.alternate();
        let mut wr = WriterFormatter { inner: f };
        if alternate {
            // {:#}
            serde_json::ser::to_writer_pretty(&mut wr, self).map_err(|_| fmt::Error)
        } else {
            // {}
            serde_json::ser::to_writer(&mut wr, self).map_err(|_| fmt::Error)
        }
    }
}

fn parse_index(s: &str) -> Option<usize> {
    if s.starts_with('+') || (s.starts_with('0') && s.len() != 1) {
        return None;
    }
    s.parse().ok()
}

impl Value {
    /// Index into a JSON array or map. A string index can be used to access a
    /// value in a map, and a usize index can be used to access an element of an
    /// array.
    ///
    /// Returns `None` if the type of `self` does not match the type of the
    /// index, for example if the index is a string and `self` is an array or a
    /// number. Also returns `None` if the given key does not exist in the map
    /// or the given index is not within the bounds of the array.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let object = json!({ "A": 65, "B": 66, "C": 67 });
    /// assert_eq!(*object.get("A").unwrap(), json!(65));
    ///
    /// let array = json!([ "A", "B", "C" ]);
    /// assert_eq!(*array.get(2).unwrap(), json!("C"));
    ///
    /// assert_eq!(array.get("A"), None);
    /// ```
    ///
    /// Square brackets can also be used to index into a value in a more concise
    /// way. This returns `Value::Null` in cases where `get` would have returned
    /// `None`.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let object = json!({
    ///     "A": ["a", "á", "à"],
    ///     "B": ["b", "b́"],
    ///     "C": ["c", "ć", "ć̣", "ḉ"],
    /// });
    /// assert_eq!(object["B"][0], json!("b"));
    ///
    /// assert_eq!(object["D"], json!(null));
    /// assert_eq!(object[0]["x"]["y"]["z"], json!(null));
    /// ```
    pub fn get<I: Index>(&self, index: I) -> Option<&Value> {
        index.index_into(self)
    }

    /// Mutably index into a JSON array or map. A string index can be used to
    /// access a value in a map, and a usize index can be used to access an
    /// element of an array.
    ///
    /// Returns `None` if the type of `self` does not match the type of the
    /// index, for example if the index is a string and `self` is an array or a
    /// number. Also returns `None` if the given key does not exist in the map
    /// or the given index is not within the bounds of the array.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let mut object = json!({ "A": 65, "B": 66, "C": 67 });
    /// *object.get_mut("A").unwrap() = json!(69);
    ///
    /// let mut array = json!([ "A", "B", "C" ]);
    /// *array.get_mut(2).unwrap() = json!("D");
    /// ```
    pub fn get_mut<I: Index>(&mut self, index: I) -> Option<&mut Value> {
        index.index_into_mut(self)
    }

    /// Returns true if the `Value` is an Object. Returns false otherwise.
    ///
    /// For any Value on which `is_object` returns true, `as_object` and
    /// `as_object_mut` are guaranteed to return the map representation of the
    /// object.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let obj = json!({ "a": { "nested": true }, "b": ["an", "array"] });
    ///
    /// assert!(obj.is_object());
    /// assert!(obj["a"].is_object());
    ///
    /// // array, not an object
    /// assert!(!obj["b"].is_object());
    /// ```
    pub fn is_object(&self) -> bool {
        self.as_object().is_some()
    }

    /// If the `Value` is an Object, returns the associated Map. Returns None
    /// otherwise.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let v = json!({ "a": { "nested": true }, "b": ["an", "array"] });
    ///
    /// // The length of `{"nested": true}` is 1 entry.
    /// assert_eq!(v["a"].as_object().unwrap().len(), 1);
    ///
    /// // The array `["an", "array"]` is not an object.
    /// assert_eq!(v["b"].as_object(), None);
    /// ```
    pub fn as_object(&self) -> Option<&InOMap<InternedString, Value>> {
        match self {
            Value::Object(map) => Some(map),
            _ => None,
        }
    }

    /// If the `Value` is an Object, returns the associated mutable Map.
    /// Returns None otherwise.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let mut v = json!({ "a": { "nested": true } });
    ///
    /// v["a"].as_object_mut().unwrap().clear();
    /// assert_eq!(v, json!({ "a": {} }));
    /// ```
    pub fn as_object_mut(&mut self) -> Option<&mut InOMap<InternedString, Value>> {
        match self {
            Value::Object(map) => Some(map),
            _ => None,
        }
    }

    /// Returns true if the `Value` is an Array. Returns false otherwise.
    ///
    /// For any Value on which `is_array` returns true, `as_array` and
    /// `as_array_mut` are guaranteed to return the vector representing the
    /// array.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let obj = json!({ "a": ["an", "array"], "b": { "an": "object" } });
    ///
    /// assert!(obj["a"].is_array());
    ///
    /// // an object, not an array
    /// assert!(!obj["b"].is_array());
    /// ```
    pub fn is_array(&self) -> bool {
        self.as_array().is_some()
    }

    /// If the `Value` is an Array, returns the associated vector. Returns None
    /// otherwise.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let v = json!({ "a": ["an", "array"], "b": { "an": "object" } });
    ///
    /// // The length of `["an", "array"]` is 2 elements.
    /// assert_eq!(v["a"].as_array().unwrap().len(), 2);
    ///
    /// // The object `{"an": "object"}` is not an array.
    /// assert_eq!(v["b"].as_array(), None);
    /// ```
    pub fn as_array(&self) -> Option<&Vector<Value>> {
        match self {
            Value::Array(array) => Some(array),
            _ => None,
        }
    }

    /// If the `Value` is an Array, returns the associated mutable vector.
    /// Returns None otherwise.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let mut v = json!({ "a": ["an", "array"] });
    ///
    /// v["a"].as_array_mut().unwrap().clear();
    /// assert_eq!(v, json!({ "a": [] }));
    /// ```
    pub fn as_array_mut(&mut self) -> Option<&mut Vector<Value>> {
        match self {
            Value::Array(list) => Some(list),
            _ => None,
        }
    }

    /// Returns true if the `Value` is a String. Returns false otherwise.
    ///
    /// For any Value on which `is_string` returns true, `as_str` is guaranteed
    /// to return the string slice.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let v = json!({ "a": "some string", "b": false });
    ///
    /// assert!(v["a"].is_string());
    ///
    /// // The boolean `false` is not a string.
    /// assert!(!v["b"].is_string());
    /// ```
    pub fn is_string(&self) -> bool {
        self.as_str().is_some()
    }

    /// If the `Value` is a String, returns the associated str. Returns None
    /// otherwise.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let v = json!({ "a": "some string", "b": false });
    ///
    /// assert_eq!(v["a"].as_str(), Some("some string"));
    ///
    /// // The boolean `false` is not a string.
    /// assert_eq!(v["b"].as_str(), None);
    ///
    /// // JSON values are printed in JSON representation, so strings are in quotes.
    /// //
    /// //    The value is: "some string"
    /// println!("The value is: {}", v["a"]);
    ///
    /// // Rust strings are printed without quotes.
    /// //
    /// //    The value is: some string
    /// println!("The value is: {}", v["a"].as_str().unwrap());
    /// ```
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Returns true if the `Value` is a Number. Returns false otherwise.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let v = json!({ "a": 1, "b": "2" });
    ///
    /// assert!(v["a"].is_number());
    ///
    /// // The string `"2"` is a string, not a number.
    /// assert!(!v["b"].is_number());
    /// ```
    pub fn is_number(&self) -> bool {
        match *self {
            Value::Number(_) => true,
            _ => false,
        }
    }

    /// Returns true if the `Value` is an integer between `i64::MIN` and
    /// `i64::MAX`.
    ///
    /// For any Value on which `is_i64` returns true, `as_i64` is guaranteed to
    /// return the integer value.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let big = i64::max_value() as u64 + 10;
    /// let v = json!({ "a": 64, "b": big, "c": 256.0 });
    ///
    /// assert!(v["a"].is_i64());
    ///
    /// // Greater than i64::MAX.
    /// assert!(!v["b"].is_i64());
    ///
    /// // Numbers with a decimal point are not considered integers.
    /// assert!(!v["c"].is_i64());
    /// ```
    pub fn is_i64(&self) -> bool {
        match self {
            Value::Number(n) => n.is_i64(),
            _ => false,
        }
    }

    /// Returns true if the `Value` is an integer between zero and `u64::MAX`.
    ///
    /// For any Value on which `is_u64` returns true, `as_u64` is guaranteed to
    /// return the integer value.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let v = json!({ "a": 64, "b": -64, "c": 256.0 });
    ///
    /// assert!(v["a"].is_u64());
    ///
    /// // Negative integer.
    /// assert!(!v["b"].is_u64());
    ///
    /// // Numbers with a decimal point are not considered integers.
    /// assert!(!v["c"].is_u64());
    /// ```
    pub fn is_u64(&self) -> bool {
        match self {
            Value::Number(n) => n.is_u64(),
            _ => false,
        }
    }

    /// Returns true if the `Value` is a number that can be represented by f64.
    ///
    /// For any Value on which `is_f64` returns true, `as_f64` is guaranteed to
    /// return the floating point value.
    ///
    /// Currently this function returns true if and only if both `is_i64` and
    /// `is_u64` return false but this is not a guarantee in the future.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let v = json!({ "a": 256.0, "b": 64, "c": -64 });
    ///
    /// assert!(v["a"].is_f64());
    ///
    /// // Integers.
    /// assert!(!v["b"].is_f64());
    /// assert!(!v["c"].is_f64());
    /// ```
    pub fn is_f64(&self) -> bool {
        match self {
            Value::Number(n) => n.is_f64(),
            _ => false,
        }
    }

    /// If the `Value` is an integer, represent it as i64 if possible. Returns
    /// None otherwise.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let big = i64::max_value() as u64 + 10;
    /// let v = json!({ "a": 64, "b": big, "c": 256.0 });
    ///
    /// assert_eq!(v["a"].as_i64(), Some(64));
    /// assert_eq!(v["b"].as_i64(), None);
    /// assert_eq!(v["c"].as_i64(), None);
    /// ```
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Number(n) => n.as_i64(),
            _ => None,
        }
    }

    /// If the `Value` is an integer, represent it as u64 if possible. Returns
    /// None otherwise.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let v = json!({ "a": 64, "b": -64, "c": 256.0 });
    ///
    /// assert_eq!(v["a"].as_u64(), Some(64));
    /// assert_eq!(v["b"].as_u64(), None);
    /// assert_eq!(v["c"].as_u64(), None);
    /// ```
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::Number(n) => n.as_u64(),
            _ => None,
        }
    }

    /// If the `Value` is a number, represent it as f64 if possible. Returns
    /// None otherwise.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let v = json!({ "a": 256.0, "b": 64, "c": -64 });
    ///
    /// assert_eq!(v["a"].as_f64(), Some(256.0));
    /// assert_eq!(v["b"].as_f64(), Some(64.0));
    /// assert_eq!(v["c"].as_f64(), Some(-64.0));
    /// ```
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => n.as_f64(),
            _ => None,
        }
    }

    /// Returns true if the `Value` is a Boolean. Returns false otherwise.
    ///
    /// For any Value on which `is_boolean` returns true, `as_bool` is
    /// guaranteed to return the boolean value.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let v = json!({ "a": false, "b": "false" });
    ///
    /// assert!(v["a"].is_boolean());
    ///
    /// // The string `"false"` is a string, not a boolean.
    /// assert!(!v["b"].is_boolean());
    /// ```
    pub fn is_boolean(&self) -> bool {
        self.as_bool().is_some()
    }

    /// If the `Value` is a Boolean, returns the associated bool. Returns None
    /// otherwise.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let v = json!({ "a": false, "b": "false" });
    ///
    /// assert_eq!(v["a"].as_bool(), Some(false));
    ///
    /// // The string `"false"` is a string, not a boolean.
    /// assert_eq!(v["b"].as_bool(), None);
    /// ```
    pub fn as_bool(&self) -> Option<bool> {
        match *self {
            Value::Bool(b) => Some(b),
            _ => None,
        }
    }

    /// Returns true if the `Value` is a Null. Returns false otherwise.
    ///
    /// For any Value on which `is_null` returns true, `as_null` is guaranteed
    /// to return `Some(())`.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let v = json!({ "a": null, "b": false });
    ///
    /// assert!(v["a"].is_null());
    ///
    /// // The boolean `false` is not null.
    /// assert!(!v["b"].is_null());
    /// ```
    pub fn is_null(&self) -> bool {
        self.as_null().is_some()
    }

    /// If the `Value` is a Null, returns (). Returns None otherwise.
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let v = json!({ "a": null, "b": false });
    ///
    /// assert_eq!(v["a"].as_null(), Some(()));
    ///
    /// // The boolean `false` is not null.
    /// assert_eq!(v["b"].as_null(), None);
    /// ```
    pub fn as_null(&self) -> Option<()> {
        match *self {
            Value::Null => Some(()),
            _ => None,
        }
    }

    /// Looks up a value by a JSON Pointer.
    ///
    /// JSON Pointer defines a string syntax for identifying a specific value
    /// within a JavaScript Object Notation (JSON) document.
    ///
    /// A Pointer is a Unicode string with the reference tokens separated by `/`.
    /// Inside tokens `/` is replaced by `~1` and `~` is replaced by `~0`. The
    /// addressed value is returned and if there is no such value `None` is
    /// returned.
    ///
    /// For more information read [RFC6901](https://tools.ietf.org/html/rfc6901).
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
    /// assert_eq!(data.pointer("/x/y/1").unwrap(), &json!("zz"));
    /// assert_eq!(data.pointer("/a/b/c"), None);
    /// ```
    pub fn pointer(&self, pointer: &str) -> Option<&Value> {
        if pointer.is_empty() {
            return Some(self);
        }
        if !pointer.starts_with('/') {
            return None;
        }
        pointer
            .split('/')
            .skip(1)
            .map(|x| x.replace("~1", "/").replace("~0", "~"))
            .map(Arc::new)
            .try_fold(self, |target, token| match target {
                Value::Object(map) => map.get(&**token),
                Value::Array(list) => parse_index(&token).and_then(|x| list.get(x)),
                _ => None,
            })
    }

    /// Looks up a value by a JSON Pointer and returns a mutable reference to
    /// that value.
    ///
    /// JSON Pointer defines a string syntax for identifying a specific value
    /// within a JavaScript Object Notation (JSON) document.
    ///
    /// A Pointer is a Unicode string with the reference tokens separated by `/`.
    /// Inside tokens `/` is replaced by `~1` and `~` is replaced by `~0`. The
    /// addressed value is returned and if there is no such value `None` is
    /// returned.
    ///
    /// For more information read [RFC6901](https://tools.ietf.org/html/rfc6901).
    ///
    /// # Example of Use
    ///
    /// ```
    /// use imbl_value::Value;
    ///
    /// fn main() {
    ///     let s = r#"{"x": 1.0, "y": 2.0}"#;
    ///     let mut value: Value = serde_json::from_str(s).unwrap();
    ///
    ///     // Check value using read-only pointer
    ///     assert_eq!(value.pointer("/x"), Some(&1.0.into()));
    ///     // Change value with direct assignment
    ///     *value.pointer_mut("/x").unwrap() = 1.5.into();
    ///     // Check that new value was written
    ///     assert_eq!(value.pointer("/x"), Some(&1.5.into()));
    ///     // Or change the value only if it exists
    ///     value.pointer_mut("/x").map(|v| *v = 1.5.into());
    ///
    ///     // "Steal" ownership of a value. Can replace with any valid Value.
    ///     let old_x = value.pointer_mut("/x").map(Value::take).unwrap();
    ///     assert_eq!(old_x, 1.5);
    ///     assert_eq!(value.pointer("/x").unwrap(), &Value::Null);
    /// }
    /// ```
    pub fn pointer_mut(&mut self, pointer: &str) -> Option<&mut Value> {
        if pointer.is_empty() {
            return Some(self);
        }
        if !pointer.starts_with('/') {
            return None;
        }
        pointer
            .split('/')
            .skip(1)
            .map(|x| x.replace("~1", "/").replace("~0", "~"))
            .map(Arc::new)
            .try_fold(self, |target, token| match target {
                Value::Object(map) => map.get_mut(&**token),
                Value::Array(list) => parse_index(&token).and_then(move |x| list.get_mut(x)),
                _ => None,
            })
    }

    /// Takes the value out of the `Value`, leaving a `Null` in its place.
    ///
    /// ```
    /// use serde_json::json;
    ///
    /// let mut v = json!({ "x": "y" });
    /// assert_eq!(v["x"].take(), json!("y"));
    /// assert_eq!(v, json!({ "x": null }));
    /// ```
    pub fn take(&mut self) -> Value {
        ::std::mem::replace(self, Value::Null)
    }

    fn discriminant(&self) -> u8 {
        match self {
            Value::Null => 0,
            Value::Bool(_) => 1,
            Value::Number(_) => 2,
            Value::String(_) => 3,
            Value::Array(_) => 4,
            Value::Object(_) => 5,
        }
    }

    /// Compares whether the values refer to the same data in memory
    ///
    /// ```
    /// use imbl_value::json;
    ///
    /// let a = json!({ "x": "y" });
    /// let b = a.clone();
    /// let c = json!({ "x": "y" });
    /// assert!(a.ptr_eq(&b));
    /// assert!(!a.ptr_eq(&c));
    /// ```
    pub fn ptr_eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Self::Array(a), Self::Array(b)) => a.ptr_eq(b),
            (Self::Object(a), Self::Object(b)) => a.ptr_eq(b),
            (Self::String(a), Self::String(b)) => Arc::ptr_eq(a, b),
            (a, b) => a == b, // primitive - trivial comparison
        }
    }
}

/// The default value is `Value::Null`.
///
/// This is useful for handling omitted `Value` fields when deserializing.
///
/// # Examples
///
/// ```
/// # use serde::Deserialize;
/// use imbl_value::Value;
///
/// #[derive(Deserialize)]
/// struct Settings {
///     level: i32,
///     #[serde(default)]
///     extras: Value,
/// }
///
/// # fn try_main() -> Result<(), serde_json::Error> {
/// let data = r#" { "level": 42 } "#;
/// let s: Settings = serde_json::from_str(data)?;
///
/// assert_eq!(s.level, 42);
/// assert_eq!(s.extras, Value::Null);
/// #
/// #     Ok(())
/// # }
/// #
/// # try_main().unwrap()
/// ```
impl Default for Value {
    fn default() -> Value {
        Value::Null
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Array(a), Value::Array(b)) => a.ptr_eq(b) || a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Null, Value::Null) => true,
            (Value::Number(a), Value::Number(b)) => a == b,

            (Value::Object(a), Value::Object(b)) => a.ptr_eq(b) || a == b,
            (Value::String(a), Value::String(b)) => Arc::ptr_eq(a, b) || a == b,
            _ => false,
        }
    }
}
impl Eq for Value {}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Value {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Value::Null, Value::Null) => Ordering::Equal,
            (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
            (Value::Number(a), Value::Number(b)) => a.to_string().cmp(&b.to_string()),
            (Value::String(a), Value::String(b)) => a.cmp(b),
            (Value::Array(a), Value::Array(b)) => a.cmp(b),
            (Value::Object(a), Value::Object(b)) => a.cmp(b),
            _ => self.discriminant().cmp(&other.discriminant()),
        }
    }
}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Value::Null => {}
            Value::Bool(b) => b.hash(state),
            Value::Number(n) => n.to_string().hash(state),
            Value::String(s) => s.hash(state),
            Value::Array(a) => a.hash(state),
            Value::Object(o) => o.hash(state),
        }
    }
}

impl PartialEq<f64> for Value {
    fn eq(&self, other: &f64) -> bool {
        match self {
            Value::Number(n) => n.as_f64() == Some(*other),
            _ => false,
        }
    }
}
impl PartialEq<i64> for Value {
    fn eq(&self, other: &i64) -> bool {
        match self {
            Value::Number(n) => n.as_i64() == Some(*other),
            _ => false,
        }
    }
}
impl PartialEq<u64> for Value {
    fn eq(&self, other: &u64) -> bool {
        match self {
            Value::Number(n) => n.as_u64() == Some(*other),
            _ => false,
        }
    }
}
impl PartialEq<str> for Value {
    fn eq(&self, other: &str) -> bool {
        match self {
            Value::String(s) => &**s == other,
            _ => false,
        }
    }
}

pub fn to_value<T>(value: &T) -> Result<Value, Error>
where
    T: Serialize,
{
    value.serialize(ser::Serializer).map_err(|e| Error {
        kind: ErrorKind::Serialization,
        source: e,
    })
}

pub fn from_value<T>(value: Value) -> Result<T, Error>
where
    T: DeserializeOwned,
{
    T::deserialize(value).map_err(|e| Error {
        kind: ErrorKind::Deserialization,
        source: e,
    })
}

#[test]
fn test_serialize_loop() {
    let value = json!({
        "a": "hello I'm a",
        "b": 1,
        "c": true,
        "d": null,
        "e": [123, "testing"],
        "f": { "h": 'i'}
    });
    assert_eq!(
        &serde_json::to_string(&value)
        .unwrap(),
        "{\"a\":\"hello I'm a\",\"b\":1,\"c\":true,\"d\":null,\"e\":[123,\"testing\"],\"f\":{\"h\":\"i\"}}"
    );
    assert_eq!(
        value,
        serde_json::from_str::<Value>(&serde_json::to_string(&value).unwrap()).unwrap(),
    );

    assert_eq!(value["f"]["h"].as_str().unwrap(), "i");
}
