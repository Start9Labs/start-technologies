use color_eyre::eyre::eyre;
use digest::KeyInit;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::ScheduledBackupCredential;
use crate::prelude::*;
use crate::util::crypto::{unwrap_backup_key, wrap_backup_key};

type HmacSha256 = Hmac<Sha256>;
const MAC_LENGTH: usize = 32;

impl ScheduledBackupCredential {
    /// Encrypts and authenticates a target key with the device-local key.
    pub fn seal(
        target_instance_id: String,
        encryption_key: &str,
        device_key: &[u8],
    ) -> Result<Self, Error> {
        validate_device_key(device_key)?;
        let ciphertext = wrap_backup_key(encryption_key, device_key)?.into_bytes();
        let mut mac = HmacSha256::new_from_slice(device_key).map_err(|_| invalid_device_key())?;
        mac.update(target_instance_id.as_bytes());
        mac.update(&ciphertext);
        let mut sealed_key = mac.finalize().into_bytes().to_vec();
        sealed_key.extend(ciphertext);
        Ok(Self {
            target_instance_id,
            sealed_key,
        })
    }

    /// Verifies and decrypts the target key with the device-local key.
    pub fn open(&self, device_key: &[u8]) -> Result<String, Error> {
        validate_device_key(device_key)?;
        let (expected_mac, ciphertext) =
            self.sealed_key
                .split_at_checked(MAC_LENGTH)
                .ok_or_else(|| {
                    Error::new(
                        eyre!("{}", t!("backup.scheduled.invalid-credential")),
                        ErrorKind::Backup,
                    )
                })?;
        let mut mac = HmacSha256::new_from_slice(device_key).map_err(|_| invalid_device_key())?;
        mac.update(self.target_instance_id.as_bytes());
        mac.update(ciphertext);
        mac.verify_slice(expected_mac).map_err(|_| {
            Error::new(
                eyre!("{}", t!("backup.scheduled.reauth-required")),
                ErrorKind::Authorization,
            )
        })?;
        let wrapped = if ciphertext.starts_with(b"v1:") {
            String::from_utf8(ciphertext.to_vec()).map_err(|_| invalid_device_key())?
        } else {
            base32::encode(base32::Alphabet::Rfc4648 { padding: true }, ciphertext)
        };
        unwrap_backup_key(&wrapped, device_key)
            .map(|(key, _)| key)
            .map_err(|_| {
                Error::new(
                    eyre!("{}", t!("backup.scheduled.reauth-required")),
                    ErrorKind::Authorization,
                )
            })
    }
}

pub fn generate_scheduled_backup_device_key() -> Vec<u8> {
    rand::random::<[u8; 32]>().to_vec()
}

fn validate_device_key(device_key: &[u8]) -> Result<(), Error> {
    if device_key.len() == 32 {
        Ok(())
    } else {
        Err(invalid_device_key())
    }
}

fn invalid_device_key() -> Error {
    Error::new(
        eyre!("{}", t!("backup.scheduled.device-key-unavailable")),
        ErrorKind::Authorization,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::crypto::backup_key_tests::KEY;

    #[test]
    fn sealed_credentials_round_trip_without_storing_passwords() {
        let device_key = generate_scheduled_backup_device_key();
        let credential =
            ScheduledBackupCredential::seal("target-instance".into(), KEY, &device_key).unwrap();
        assert_eq!(credential.open(&device_key).unwrap(), KEY);
        assert!(!String::from_utf8_lossy(&credential.sealed_key).contains(KEY));
    }

    #[test]
    fn wrong_device_key_fails_closed() {
        let credential = ScheduledBackupCredential::seal(
            "target-instance".into(),
            KEY,
            &generate_scheduled_backup_device_key(),
        )
        .unwrap();
        assert!(
            credential
                .open(&generate_scheduled_backup_device_key())
                .is_err()
        );
    }

    #[test]
    fn legacy_device_credentials_remain_readable() {
        let device_key = [19; 32];
        let prefix = [11; 32];
        let mut key = [0; 32];
        pbkdf2::pbkdf2::<HmacSha256>(&device_key, &prefix[16..], 1000, &mut key).unwrap();
        let ciphertext = openssl::symm::encrypt(
            openssl::symm::Cipher::aes_256_ctr(),
            &key,
            Some(&prefix[..16]),
            KEY.as_bytes(),
        )
        .unwrap();
        let ciphertext = [prefix.as_slice(), &ciphertext].concat();
        let mut mac = HmacSha256::new_from_slice(&device_key).unwrap();
        mac.update(b"target-instance");
        mac.update(&ciphertext);
        let credential = ScheduledBackupCredential {
            target_instance_id: "target-instance".into(),
            sealed_key: [mac.finalize().into_bytes().as_slice(), &ciphertext].concat(),
        };
        assert_eq!(credential.open(&device_key).unwrap(), KEY);
    }

    #[test]
    fn credentials_reject_key_tampering_and_target_substitution() {
        let device_key = generate_scheduled_backup_device_key();
        let mut credential =
            ScheduledBackupCredential::seal("original".into(), KEY, &device_key).unwrap();
        assert!(credential.sealed_key[MAC_LENGTH..].starts_with(b"v1:"));
        credential.target_instance_id = "other".into();
        assert!(credential.open(&device_key).is_err());
        credential.target_instance_id = "original".into();
        *credential.sealed_key.last_mut().unwrap() ^= 1;
        assert!(credential.open(&device_key).is_err());
    }
}
