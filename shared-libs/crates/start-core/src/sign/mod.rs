use std::fmt::Display;
use std::str::FromStr;

use ::ed25519::pkcs8::BitStringRef;
use clap::builder::ValueParserFactory;
use pkcs8::der::AnyRef;
use pkcs8::{PrivateKeyInfoRef, SubjectPublicKeyInfo};
use serde::{Deserialize, Serialize};

use crate::prelude::*;
use crate::sign::commitment::{Digestable, SignedCommitment};
use crate::sign::ed25519::{ED25519PH_ALGORITHM_ID_DER, Ed25519ph};
use crate::sign::gpg_agent::GpgAgentKey;
use crate::util::FromStrParser;
use crate::util::serde::{Der64, DerEncoding, deserialize_from_str};

pub mod commitment;
pub mod compat;
pub mod ed25519;
pub mod gpg_agent;

pub fn canonicalize_verifying_keys<T>(
    keys: std::collections::BTreeMap<InternedString, T>,
) -> Result<std::collections::BTreeMap<InternedString, T>, Error> {
    keys.into_iter()
        .map(|(key, value)| Ok((key.parse::<AnyVerifyingKey>()?.interned_der64(), value)))
        .collect()
}

/// Protocol tag prefixed to pure-Ed25519 commitment signing messages.
const COMMITMENT_SIG_TAG: &[u8] = b"Start-Commitment-Sig v1\0";

/// The tag, the context prefixed with its length as one byte, the commitment type, then the
/// commitment.
pub(crate) fn signing_message<C: SignedCommitment>(
    tag: &[u8],
    commitment: &C,
    context: &str,
) -> Result<Vec<u8>, Error> {
    let context_len = u8::try_from(context.len()).map_err(|_| {
        Error::new(
            eyre!("{}", t!("sign.context-too-long")),
            ErrorKind::InvalidRequest,
        )
    })?;
    let mut msg = Vec::with_capacity(tag.len() + 2 + context.len() + 64);
    msg.extend_from_slice(tag);
    msg.push(context_len);
    msg.extend_from_slice(context.as_bytes());
    msg.push(C::TYPE_ID);
    append_commitment(&mut msg, commitment);
    Ok(msg)
}

pub(crate) fn append_commitment(msg: &mut Vec<u8>, commitment: &impl Digestable) {
    struct Sink<'a>(&'a mut Vec<u8>);
    impl digest::Update for Sink<'_> {
        fn update(&mut self, data: &[u8]) {
            self.0.extend_from_slice(data);
        }
    }
    commitment.update(&mut Sink(msg));
}

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

#[derive(Clone, Debug, PartialEq, Eq, VisitVariants)]
#[visit(opaque, ts(type = "z.string()"))]
#[non_exhaustive]
pub enum AnySigningKey {
    Ed25519(<Ed25519ph as SignatureScheme>::SigningKey),
}

rpc_toolkit::reflect_ts!(AnySigningKey);
rpc_toolkit::ts_export!(AnySigningKey, namespaces = [""]);
impl AnySigningKey {
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
        match self {
            Self::Ed25519(s) => s.to_pkcs8_der(),
        }
    }
}
impl DerEncoding for AnySigningKey {
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

impl FromStr for AnySigningKey {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<Der64<Self>>().map(|key| key.0)
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
        Der64::new_ref(self).serialize(serializer)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, VisitVariants)]
#[visit(opaque, ts(type = "z.string()"))]
#[non_exhaustive]
pub enum AnyVerifyingKey {
    Ed25519(<Ed25519ph as SignatureScheme>::VerifyingKey),
}

