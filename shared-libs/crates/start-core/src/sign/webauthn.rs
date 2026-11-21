use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::prelude::*;
use crate::sign::AnyVerifyingKey;

/// DER `AlgorithmIdentifier` for OID 2.25.202066033202335500459814983859987193745, absent
/// parameters. Hand-encoded: `const-oid` rejects arcs wider than 32 bits.
pub(super) const WEBAUTHN_ALGORITHM_ID_DER: &[u8] = &[
    0x30, 0x16, 0x06, 0x14, 0x69, 0x82, 0xb0, 0x84, 0xc0, 0xa7, 0xbb, 0xcd, 0xa2, 0xba, 0xff, 0xa9,
    0xec, 0xd8, 0xef, 0xfa, 0x80, 0x8a, 0xbf, 0x11,
];

const FLAG_USER_PRESENT: u8 = 0x01;
const FLAG_USER_VERIFIED: u8 = 0x04;
const REQUIRED_FLAGS: u8 = FLAG_USER_PRESENT | FLAG_USER_VERIFIED;
const FLAGS_OFFSET: usize = 32;
const MIN_AUTHENTICATOR_DATA_LEN: usize = 37;

/// A WebAuthn assertion whose challenge is the signed message.
///
/// The signature is ASN.1 DER ECDSA for a P-256 key and raw EdDSA for an Ed25519 key. The
/// relying party and origin are not bound to the key.
#[derive(Clone, Debug, PartialEq, Eq, der::Sequence)]
pub struct WebAuthnAssertion {
    pub authenticator_data: der::asn1::OctetString,
    pub client_data_json: der::asn1::OctetString,
    pub signature: der::asn1::OctetString,
}

#[derive(Deserialize)]
struct ClientData {
    #[serde(rename = "type")]
    ty: String,
    challenge: String,
}

fn invalid_assertion() -> Error {
    Error::new(
        eyre!("{}", t!("sign.webauthn.invalid-assertion")),
        ErrorKind::InvalidSignature,
    )
}

impl WebAuthnAssertion {
    pub fn new(
        authenticator_data: Vec<u8>,
        client_data_json: Vec<u8>,
        signature: Vec<u8>,
    ) -> Result<Self, Error> {
        let octets =
            |bytes| der::asn1::OctetString::new(bytes).with_kind(ErrorKind::Deserialization);
        Ok(Self {
            authenticator_data: octets(authenticator_data)?,
            client_data_json: octets(client_data_json)?,
            signature: octets(signature)?,
        })
    }

    /// Requires a `webauthn.get` ceremony with user presence and verification.
    pub fn verify(&self, key: &AnyVerifyingKey, message: &[u8]) -> Result<(), Error> {
        let authenticator_data = self.authenticator_data.as_bytes();
        let client_data_json = self.client_data_json.as_bytes();
        let client_data: ClientData =
            serde_json::from_slice(client_data_json).map_err(|_| invalid_assertion())?;
        if client_data.ty != "webauthn.get"
            || URL_SAFE_NO_PAD
                .decode(client_data.challenge)
                .map_err(|_| invalid_assertion())?
                != message
            || authenticator_data.len() < MIN_AUTHENTICATOR_DATA_LEN
            || authenticator_data[FLAGS_OFFSET] & REQUIRED_FLAGS != REQUIRED_FLAGS
        {
            return Err(invalid_assertion());
        }
        let mut signed = authenticator_data.to_vec();
        signed.extend_from_slice(&Sha256::digest(client_data_json));
        let signature = self.signature.as_bytes();
        match key {
            AnyVerifyingKey::Ed25519(key) => {
                key.verify_strict(&signed, &ed25519_dalek::Signature::from_slice(signature)?)?
            }
            AnyVerifyingKey::P256(key) => {
                use p256::ecdsa::signature::Verifier as _;
                key.verify(
                    &signed,
                    &p256::ecdsa::Signature::from_der(signature)
                        .map_err(|_| invalid_assertion())?,
                )
                .map_err(|_| invalid_assertion())?
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod test_util {
    use p256::ecdsa::signature::Signer as _;

    use super::*;

    pub const RP_ID_HASH: [u8; 32] = [7; 32];

    pub fn authenticator_data(flags: u8) -> Vec<u8> {
        let mut data = RP_ID_HASH.to_vec();
        data.push(flags);
        data.extend_from_slice(&1u32.to_be_bytes());
        data
    }

    pub fn client_data(ty: &str, challenge: &[u8]) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "type": ty,
            "challenge": URL_SAFE_NO_PAD.encode(challenge),
            "origin": "https://start9.local",
            "crossOrigin": false,
        }))
        .unwrap()
    }

    pub fn signed_bytes(authenticator_data: &[u8], client_data_json: &[u8]) -> Vec<u8> {
        let mut signed = authenticator_data.to_vec();
        signed.extend_from_slice(&Sha256::digest(client_data_json));
        signed
    }

