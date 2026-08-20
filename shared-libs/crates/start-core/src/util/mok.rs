use std::path::Path;

use tokio::process::Command;

use crate::prelude::*;
use crate::util::Invoke;
use crate::util::io::{delete_file, maybe_open_file, write_file_atomic};

pub const DKMS_MOK_KEY: &str = "/var/lib/dkms/mok.key";
pub const DKMS_MOK_PUB: &str = "/var/lib/dkms/mok.pub";

pub async fn is_secure_boot_enabled() -> bool {
    String::from_utf8_lossy(
        &Command::new("mokutil")
            .arg("--sb-state")
            .env("LANG", "C.UTF-8")
            .invoke(ErrorKind::Bios)
            .await
            .unwrap_or_default(),
    )
    .contains("SecureBoot enabled")
}

/// Generate a DKMS MOK key pair if one doesn't exist.
pub async fn ensure_dkms_key(root: &Path) -> Result<bool, Error> {
    let key_path = root.join(DKMS_MOK_KEY.trim_start_matches('/'));
    if maybe_open_file(&key_path).await?.is_some() {
        return Ok(false); // Already exists
    }
    Command::new("chroot")
        .arg(root)
        .arg("dkms")
        .arg("generate_mok")
        .invoke(ErrorKind::Bios)
        .await?;
    Ok(true) // Newly generated
}

/// Sign all unsigned kernel modules in the given root using the DKMS MOK key.
/// Calls the sign-unsigned-modules script inside the chroot.
pub async fn sign_unsigned_modules(root: &Path) -> Result<(), Error> {
    Command::new("chroot")
        .arg(root)
        .arg("/usr/lib/startos/scripts/sign-unsigned-modules")
        .invoke(ErrorKind::OpenSsl)
        .await?;
    Ok(())
}

/// Read the start9 user's password hash from `<root>/etc/shadow`.
/// Returns None if the user doesn't exist or the password is locked.
async fn start9_shadow_hash(root: &Path) -> Result<Option<String>, Error> {
    let shadow = tokio::fs::read_to_string(root.join("etc/shadow")).await?;
    for line in shadow.lines() {
        if let Some(("start9", rest)) = line.split_once(':') {
            if let Some((hash, _)) = rest.split_once(':') {
                let hash = hash.trim_start_matches("!");
                if hash.starts_with('$') {
                    return Ok(Some(hash.to_owned()));
                }
                // Locked or invalid password
                return Ok(None);
            }
        }
    }
    Ok(None)
}

/// Enroll the DKMS MOK certificate, protected by the start9 user's password.
/// `root` holds both the certificate and `etc/shadow`: `/` at runtime, or the target
/// overlay during install. Idempotent: skips if already enrolled, or if the password
/// is not yet set — so it must also be called once the password is written, not only
/// at boot. Returns true if a new enrollment was staged.
pub async fn enroll_mok(root: &Path) -> Result<bool, Error> {
    let mok_pub = &root.join(DKMS_MOK_PUB.trim_start_matches('/'));
    tracing::info!("enroll_mok: checking EFI and mok_pub={}", mok_pub.display());
    if tokio::fs::metadata("/sys/firmware/efi").await.is_err() {
        tracing::info!("enroll_mok: no EFI, skipping");
        return Ok(false);
    }
    if maybe_open_file(mok_pub).await?.is_none() {
        tracing::info!("enroll_mok: mok_pub not found, skipping");
        return Ok(false);
    }

    // Check if already enrolled in firmware
    let test_output = Command::new("mokutil")
        .arg("--test-key")
        .arg(mok_pub)
        .env("LANG", "C.UTF-8")
        .invoke(ErrorKind::Bios)
        .await?;
    let test_str = String::from_utf8(test_output)?;
    tracing::info!("enroll_mok: mokutil --test-key output: {test_str:?}");
    if test_str.contains("is enrolled") {
        tracing::info!("enroll_mok: already enrolled, skipping");
        return Ok(false);
    }

    let Some(hash) = start9_shadow_hash(root).await? else {
        tracing::info!("enroll_mok: start9 user password not set, skipping");
        return Ok(false);
    };

    // Revoke any pending enrollment (so we can re-import with current password)
    let _ = Command::new("mokutil")
        .arg("--revoke-import")
        .arg(mok_pub)
        .invoke(ErrorKind::Bios)
        .await;

    let hash_file = Path::new("/tmp/mok-password-hash");
    write_file_atomic(hash_file, &hash).await?;

    tracing::info!("Enrolling DKMS MOK certificate");
    let result = Command::new("mokutil")
        .arg("--import")
        .arg(mok_pub)
        .arg("--hash-file")
        .arg(hash_file)
        .invoke(ErrorKind::Bios)
        .await;

    delete_file(hash_file).await.log_err();
    result?;
    Ok(true)
}
