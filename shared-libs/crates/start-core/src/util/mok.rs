use std::path::{Path, PathBuf};
use std::process::Output;

use tokio::process::Command;

use crate::prelude::*;
use crate::util::io::{maybe_open_file, write_file_private_atomic};
use crate::util::{GeneralBoxedGuard, Invoke, new_guid};

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
        return Ok(false);
    }
    Command::new("chroot")
        .arg(root)
        .arg("dkms")
        .arg("generate_mok")
        .invoke(ErrorKind::Bios)
        .await?;
    Ok(true)
}

/// Sign all unsigned kernel modules in the given root using the DKMS MOK key.
pub async fn sign_unsigned_modules(root: &Path) -> Result<(), Error> {
    Command::new("chroot")
        .arg(root)
        .arg("/usr/lib/startos/scripts/sign-unsigned-modules")
        .invoke(ErrorKind::OpenSsl)
        .await?;
    Ok(())
}

/// The start9 user's password hash without shadow lock markers.
async fn start9_shadow_hash(root: &Path) -> Result<Option<String>, Error> {
    let shadow = tokio::fs::read_to_string(root.join("etc/shadow")).await?;
    for line in shadow.lines() {
        if let Some(("start9", rest)) = line.split_once(':') {
            if let Some((hash, _)) = rest.split_once(':') {
                let hash = hash.trim_start_matches("!");
                if hash.starts_with('$') {
                    return Ok(Some(hash.to_owned()));
                }
                return Ok(None);
            }
        }
    }
    Ok(None)
}

fn mok_enrolled(certificate: &Path, output: Output) -> Result<bool, Error> {
    let message = String::from_utf8_lossy(&output.stdout);
    let certificate = certificate.display();
    match output.status.code() {
        Some(1) if message.trim() == format!("{certificate} is already enrolled") => {
            return Ok(true);
        }
        Some(0) if message.trim() == format!("{certificate} is not enrolled") => return Ok(false),
        Some(1)
            if message.trim() == format!("{certificate} is already in the enrollment request") =>
        {
            return Ok(false);
        }
        _ => {}
    }
    super::check_command_output(output, ErrorKind::Bios, "mokutil --test-key")?;
    Err(Error::new(
        eyre!("{}", t!("util.mok.invalid-status")),
        ErrorKind::Bios,
    ))
}

async fn password_hash_file(
    directory: &Path,
    hash: &str,
) -> Result<(PathBuf, GeneralBoxedGuard), Error> {
    let path = directory.join(format!("startos-mok-{}", new_guid()));
    let cleanup = GeneralBoxedGuard::new({
        let path = path.clone();
        move || {
            let _ = std::fs::remove_file(path);
        }
    });
    write_file_private_atomic(&path, hash).await?;
    Ok((path, cleanup))
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

    let test_output = Command::new("mokutil")
        .arg("--test-key")
        .arg(mok_pub)
        .env("LC_ALL", "C")
        .kill_on_drop(true)
        .output()
        .await?;
    if mok_enrolled(mok_pub, test_output)? {
        tracing::info!("enroll_mok: already enrolled, skipping");
        return Ok(false);
    }

    let Some(hash) = start9_shadow_hash(root).await? else {
        tracing::info!("enroll_mok: start9 user password not set, skipping");
        return Ok(false);
    };

    let _ = Command::new("mokutil")
        .arg("--revoke-import")
        .arg(mok_pub)
        .invoke(ErrorKind::Bios)
        .await;

    let (hash_file, _cleanup) = password_hash_file(Path::new("/run"), &hash).await?;

    tracing::info!("Enrolling DKMS MOK certificate");
    Command::new("mokutil")
        .arg("--import")
        .arg(mok_pub)
        .arg("--hash-file")
        .arg(&hash_file)
        .invoke(ErrorKind::Bios)
        .await?;
    Ok(true)
}

#[cfg(test)]
mod enrollment_tests {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::ExitStatusExt;

    use super::*;

    fn output(code: i32, suffix: &str) -> Output {
        Output {
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: format!("/test/mok.pub {suffix}\n").into_bytes(),
            stderr: Vec::new(),
        }
    }

    #[test]
    fn enrollment_probe_distinguishes_enrolled_pending_and_absent_certificates() {
        let certificate = Path::new("/test/mok.pub");
        for (code, message, enrolled) in [
            (1, "is already enrolled", true),
            (0, "is not enrolled", false),
            (1, "is already in the enrollment request", false),
        ] {
            assert_eq!(
                mok_enrolled(certificate, output(code, message)).unwrap(),
                enrolled
            );
        }
        for (code, message) in [
            (1, "is blocked in dbx"),
            (1, "is blocked in MokListX"),
            (1, "is already in db"),
            (0, "unexpected output"),
            (0, "is already enrolled"),
            (1, "is not enrolled"),
            (255, "is already enrolled"),
        ] {
            assert!(mok_enrolled(certificate, output(code, message)).is_err());
        }
    }

    #[tokio::test]
    async fn pending_password_hashes_are_private_independent_and_removed_on_drop() {
        let directory = tempfile::tempdir().unwrap();
        let (one, one_guard) = password_hash_file(directory.path(), "$6$test-hash")
            .await
            .unwrap();
        let (two, two_guard) = password_hash_file(directory.path(), "$6$other-hash")
            .await
            .unwrap();
        assert_ne!(one, two);
        for path in [&one, &two] {
            assert_eq!(
                std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert_eq!(std::fs::read(&one).unwrap(), b"$6$test-hash");
        assert_eq!(std::fs::read(&two).unwrap(), b"$6$other-hash");
        drop(one_guard);
        assert!(!one.exists());
        assert!(two.exists());
        drop(two_guard);
        assert!(!two.exists());
    }
}
