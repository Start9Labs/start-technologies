use prost::Message;
use snow::HandshakeState;
use snow::params::NoiseParams;
use zeroize::Zeroizing;

use super::{AnySignature, AnyVerifyingKey, Signer};
use crate::prelude::*;

const PROTOCOL: &str = "Noise_XX_25519_ChaChaPoly_SHA256";
const STATIC_KEY_SIGNATURE_PREFIX: &[u8] = b"noise-libp2p-static-key:";
const ED25519_KEY_TYPE: i32 = 1;
const FINGERPRINT_PREFIX: &str = "SHA256:";
const FINGERPRINT_ALGORITHM: &str = "SHA-256";

/// SHA-256 fingerprint of the raw Ed25519 identity public key.
pub fn fingerprint(key: &AnyVerifyingKey) -> String {
    use sha2::Digest;
    let AnyVerifyingKey::Ed25519(key) = key;
    format!(
        "{FINGERPRINT_PREFIX}{}",
        hex::encode(sha2::Sha256::digest(key.as_bytes()))
    )
}

#[derive(Clone, PartialEq, Message)]
struct PublicKey {
    #[prost(int32, tag = "1")]
    kind: i32,
    #[prost(bytes = "vec", tag = "2")]
    data: Vec<u8>,
}

#[derive(Clone, PartialEq, Message)]
struct Certificate {
    #[prost(bytes = "vec", tag = "1")]
    key: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    signature: Vec<u8>,
}

pub(crate) fn wire_profile() -> serde_json::Value {
    serde_json::json!({
        "protocol": PROTOCOL,
        "signaturePrefix": STATIC_KEY_SIGNATURE_PREFIX,
        "fingerprint": { "algorithm": FINGERPRINT_ALGORITHM, "prefix": FINGERPRINT_PREFIX },
    })
}

fn protocol() -> NoiseParams {
    PROTOCOL
        .parse()
        .expect("valid Noise XX protocol parameters")
}

fn signature_message(public_key: &[u8]) -> Vec<u8> {
    [STATIC_KEY_SIGNATURE_PREFIX, public_key].concat()
}

fn invalid_certificate() -> Error {
    Error::new(
        eyre!("{}", t!("middleware.auth.invalid-request-signature")),
        ErrorKind::InvalidSignature,
    )
}

/// An in-memory Noise static key certified by an existing Ed25519 identity.
/// Identity signing works without exporting its private key, including gpg-agent keys.
pub struct NoiseIdentity {
    secret: Zeroizing<Vec<u8>>,
    certificate: Vec<u8>,
}

impl NoiseIdentity {
    /// Certifies a fresh DH key once; subsequent transport messages need no identity signatures.
    pub async fn new(signer: &Signer) -> Result<Self, Error> {
        let pair = snow::Builder::new(protocol())
            .generate_keypair()
            .with_kind(ErrorKind::Authorization)?;
        let secret = Zeroizing::new(pair.private);
        let AnyVerifyingKey::Ed25519(key) = signer.verifying_key().await?;
        let AnySignature::Ed25519(signature) = signer
            .sign_message(&signature_message(&pair.public))
            .await?
        else {
            return Err(invalid_certificate());
        };
        let certificate = Certificate {
            key: PublicKey {
                kind: ED25519_KEY_TYPE,
                data: key.as_bytes().to_vec(),
            }
            .encode_to_vec(),
            signature: signature.to_bytes().to_vec(),
        }
        .encode_to_vec();
        Ok(Self {
            secret,
            certificate,
        })
    }

    /// Both peers must supply identical protocol context bytes.
    pub fn initiator(&self, prologue: &[u8]) -> Result<HandshakeState, Error> {
        self.builder(prologue)?
            .build_initiator()
            .with_kind(ErrorKind::Authorization)
    }

    /// Both peers must supply identical protocol context bytes.
    pub fn responder(&self, prologue: &[u8]) -> Result<HandshakeState, Error> {
        self.builder(prologue)?
            .build_responder()
            .with_kind(ErrorKind::Authorization)
    }

    fn builder<'a>(&'a self, prologue: &'a [u8]) -> Result<snow::Builder<'a>, Error> {
        snow::Builder::new(protocol())
            .local_private_key(&self.secret)
            .and_then(|builder| builder.prologue(prologue))
            .with_kind(ErrorKind::Authorization)
    }

