use core::fmt;
use core::str::FromStr;
use std::marker::PhantomData;

use crate::object::{ValueStr, ValueVector};
use serde::de::value::StrDeserializer;
use serde::de::{
    self, Deserialize, DeserializeSeed, EnumAccess, Expected, IntoDeserializer, MapAccess,
    SeqAccess, Unexpected, VariantAccess, Visitor,
};
use serde::forward_to_deserialize_any;
use serde_json::{Error, Number};

use crate::{GenericValue, InternedString, ObjectKind, ObjectMap};

impl<'de, K: ObjectKind> Deserialize<'de> for GenericValue<K> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ValueVisitor<K>(PhantomData<fn() -> K>);

        impl<'de, K: ObjectKind> Visitor<'de> for ValueVisitor<K> {
            type Value = GenericValue<K>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("any valid JSON value")
            }

            #[inline]
            fn visit_bool<E>(self, value: bool) -> Result<GenericValue<K>, E> {
                Ok(GenericValue::Bool(value))
            }

            #[inline]
            fn visit_i64<E>(self, value: i64) -> Result<GenericValue<K>, E> {
                Ok(GenericValue::Number(value.into()))
            }

            #[inline]
            fn visit_u64<E>(self, value: u64) -> Result<GenericValue<K>, E> {
                Ok(GenericValue::Number(value.into()))
            }

            #[inline]
            fn visit_f64<E>(self, value: f64) -> Result<GenericValue<K>, E> {
                Ok(Number::from_f64(value).map_or(GenericValue::Null, GenericValue::Number))
            }

            #[inline]
            fn visit_str<E>(self, value: &str) -> Result<GenericValue<K>, E>
            where
                E: serde::de::Error,
            {
                self.visit_string(String::from(value))
            }

            #[inline]
            fn visit_string<E>(self, value: String) -> Result<GenericValue<K>, E> {
                Ok(GenericValue::String(K::Str::from(value)))
            }

            #[inline]
            fn visit_none<E>(self) -> Result<GenericValue<K>, E> {
                Ok(GenericValue::Null)
            }

            #[inline]
            fn visit_some<D>(self, deserializer: D) -> Result<GenericValue<K>, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                Deserialize::deserialize(deserializer)
            }

            #[inline]
            fn visit_unit<E>(self) -> Result<GenericValue<K>, E> {
                Ok(GenericValue::Null)
            }

            #[inline]
            fn visit_seq<V>(self, mut visitor: V) -> Result<GenericValue<K>, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let mut vec = ValueVector::<K>::new();

                while let Some(elem) = visitor.next_element()? {
                    vec.push_back(elem);
                }

                Ok(GenericValue::Array(vec))
            }

            fn visit_map<V>(self, mut visitor: V) -> Result<GenericValue<K>, V::Error>
            where
                V: MapAccess<'de>,
            {
                match visitor.next_key_seed(KeyClassifier)? {
                    Some(KeyClass::Map(first_key)) => {
                        let mut values = K::Map::default();

                        values.insert(first_key, visitor.next_value()?);
                        while let Some((key, value)) = visitor.next_entry()? {
                            values.insert(key, value);
                        }

                        Ok(GenericValue::Object(values))
                    }
                    None => Ok(GenericValue::Object(K::Map::default())),
                }
            }
        }

        deserializer.deserialize_any(ValueVisitor(PhantomData))
    }
}

impl<K: ObjectKind> FromStr for GenericValue<K> {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Error> {
        serde_json::from_str(s)
    }
}

/// How a by-reference walk hands string data to the visitor.
trait StrMode<'a, 'de> {
    fn visit_str<V: Visitor<'de>>(s: &'a str, visitor: V) -> Result<V::Value, Error>;
}

/// Lends strings for `'de`; output may borrow from the value.
enum Borrowed {}

impl<'de> StrMode<'de, 'de> for Borrowed {
    fn visit_str<V: Visitor<'de>>(s: &'de str, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_borrowed_str(s)
    }
}

/// Copies strings; output never borrows from the value.
enum Copied {}

