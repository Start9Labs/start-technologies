use core::convert::TryFrom;
use core::fmt::Display;
use core::result;
use std::marker::PhantomData;

use crate::object::{ValueStr, ValueVector};
use serde::ser::{Impossible, Serialize};
use serde_json::{Error, Number, Result};

use crate::{GenericValue, InternedString, ObjectKind, ObjectMap};

impl<K: ObjectKind> Serialize for GenericValue<K> {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> result::Result<S::Ok, S::Error>
    where
        S: ::serde::Serializer,
    {
        match self {
            GenericValue::Null => serializer.serialize_unit(),
            GenericValue::Bool(b) => serializer.serialize_bool(*b),
            GenericValue::Number(n) => n.serialize(serializer),
            GenericValue::String(s) => serializer.serialize_str(s.as_str()),
            GenericValue::Array(v) => v.serialize(serializer),
            GenericValue::Object(m) => {
                use serde::ser::SerializeMap;
                let mut map = serializer.serialize_map(Some(m.len()))?;
                for (k, v) in m.entries() {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}

/// Serializer whose output is a `Value`.
///
/// This is the serializer that backs [`serde_json::to_value`][crate::to_value].
/// Unlike the main serde_json serializer which goes from some serializable
/// value of type `T` to JSON text, this one goes from `T` to
/// `serde_json::Value`.
///
/// The `to_value` function is implementable as:
///
/// ```
/// use serde::Serialize;
/// use serde_json::{Error, Value};
///
/// pub fn to_value<T>(input: T) -> Result<Value, Error>
/// where
///     T: Serialize,
/// {
///     input.serialize(serde_json::value::Serializer)
/// }
/// ```
pub struct Serializer<K>(PhantomData<fn() -> K>);

impl<K> Serializer<K> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<K> Default for Serializer<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: ObjectKind> serde::Serializer for Serializer<K> {
    type Ok = GenericValue<K>;
    type Error = Error;

    type SerializeSeq = SerializeVec<K>;
    type SerializeTuple = SerializeVec<K>;
    type SerializeTupleStruct = SerializeVec<K>;
    type SerializeTupleVariant = SerializeTupleVariant<K>;
    type SerializeMap = SerializeMap<K>;
    type SerializeStruct = SerializeMap<K>;
    type SerializeStructVariant = SerializeStructVariant<K>;

    #[inline]
    fn serialize_bool(self, value: bool) -> Result<GenericValue<K>> {
        Ok(GenericValue::Bool(value))
    }

    #[inline]
    fn serialize_i8(self, value: i8) -> Result<GenericValue<K>> {
        self.serialize_i64(value as i64)
    }

    #[inline]
    fn serialize_i16(self, value: i16) -> Result<GenericValue<K>> {
        self.serialize_i64(value as i64)
    }

    #[inline]
    fn serialize_i32(self, value: i32) -> Result<GenericValue<K>> {
        self.serialize_i64(value as i64)
    }

    fn serialize_i64(self, value: i64) -> Result<GenericValue<K>> {
        Ok(GenericValue::Number(value.into()))
    }

    fn serialize_i128(self, value: i128) -> Result<GenericValue<K>> {
        if let Ok(value) = u64::try_from(value) {
            Ok(GenericValue::Number(value.into()))
        } else if let Ok(value) = i64::try_from(value) {
            Ok(GenericValue::Number(value.into()))
        } else {
            Err(<Error as serde::ser::Error>::custom("number out of range"))
        }
    }

    #[inline]
    fn serialize_u8(self, value: u8) -> Result<GenericValue<K>> {
        self.serialize_u64(value as u64)
    }

    #[inline]
    fn serialize_u16(self, value: u16) -> Result<GenericValue<K>> {
        self.serialize_u64(value as u64)
    }

    #[inline]
    fn serialize_u32(self, value: u32) -> Result<GenericValue<K>> {
        self.serialize_u64(value as u64)
    }

    #[inline]
    fn serialize_u64(self, value: u64) -> Result<GenericValue<K>> {
        Ok(GenericValue::Number(value.into()))
    }

    fn serialize_u128(self, value: u128) -> Result<GenericValue<K>> {
        if let Ok(value) = u64::try_from(value) {
            Ok(GenericValue::Number(value.into()))
        } else {
            Err(<Error as serde::ser::Error>::custom("number out of range"))
        }
    }

    #[inline]
    fn serialize_f32(self, value: f32) -> Result<GenericValue<K>> {
        self.serialize_f64(value as f64)
    }

    #[inline]
    fn serialize_f64(self, value: f64) -> Result<GenericValue<K>> {
        Ok(Number::from_f64(value).map_or(GenericValue::Null, GenericValue::Number))
    }

    #[inline]
    fn serialize_char(self, value: char) -> Result<GenericValue<K>> {
        let mut s = String::new();
        s.push(value);
        Ok(GenericValue::String(K::Str::from(s)))
    }

    #[inline]
    fn serialize_str(self, value: &str) -> Result<GenericValue<K>> {
        Ok(GenericValue::String(K::Str::from_str_ref(value)))
    }

    fn serialize_bytes(self, value: &[u8]) -> Result<GenericValue<K>> {
        let vec = value
            .iter()
            .map(|&b| GenericValue::Number(b.into()))
            .collect();
        Ok(GenericValue::Array(vec))
    }

    #[inline]
    fn serialize_unit(self) -> Result<GenericValue<K>> {
        Ok(GenericValue::Null)
    }

    #[inline]
    fn serialize_unit_struct(self, _name: &'static str) -> Result<GenericValue<K>> {
        self.serialize_unit()
    }

    #[inline]
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<GenericValue<K>> {
        self.serialize_str(variant)
    }

    #[inline]
    fn serialize_newtype_struct<T>(self, _name: &'static str, value: &T) -> Result<GenericValue<K>>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<GenericValue<K>>
    where
        T: ?Sized + Serialize,
    {
        let mut values = K::Map::default();
        values.insert(
            InternedString::intern(variant),
            value.serialize(Serializer::<K>::new())?,
        );
        Ok(GenericValue::Object(values))
    }

    #[inline]
    fn serialize_none(self) -> Result<GenericValue<K>> {
        self.serialize_unit()
    }

    #[inline]
    fn serialize_some<T>(self, value: &T) -> Result<GenericValue<K>>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        Ok(SerializeVec {
            vec: ValueVector::<K>::new(),
        })
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        Ok(SerializeTupleVariant {
            name: InternedString::intern(variant),
            vec: ValueVector::<K>::new(),
        })
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        Ok(SerializeMap::Map {
            map: K::Map::default(),
            next_key: None,
        })
    }

    fn serialize_struct(self, name: &'static str, len: usize) -> Result<Self::SerializeStruct> {
        match name {
            _ => self.serialize_map(Some(len)),
        }
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        Ok(SerializeStructVariant {
            name: InternedString::intern(variant),
            map: K::Map::default(),
        })
    }

    fn collect_str<T>(self, value: &T) -> Result<GenericValue<K>>
    where
        T: ?Sized + Display,
    {
        Ok(GenericValue::String(K::Str::from(value.to_string())))
    }
}

pub struct SerializeVec<K: ObjectKind> {
    vec: ValueVector<K>,
}

pub struct SerializeTupleVariant<K: ObjectKind> {
    name: InternedString,
    vec: ValueVector<K>,
}

pub enum SerializeMap<K: ObjectKind> {
    Map {
        map: K::Map,
        next_key: Option<InternedString>,
    },
}

pub struct SerializeStructVariant<K: ObjectKind> {
    name: InternedString,
    map: K::Map,
}

impl<K: ObjectKind> serde::ser::SerializeSeq for SerializeVec<K> {
    type Ok = GenericValue<K>;
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.vec.push_back(value.serialize(Serializer::<K>::new())?);
        Ok(())
    }

    fn end(self) -> Result<GenericValue<K>> {
        Ok(GenericValue::Array(self.vec))
    }
}

impl<K: ObjectKind> serde::ser::SerializeTuple for SerializeVec<K> {
    type Ok = GenericValue<K>;
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        serde::ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<GenericValue<K>> {
        serde::ser::SerializeSeq::end(self)
    }
}

impl<K: ObjectKind> serde::ser::SerializeTupleStruct for SerializeVec<K> {
    type Ok = GenericValue<K>;
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        serde::ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<GenericValue<K>> {
        serde::ser::SerializeSeq::end(self)
    }
}

impl<K: ObjectKind> serde::ser::SerializeTupleVariant for SerializeTupleVariant<K> {
    type Ok = GenericValue<K>;
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.vec.push_back(value.serialize(Serializer::<K>::new())?);
        Ok(())
    }

    fn end(self) -> Result<GenericValue<K>> {
        let mut object = K::Map::default();

        object.insert(self.name, GenericValue::Array(self.vec));

        Ok(GenericValue::Object(object))
    }
}

impl<K: ObjectKind> serde::ser::SerializeMap for SerializeMap<K> {
    type Ok = GenericValue<K>;
    type Error = Error;

    fn serialize_key<T>(&mut self, key: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        match self {
            SerializeMap::Map { next_key, .. } => {
                *next_key = Some(key.serialize(MapKeySerializer)?);
                Ok(())
            }
        }
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        match self {
            SerializeMap::Map { map, next_key } => {
                let key = next_key.take();
                // Panic because this indicates a bug in the program rather than an
                // expected failure.
                let key = key.expect("serialize_value called before serialize_key");
                map.insert(key, value.serialize(Serializer::<K>::new())?);
                Ok(())
            }
        }
    }

    fn end(self) -> Result<GenericValue<K>> {
        match self {
            SerializeMap::Map { map, .. } => Ok(GenericValue::Object(map)),
        }
    }
}

struct MapKeySerializer;

fn key_must_be_a_string() -> Error {
    <Error as serde::ser::Error>::custom("key must be a string")
}

impl serde::Serializer for MapKeySerializer {
    type Ok = InternedString;
    type Error = Error;

    type SerializeSeq = Impossible<InternedString, Error>;
    type SerializeTuple = Impossible<InternedString, Error>;
    type SerializeTupleStruct = Impossible<InternedString, Error>;
    type SerializeTupleVariant = Impossible<InternedString, Error>;
    type SerializeMap = Impossible<InternedString, Error>;
    type SerializeStruct = Impossible<InternedString, Error>;
    type SerializeStructVariant = Impossible<InternedString, Error>;

    #[inline]
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<InternedString> {
        Ok(InternedString::intern(variant))
    }

    #[inline]
    fn serialize_newtype_struct<T>(self, _name: &'static str, value: &T) -> Result<InternedString>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_bool(self, _value: bool) -> Result<InternedString> {
        Err(key_must_be_a_string())
    }

    fn serialize_i8(self, value: i8) -> Result<InternedString> {
        Ok(InternedString::from_display(&value))
    }

    fn serialize_i16(self, value: i16) -> Result<InternedString> {
        Ok(InternedString::from_display(&value))
    }

    fn serialize_i32(self, value: i32) -> Result<InternedString> {
        Ok(InternedString::from_display(&value))
    }

    fn serialize_i64(self, value: i64) -> Result<InternedString> {
        Ok(InternedString::from_display(&value))
    }

    fn serialize_u8(self, value: u8) -> Result<InternedString> {
        Ok(InternedString::from_display(&value))
    }

    fn serialize_u16(self, value: u16) -> Result<InternedString> {
        Ok(InternedString::from_display(&value))
    }

    fn serialize_u32(self, value: u32) -> Result<InternedString> {
        Ok(InternedString::from_display(&value))
    }

    fn serialize_u64(self, value: u64) -> Result<InternedString> {
        Ok(InternedString::from_display(&value))
    }

    fn serialize_f32(self, _value: f32) -> Result<InternedString> {
        Err(key_must_be_a_string())
    }

    fn serialize_f64(self, _value: f64) -> Result<InternedString> {
        Err(key_must_be_a_string())
    }

    #[inline]
    fn serialize_char(self, value: char) -> Result<InternedString> {
        Ok(InternedString::from_display(&value))
    }

    #[inline]
    fn serialize_str(self, value: &str) -> Result<InternedString> {
        Ok(InternedString::intern(value))
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<InternedString> {
        Err(key_must_be_a_string())
    }

    fn serialize_unit(self) -> Result<InternedString> {
        Err(key_must_be_a_string())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<InternedString> {
        Err(key_must_be_a_string())
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<InternedString>
    where
        T: ?Sized + Serialize,
    {
        Err(key_must_be_a_string())
    }

    fn serialize_none(self) -> Result<InternedString> {
        Err(key_must_be_a_string())
    }

    fn serialize_some<T>(self, _value: &T) -> Result<InternedString>
    where
        T: ?Sized + Serialize,
    {
        Err(key_must_be_a_string())
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        Err(key_must_be_a_string())
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> {
        Err(key_must_be_a_string())
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        Err(key_must_be_a_string())
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        Err(key_must_be_a_string())
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        Err(key_must_be_a_string())
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        Err(key_must_be_a_string())
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        Err(key_must_be_a_string())
    }

    fn collect_str<T>(self, value: &T) -> Result<InternedString>
    where
        T: ?Sized + Display,
    {
        Ok(InternedString::from_display(value))
    }
}

impl<K: ObjectKind> serde::ser::SerializeStruct for SerializeMap<K> {
    type Ok = GenericValue<K>;
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        match self {
            SerializeMap::Map { .. } => serde::ser::SerializeMap::serialize_entry(self, key, value),
        }
    }

    fn end(self) -> Result<GenericValue<K>> {
        match self {
            SerializeMap::Map { .. } => serde::ser::SerializeMap::end(self),
        }
    }
}

impl<K: ObjectKind> serde::ser::SerializeStructVariant for SerializeStructVariant<K> {
    type Ok = GenericValue<K>;
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.map.insert(
            InternedString::intern(key),
            value.serialize(Serializer::<K>::new())?,
        );
        Ok(())
    }

    fn end(self) -> Result<GenericValue<K>> {
        let mut object = K::Map::default();

        object.insert(self.name, GenericValue::Object(self.map));

        Ok(GenericValue::Object(object))
    }
}
