//! Enable-owned signing identities and shared MOK-request authentication.
//! Firmware approval codes authorize enrollment, never data unlock or recovery.

use std::fmt;
use std::os::unix::fs::DirBuilderExt;
use std::path::Path;

use openssl::pkey::{PKey, Private};
use openssl::x509::X509;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use zeroize::Zeroizing;

use crate::context::RpcContext;
use crate::prelude::*;
use crate::util::crypto::gen_nistp256;
use crate::util::future::complete_owned_operation;
use crate::util::io::{delete_file, read_file_to_string, write_file_private_atomic};
use crate::util::mok::{DKMS_MOK_PUB, MokEnrollment, enrollment_status, request_enrollment};
use crate::util::serde::Pem;
use crate::util::{GeneralBoxedGuard, Invoke, new_guid};

/// Reserves shared MOK authentication; presence does not attest enrollment.
pub(crate) const REQUEST_CERTIFICATE: &str = "/media/startos/config/fde/enrollment.der";

#[derive(Clone, Deserialize, Serialize)]
pub struct Enrollment {
    pub(crate) image_key: Pem<PKey<Private>>,
    pub(crate) image_certificate: Pem<X509>,
    pub(crate) manifest_key: Pem<PKey<Private>>,
    pub(crate) policy_key: Pem<PKey<Private>>,
    approval_code: Option<String>,
}

impl fmt::Debug for Enrollment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Enrollment").finish_non_exhaustive()
    }
}

/// Enrollment observation, not proof of active disk protection.
pub struct Progress {
    pub enrolled: bool,
    pub approval_code: Option<String>,
    pub certificate_sha256: String,
}

pub(crate) async fn owns_enrollment_request() -> Result<bool, Error> {
    request_marker(Path::new(REQUEST_CERTIFICATE)).await
}

