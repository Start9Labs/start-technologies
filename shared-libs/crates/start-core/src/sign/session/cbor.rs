use std::fmt;
use std::io::Cursor;

use serde::de::{DeserializeOwned, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_cbor::Value;

use super::{Error, MAX_HANDSHAKE_DEPTH, invalid};

pub(super) fn read<T: DeserializeOwned>(reader: &mut Cursor<&[u8]>) -> Result<T, Error> {
    let value: AdmissionValue =
        serde_cbor::de::from_reader_with_recursion_limit(reader, MAX_HANDSHAKE_DEPTH)
            .map_err(|_| invalid())?;
    value.0.deserialized().map_err(|_| invalid())
}

struct AdmissionValue(Value);

impl<'de> Deserialize<'de> for AdmissionValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct AdmissionVisitor;

        impl<'de> Visitor<'de> for AdmissionVisitor {
            type Value = AdmissionValue;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an untagged admission value with text map keys")
            }

            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(AdmissionValue(Value::Integer(value.into())))
            }

            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(AdmissionValue(Value::Integer(value.into())))
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                self.visit_string(value.into())
            }

            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(AdmissionValue(Value::Text(value)))
            }

            // Bounded records fit Ciborium's scratch buffer; indefinite bytes invoke visit_byte_buf.
            fn visit_bytes<E: serde::de::Error>(self, value: &[u8]) -> Result<Self::Value, E> {
                Ok(AdmissionValue(Value::Bytes(value.to_vec())))
            }

            fn visit_byte_buf<E: serde::de::Error>(self, _: Vec<u8>) -> Result<Self::Value, E> {
                Err(E::custom("expected a definite byte string"))
            }

            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut fields = Vec::new();
                while let Some(AdmissionValue(key)) = map.next_key::<AdmissionValue>()? {
                    if !matches!(key, Value::Text(_)) {
                        return Err(serde::de::Error::custom("expected a text map key"));
                    }
                    fields.push((key, map.next_value::<AdmissionValue>()?.0));
                }
                Ok(AdmissionValue(Value::Map(fields)))
            }
        }

        deserializer.deserialize_any(AdmissionVisitor)
    }
}
