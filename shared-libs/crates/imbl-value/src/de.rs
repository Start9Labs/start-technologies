use core::fmt;
use core::str::FromStr;
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::Arc;

use imbl::shared_ptr::DefaultSharedPtr;
use imbl::Vector;
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
                Ok(GenericValue::String(Arc::new(value)))
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
                let mut vec = Vector::new();

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

macro_rules! deserialize_number {
    ($method:ident) => {
        fn $method<V>(self, visitor: V) -> Result<V::Value, Error>
        where
            V: Visitor<'de>,
        {
            match self {
                GenericValue::Number(n) => n.$method(visitor),
                _ => self.deserialize_any(visitor),
            }
        }
    };
}

fn visit_array<'de, K, V>(array: Vector<GenericValue<K>>, visitor: V) -> Result<V::Value, Error>
where
    K: ObjectKind,
    V: Visitor<'de>,
{
    let len = array.len();
    let mut deserializer = SeqDeserializer::new(array);
    let seq = visitor.visit_seq(&mut deserializer)?;
    let remaining = deserializer.iter.len();
    if remaining == 0 {
        Ok(seq)
    } else {
        Err(serde::de::Error::invalid_length(
            len,
            &"fewer elements in array",
        ))
    }
}

fn visit_object<'de, K, V>(object: K::Map, visitor: V) -> Result<V::Value, Error>
where
    K: ObjectKind,
    V: Visitor<'de>,
{
    let len = object.len();
    let mut deserializer = MapDeserializer::<K>::new(object);
    let map = visitor.visit_map(&mut deserializer)?;
    let remaining = deserializer.iter.len();
    if remaining == 0 {
        Ok(map)
    } else {
        Err(serde::de::Error::invalid_length(
            len,
            &"fewer elements in map",
        ))
    }
}

impl<'de, K: ObjectKind> serde::Deserializer<'de> for GenericValue<K> {
    type Error = Error;