async fn request_marker(path: &Path) -> Result<bool, Error> {
    match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) if metadata.file_type().is_file() => Ok(true),
        Ok(_) => Err(Error::new(
            eyre!(t!("error.invalid-request")),
            ErrorKind::Filesystem,
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

impl Enrollment {
    async fn generate(runtime: &Path) -> Result<Self, Error> {
        super::memory::require_tmpfs(runtime)?;
        let memory = super::memory::ProcessSecretMemory::protect()?;
        let directory = runtime.join(format!("startos-fde-enrollment-{}", new_guid()));
        std::fs::DirBuilder::new().mode(0o700).create(&directory)?;
        let _cleanup = GeneralBoxedGuard::new({
            let directory = directory.clone();
            move || {
                let _ = std::fs::remove_dir_all(directory);
            }
        });
        let key = directory.join("image.key");
        let certificate = directory.join("image.pem");
        Command::new("/usr/lib/systemd/ukify")
            .args(["genkey", "--config=/dev/null"])
            .arg(format!("--secureboot-private-key={}", key.display()))
            .arg(format!(
                "--secureboot-certificate={}",
                certificate.display()
            ))
            .invoke(ErrorKind::OpenSsl)
            .await?;
        let encoded_key = Zeroizing::new(read_file_to_string(&key).await?);
        let image_key = PKey::private_key_from_pem(encoded_key.as_bytes())?;
        let image_certificate =
            X509::from_pem(read_file_to_string(&certificate).await?.as_bytes())?;
        image_key.rsa()?;
        if !image_key.public_eq(image_certificate.public_key()?.as_ref()) {
            return Err(Error::new(
                eyre!(t!("error.invalid-request")),
                ErrorKind::OpenSsl,
            ));
        }
        memory.check()?;
        Ok(Self {
            image_key: Pem(image_key),
            image_certificate: Pem(image_certificate),
            manifest_key: Pem(gen_nistp256()?),
            policy_key: Pem(gen_nistp256()?),
            approval_code: Some(new_guid().chars().take(16).collect()),
        })
    }
}

fn require_acknowledgment(acknowledged: bool) -> Result<(), Error> {
    if !acknowledged {
        return Err(Error::new(
            eyre!(t!("fde.enrollment.offline-acknowledgment")),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(())
}

/// Begins firmware enrollment inside an acknowledged enable request.
/// The caller confines FDE to supported generic UEFI targets; activation is separate.
pub async fn begin(
    ctx: RpcContext,
    password: Zeroizing<String>,
    acknowledged_offline: bool,
) -> Result<Progress, Error> {
    complete_owned_operation(async move {
        let _exclusive = ctx.password_changes.lock().await;
        require_acknowledgment(acknowledged_offline)?;
        let snapshot = ctx.db.peek().await;
        crate::auth::check_password(&snapshot.as_private().as_password().de()?, &password)?;
        crate::disk::fde::LuksPool::new(&ctx.disk_guid, "/media/startos/config")?;
        if !crate::util::mok::is_secure_boot_enabled().await
            || crate::util::io::read_file_to_string("/sys/class/tpm/tpm0/tpm_version_major")
                .await?
                .trim()
                != "2"
        {
            return Err(Error::new(
                eyre!(t!("fde.enrollment.hardware-required")),
                ErrorKind::Bios,
            ));
        }
        super::memory::require_tmpfs(Path::new("/run"))?;
        let memory = super::memory::ProcessSecretMemory::protect()?;
        let mut enrollment = match snapshot.as_private().as_fde_enrollment().de()? {
            Some(enrollment) => enrollment,
            None => {
                let enrollment = Enrollment::generate(Path::new("/run")).await?;
                ctx.db
                    .mutate(|db| {
                        db.as_private_mut()
                            .as_fde_enrollment_mut()
                            .ser(&Some(enrollment.clone()))
                    })
                    .await
                    .result?;
                enrollment
            }
        };
        write_file_private_atomic(
            Path::new(REQUEST_CERTIFICATE),
            &enrollment.image_certificate.to_der()?,
        )
        .await?;
        let mut certificates = vec![Path::new(REQUEST_CERTIFICATE).to_owned()];
        if request_marker(Path::new(DKMS_MOK_PUB)).await? {
            certificates.push(DKMS_MOK_PUB.into());
        }
        let mut pending = Vec::new();
        let mut needs_import = false;
        for certificate in certificates {
            match enrollment_status(&certificate).await? {
                MokEnrollment::Enrolled => {}
                MokEnrollment::Pending => pending.push(certificate),
                MokEnrollment::NotEnrolled => {
                    needs_import = true;
                    pending.push(certificate);
                }
            }
        }
        if needs_import {
            let code = enrollment.approval_code.as_ref().ok_or_else(|| {
                Error::new(
                    eyre!(t!("fde.enrollment.trusted-reenrollment")),
                    ErrorKind::Bios,
                )
            })?;
            let hash = crate::util::crypto::shadow_password_hash(code)?;
            request_enrollment(&pending, &hash).await?;
        }
        let mut enrolled = true;
        for certificate in &pending {
            match enrollment_status(certificate).await? {
                MokEnrollment::Enrolled => {}
                MokEnrollment::Pending => enrolled = false,
                MokEnrollment::NotEnrolled => {
                    return Err(Error::new(
                        eyre!(t!("fde.enrollment.request-not-recorded")),
                        ErrorKind::Bios,
                    ));
                }
            }
        }
        if enrolled {
            enrollment.approval_code = None;
            ctx.db
                .mutate(|db| {
                    db.as_private_mut()
                        .as_fde_enrollment_mut()
                        .ser(&Some(enrollment.clone()))
                })
                .await
                .result?;
            delete_file(REQUEST_CERTIFICATE).await?;
        }
        memory.check()?;
        Ok(Progress {
            enrolled,
            approval_code: enrollment.approval_code,
            certificate_sha256: hex::encode(openssl::sha::sha256(
                &enrollment.image_certificate.to_der()?,
            )),
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_acknowledgment_precedes_enrollment() {
        assert!(require_acknowledgment(false).is_err());
        require_acknowledgment(true).unwrap();
    }

    #[tokio::test]
    async fn request_ownership_is_intent_not_enrollment_and_rejects_symlinks() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("request.der");
        assert!(!request_marker(&marker).await.unwrap());
        std::fs::write(&marker, b"public certificate").unwrap();
        assert!(request_marker(&marker).await.unwrap());
        std::fs::remove_file(&marker).unwrap();
        std::os::unix::fs::symlink("/dev/null", &marker).unwrap();
        assert!(request_marker(&marker).await.is_err());
    }

    #[tokio::test]
    #[ignore = "requires native systemd ukify, RAM storage and protected process memory"]
    async fn native_signing_identities_are_separate_and_private() {
        let runtime = tempfile::tempdir_in("/dev/shm").unwrap();
        let enrollment = Enrollment::generate(runtime.path()).await.unwrap();
        assert!(enrollment.image_key.rsa().is_ok());
        assert!(
            enrollment
                .image_key
                .public_eq(enrollment.image_certificate.public_key().unwrap().as_ref())
        );
        assert!(!enrollment.policy_key.public_eq(&enrollment.manifest_key));
        assert!(!enrollment.image_key.public_eq(&enrollment.policy_key));
        assert_eq!(enrollment.approval_code.as_ref().unwrap().len(), 16);
        assert!(!format!("{enrollment:?}").contains(enrollment.approval_code.as_ref().unwrap()));
        assert!(std::fs::read_dir(runtime.path()).unwrap().next().is_none());
        let roundtrip: Enrollment =
            serde_json::from_slice(&serde_json::to_vec(&enrollment).unwrap()).unwrap();
        assert!(roundtrip.image_key.public_eq(&enrollment.image_key));
    }
}