rpc_toolkit::reflect_ts!(AnyVerifyingKey);
rpc_toolkit::ts_export!(AnyVerifyingKey, namespaces = [""]);
impl AnyVerifyingKey {
    pub fn verify_commitment<C: SignedCommitment>(
        &self,
        commitment: &C,
        context: &str,
        signature: &AnySignature,
    ) -> Result<(), Error> {
        match (self, signature) {
            (Self::Ed25519(k), AnySignature::Ed25519ph(s)) => {
                Ed25519ph.verify_commitment(k, commitment, context, s)
            }
            (Self::Ed25519(k), AnySignature::Ed25519(s)) => {
                k.verify_strict(
                    &signing_message(COMMITMENT_SIG_TAG, commitment, context)?,
                    s,
                )?;
                Ok(())
            }
        }
    }
    /// Canonical SPKI map key for persisted signer indexes.
    pub fn interned_der64(&self) -> InternedString {
        InternedString::from_display(Der64::new_ref(self))
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
        match self {
            Self::Ed25519(s) => s.to_public_key_der(),
        }
    }
}
impl DerEncoding for AnyVerifyingKey {
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

impl FromStr for AnyVerifyingKey {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<Der64<Self>>().map(|key| key.0)
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
        Der64::new_ref(self).serialize(serializer)
    }
}
impl ValueParserFactory for AnyVerifyingKey {
    type Parser = FromStrParser<Self>;
    fn value_parser() -> Self::Parser {
        Self::Parser::new()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, VisitVariants)]
#[visit(opaque, ts(type = "z.string()"))]
pub enum AnySignature {
    Ed25519(ed25519_dalek::Signature),
    Ed25519ph(ed25519_dalek::Signature),
}

rpc_toolkit::reflect_ts!(AnySignature);
rpc_toolkit::ts_export!(AnySignature, namespaces = [""]);
#[derive(der::Sequence)]
struct AnySignatureDer {
    alg: der::asn1::Any,
    sig: der::asn1::OctetString,
}
impl der::pem::PemLabel for AnySignatureDer {
    const PEM_LABEL: &'static str = "SIGNATURE";
}
fn ed25519_algorithm_id() -> der::asn1::Any {
    der::asn1::Any::encode_from(&ed25519_dalek::pkcs8::ALGORITHM_ID)
        .expect("id-Ed25519 AlgorithmIdentifier encodes")
}
fn ed25519ph_algorithm_id() -> der::asn1::Any {
    use der::Decode;
    der::asn1::Any::from_der(ED25519PH_ALGORITHM_ID_DER)
        .expect("Ed25519ph AlgorithmIdentifier decodes")
}
impl TryFrom<AnySignatureDer> for AnySignature {
    type Error = Error;
    fn try_from(der: AnySignatureDer) -> Result<Self, Error> {
        let sig = ed25519_dalek::Signature::from_slice(der.sig.as_bytes())
            .with_kind(ErrorKind::Deserialization)?;
        if der.alg == ed25519_algorithm_id() {
            Ok(Self::Ed25519(sig))
        } else if der.alg == ed25519ph_algorithm_id() {
            Ok(Self::Ed25519ph(sig))
        } else {
            Err(Error::new(
                eyre!("{}", t!("sign.unknown-signature-algorithm")),
                ErrorKind::Deserialization,
            ))
        }
    }
}

impl DerEncoding for AnySignature {
    fn from_der<E: serde::de::Error>(der: &[u8]) -> Result<Self, E> {
        use der::Decode;
        AnySignatureDer::from_der(der)
            .map_err(E::custom)?
            .try_into()
            .map_err(E::custom)
    }
    fn to_der<E: serde::ser::Error>(&self) -> Result<Vec<u8>, E> {
        use der::Encode;
        self.to_der_struct().to_der().map_err(E::custom)
    }
}

impl AnySignature {
    fn to_der_struct(&self) -> AnySignatureDer {
        let (alg, sig) = match self {
            Self::Ed25519(s) => (ed25519_algorithm_id(), s),
            Self::Ed25519ph(s) => (ed25519ph_algorithm_id(), s),
        };
        AnySignatureDer {
            alg,
            sig: der::asn1::OctetString::new(sig.to_bytes())
                .expect("64-byte signature encodes as OCTET STRING"),
        }
    }
}

impl FromStr for AnySignature {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<Der64<Self>>().map(|signature| signature.0)
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
        Der64::new_ref(self).serialize(serializer)
    }
}