    pub fn p256_assertion(
        key: &p256::ecdsa::SigningKey,
        authenticator_data: Vec<u8>,
        client_data_json: Vec<u8>,
    ) -> WebAuthnAssertion {
        let signature: p256::ecdsa::Signature =
            key.sign(&signed_bytes(&authenticator_data, &client_data_json));
        WebAuthnAssertion::new(
            authenticator_data,
            client_data_json,
            signature.to_der().as_bytes().to_vec(),
        )
        .unwrap()
    }

    /// A user-present P-256 passkey assertion over the challenge.
    pub fn passkey_assertion(key: &p256::ecdsa::SigningKey, challenge: &[u8]) -> WebAuthnAssertion {
        p256_assertion(
            key,
            authenticator_data(REQUIRED_FLAGS),
            client_data("webauthn.get", challenge),
        )
    }

    pub fn p256_key() -> p256::ecdsa::SigningKey {
        p256::ecdsa::SigningKey::from_slice(&[3; 32]).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use p256::ecdsa::signature::Signer as _;

    use super::test_util::*;
    use super::*;

    #[test]
    fn p256_assertion_verifies_its_challenge() {
        let key = p256_key();
        let public = AnyVerifyingKey::P256(*key.verifying_key());
        let assertion = passkey_assertion(&key, b"message");
        assertion.verify(&public, b"message").unwrap();
        assert!(assertion.verify(&public, b"other").is_err());
        let other = p256::ecdsa::SigningKey::from_slice(&[4; 32]).unwrap();
        assert!(
            assertion
                .verify(&AnyVerifyingKey::P256(*other.verifying_key()), b"message")
                .is_err()
        );
    }

    #[test]
    fn accepts_high_s_ecdsa_signatures() {
        let key = p256_key();
        let authenticator_data = authenticator_data(REQUIRED_FLAGS);
        let client_data_json = client_data("webauthn.get", b"message");
        let signature: p256::ecdsa::Signature =
            key.sign(&signed_bytes(&authenticator_data, &client_data_json));
        let (r, s) = signature.split_scalars();
        let high = p256::ecdsa::Signature::from_scalars(r, -*s).unwrap();
        assert_ne!(high, signature);
        WebAuthnAssertion::new(
            authenticator_data,
            client_data_json,
            high.to_der().as_bytes().to_vec(),
        )
        .unwrap()
        .verify(&AnyVerifyingKey::P256(*key.verifying_key()), b"message")
        .unwrap();
    }

    #[test]
    fn ed25519_assertion_verifies_its_challenge() {
        use ed25519_dalek::Signer as _;
        let key = ed25519_dalek::SigningKey::from_bytes(&[5; 32]);
        let authenticator_data = authenticator_data(REQUIRED_FLAGS);
        let client_data_json = client_data("webauthn.get", b"message");
        let signature = key.sign(&signed_bytes(&authenticator_data, &client_data_json));
        let assertion = WebAuthnAssertion::new(
            authenticator_data,
            client_data_json,
            signature.to_bytes().to_vec(),
        )
        .unwrap();
        let public = AnyVerifyingKey::Ed25519(key.verifying_key());
        assertion.verify(&public, b"message").unwrap();
        assert!(assertion.verify(&public, b"other").is_err());
    }

    #[test]
    fn rejects_registration_ceremonies_and_unverified_users() {
        let key = p256_key();
        let public = AnyVerifyingKey::P256(*key.verifying_key());
        for assertion in [
            p256_assertion(
                &key,
                authenticator_data(REQUIRED_FLAGS),
                client_data("webauthn.create", b"message"),
            ),
            p256_assertion(
                &key,
                authenticator_data(FLAG_USER_VERIFIED),
                client_data("webauthn.get", b"message"),
            ),
            p256_assertion(
                &key,
                authenticator_data(FLAG_USER_PRESENT),
                client_data("webauthn.get", b"message"),
            ),
            p256_assertion(
                &key,
                RP_ID_HASH.to_vec(),
                client_data("webauthn.get", b"message"),
            ),
            p256_assertion(&key, authenticator_data(REQUIRED_FLAGS), b"{}".to_vec()),
        ] {
            assert_eq!(
                assertion.verify(&public, b"message").unwrap_err().kind,
                ErrorKind::InvalidSignature
            );
        }
    }

    #[test]
    fn signature_covers_authenticator_data() {
        let key = p256_key();
        let mut assertion = passkey_assertion(&key, b"message");
        assertion.authenticator_data =
            der::asn1::OctetString::new(authenticator_data(REQUIRED_FLAGS | 0x08)).unwrap();
        assert!(
            assertion
                .verify(&AnyVerifyingKey::P256(*key.verifying_key()), b"message")
                .is_err()
        );
    }
}
