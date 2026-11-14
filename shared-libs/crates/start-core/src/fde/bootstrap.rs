use openssl::symm::{Cipher, decrypt_aead, encrypt_aead};
use zeroize::Zeroizing;

use crate::prelude::*;

const FORMAT: &[u8] = b"StartOS FDE bootstrap\0\x01";
const NONCE_LENGTH: usize = 12;
const TAG_LENGTH: usize = 16;

pub fn generate_key() -> Result<Zeroizing<[u8; 32]>, Error> {
    let mut key = Zeroizing::new([0; 32]);
    openssl::rand::rand_bytes(key.as_mut())?;
    Ok(key)
}

/// Authenticates the complete bootstrap payload independently of its disk location.
pub fn encrypt(plaintext: &[u8], key: &[u8; 32]) -> Result<Vec<u8>, Error> {
    let mut nonce = [0; NONCE_LENGTH];
    openssl::rand::rand_bytes(&mut nonce)?;
    let mut tag = [0; TAG_LENGTH];
    let ciphertext = encrypt_aead(
        Cipher::aes_256_gcm(),
        key,
        Some(&nonce),
        FORMAT,
        plaintext,
        &mut tag,
    )?;
    let mut envelope =
        Vec::with_capacity(FORMAT.len() + nonce.len() + tag.len() + ciphertext.len());
    envelope.extend_from_slice(FORMAT);
    envelope.extend_from_slice(&nonce);
    envelope.extend_from_slice(&tag);
    envelope.extend_from_slice(&ciphertext);
    Ok(envelope)
}

pub fn decrypt(envelope: &[u8], key: &[u8; 32]) -> Result<Zeroizing<Vec<u8>>, Error> {
    let header_length = FORMAT.len() + NONCE_LENGTH + TAG_LENGTH;
    if envelope.len() < header_length || !envelope.starts_with(FORMAT) {
        return Err(Error::new(
            eyre!("{}", t!("fde.bootstrap.invalid-envelope")),
            ErrorKind::Deserialization,
        ));
    }
    let nonce = &envelope[FORMAT.len()..FORMAT.len() + NONCE_LENGTH];
    let tag = &envelope[FORMAT.len() + NONCE_LENGTH..header_length];
    Ok(Zeroizing::new(decrypt_aead(
        Cipher::aes_256_gcm(),
        key,
        Some(nonce),
        FORMAT,
        &envelope[header_length..],
        tag,
    )?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_is_authenticated_and_randomized() {
        let key = generate_key().unwrap();
        let plaintext = b"a TLS identity and a WireGuard profile";
        let envelope = encrypt(plaintext, &key).unwrap();
        assert_eq!(decrypt(&envelope, &key).unwrap().as_slice(), plaintext);
        assert_ne!(envelope, encrypt(plaintext, &key).unwrap());
        for offset in [
            0,
            FORMAT.len(),
            FORMAT.len() + NONCE_LENGTH,
            envelope.len() - 1,
        ] {
            let mut changed = envelope.clone();
            changed[offset] ^= 1;
            assert!(decrypt(&changed, &key).is_err());
        }
        assert!(decrypt(&envelope, &generate_key().unwrap()).is_err());
    }

    #[test]
    fn incomplete_bootstrap_is_rejected() {
        let key = generate_key().unwrap();
        let envelope = encrypt(b"", &key).unwrap();
        assert!(decrypt(&envelope, &key).unwrap().is_empty());
        for length in 0..envelope.len() {
            assert!(decrypt(&envelope[..length], &key).is_err());
        }
    }
}
