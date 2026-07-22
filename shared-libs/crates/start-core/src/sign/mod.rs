use std::fmt::Display;
use std::str::FromStr;

use ::ed25519::pkcs8::BitStringRef;
use clap::builder::ValueParserFactory;
use der::referenced::OwnedToRef;
use pkcs8::der::AnyRef;
use pkcs8::{PrivateKeyInfoRef, SubjectPublicKeyInfo};
use serde::{Deserialize, Serialize};
use sha2::Sha512;
use ts_rs::TS;

use crate::prelude::*;
use crate::sign::commitment::Digestable;
use crate::sign::ed25519::Ed25519;
use crate::util::FromStrParser;
use crate::util::serde::{deserialize_from_str, serialize_display};

pub mod commitment;
pub mod ed25519;

pub trait SignatureScheme {
    type SigningKey;
    type VerifyingKey;
    type Signature;
    type Digest: digest::Update;
    fn new_digest(&self) -> Self::Digest;
    fn sign(
        &self,
        key: &Self::SigningKey,
        digest: Self::Digest,
        context: &str,
    ) -> Result<Self::Signature, Error>;
    fn sign_commitment<C: Digestable>(
        &self,
        key: &Self::SigningKey,
        commitment: &C,
        context: &str,
    ) -> Result<Self::Signature, Error> {
        let mut digest = self.new_digest();
        commitment.update(&mut digest);
        self.sign(key, digest, context)
    }
    fn verify(
        &self,
        key: &Self::VerifyingKey,
        digest: Self::Digest,
        context: &str,
        signature: &Self::Signature,
    ) -> Result<(), Error>;
    fn verify_commitment<C: Digestable>(
        &self,
        key: &Self::VerifyingKey,
        commitment: &C,
        context: &str,
        signature: &Self::Signature,
    ) -> Result<(), Error> {
        let mut digest = self.new_digest();
        commitment.update(&mut digest);
        self.verify(key, digest, context, signature)
    }
}