impl<'a, 'de> StrMode<'a, 'de> for Copied {
    fn visit_str<V: Visitor<'de>>(s: &'a str, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_str(s)
    }
}

/// The deserializer behind both `GenericValue` and `&GenericValue`.
///
/// Walks the value by reference, so a value whose storage is shared is
/// never copied to be taken apart.
struct RefDeserializer<'a, K: ObjectKind, M> {
    value: &'a GenericValue<K>,
    mode: PhantomData<M>,
}

impl<'a, K: ObjectKind, M> RefDeserializer<'a, K, M> {
    fn new(value: &'a GenericValue<K>) -> Self {
        Self {
            value,
            mode: PhantomData,
        }
    }
}

fn visit_array<'a, 'de, K, M, V>(array: &'a ValueVector<K>, visitor: V) -> Result<V::Value, Error>
where
    K: ObjectKind,
    M: StrMode<'a, 'de>,
    V: Visitor<'de>,
{
    let len = array.len();
    let mut deserializer = SeqRefDeserializer::<K, M> {
        iter: array.iter(),
        mode: PhantomData,
    };
    let seq = visitor.visit_seq(&mut deserializer)?;
    if deserializer.iter.len() == 0 {
        Ok(seq)
    } else {
        Err(serde::de::Error::invalid_length(
            len,
            &"fewer elements in array",
        ))
    }
}

fn visit_object<'a, 'de, K, M, V>(object: &'a K::Map, visitor: V) -> Result<V::Value, Error>
where
    K: ObjectKind,
    M: StrMode<'a, 'de>,
    V: Visitor<'de>,
{
    let len = object.len();
    let mut deserializer = MapRefDeserializer::<K, M> {
        iter: object.entries(),
        value: None,
        mode: PhantomData,
    };
    let map = visitor.visit_map(&mut deserializer)?;
    if deserializer.iter.len() == 0 {
        Ok(map)
    } else {
        Err(serde::de::Error::invalid_length(
            len,
            &"fewer elements in map",
        ))
    }
}

macro_rules! deserialize_number {
    ($method:ident) => {
        fn $method<V>(self, visitor: V) -> Result<V::Value, Error>
        where
            V: Visitor<'de>,
        {
            match self.value {
                GenericValue::Number(n) => n.$method(visitor),
                _ => self.deserialize_any(visitor),
            }
        }
    };
}

impl<'a, 'de, K, M> serde::Deserializer<'de> for RefDeserializer<'a, K, M>
where
    K: ObjectKind,
    M: StrMode<'a, 'de>,
{
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            GenericValue::Null => visitor.visit_unit(),
            GenericValue::Bool(v) => visitor.visit_bool(*v),
            GenericValue::Number(n) => n.deserialize_any(visitor),
            GenericValue::String(v) => M::visit_str(v.as_str(), visitor),
            GenericValue::Array(v) => visit_array::<K, M, V>(v, visitor),
            GenericValue::Object(v) => visit_object::<K, M, V>(v, visitor),
        }
    }

    deserialize_number!(deserialize_i8);
    deserialize_number!(deserialize_i16);
    deserialize_number!(deserialize_i32);
    deserialize_number!(deserialize_i64);
    deserialize_number!(deserialize_i128);
    deserialize_number!(deserialize_u8);
    deserialize_number!(deserialize_u16);
    deserialize_number!(deserialize_u32);
    deserialize_number!(deserialize_u64);
    deserialize_number!(deserialize_u128);
    deserialize_number!(deserialize_f32);
    deserialize_number!(deserialize_f64);

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            GenericValue::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_enum<V>(
        self,
        _name: &str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        let (variant, value) = match self.value {
            GenericValue::Object(value) => {
                let mut iter = value.entries();
                let (variant, value) = match iter.next() {
                    Some(v) => v,
                    None => {
                        return Err(serde::de::Error::invalid_value(
                            Unexpected::Map,
                            &"map with a single key",
                        ));
                    }
                };
                // enums are encoded in json as maps with a single key:value pair
                if iter.next().is_some() {
                    return Err(serde::de::Error::invalid_value(
                        Unexpected::Map,
                        &"map with a single key",
                    ));
                }
                (&**variant, Some(value))
            }
            GenericValue::String(variant) => (variant.as_str(), None),
            other => {
                return Err(serde::de::Error::invalid_type(
                    other.unexpected(),
                    &"string or map",
                ));
            }
        };

        visitor.visit_enum(EnumRefDeserializer::<K, M> {
            variant,
            value,
            mode: PhantomData,
        })
    }

    #[inline]
    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            GenericValue::Bool(v) => visitor.visit_bool(*v),
            other => Err(other.invalid_type(&visitor)),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            GenericValue::String(v) => M::visit_str(v.as_str(), visitor),
            other => Err(other.invalid_type(&visitor)),
        }
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            GenericValue::String(v) => M::visit_str(v.as_str(), visitor),
            GenericValue::Array(v) => visit_array::<K, M, V>(v, visitor),
            other => Err(other.invalid_type(&visitor)),
        }
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            GenericValue::Null => visitor.visit_unit(),
            other => Err(other.invalid_type(&visitor)),
        }
    }

    fn deserialize_unit_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            GenericValue::Array(v) => visit_array::<K, M, V>(v, visitor),
            other => Err(other.invalid_type(&visitor)),
        }
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            GenericValue::Object(v) => visit_object::<K, M, V>(v, visitor),
            other => Err(other.invalid_type(&visitor)),
        }
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            GenericValue::Array(v) => visit_array::<K, M, V>(v, visitor),
            GenericValue::Object(v) => visit_object::<K, M, V>(v, visitor),
            other => Err(other.invalid_type(&visitor)),
        }
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