/// An identity that signs: a key in hand, or one held by gpg-agent.
#[derive(Clone, Debug)]
pub enum Signer {
    Key(AnySigningKey),
    GpgAgent(GpgAgentKey),
}
impl From<AnySigningKey> for Signer {
    fn from(value: AnySigningKey) -> Self {
        Self::Key(value)
    }
}
impl Signer {
    pub async fn verifying_key(&self) -> Result<AnyVerifyingKey, Error> {
        match self {
            Self::Key(k) => Ok(k.verifying_key()),
            Self::GpgAgent(a) => Ok(AnyVerifyingKey::Ed25519(a.verifying_key().await?)),
        }
    }
    /// Ed25519ph for a key in hand; pure Ed25519 for gpg-agent, which cannot prehash.
    pub async fn sign_commitment<C: SignedCommitment>(
        &self,
        commitment: &C,
        context: &str,
    ) -> Result<AnySignature, Error> {
        match self {
            Self::Key(AnySigningKey::Ed25519(k)) => Ok(AnySignature::Ed25519ph(
                Ed25519ph.sign_commitment(k, commitment, context)?,
            )),
            Self::GpgAgent(a) => Ok(AnySignature::Ed25519(
                a.sign(&signing_message(COMMITMENT_SIG_TAG, commitment, context)?)
                    .await?,
            )),
        }
    }
    /// Pure Ed25519.
    pub async fn sign_message(&self, msg: &[u8]) -> Result<AnySignature, Error> {
        use ed25519_dalek::Signer as _;
        match self {
            Self::Key(AnySigningKey::Ed25519(k)) => Ok(AnySignature::Ed25519(k.sign(msg))),
            Self::GpgAgent(a) => Ok(AnySignature::Ed25519(a.sign(msg).await?)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sign::commitment::blake3::Blake3Commitment;

    #[test]
    fn signature_labels_round_trip() {
        let s = ed25519_dalek::Signature::from_bytes(&[4; 64]);
        for sig in [AnySignature::Ed25519(s), AnySignature::Ed25519ph(s)] {
            assert_eq!(sig.to_string().parse::<AnySignature>().unwrap(), sig);
            assert_eq!(
                Der64::<AnySignature>::from_str(&Der64::new_ref(&sig).to_string())
                    .unwrap()
                    .0,
                sig
            );
        }
        assert_ne!(
            AnySignature::Ed25519(s).to_string(),
            AnySignature::Ed25519ph(s).to_string()
        );
    }

    #[tokio::test]
    async fn key_signer_verifies_and_rejects_cross_label() {
        let key = ed25519_dalek::SigningKey::from_bytes(&[6; 32]);
        let signer = Signer::Key(AnySigningKey::Ed25519(key.clone()));
        let commitment = Blake3Commitment {
            hash: crate::util::serde::Base64([2; 32]),
            size: 9,
        };
        let sig = signer
            .sign_commitment(&commitment, "startos")
            .await
            .unwrap();
        let AnySignature::Ed25519ph(raw) = sig else {
            panic!("file keys sign Ed25519ph");
        };
        let vk = signer.verifying_key().await.unwrap();
        vk.verify_commitment(&commitment, "startos", &sig).unwrap();
        assert!(vk.verify_commitment(&commitment, "s9pk", &sig).is_err());
        assert!(
            vk.verify_commitment(&commitment, "startos", &AnySignature::Ed25519(raw))
                .is_err()
        );
    }

    #[test]
    fn pure_commitment_signature_binds_context_and_type() {
        use ed25519_dalek::Signer as _;

        use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;

        let key = ed25519_dalek::SigningKey::from_bytes(&[8; 32]);
        let blake3 = Blake3Commitment {
            hash: crate::util::serde::Base64([3; 32]),
            size: 11,
        };
        let merkle = MerkleArchiveCommitment {
            root_sighash: crate::util::serde::Base64([3; 32]),
            root_maxsize: 11,
        };
        let sig = AnySignature::Ed25519(
            key.sign(&signing_message(COMMITMENT_SIG_TAG, &blake3, "startos").unwrap()),
        );
        let vk = AnyVerifyingKey::Ed25519(key.verifying_key());
        vk.verify_commitment(&blake3, "startos", &sig).unwrap();
        assert!(vk.verify_commitment(&blake3, "s9pk", &sig).is_err());
        assert!(vk.verify_commitment(&merkle, "startos", &sig).is_err());
    }
}
