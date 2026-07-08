use ed25519_dalek::{EXPANDED_SECRET_KEY_LENGTH, SecretKey};

#[inline]
pub fn ed25519_expand_key(key: &SecretKey) -> [u8; EXPANDED_SECRET_KEY_LENGTH] {
    ed25519_dalek_v1::ExpandedSecretKey::from(
        &ed25519_dalek_v1::SecretKey::from_bytes(key).unwrap(),
    )
    .to_bytes()
}

use aes::cipher::{KeyIvInit, StreamCipher};
use hmac::Hmac;
use josekit::jwk::Jwk;
use rand::rand_core::UnwrapErr;
use rand::rngs::ThreadRng;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tracing::instrument;
use ts_rs::TS;

use crate::prelude::*;

/// Infallible CSPRNG for ed25519-dalek / ssh-key key generation, which require a
/// `rand_core` 0.10 `CryptoRng` (i.e. infallible). `rand::rng()` alone does not
/// qualify — in rand 0.10 `ThreadRng` is only the fallible `TryCryptoRng` — so wrap
/// it in `UnwrapErr`, which panics only on the effectively-never RNG failure.
pub fn os_rng() -> UnwrapErr<ThreadRng> {
    UnwrapErr(rand::rng())
}

// aes 0.7's `Aes256Ctr` was a 64-bit big-endian counter; keep that exact mode so
// password-wrapped backup keys written by older versions still decrypt.
type Aes256Ctr = ctr::Ctr64BE<aes::Aes256>;

pub fn pbkdf2(password: impl AsRef<[u8]>, salt: impl AsRef<[u8]>) -> aes::cipher::Key<Aes256Ctr> {
    let mut aeskey = aes::cipher::Key::<Aes256Ctr>::default();
    pbkdf2::pbkdf2::<Hmac<Sha256>>(password.as_ref(), salt.as_ref(), 1000, &mut aeskey).unwrap();
    aeskey
}

pub fn encrypt_slice(input: impl AsRef<[u8]>, password: impl AsRef<[u8]>) -> Vec<u8> {
    let prefix: [u8; 32] = rand::random();
    let aeskey = pbkdf2(password.as_ref(), &prefix[16..]);
    let mut aes = Aes256Ctr::new_from_slices(&aeskey, &prefix[..16]).unwrap();
    let mut res = Vec::with_capacity(32 + input.as_ref().len());
    res.extend_from_slice(&prefix[..]);
    res.extend_from_slice(input.as_ref());
    aes.apply_keystream(&mut res[32..]);
    res
}

pub fn decrypt_slice(input: impl AsRef<[u8]>, password: impl AsRef<[u8]>) -> Vec<u8> {
    if input.as_ref().len() < 32 {
        return Vec::new();
    }
    let (prefix, rest) = input.as_ref().split_at(32);
    let aeskey = pbkdf2(password.as_ref(), &prefix[16..]);
    let mut aes = Aes256Ctr::new_from_slices(&aeskey, &prefix[..16]).unwrap();
    let mut res = rest.to_vec();
    aes.apply_keystream(&mut res);
    res
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct EncryptedWire {
    #[ts(type = "any")]
    encrypted: Value,
}
impl EncryptedWire {
    /// Encrypt `plaintext` against the recipient's public ECDH key,
    /// producing the same JWE flattened-JSON payload shape that the
    /// setup-mode web UI sends. Decrypts cleanly via
    /// [`EncryptedWire::decrypt`] on the matching private JWK.
    pub fn encrypt(plaintext: &str, public_key: &Jwk) -> Result<Self, Error> {
        use josekit::jwe::alg::ecdh_es::EcdhEsJweAlgorithm;
        use josekit::jwe::{self, JweHeaderSet};

        let encrypter = EcdhEsJweAlgorithm::EcdhEs
            .encrypter_from_jwk(public_key)
            .map_err(|e| Error::new(eyre!("{e}"), crate::ErrorKind::OpenSsl))?;
        let mut header = JweHeaderSet::new();
        header.set_algorithm("ECDH-ES", true);
        header.set_content_encryption("A128CBC-HS256", true);
        let jwe_json = jwe::serialize_flattened_json(
            plaintext.as_bytes(),
            Some(&header),
            None,
            None,
            &*encrypter,
        )
        .map_err(|e| Error::new(eyre!("{e}"), crate::ErrorKind::OpenSsl))?;
        let encrypted: Value =
            serde_json::from_str(&jwe_json).with_kind(crate::ErrorKind::Deserialization)?;
        Ok(EncryptedWire { encrypted })
    }

    #[instrument(skip_all)]
    pub fn decrypt(self, current_secret: impl AsRef<Jwk>) -> Option<String> {
        let current_secret = current_secret.as_ref();

        let decrypter = match josekit::jwe::alg::ecdh_es::EcdhEsJweAlgorithm::EcdhEs
            .decrypter_from_jwk(current_secret)
        {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("Could not setup awk");
                tracing::debug!("{:?}", e);
                return None;
            }
        };
        let encrypted = match serde_json::to_string(&self.encrypted) {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("Could not deserialize");
                tracing::debug!("{:?}", e);

                return None;
            }
        };
        let (decoded, _) = match josekit::jwe::deserialize_json(&encrypted, &decrypter) {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("Could not decrypt");
                tracing::debug!("{:?}", e);
                return None;
            }
        };
        match String::from_utf8(decoded) {
            Ok(a) => Some(a),
            Err(e) => {
                tracing::warn!("Could not decrypt into utf8");
                tracing::debug!("{:?}", e);
                return None;
            }
        }
    }
}

