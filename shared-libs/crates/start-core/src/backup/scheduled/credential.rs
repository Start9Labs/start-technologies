use color_eyre::eyre::eyre;
use digest::KeyInit;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::ScheduledBackupCredential;
use crate::prelude::*;
use crate::util::crypto::{decrypt_slice, encrypt_slice};

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
        let ciphertext = encrypt_slice(encryption_key, device_key);
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
        String::from_utf8(decrypt_slice(ciphertext, device_key)).map_err(|_| {
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

    #[test]
    fn sealed_credentials_round_trip_without_storing_passwords() {
        let device_key = generate_scheduled_backup_device_key();
        let credential = ScheduledBackupCredential::seal(
            "target-instance".into(),
            "target-encryption-key",
            &device_key,
        )
        .unwrap();
        assert_eq!(
            credential.open(&device_key).unwrap(),
            "target-encryption-key"
        );
        assert!(!String::from_utf8_lossy(&credential.sealed_key).contains("target-encryption-key"));
    }

    #[test]
    fn wrong_device_key_fails_closed() {
        let credential = ScheduledBackupCredential::seal(
            "target-instance".into(),
            "target-encryption-key",
            &generate_scheduled_backup_device_key(),
        )
        .unwrap();
        assert!(
            credential
                .open(&generate_scheduled_backup_device_key())
                .is_err()
        );
    }
}