    /// The standard libp2p Noise certificate, carried in encrypted XX messages two and three.
    pub fn certificate(&self) -> &[u8] {
        &self.certificate
    }
}

/// Verifies an identity certificate against the handshake's remote static key.
/// Connection authorization still requires a completed handshake and live enrollment checks.
pub fn authenticate_peer(
    handshake: &HandshakeState,
    certificate: &[u8],
) -> Result<AnyVerifyingKey, Error> {
    let certificate = Certificate::decode(certificate).map_err(|_| invalid_certificate())?;
    let public =
        PublicKey::decode(certificate.key.as_slice()).map_err(|_| invalid_certificate())?;
    if public.kind != ED25519_KEY_TYPE {
        return Err(invalid_certificate());
    }
    let public: [u8; 32] = public.data.try_into().map_err(|_| invalid_certificate())?;
    let key =
        ed25519_dalek::VerifyingKey::from_bytes(&public).map_err(|_| invalid_certificate())?;
    let signature = ed25519_dalek::Signature::from_slice(&certificate.signature)
        .map_err(|_| invalid_certificate())?;
    let remote_static = handshake
        .get_remote_static()
        .ok_or_else(invalid_certificate)?;
    key.verify_strict(&signature_message(remote_static), &signature)
        .map_err(|_| invalid_certificate())?;
    Ok(AnyVerifyingKey::Ed25519(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sign::AnySigningKey;

    fn signer(seed: u8) -> Signer {
        AnySigningKey::Ed25519(ed25519_dalek::SigningKey::from_bytes(&[seed; 32])).into()
    }

    fn exchange(state: &mut HandshakeState, payload: &[u8], peer: &mut HandshakeState) -> Vec<u8> {
        let mut wire = vec![0; 65535];
        let size = state.write_message(payload, &mut wire).unwrap();
        let mut decoded = vec![0; 65535];
        let size = peer.read_message(&wire[..size], &mut decoded).unwrap();
        decoded.truncate(size);
        decoded
    }

    #[test]
    fn identity_fingerprint_matches_the_published_ed25519_fixture() {
        let key = ed25519_dalek::SigningKey::from_bytes(&[21; 32]).verifying_key();
        assert_eq!(
            fingerprint(&AnyVerifyingKey::Ed25519(key)),
            "SHA256:d7e1e084be213b01e506852af8198b9980b43e03749c67d82dcd0903f4001c77"
        );
    }

    #[tokio::test]
    async fn identities_authenticate_both_directions_and_transport_rejects_replay() {
        let a = signer(11);
        let b = signer(12);
        let a_identity = NoiseIdentity::new(&a).await.unwrap();
        let b_identity = NoiseIdentity::new(&b).await.unwrap();
        let mut initiator = a_identity.initiator(b"start-rpc-test-v1").unwrap();
        let mut responder = b_identity.responder(b"start-rpc-test-v1").unwrap();
        assert!(exchange(&mut initiator, &[], &mut responder).is_empty());
        let certificate = exchange(&mut responder, b_identity.certificate(), &mut initiator);
        assert_eq!(
            authenticate_peer(&initiator, &certificate).unwrap(),
            b.verifying_key().await.unwrap()
        );
        let certificate = exchange(&mut initiator, a_identity.certificate(), &mut responder);
        assert_eq!(
            authenticate_peer(&responder, &certificate).unwrap(),
            a.verifying_key().await.unwrap()
        );
        assert!(initiator.is_handshake_finished() && responder.is_handshake_finished());
        let mut sender = initiator.into_transport_mode().unwrap();
        let mut receiver = responder.into_transport_mode().unwrap();
        let mut ciphertext = [0; 128];
        let size = sender
            .write_message(b"encrypted rpc", &mut ciphertext)
            .unwrap();
        let mut plaintext = [0; 128];
        let length = receiver
            .read_message(&ciphertext[..size], &mut plaintext)
            .unwrap();
        assert_eq!(&plaintext[..length], b"encrypted rpc");
        assert!(
            receiver
                .read_message(&ciphertext[..size], &mut plaintext)
                .is_err()
        );
    }

    #[tokio::test]
    async fn certificates_cannot_be_replayed_into_a_different_key_exchange() {
        let identity_signer = signer(13);
        let old = NoiseIdentity::new(&identity_signer).await.unwrap();
        let current = NoiseIdentity::new(&identity_signer).await.unwrap();
        let peer = NoiseIdentity::new(&signer(14)).await.unwrap();
        let mut initiator = current.initiator(b"same context").unwrap();
        let mut responder = peer.responder(b"same context").unwrap();
        exchange(&mut initiator, &[], &mut responder);
        exchange(&mut responder, peer.certificate(), &mut initiator);
        let certificate = exchange(&mut initiator, old.certificate(), &mut responder);
        assert!(authenticate_peer(&responder, &certificate).is_err());
    }

    #[tokio::test]
    async fn unrelated_identity_cannot_claim_a_valid_certificate() {
        let local = NoiseIdentity::new(&signer(15)).await.unwrap();
        let peer = NoiseIdentity::new(&signer(16)).await.unwrap();
        let mut initiator = local.initiator(b"context").unwrap();
        let mut responder = peer.responder(b"context").unwrap();
        exchange(&mut initiator, &[], &mut responder);
        let mut certificate = Certificate::decode(peer.certificate()).unwrap();
        let AnyVerifyingKey::Ed25519(unrelated) = signer(21).verifying_key().await.unwrap();
        certificate.key = PublicKey {
            kind: ED25519_KEY_TYPE,
            data: unrelated.as_bytes().to_vec(),
        }
        .encode_to_vec();
        let received = exchange(&mut responder, &certificate.encode_to_vec(), &mut initiator);
        assert!(authenticate_peer(&initiator, &received).is_err());
    }

    #[tokio::test]
    async fn protocol_context_is_authenticated() {
        let a = NoiseIdentity::new(&signer(17)).await.unwrap();
        let b = NoiseIdentity::new(&signer(18)).await.unwrap();
        let mut initiator = a.initiator(b"context a").unwrap();
        let mut responder = b.responder(b"context b").unwrap();
        exchange(&mut initiator, &[], &mut responder);
        let mut wire = vec![0; 65535];
        let size = responder.write_message(b.certificate(), &mut wire).unwrap();
        assert!(
            initiator
                .read_message(&wire[..size], &mut vec![0; 65535])
                .is_err()
        );
    }

    #[tokio::test]
    async fn javascript_certificate_authenticates_without_reencoding() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../testdata/noise-identity.json")).unwrap();
        let private = hex::decode(fixture["noisePrivateKey"].as_str().unwrap()).unwrap();
        let certificate = hex::decode(fixture["certificate"].as_str().unwrap()).unwrap();
        let public = hex::decode(fixture["identityPublicKey"].as_str().unwrap()).unwrap();
        let identity = NoiseIdentity::new(&signer(22)).await.unwrap();
        let mut initiator = identity.initiator(b"fixture context").unwrap();
        let mut responder = snow::Builder::new(protocol())
            .local_private_key(&private)
            .unwrap()
            .prologue(b"fixture context")
            .unwrap()
            .build_responder()
            .unwrap();
        exchange(&mut initiator, &[], &mut responder);
        let received = exchange(&mut responder, &certificate, &mut initiator);
        let AnyVerifyingKey::Ed25519(peer) = authenticate_peer(&initiator, &received).unwrap();
        assert_eq!(peer.as_bytes().as_slice(), public);
    }

    #[tokio::test]
    async fn malformed_certificates_are_rejected() {
        let a = NoiseIdentity::new(&signer(19)).await.unwrap();
        let b = NoiseIdentity::new(&signer(20)).await.unwrap();
        let mut initiator = a.initiator(b"context").unwrap();
        let mut responder = b.responder(b"context").unwrap();
        exchange(&mut initiator, &[], &mut responder);
        exchange(&mut responder, b.certificate(), &mut initiator);
        let valid = Certificate::decode(b.certificate()).unwrap();
        let mut malformed = vec![vec![], vec![255]];
        for public in [
            PublicKey {
                kind: 99,
                data: vec![0; 32],
            },
            PublicKey {
                kind: ED25519_KEY_TYPE,
                data: vec![0; 31],
            },
        ] {
            malformed.push(
                Certificate {
                    key: public.encode_to_vec(),
                    ..valid.clone()
                }
                .encode_to_vec(),
            );
        }
        for signature in [vec![0; 63], vec![0; 64]] {
            malformed.push(
                Certificate {
                    signature,
                    ..valid.clone()
                }
                .encode_to_vec(),
            );
        }
        for certificate in malformed {
            assert!(authenticate_peer(&initiator, &certificate).is_err());
        }
    }
}