macro_rules! forward_to_ref {
    ($mode:ty, $this:ident => $value:expr) => {
        forward_to_ref! {
            @methods $mode, $this => $value;
            deserialize_any() deserialize_bool() deserialize_i8() deserialize_i16()
            deserialize_i32() deserialize_i64() deserialize_i128() deserialize_u8()
            deserialize_u16() deserialize_u32() deserialize_u64() deserialize_u128()
            deserialize_f32() deserialize_f64() deserialize_char() deserialize_str()
            deserialize_string() deserialize_bytes() deserialize_byte_buf()
            deserialize_option() deserialize_unit() deserialize_unit_struct(name: &'static str)
            deserialize_newtype_struct(name: &'static str) deserialize_seq()
            deserialize_tuple(len: usize) deserialize_tuple_struct(name: &'static str, len: usize)
            deserialize_map()
            deserialize_struct(name: &'static str, fields: &'static [&'static str])
            deserialize_enum(name: &'static str, variants: &'static [&'static str])
            deserialize_identifier() deserialize_ignored_any()
        }
    };
    (@methods $mode:ty, $this:ident => $value:expr; $($method:ident($($arg:ident: $ty:ty),*))*) => {
        $(
            fn $method<V>(self, $($arg: $ty,)* visitor: V) -> Result<V::Value, Error>
            where
                V: Visitor<'de>,
            {
                let $this = self;
                RefDeserializer::<K, $mode>::new($value).$method($($arg,)* visitor)
            }
        )*
    };
}

impl<'de, K: ObjectKind> serde::Deserializer<'de> for GenericValue<K> {
    type Error = Error;

    forward_to_ref!(Copied, this => &this);
}

impl<'de, K: ObjectKind> serde::Deserializer<'de> for &'de GenericValue<K> {
    type Error = Error;

    forward_to_ref!(Borrowed, this => this);
}

impl<'de, K: ObjectKind> IntoDeserializer<'de, Error> for GenericValue<K> {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

struct EnumRefDeserializer<'a, K: ObjectKind, M> {
    variant: &'a str,
    value: Option<&'a GenericValue<K>>,
    mode: PhantomData<M>,
}

impl<'a, 'de, K, M> EnumAccess<'de> for EnumRefDeserializer<'a, K, M>
where
    K: ObjectKind,
    M: StrMode<'a, 'de>,
{
    type Error = Error;
    type Variant = VariantRefDeserializer<'a, K, M>;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Error>
    where
        V: DeserializeSeed<'de>,
    {
        let variant = StrDeserializer::<Error>::new(self.variant);
        let visitor = VariantRefDeserializer {
            value: self.value,
            mode: PhantomData,
        };
        seed.deserialize(variant).map(|v| (v, visitor))
    }
}

struct VariantRefDeserializer<'a, K: ObjectKind, M> {
    value: Option<&'a GenericValue<K>>,
    mode: PhantomData<M>,
}

impl<'a, 'de, K, M> VariantAccess<'de> for VariantRefDeserializer<'a, K, M>
where
    K: ObjectKind,
    M: StrMode<'a, 'de>,
{
    type Error = Error;

    fn unit_variant(self) -> Result<(), Error> {
        match self.value {
            Some(value) => Deserialize::deserialize(RefDeserializer::<K, M>::new(value)),
            None => Ok(()),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.value {
            Some(value) => seed.deserialize(RefDeserializer::<K, M>::new(value)),
            None => Err(serde::de::Error::invalid_type(
                Unexpected::UnitVariant,
                &"newtype variant",
            )),
        }
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            Some(GenericValue::Array(v)) => {
                if v.is_empty() {
                    visitor.visit_unit()
                } else {
                    visit_array::<K, M, V>(v, visitor)
                }
            }
            Some(other) => Err(serde::de::Error::invalid_type(
                other.unexpected(),
                &"tuple variant",
            )),
            None => Err(serde::de::Error::invalid_type(
                Unexpected::UnitVariant,
                &"tuple variant",
            )),
        }
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            Some(GenericValue::Object(v)) => visit_object::<K, M, V>(v, visitor),
            Some(other) => Err(serde::de::Error::invalid_type(
                other.unexpected(),
                &"struct variant",
            )),
            None => Err(serde::de::Error::invalid_type(
                Unexpected::UnitVariant,
                &"struct variant",
            )),
        }
    }
}

struct SeqRefDeserializer<'a, K: ObjectKind, M> {
    iter: imbl::vector::Iter<'a, GenericValue<K>, K::Ptr>,
    mode: PhantomData<M>,
}

impl<'a, 'de, K, M> SeqAccess<'de> for SeqRefDeserializer<'a, K, M>
where
    K: ObjectKind,
    M: StrMode<'a, 'de>,
{
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.iter.next() {
            Some(value) => seed
                .deserialize(RefDeserializer::<K, M>::new(value))
                .map(Some),
            None => Ok(None),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.iter.len())
    }
}

struct MapRefDeserializer<'a, K: ObjectKind, M> {
    iter: <K::Map as ObjectMap<GenericValue<K>>>::Entries<'a>,
    value: Option<&'a GenericValue<K>>,
    mode: PhantomData<M>,
}

impl<'a, 'de, K, M> MapAccess<'de> for MapRefDeserializer<'a, K, M>
where
    K: ObjectKind,
    M: StrMode<'a, 'de>,
{
    type Error = Error;

    fn next_key_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.iter.next() {
            Some((key, value)) => {
                self.value = Some(value);
                let key_de = MapKeyDeserializer { key: key.clone() };
                seed.deserialize(key_de).map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<T>(&mut self, seed: T) -> Result<T::Value, Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.value.take() {
            Some(value) => seed.deserialize(RefDeserializer::<K, M>::new(value)),
            None => Err(serde::de::Error::custom("value is missing")),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.iter.len())
    }
}

struct MapKeyDeserializer {
    key: InternedString,
}

macro_rules! deserialize_integer_key {
    ($method:ident => $visit:ident) => {
        fn $method<V>(self, visitor: V) -> Result<V::Value, Error>
        where
            V: Visitor<'de>,
        {
            match self.key.parse() {
                Ok(integer) => visitor.$visit(integer),
                Err(_) => visitor.visit_str(&*self.key),
            }
        }
    };
}

impl<'de> serde::Deserializer<'de> for MapKeyDeserializer {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        InternedStringDeserializer::new(self.key).deserialize_any(visitor)
    }

    deserialize_integer_key!(deserialize_i8 => visit_i8);
    deserialize_integer_key!(deserialize_i16 => visit_i16);
    deserialize_integer_key!(deserialize_i32 => visit_i32);
    deserialize_integer_key!(deserialize_i64 => visit_i64);
    deserialize_integer_key!(deserialize_i128 => visit_i128);
    deserialize_integer_key!(deserialize_u8 => visit_u8);
    deserialize_integer_key!(deserialize_u16 => visit_u16);
    deserialize_integer_key!(deserialize_u32 => visit_u32);
    deserialize_integer_key!(deserialize_u64 => visit_u64);
    deserialize_integer_key!(deserialize_u128 => visit_u128);

    #[inline]
    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        // Map keys cannot be null.
        visitor.visit_some(self)
    }

    #[inline]
    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        InternedStringDeserializer::new(self.key).deserialize_enum(name, variants, visitor)
    }

    forward_to_deserialize_any! {
        bool f32 f64 char str string bytes byte_buf unit unit_struct seq tuple
        tuple_struct map struct identifier ignored_any
    }
}

