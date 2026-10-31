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
use zeroize::Zeroizing;

use crate::prelude::*;

/// Supplies infallible randomness required by ed25519-dalek and ssh-key.
pub fn os_rng() -> UnwrapErr<ThreadRng> {
    UnwrapErr(rand::rng())
}

// Legacy backups use a 64-bit big-endian AES-CTR counter.
type Aes256Ctr = ctr::Ctr64BE<aes::Aes256>;

const BACKUP_KEY_LEN: usize = 52;
const WRAP_VERSION: &str = "v1:";
const WRAP_SALT_LEN: usize = 16;
const WRAP_NONCE_LEN: usize = 12;
const WRAP_TAG_LEN: usize = 16;
const WRAP_HEADER_LEN: usize = WRAP_SALT_LEN + WRAP_NONCE_LEN;
const WRAP_LEN: usize = WRAP_HEADER_LEN + BACKUP_KEY_LEN + WRAP_TAG_LEN;
const LEGACY_WRAP_LEN: usize = 32 + BACKUP_KEY_LEN;

fn invalid_backup_key() -> Error {
    Error::new(
        eyre!(t!("util.crypto.invalid-backup-key")),
        crate::ErrorKind::Backup,
    )
}

fn validate_backup_key(key: &[u8]) -> Result<&str, Error> {
    let key = std::str::from_utf8(key).map_err(|_| invalid_backup_key())?;
    if key.len() != BACKUP_KEY_LEN
        || base32::decode(base32::Alphabet::Rfc4648 { padding: false }, key)
            .is_none_or(|bytes| bytes.len() != 32)
    {
        return Err(invalid_backup_key());
    }
    Ok(key)
}

fn backup_wrapping_key(password: &str, salt: &[u8]) -> Result<Zeroizing<Vec<u8>>, Error> {
    argon2::hash_raw(
        password.as_bytes(),
        salt,
        &argon2::Config::rfc9106_low_mem(),
    )
    .map(Zeroizing::new)
    .with_kind(crate::ErrorKind::PasswordHashGeneration)
}

fn backup_wrapping_aad(header: &[u8]) -> Vec<u8> {
    [WRAP_VERSION.as_bytes(), header].concat()
}

/// Wraps a base32 backup key with Argon2id and AES-256-GCM.
pub fn wrap_backup_key(key: &str, password: &str) -> Result<String, Error> {
    validate_backup_key(key.as_bytes())?;
    let header: [u8; WRAP_HEADER_LEN] = rand::random();
    let aeskey = backup_wrapping_key(password, &header[..WRAP_SALT_LEN])?;
    let mut tag = [0; WRAP_TAG_LEN];
    let ciphertext = openssl::symm::encrypt_aead(
        openssl::symm::Cipher::aes_256_gcm(),
        &aeskey,
        Some(&header[WRAP_SALT_LEN..]),
        &backup_wrapping_aad(&header),
        key.as_bytes(),
        &mut tag,
    )
    .with_kind(crate::ErrorKind::OpenSsl)?;
    let mut wrapped = header.to_vec();
    wrapped.extend_from_slice(&ciphertext);
    wrapped.extend_from_slice(&tag);
    Ok(format!(
        "{WRAP_VERSION}{}",
        base32::encode(base32::Alphabet::Rfc4648 { padding: true }, &wrapped)
    ))
}

/// Returns the backup key and whether its wrapping needs upgrading.
pub fn unwrap_backup_key(wrapped: &str, password: &str) -> Result<(String, bool), Error> {
    let (encoded, legacy) = match wrapped.strip_prefix(WRAP_VERSION) {
        Some(encoded) => (encoded, false),
        None if !wrapped.contains(':') => (wrapped, true),
        None => return Err(invalid_backup_key()),
    };
    let expected_len = if legacy { LEGACY_WRAP_LEN } else { WRAP_LEN };
    if encoded.len() > expected_len.div_ceil(5) * 8 {
        return Err(invalid_backup_key());
    }
    let wrapped = base32::decode(base32::Alphabet::Rfc4648 { padding: true }, encoded)
        .filter(|bytes| bytes.len() == expected_len)
        .ok_or_else(invalid_backup_key)?;
    let plaintext = Zeroizing::new(if legacy {
        let (prefix, ciphertext) = wrapped.split_at(32);
        let mut aeskey = aes::cipher::Key::<Aes256Ctr>::default();
        pbkdf2::pbkdf2::<Hmac<Sha256>>(password.as_bytes(), &prefix[16..], 1000, &mut aeskey)
            .map_err(|_| invalid_backup_key())?;
        let mut aes =
            Aes256Ctr::new_from_slices(&aeskey, &prefix[..16]).map_err(|_| invalid_backup_key())?;
        let mut plaintext = ciphertext.to_vec();
        aes.apply_keystream(&mut plaintext);
        plaintext
    } else {
        let (header, body) = wrapped.split_at(WRAP_HEADER_LEN);
        let (ciphertext, tag) = body.split_at(BACKUP_KEY_LEN);
        let aeskey = backup_wrapping_key(password, &header[..WRAP_SALT_LEN])?;
        openssl::symm::decrypt_aead(
            openssl::symm::Cipher::aes_256_gcm(),
            &aeskey,
            Some(&header[WRAP_SALT_LEN..]),
            &backup_wrapping_aad(header),
            ciphertext,
            tag,
        )
        .map_err(|_| invalid_backup_key())?
    });
    Ok((validate_backup_key(&plaintext)?.to_owned(), legacy))
}

