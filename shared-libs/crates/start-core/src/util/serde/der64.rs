use std::ops::Deref;
use std::str::FromStr;

use base64::Engine;
use clap::builder::ValueParserFactory;
use openssl::pkey::{PKey, Private};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use visit_rs::VisitFields;

use super::{BASE64, Base64};
use crate::prelude::*;
use crate::util::FromStrParser;

#[cfg(test)]
mod tests;

/// A value with an authoritative DER representation.
pub trait DerEncoding: Sized {
    fn from_der<E: serde::de::Error>(der: &[u8]) -> Result<Self, E>;
    fn to_der<E: serde::ser::Error>(&self) -> Result<Vec<u8>, E>;
}

impl DerEncoding for PKey<Private> {
    fn from_der<E: serde::de::Error>(der: &[u8]) -> Result<Self, E> {
        Self::private_key_from_pkcs8(der).map_err(E::custom)
    }
    fn to_der<E: serde::ser::Error>(&self) -> Result<Vec<u8>, E> {
        self.private_key_to_pkcs8().map_err(E::custom)
    }
}

impl DerEncoding for ed25519_dalek::SigningKey {
    fn from_der<E: serde::de::Error>(der: &[u8]) -> Result<Self, E> {
        use pkcs8::DecodePrivateKey;
        Self::from_pkcs8_der(der).map_err(E::custom)
    }
    fn to_der<E: serde::ser::Error>(&self) -> Result<Vec<u8>, E> {
        use pkcs8::EncodePrivateKey;
        self.to_pkcs8_der()
            .map(|doc| doc.as_bytes().to_vec())
            .map_err(E::custom)
    }
}

impl DerEncoding for ed25519_dalek::VerifyingKey {
    fn from_der<E: serde::de::Error>(der: &[u8]) -> Result<Self, E> {
        use pkcs8::DecodePublicKey;
        Self::from_public_key_der(der).map_err(E::custom)
    }
    fn to_der<E: serde::ser::Error>(&self) -> Result<Vec<u8>, E> {
        use pkcs8::EncodePublicKey;
        self.to_public_key_der()
            .map(|doc| doc.as_bytes().to_vec())
            .map_err(E::custom)
    }
}

impl DerEncoding for pkcs8::Document {
    fn from_der<E: serde::de::Error>(der: &[u8]) -> Result<Self, E> {
        Self::try_from(der).map_err(E::custom)
    }
    fn to_der<E: serde::ser::Error>(&self) -> Result<Vec<u8>, E> {
        Ok(self.as_bytes().to_vec())
    }
}

pub fn serialize<T: DerEncoding, S: Serializer>(
    value: &T,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&encode::<_, S::Error>(value)?)
}

pub fn deserialize<'de, T: DerEncoding, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<T, D::Error> {
    let value = String::deserialize(deserializer)?;
    decode(&value)
}

fn encode<T: DerEncoding, E: serde::ser::Error>(value: &T) -> Result<String, E> {
    Ok(Base64(value.to_der::<E>()?).to_padded_string())
}

fn decode<T: DerEncoding, E: serde::de::Error>(value: &str) -> Result<T, E> {
    let der = if value.starts_with("-----BEGIN ") {
        der::pem::decode_vec(value.as_bytes())
            .map(|(_, der)| der)
            .map_err(E::custom)?
    } else {
        BASE64
            .decode(value.replace('-', "+").replace('_', "/"))
            .map_err(E::custom)?
    };
    T::from_der(&der)
}

/// Standard Base64 DER with padding. Deserialization also accepts legacy PEM armor.
#[repr(transparent)]
#[derive(
    Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash, VisitFields,
)]
#[visit(opaque, ts(type = "z.string()"))]
pub struct Der64<T: DerEncoding>(#[serde(with = "self")] pub T);

rpc_toolkit::reflect_ts!(impl [T: DerEncoding] for Der64<T> where []);

impl<T: DerEncoding> Der64<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }
    pub fn new_ref(value: &T) -> &Self {
        unsafe { &*(value as *const T as *const Self) }
    }
}
impl<T: DerEncoding> Deref for Der64<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}
impl<T: DerEncoding> std::fmt::Display for Der64<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&encode::<_, serde_json::Error>(&self.0).map_err(|_| std::fmt::Error)?)
    }
}
impl<T: DerEncoding> FromStr for Der64<T> {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        decode::<T, serde_json::Error>(value)
            .map(Self)
            .with_kind(ErrorKind::Deserialization)
    }
}
impl<T: DerEncoding> ValueParserFactory for Der64<T> {
    type Parser = FromStrParser<Self>;
    fn value_parser() -> Self::Parser {
        Self::Parser::new()
    }
}