struct KeyClassifier;

enum KeyClass {
    Map(InternedString),
}

impl<'de> DeserializeSeed<'de> for KeyClassifier {
    type Value = KeyClass;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(self)
    }
}

impl<'de> Visitor<'de> for KeyClassifier {
    type Value = KeyClass;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string key")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match s {
            _ => Ok(KeyClass::Map(InternedString::intern(s))),
        }
    }

    fn visit_string<E>(self, s: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match s.as_str() {
            _ => Ok(KeyClass::Map(InternedString::intern(s))),
        }
    }
}

impl<K: ObjectKind> GenericValue<K> {
    #[cold]
    fn invalid_type<E>(&self, exp: &dyn Expected) -> E
    where
        E: serde::de::Error,
    {
        serde::de::Error::invalid_type(self.unexpected(), exp)
    }

    #[cold]
    fn unexpected(&self) -> Unexpected<'_> {
        match self {
            GenericValue::Null => Unexpected::Unit,
            GenericValue::Bool(b) => Unexpected::Bool(*b),
            GenericValue::Number(n) => {
                if let Some(n) = n.as_u64() {
                    Unexpected::Unsigned(n)
                } else if let Some(n) = n.as_i64() {
                    Unexpected::Signed(n)
                } else if let Some(n) = n.as_f64() {
                    Unexpected::Float(n)
                } else {
                    unreachable!()
                }
            }
            GenericValue::String(s) => Unexpected::Str(s.as_str()),
            GenericValue::Array(_) => Unexpected::Seq,
            GenericValue::Object(_) => Unexpected::Map,
        }
    }
}