    #[inline]
    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self {
            GenericValue::Null => visitor.visit_unit(),
            GenericValue::Bool(v) => visitor.visit_bool(v),
            GenericValue::Number(n) => n.deserialize_any(visitor),
            GenericValue::String(v) => match Arc::try_unwrap(v) {
                Err(v) => visitor.visit_str(v.as_str()),
                Ok(v) => visitor.visit_string(v),
            },
            GenericValue::Array(v) => visit_array(v, visitor),
            GenericValue::Object(v) => visit_object::<K, _>(v, visitor),
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

    #[inline]
    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self {
            GenericValue::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    #[inline]
    fn deserialize_enum<V>(
        self,
        _name: &str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        let (variant, value) = match self {
            GenericValue::Object(value) => {
                let mut iter = value.into_entries();
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
                (variant, Some(value))
            }
            GenericValue::String(variant) => (
                InternedString::intern({
                    struct ArcStr(Arc<String>);
                    impl fmt::Display for ArcStr {
                        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                            fmt::Display::fmt(&self.0, f)
                        }
                    }
                    impl From<ArcStr> for String {
                        fn from(value: ArcStr) -> Self {
                            Arc::try_unwrap(value.0).unwrap_or_else(|v| v.deref().to_owned())
                        }
                    }
                    ArcStr(variant)
                }),
                None,
            ),
            other => {
                return Err(serde::de::Error::invalid_type(
                    other.unexpected(),
                    &"string or map",
                ));
            }
        };

        visitor.visit_enum(EnumDeserializer { variant, value })
    }

    #[inline]
    fn deserialize_newtype_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        let _ = name;
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self {
            GenericValue::Bool(v) => visitor.visit_bool(v),
            _ => Err(self.invalid_type(&visitor)),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self {
            GenericValue::String(v) => match Arc::try_unwrap(v) {
                Err(v) => visitor.visit_str(v.as_str()),
                Ok(v) => visitor.visit_string(v),
            },
            _ => Err(self.invalid_type(&visitor)),
        }
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_byte_buf(visitor)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self {
            GenericValue::String(v) => match Arc::try_unwrap(v) {
                Err(v) => visitor.visit_str(v.as_str()),
                Ok(v) => visitor.visit_string(v),
            },
            GenericValue::Array(v) => visit_array(v, visitor),
            _ => Err(self.invalid_type(&visitor)),
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self {
            GenericValue::Null => visitor.visit_unit(),
            _ => Err(self.invalid_type(&visitor)),
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
        match self {
            GenericValue::Array(v) => visit_array(v, visitor),
            _ => Err(self.invalid_type(&visitor)),
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
        match self {
            GenericValue::Object(v) => visit_object::<K, _>(v, visitor),
            _ => Err(self.invalid_type(&visitor)),
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
        match self {
            GenericValue::Array(v) => visit_array(v, visitor),
            GenericValue::Object(v) => visit_object::<K, _>(v, visitor),
            _ => Err(self.invalid_type(&visitor)),
        }
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        drop(self);
        visitor.visit_unit()
    }
}

struct EnumDeserializer<K: ObjectKind> {
    variant: InternedString,
    value: Option<GenericValue<K>>,
}

impl<'de, K: ObjectKind> EnumAccess<'de> for EnumDeserializer<K> {
    type Error = Error;
    type Variant = VariantDeserializer<K>;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, VariantDeserializer<K>), Error>
    where
        V: DeserializeSeed<'de>,
    {
        let variant = self.variant.into_deserializer();
        let visitor = VariantDeserializer { value: self.value };
        seed.deserialize(variant).map(|v| (v, visitor))
    }
}

impl<'de, K: ObjectKind> IntoDeserializer<'de, Error> for GenericValue<K> {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

struct VariantDeserializer<K: ObjectKind> {
    value: Option<GenericValue<K>>,
}

impl<'de, K: ObjectKind> VariantAccess<'de> for VariantDeserializer<K> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Error> {
        match self.value {
            Some(value) => Deserialize::deserialize(value),
            None => Ok(()),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.value {
            Some(value) => seed.deserialize(value),
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
                    visit_array(v, visitor)
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
            Some(GenericValue::Object(v)) => visit_object::<K, _>(v, visitor),
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

struct SeqDeserializer<K: ObjectKind> {
    iter: imbl::vector::ConsumingIter<GenericValue<K>, DefaultSharedPtr>,
}

impl<K: ObjectKind> SeqDeserializer<K> {
    fn new(vec: Vector<GenericValue<K>>) -> Self {
        SeqDeserializer {
            iter: vec.into_iter(),
        }
    }
}

impl<'de, K: ObjectKind> SeqAccess<'de> for SeqDeserializer<K> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.iter.next() {
            Some(value) => seed.deserialize(value).map(Some),
            None => Ok(None),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        match self.iter.size_hint() {
            (lower, Some(upper)) if lower == upper => Some(upper),
            _ => None,
        }
    }
}

struct MapDeserializer<K: ObjectKind> {
    iter: <K::Map as ObjectMap<GenericValue<K>>>::IntoEntries,
    value: Option<GenericValue<K>>,
}

impl<K: ObjectKind> MapDeserializer<K> {
    fn new(map: K::Map) -> Self {
        MapDeserializer {
            iter: map.into_entries(),
            value: None,
        }
    }
}

impl<'de, K: ObjectKind> MapAccess<'de> for MapDeserializer<K> {
    type Error = Error;

    fn next_key_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.iter.next() {
            Some((key, value)) => {
                self.value = Some(value);
                let key_de = MapKeyDeserializer { key };
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
            Some(value) => seed.deserialize(value),
            None => Err(serde::de::Error::custom("value is missing")),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        match self.iter.size_hint() {
            (lower, Some(upper)) if lower == upper => Some(upper),
            _ => None,
        }
    }
}

macro_rules! deserialize_value_ref_number {
    ($method:ident) => {
        fn $method<V>(self, visitor: V) -> Result<V::Value, Error>
        where
            V: Visitor<'de>,
        {
            match self {
                GenericValue::Number(n) => n.deserialize_any(visitor),
                _ => Err(self.invalid_type(&visitor)),
            }
        }
    };
}

fn visit_array_ref<'de, K, V>(
    array: &'de Vector<GenericValue<K>>,
    visitor: V,
) -> Result<V::Value, Error>
where
    K: ObjectKind,
    V: Visitor<'de>,
{
    let len = array.len();
    let mut deserializer = SeqRefDeserializer::new(array);
    let seq = visitor.visit_seq(&mut deserializer)?;
    let remaining = deserializer.iter.len();
    if remaining == 0 {
        Ok(seq)
    } else {
        Err(serde::de::Error::invalid_length(
            len,
            &"fewer elements in array",
        ))
    }
}

fn visit_object_ref<'de, K, V>(object: &'de K::Map, visitor: V) -> Result<V::Value, Error>
where
    K: ObjectKind,
    V: Visitor<'de>,
{
    let len = object.len();
    let mut deserializer = MapRefDeserializer::<K>::new(object);
    let map = visitor.visit_map(&mut deserializer)?;
    let remaining = deserializer.iter.len();
    if remaining == 0 {
        Ok(map)
    } else {
        Err(serde::de::Error::invalid_length(
            len,
            &"fewer elements in map",
        ))
    }
}

impl<'de, K: ObjectKind> serde::Deserializer<'de> for &'de GenericValue<K> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self {
            GenericValue::Null => visitor.visit_unit(),
            GenericValue::Bool(v) => visitor.visit_bool(*v),
            GenericValue::Number(n) => n.deserialize_any(visitor),
            GenericValue::String(v) => visitor.visit_borrowed_str(v),
            GenericValue::Array(v) => visit_array_ref(v, visitor),
            GenericValue::Object(v) => visit_object_ref::<K, _>(v, visitor),
        }
    }

    deserialize_value_ref_number!(deserialize_i8);
    deserialize_value_ref_number!(deserialize_i16);
    deserialize_value_ref_number!(deserialize_i32);
    deserialize_value_ref_number!(deserialize_i64);
    deserialize_number!(deserialize_i128);
    deserialize_value_ref_number!(deserialize_u8);
    deserialize_value_ref_number!(deserialize_u16);
    deserialize_value_ref_number!(deserialize_u32);
    deserialize_value_ref_number!(deserialize_u64);
    deserialize_number!(deserialize_u128);
    deserialize_value_ref_number!(deserialize_f32);
    deserialize_value_ref_number!(deserialize_f64);

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match *self {
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
        let (variant, value) = match self {
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
            GenericValue::String(variant) => (&***variant, None),
            other => {
                return Err(serde::de::Error::invalid_type(
                    other.unexpected(),
                    &"string or map",
                ));
            }
        };

        visitor.visit_enum(EnumRefDeserializer { variant, value })
    }

    #[inline]
    fn deserialize_newtype_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        let _ = name;
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match *self {
            GenericValue::Bool(v) => visitor.visit_bool(v),
            _ => Err(self.invalid_type(&visitor)),
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
        match self {
            GenericValue::String(v) => visitor.visit_borrowed_str(v),
            _ => Err(self.invalid_type(&visitor)),
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
        match self {
            GenericValue::String(v) => visitor.visit_borrowed_str(v),
            GenericValue::Array(v) => visit_array_ref(v, visitor),
            _ => Err(self.invalid_type(&visitor)),
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
        match *self {
            GenericValue::Null => visitor.visit_unit(),
            _ => Err(self.invalid_type(&visitor)),
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
        match self {
            GenericValue::Array(v) => visit_array_ref(v, visitor),
            _ => Err(self.invalid_type(&visitor)),
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
        match self {
            GenericValue::Object(v) => visit_object_ref::<K, _>(v, visitor),
            _ => Err(self.invalid_type(&visitor)),
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
        match self {
            GenericValue::Array(v) => visit_array_ref(v, visitor),
            GenericValue::Object(v) => visit_object_ref::<K, _>(v, visitor),
            _ => Err(self.invalid_type(&visitor)),
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

struct EnumRefDeserializer<'de, K: ObjectKind> {
    variant: &'de str,
    value: Option<&'de GenericValue<K>>,
}

impl<'de, K: ObjectKind> EnumAccess<'de> for EnumRefDeserializer<'de, K> {
    type Error = Error;
    type Variant = VariantRefDeserializer<'de, K>;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Error>
    where
        V: DeserializeSeed<'de>,
    {
        let variant = self.variant.into_deserializer();
        let visitor = VariantRefDeserializer { value: self.value };
        seed.deserialize(variant).map(|v| (v, visitor))
    }
}

struct VariantRefDeserializer<'de, K: ObjectKind> {
    value: Option<&'de GenericValue<K>>,
}

impl<'de, K: ObjectKind> VariantAccess<'de> for VariantRefDeserializer<'de, K> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Error> {
        match self.value {
            Some(value) => Deserialize::deserialize(value),
            None => Ok(()),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.value {
            Some(value) => seed.deserialize(value),
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
                    visit_array_ref(v, visitor)
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
            Some(GenericValue::Object(v)) => visit_object_ref::<K, _>(v, visitor),
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

struct SeqRefDeserializer<'de, K: ObjectKind> {
    iter: imbl::vector::Iter<'de, GenericValue<K>, DefaultSharedPtr>,
}

impl<'de, K: ObjectKind> SeqRefDeserializer<'de, K> {
    fn new(slice: &'de Vector<GenericValue<K>>) -> Self {
        SeqRefDeserializer { iter: slice.iter() }
    }
}

impl<'de, K: ObjectKind> SeqAccess<'de> for SeqRefDeserializer<'de, K> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.iter.next() {
            Some(value) => seed.deserialize(value).map(Some),
            None => Ok(None),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        match self.iter.size_hint() {
            (lower, Some(upper)) if lower == upper => Some(upper),
            _ => None,
        }
    }
}

struct MapRefDeserializer<'de, K: ObjectKind> {
    iter: <K::Map as ObjectMap<GenericValue<K>>>::Entries<'de>,
    value: Option<&'de GenericValue<K>>,
}

impl<'de, K: ObjectKind> MapRefDeserializer<'de, K> {
    fn new(map: &'de K::Map) -> Self {
        MapRefDeserializer {
            iter: map.entries(),
            value: None,
        }
    }
}

impl<'de, K: ObjectKind> MapAccess<'de> for MapRefDeserializer<'de, K> {
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
            Some(value) => seed.deserialize(value),
            None => Err(serde::de::Error::custom("value is missing")),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        match self.iter.size_hint() {
            (lower, Some(upper)) if lower == upper => Some(upper),
            _ => None,
        }
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
            GenericValue::String(s) => Unexpected::Str(s),
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
