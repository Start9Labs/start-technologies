use std::fmt;
use std::marker::PhantomData;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use visit_rs::VisitFields;

use super::DerEncoding;

/// A binary byte string, without a text encoding or an integer-array fallback.
#[derive(Clone, Debug, VisitFields)]
#[visit(opaque, ts(type = "z.instanceof(Uint8Array)"))]
pub struct ByteString<T>(pub T);

rpc_toolkit::reflect_ts!(impl [T] for ByteString<T> where []);

impl<T: AsRef<[u8]>> Serialize for ByteString<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(self.0.as_ref())
    }
}

impl<'de, T: TryFrom<Vec<u8>>> Deserialize<'de> for ByteString<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor<T>(PhantomData<T>);
        impl<'de, T: TryFrom<Vec<u8>>> serde::de::Visitor<'de> for Visitor<T> {
            type Value = ByteString<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a byte string of the required length")
            }

            fn visit_bytes<E: serde::de::Error>(self, value: &[u8]) -> Result<Self::Value, E> {
                self.visit_byte_buf(value.to_vec())
            }

            fn visit_byte_buf<E: serde::de::Error>(self, value: Vec<u8>) -> Result<Self::Value, E> {
                let length = value.len();
                T::try_from(value)
                    .map(ByteString)
                    .map_err(|_| E::invalid_length(length, &self))
            }
        }
        deserializer.deserialize_bytes(Visitor(PhantomData))
    }
}

/// A CBOR integer whose JavaScript decoder may return a bigint.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, VisitFields)]
#[serde(transparent)]
#[visit(opaque, ts(type = "z.union([z.bigint(), z.number().int().safe()])"))]
pub struct Integer<T>(pub T);

rpc_toolkit::reflect_ts!(impl [T] for Integer<T> where []);

/// An algorithm-tagged DER value carried as a byte string.
#[derive(Clone, Debug, VisitFields)]
#[visit(opaque, ts(type = "z.instanceof(Uint8Array)"))]
pub struct DerBytes<T>(pub T);

rpc_toolkit::reflect_ts!(impl [T] for DerBytes<T> where []);

impl<T: DerEncoding> Serialize for DerBytes<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ByteString(self.0.to_der::<S::Error>()?).serialize(serializer)
    }
}

impl<'de, T: DerEncoding> Deserialize<'de> for DerBytes<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bytes = ByteString::<Vec<u8>>::deserialize(deserializer)?;
        T::from_der::<D::Error>(&bytes.0).map(Self)
    }
}