#[non_exhaustive]
pub enum AnyScheme {
    Ed25519(Ed25519),
}
impl From<Ed25519> for AnyScheme {
    fn from(value: Ed25519) -> Self {
        Self::Ed25519(value)
    }
}
impl SignatureScheme for AnyScheme {
    type SigningKey = AnySigningKey;
    type VerifyingKey = AnyVerifyingKey;
    type Signature = AnySignature;
    type Digest = AnyDigest;
    fn new_digest(&self) -> Self::Digest {
        match self {
            Self::Ed25519(s) => AnyDigest::Sha512(s.new_digest()),
        }
    }
    fn sign(
        &self,
        key: &Self::SigningKey,
        digest: Self::Digest,
        context: &str,
    ) -> Result<Self::Signature, Error> {
        match (self, key, digest) {
            (Self::Ed25519(s), AnySigningKey::Ed25519(key), AnyDigest::Sha512(digest)) => {
                Ok(AnySignature::Ed25519(s.sign(key, digest, context)?))
            }
            #[allow(unreachable_patterns)]
            _ => Err(Error::new(
                eyre!("mismatched signature algorithm"),
                ErrorKind::InvalidSignature,
            )),
        }
    }
    fn verify(
        &self,
        key: &Self::VerifyingKey,
        digest: Self::Digest,
        context: &str,
        signature: &Self::Signature,
    ) -> Result<(), Error> {
        match (self, key, digest, signature) {
            (
                Self::Ed25519(s),
                AnyVerifyingKey::Ed25519(key),
                AnyDigest::Sha512(digest),
                AnySignature::Ed25519(signature),
            ) => s.verify(key, digest, context, signature),
            #[allow(unreachable_patterns)]
            _ => Err(Error::new(
                eyre!("mismatched signature algorithm"),
                ErrorKind::InvalidSignature,
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, TS)]
#[ts(export, type = "string")]
#[non_exhaustive]
pub enum AnySigningKey {
    Ed25519(<Ed25519 as SignatureScheme>::SigningKey),
}
impl AnySigningKey {
    pub fn scheme(&self) -> AnyScheme {
        match self {
            Self::Ed25519(_) => AnyScheme::Ed25519(Ed25519),
        }
    }
    pub fn verifying_key(&self) -> AnyVerifyingKey {
        match self {
            Self::Ed25519(k) => AnyVerifyingKey::Ed25519(k.into()),
        }
    }
}
impl<'a> TryFrom<PrivateKeyInfoRef<'a>> for AnySigningKey {
    type Error = pkcs8::Error;
    fn try_from(value: PrivateKeyInfoRef<'a>) -> Result<Self, Self::Error> {
        if value.algorithm == ed25519_dalek::pkcs8::ALGORITHM_ID {
            Ok(Self::Ed25519(ed25519_dalek::SigningKey::try_from(value)?))
        } else {
            Err(pkcs8::spki::Error::OidUnknown {
                oid: value.algorithm.oid,
            }
            .into())
        }
    }
}
impl pkcs8::EncodePrivateKey for AnySigningKey {
    fn to_pkcs8_der(&self) -> pkcs8::Result<pkcs8::SecretDocument> {
        use pkcs8::EncodePrivateKey;
        match self {
            Self::Ed25519(s) => s.to_pkcs8_der(),
        }
    }
}
impl FromStr for AnySigningKey {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use pkcs8::DecodePrivateKey;
        Self::from_pkcs8_pem(s).with_kind(ErrorKind::Deserialization)
    }
}
impl Display for AnySigningKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use pkcs8::EncodePrivateKey;
        f.write_str(
            &self
                .to_pkcs8_pem(pkcs8::LineEnding::LF)
                .map_err(|_| std::fmt::Error)?,
        )
    }
}
impl<'de> Deserialize<'de> for AnySigningKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_from_str(deserializer)
    }
}
impl Serialize for AnySigningKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_display(self, serializer)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, TS)]
#[ts(export, type = "string")]
#[non_exhaustive]
pub enum AnyVerifyingKey {
    Ed25519(<Ed25519 as SignatureScheme>::VerifyingKey),
}
impl AnyVerifyingKey {
    pub fn scheme(&self) -> AnyScheme {
        match self {
            Self::Ed25519(_) => AnyScheme::Ed25519(Ed25519),
        }
    }
    /// The key used to look this verifying key up in an `AuthKeys` map: its
    /// canonical PEM encoding, interned.
    pub fn interned_pem(&self) -> InternedString {
        InternedString::intern(self.to_string())
    }
    /// Compact wire form: unpadded base64url of the DER (SPKI) encoding, no
    /// PEM armor — url-safe so it survives a form-urlencoded container
    /// without percent-escaping.
    pub fn to_base64_der(&self) -> Result<String, Error> {
        use base64::Engine;
        use pkcs8::EncodePublicKey;
        Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            self.to_public_key_der()
                .with_kind(ErrorKind::Serialization)?
                .as_bytes(),
        ))
    }
    /// Parse the compact wire form produced by [`Self::to_base64_der`].
    pub fn from_base64_der(s: &str) -> Result<Self, Error> {
        use base64::Engine;
        use pkcs8::DecodePublicKey;
        Self::from_public_key_der(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(s)
                .with_kind(ErrorKind::Deserialization)?,
        )
        .with_kind(ErrorKind::Deserialization)
    }
}
impl<'a> TryFrom<SubjectPublicKeyInfo<AnyRef<'a>, BitStringRef<'a>>> for AnyVerifyingKey {
    type Error = pkcs8::spki::Error;
    fn try_from(
        value: SubjectPublicKeyInfo<AnyRef<'a>, BitStringRef<'a>>,
    ) -> Result<Self, Self::Error> {
        if value.algorithm == ed25519_dalek::pkcs8::ALGORITHM_ID {
            Ok(Self::Ed25519(ed25519_dalek::VerifyingKey::try_from(value)?))
        } else {
            Err(pkcs8::spki::Error::OidUnknown {
                oid: value.algorithm.oid,
            })
        }
    }
}
impl pkcs8::EncodePublicKey for AnyVerifyingKey {
    fn to_public_key_der(&self) -> pkcs8::spki::Result<pkcs8::Document> {
        use pkcs8::spki::EncodePublicKey;
        match self {
            Self::Ed25519(s) => s.to_public_key_der(),
        }
    }
}
impl FromStr for AnyVerifyingKey {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use pkcs8::DecodePublicKey;
        Self::from_public_key_pem(s).with_kind(ErrorKind::Deserialization)
    }
}
impl Display for AnyVerifyingKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use pkcs8::EncodePublicKey;
        f.write_str(
            &self
                .to_public_key_pem(pkcs8::LineEnding::LF)
                .map_err(|_| std::fmt::Error)?,
        )
    }
}
impl<'de> Deserialize<'de> for AnyVerifyingKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_from_str(deserializer)
    }
}
impl Serialize for AnyVerifyingKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_display(self, serializer)
    }
}
impl ValueParserFactory for AnyVerifyingKey {
    type Parser = FromStrParser<Self>;
    fn value_parser() -> Self::Parser {
        Self::Parser::new()
    }
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum AnyDigest {
    Sha512(Sha512),
}
impl digest::Update for AnyDigest {
    fn update(&mut self, data: &[u8]) {
        match self {
            Self::Sha512(d) => digest::Update::update(d, data),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, TS)]
#[ts(export, type = "string")]
pub enum AnySignature {
    Ed25519(<Ed25519 as SignatureScheme>::Signature),
}
/// The DER document wrapping an [`AnySignature`]: the scheme's algorithm
/// identifier plus the raw signature bytes. PEM-armored ("SIGNATURE") in the
/// legacy wire form, bare base64 in the compact one.
#[derive(der::Sequence)]
struct AnySignatureDer {
    alg: pkcs8::spki::AlgorithmIdentifierOwned,
    sig: der::asn1::OctetString,
}
impl der::pem::PemLabel for AnySignatureDer {
    const PEM_LABEL: &'static str = "SIGNATURE";
}
impl TryFrom<AnySignatureDer> for AnySignature {
    type Error = Error;
    fn try_from(der: AnySignatureDer) -> Result<Self, Error> {
        if der.alg.oid == ed25519_dalek::pkcs8::ALGORITHM_ID.oid
            && der.alg.parameters.owned_to_ref() == ed25519_dalek::pkcs8::ALGORITHM_ID.parameters
        {
            Ok(Self::Ed25519(
                ed25519_dalek::Signature::from_slice(der.sig.as_bytes())
                    .with_kind(ErrorKind::Deserialization)?,
            ))
        } else {
            Err(pkcs8::spki::Error::OidUnknown { oid: der.alg.oid })
                .with_kind(ErrorKind::Deserialization)
        }
    }
}

impl AnySignature {
    /// Compact wire form: unpadded base64url of the DER encoding, no PEM
    /// armor — url-safe so it survives a form-urlencoded container without
    /// percent-escaping.
    pub fn to_base64_der(&self) -> Result<String, Error> {
        use base64::Engine;
        use der::Encode;
        Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            self.to_der_struct()
                .to_der()
                .with_kind(ErrorKind::Serialization)?,
        ))
    }
    /// Parse the compact wire form produced by [`Self::to_base64_der`].
    pub fn from_base64_der(s: &str) -> Result<Self, Error> {
        use base64::Engine;
        use der::Decode;
        AnySignatureDer::from_der(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(s)
                .with_kind(ErrorKind::Deserialization)?,
        )
        .with_kind(ErrorKind::Deserialization)?
        .try_into()
    }
    fn to_der_struct(&self) -> AnySignatureDer {
        match self {
            Self::Ed25519(s) => AnySignatureDer {
                alg: pkcs8::spki::AlgorithmIdentifierOwned {
                    oid: ed25519_dalek::pkcs8::ALGORITHM_ID.oid,
                    parameters: None,
                },
                sig: der::asn1::OctetString::new(s.to_bytes())
                    .expect("64-byte signature encodes as OCTET STRING"),
            },
        }
    }
}

impl FromStr for AnySignature {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use der::DecodePem;
        AnySignatureDer::from_pem(s.as_bytes())
            .with_kind(ErrorKind::Deserialization)?
            .try_into()
    }
}
impl Display for AnySignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use der::EncodePem;
        f.write_str(
            &self
                .to_der_struct()
                .to_pem(der::pem::LineEnding::LF)
                .map_err(|_| std::fmt::Error)?,
        )
    }
}
impl<'de> Deserialize<'de> for AnySignature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_from_str(deserializer)
    }
}
impl Serialize for AnySignature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_display(self, serializer)
    }
}