#[derive(Debug, Clone, Deserialize, Serialize, VisitFields)]
pub struct EncryptedWire {
    encrypted: Value,
}

rpc_toolkit::reflect_ts!(EncryptedWire);
rpc_toolkit::ts_export!(EncryptedWire, namespaces = [""]);
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

#[cfg(test)]
pub(crate) mod backup_key_tests {
    use super::*;

    pub(crate) const KEY: &str = "AAAQEAYEAUDAOCAJBIFQYDIOB4IBCEQTCQKRMFYYDENBWHA5DYPQ";
    pub(crate) const LEGACY_PASSWORD: &str = "legacy-password";
    const LEGACY_HEX: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1ff5b5a13fcb7e7d829eb6aff96d2d9c3ac9c7497d9da3901fce97fcc26667c8e650d6d8310903d91d51d87c840b3f5f538bc401fa";

    pub(crate) fn legacy_fixture(padding: bool) -> String {
        base32::encode(
            base32::Alphabet::Rfc4648 { padding },
            &hex::decode(LEGACY_HEX).unwrap(),
        )
    }

    #[test]
    fn authenticated_backup_key_roundtrip() {
        let wrapped = wrap_backup_key(KEY, "backup-password").unwrap();
        assert!(wrapped.starts_with(WRAP_VERSION));
        let (key, legacy) = unwrap_backup_key(&wrapped, "backup-password").unwrap();
        assert!(key == KEY);
        assert!(!legacy);
        assert!(wrap_backup_key(KEY, "backup-password").unwrap() != wrapped);
    }

    #[test]
    fn backup_key_rejects_wrong_password_and_tampering() {
        let wrapped = wrap_backup_key(KEY, "backup-password").unwrap();
        let error = unwrap_backup_key(&wrapped, "wrong-password").unwrap_err();
        for message in [format!("{error}"), format!("{error:?}")] {
            assert!(!message.contains(KEY));
            assert!(!message.contains("wrong-password"));
            assert!(!message.contains(&wrapped));
        }
        let bytes = base32::decode(
            base32::Alphabet::Rfc4648 { padding: true },
            wrapped.strip_prefix(WRAP_VERSION).unwrap(),
        )
        .unwrap();
        for index in [0, WRAP_SALT_LEN, WRAP_HEADER_LEN, WRAP_LEN - 1] {
            let mut tampered = bytes.clone();
            tampered[index] ^= 1;
            let encoded = base32::encode(base32::Alphabet::Rfc4648 { padding: true }, &tampered);
            assert!(
                unwrap_backup_key(&format!("{WRAP_VERSION}{encoded}"), "backup-password").is_err()
            );
        }
        for prefix in ["v2:", "v0:", "V1:", "", "v1"] {
            assert!(
                unwrap_backup_key(
                    &format!("{prefix}{}", wrapped.strip_prefix(WRAP_VERSION).unwrap()),
                    "backup-password",
                )
                .is_err()
            );
        }
        for len in 0..bytes.len() {
            let encoded =
                base32::encode(base32::Alphabet::Rfc4648 { padding: true }, &bytes[..len]);
            assert!(
                unwrap_backup_key(&format!("{WRAP_VERSION}{encoded}"), "backup-password").is_err()
            );
        }
        assert!(unwrap_backup_key(&format!("v1:{}", "A".repeat(1024)), "backup-password").is_err());
        assert!(wrap_backup_key("not a backup key", "backup-password").is_err());
    }

    #[test]
    fn historical_backup_key_fixture_remains_readable() {
        for padding in [false, true] {
            let (key, legacy) =
                unwrap_backup_key(&legacy_fixture(padding), LEGACY_PASSWORD).unwrap();
            assert!(key == KEY);
            assert!(legacy);
        }
        assert!(unwrap_backup_key(&legacy_fixture(true), "wrong-password").is_err());
        assert!(unwrap_backup_key("", LEGACY_PASSWORD).is_err());
    }
}