/// `EncryptedWire::encrypt` produces a wire format that the matching
/// [`EncryptedWire::decrypt`] (and therefore the setup-mode server)
/// accepts.
#[test]
fn test_encrypt_decrypt_roundtrip() {
    let private_key: Jwk = serde_json::from_str(
        r#"{
            "kty": "EC",
            "crv": "P-256",
            "d": "3P-MxbUJtEhdGGpBCRFXkUneGgdyz_DGZWfIAGSCHOU",
            "x": "yHTDYSfjU809fkSv9MmN4wuojf5c3cnD7ZDN13n-jz4",
            "y": "8Mpkn744A5KDag0DmX2YivB63srjbugYZzWc3JOpQXI"
          }"#,
    )
    .unwrap();
    let public_key = private_key.to_public_key().unwrap();

    for plaintext in ["", "hunter2", "a longer password with spaces and \u{1f600}"] {
        let wire = EncryptedWire::encrypt(plaintext, &public_key).unwrap();
        let recovered = wire
            .decrypt(std::sync::Arc::new(private_key.clone()))
            .unwrap();
        assert_eq!(plaintext, &recovered);
    }
}

/// We created this test by first making the private key, then restoring from this private key for recreatability.
/// After this the frontend then encoded an password, then we are testing that the output that we got (hand coded)
/// will be the shape we want.
#[test]
fn test_gen_awk() {
    let private_key: Jwk = serde_json::from_str(
        r#"{
            "kty": "EC",
            "crv": "P-256",
            "d": "3P-MxbUJtEhdGGpBCRFXkUneGgdyz_DGZWfIAGSCHOU",
            "x": "yHTDYSfjU809fkSv9MmN4wuojf5c3cnD7ZDN13n-jz4",
            "y": "8Mpkn744A5KDag0DmX2YivB63srjbugYZzWc3JOpQXI"
          }"#,
    )
    .unwrap();
    let encrypted: EncryptedWire =  serde_json::from_str(r#"{
        "encrypted":     {        "protected": "eyJlbmMiOiJBMTI4Q0JDLUhTMjU2IiwiYWxnIjoiRUNESC1FUyIsImtpZCI6ImgtZnNXUVh2Tm95dmJEazM5dUNsQ0NUdWc5N3MyZnJockJnWUVBUWVtclUiLCJlcGsiOnsia3R5IjoiRUMiLCJjcnYiOiJQLTI1NiIsIngiOiJmRkF0LXNWYWU2aGNkdWZJeUlmVVdUd3ZvWExaTkdKRHZIWVhIckxwOXNNIiwieSI6IjFvVFN6b00teHlFZC1SLUlBaUFHdXgzS1dJZmNYZHRMQ0JHLUh6MVkzY2sifX0",        "iv": "NbwvfvWOdLpZfYRIZUrkcw",        "ciphertext": "Zc5Br5kYOlhPkIjQKOLMJw",        "tag": "EPoch52lDuCsbUUulzZGfg"    }
      }"#).unwrap();
    assert_eq!(
        "testing12345",
        &encrypted.decrypt(std::sync::Arc::new(private_key)).unwrap()
    );
}