impl From<InternedString> for InternedStringDeserializer {
    fn from(value: InternedString) -> Self {
        Self { value }
    }
}
pub struct InternedStringDeserializer {
    value: InternedString,
}

impl InternedStringDeserializer {
    fn new(value: InternedString) -> Self {
        InternedStringDeserializer { value }
    }
}

impl<'de> de::Deserializer<'de> for InternedStringDeserializer {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_str(&*self.value)
    }

    fn deserialize_enum<V>(
        self,
        _name: &str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_enum(self)
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct identifier ignored_any
    }
}

impl<'de> de::EnumAccess<'de> for InternedStringDeserializer {
    type Error = Error;
    type Variant = UnitOnly;

    fn variant_seed<T>(self, seed: T) -> Result<(T::Value, Self::Variant), Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let value = seed.deserialize(self)?;
        Ok((value, UnitOnly))
    }
}

pub struct UnitOnly;

impl<'de> de::VariantAccess<'de> for UnitOnly {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Error> {
        Ok(())
    }

    fn newtype_variant_seed<T>(self, _seed: T) -> Result<T::Value, Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        Err(de::Error::invalid_type(
            Unexpected::UnitVariant,
            &"newtype variant",
        ))
    }

    fn tuple_variant<V>(self, _len: usize, _visitor: V) -> Result<V::Value, Error>
    where
        V: de::Visitor<'de>,
    {
        Err(de::Error::invalid_type(
            Unexpected::UnitVariant,
            &"tuple variant",
        ))
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: de::Visitor<'de>,
    {
        Err(de::Error::invalid_type(
            Unexpected::UnitVariant,
            &"struct variant",
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde::de::{Deserializer, Visitor};
    use serde::Deserialize;

    use crate::{from_value, json, OrdValue, Value};

    /// Accepts a number or its decimal string, asking for `u64` first.
    #[derive(Debug, PartialEq)]
    struct Port(u64);

    impl<'de> Deserialize<'de> for Port {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct PortVisitor;
            impl<'de> Visitor<'de> for PortVisitor {
                type Value = Port;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("a port")
                }
                fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Port, E> {
                    Ok(Port(v))
                }
                fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Port, E> {
                    v.parse().map(Port).map_err(E::custom)
                }
            }
            deserializer.deserialize_u64(PortVisitor)
        }
    }

    #[derive(Debug, PartialEq, Deserialize)]
    enum Kind {
        Unit,
        Newtype(Port),
        Tuple(u8, String),
        Struct { name: String },
    }

    #[derive(Debug, PartialEq, Deserialize)]
    struct Doc {
        ports: Vec<Port>,
        kinds: Vec<Kind>,
        labels: BTreeMap<u32, Option<String>>,
        bytes: Vec<u8>,
    }

    fn doc() -> Value {
        json!({
            "ports": [80, "443"],
            "kinds": ["Unit", { "Newtype": "8080" }, { "Tuple": [1, "a"] }, { "Struct": { "name": "b" } }],
            "labels": { "2": null, "10": "ten" },
            "bytes": [1, 2, 3],
        })
    }

    fn expected() -> Doc {
        Doc {
            ports: vec![Port(80), Port(443)],
            kinds: vec![
                Kind::Unit,
                Kind::Newtype(Port(8080)),
                Kind::Tuple(1, "a".into()),
                Kind::Struct { name: "b".into() },
            ],
            labels: BTreeMap::from([(2, None), (10, Some("ten".into()))]),
            bytes: vec![1, 2, 3],
        }
    }

    #[test]
    fn owned_and_borrowed_deserialize_alike() {
        let value = doc();
        assert_eq!(Doc::deserialize(&value).unwrap(), expected());
        assert_eq!(from_value::<Doc>(value.clone()).unwrap(), expected());
        assert_eq!(value, doc());

        let ord = OrdValue::from(doc());
        assert_eq!(Doc::deserialize(&ord).unwrap(), expected());
        assert_eq!(from_value::<Doc>(ord).unwrap(), expected());
    }

    #[test]
    fn typed_numbers_match_serde_json() {
        let number: crate::Number = "1.00000000000000001".parse().unwrap();
        let expected = f64::deserialize(&number).unwrap();
        let value = Value::Number(number.clone());
        let ord = OrdValue::Number(number);

        assert_eq!(from_value::<f64>(value.clone()).unwrap(), expected);
        assert_eq!(f64::deserialize(&value).unwrap(), expected);
        assert_eq!(from_value::<f64>(ord.clone()).unwrap(), expected);
        assert_eq!(f64::deserialize(&ord).unwrap(), expected);
    }

    #[test]
    fn borrowed_lends_strings() {
        #[derive(Deserialize)]
        struct Borrowing<'a> {
            name: &'a str,
        }
        let value = json!({ "name": "start9" });
        let borrowing = Borrowing::deserialize(&value).unwrap();
        assert!(std::ptr::eq(
            borrowing.name,
            value["name"].as_str().unwrap()
        ));
    }

    #[test]
    fn rejects_mismatched_shapes() {
        assert!(from_value::<Doc>(json!({ "ports": 1 })).is_err());
        assert!(from_value::<Kind>(json!({ "Unit": null, "Struct": {} })).is_err());
        assert!(from_value::<(u8,)>(json!([1, 2])).is_err());
        assert!(Port::deserialize(&json!(true)).is_err());
    }
}
