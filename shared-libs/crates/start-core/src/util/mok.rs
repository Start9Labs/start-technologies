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

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize, visit_rs::VisitVariants,
)]
#[serde(rename_all = "camelCase")]
pub enum MokEnrollment {
    NotEnrolled,
    Pending,
    Enrolled,
}
rpc_toolkit::reflect_ts!(MokEnrollment);

fn parse_enrollment(certificate: &Path, output: Output) -> Result<MokEnrollment, Error> {
    let message = String::from_utf8_lossy(&output.stdout);
    let certificate = certificate.display();
    match output.status.code() {
        Some(1) if message.trim() == format!("{certificate} is already enrolled") => {
            return Ok(MokEnrollment::Enrolled);
        }
        Some(0) if message.trim() == format!("{certificate} is not enrolled") => {
            return Ok(MokEnrollment::NotEnrolled);
        }
        Some(1)
            if message.trim() == format!("{certificate} is already in the enrollment request") =>
        {
            return Ok(MokEnrollment::Pending);
        }
        _ => {}
    }
    super::check_command_output(output, ErrorKind::Bios, "mokutil --test-key")?;
    Err(Error::new(
        eyre!("{}", t!("util.mok.invalid-status")),
        ErrorKind::Bios,
    ))
}

/// Reports enrolled MOKs separately from pending requests.
/// The caller keeps the certificate file stable through the probe.
pub async fn enrollment_status(certificate: &Path) -> Result<MokEnrollment, Error> {
    let output = Command::new("mokutil")
        .arg("--test-key")
        .arg(certificate)
        .env("LC_ALL", "C")
        .kill_on_drop(true)
        .output()
        .await?;
    parse_enrollment(certificate, output)
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

/// Appends certificates to the shared native enrollment request.
/// Already-pending certificates do not refresh its authentication value.
pub async fn request_enrollment(certificates: &[PathBuf], hash: &str) -> Result<(), Error> {
    let (hash_file, _cleanup) = password_hash_file(Path::new("/run"), hash).await?;
    Command::new("mokutil")
        .arg("--import")
        .args(certificates)
        .arg("--hash-file")
        .arg(&hash_file)
        .invoke(ErrorKind::Bios)
        .await?;
    Ok(())
}

/// Stages DKMS enrollment using the start9 user's password.
/// The root holds both the certificate and `etc/shadow`.
/// FDE-owned authentication remains unchanged. Returns request submission, not enrollment.
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

    if enrollment_status(mok_pub).await? == MokEnrollment::Enrolled {
        tracing::info!("enroll_mok: already enrolled, skipping");
        return Ok(false);
    }

    if crate::fde::enrollment::owns_enrollment_request().await? {
        return Ok(false);
    }

    let Some(hash) = start9_shadow_hash(root).await? else {
        tracing::info!("enroll_mok: start9 user password not set, skipping");
        return Ok(false);
    };

    // Pending imports share one `MokAuth` value.
    let _ = Command::new("mokutil")
        .arg("--revoke-import")
        .invoke(ErrorKind::Bios)
        .await;

    tracing::info!("Enrolling DKMS MOK certificate");
    request_enrollment(&[mok_pub.to_owned()], &hash).await?;
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
        for (code, message, enrollment) in [
            (1, "is already enrolled", MokEnrollment::Enrolled),
            (0, "is not enrolled", MokEnrollment::NotEnrolled),
            (
                1,
                "is already in the enrollment request",
                MokEnrollment::Pending,
            ),
        ] {
            assert_eq!(
                parse_enrollment(certificate, output(code, message)).unwrap(),
                enrollment
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
            assert!(parse_enrollment(certificate, output(code, message)).is_err());
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

/// UEFI_VARIABLE_DATA, with GUID bytes in EFI wire order and UTF-16 code-unit lengths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EfiVariable {
    pub vendor: [u8; 16],
    pub name: String,
    pub data: Vec<u8>,
}

impl EfiVariable {
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let vendor = bytes.get(..16)?.try_into().ok()?;
        let name_len =
            usize::try_from(u64::from_le_bytes(bytes.get(16..24)?.try_into().ok()?)).ok()?;
        let data_len =
            usize::try_from(u64::from_le_bytes(bytes.get(24..32)?.try_into().ok()?)).ok()?;
        let name_end = 32usize.checked_add(name_len.checked_mul(2)?)?;
        if name_end.checked_add(data_len)? != bytes.len() {
            return None;
        }
        let units: Vec<_> = bytes
            .get(32..name_end)?
            .chunks_exact(2)
            .map(|u| u16::from_le_bytes([u[0], u[1]]))
            .collect();
        let name = String::from_utf16(&units).ok()?;
        Some(Self {
            vendor,
            name,
            data: bytes[name_end..].to_vec(),
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        let units: Vec<_> = self.name.encode_utf16().collect();
        [
            self.vendor.as_slice(),
            &(units.len() as u64).to_le_bytes(),
            &(self.data.len() as u64).to_le_bytes(),
            &units
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
            &self.data,
        ]
        .concat()
    }
}

/// EFI_SIGNATURE_DATA retains the enrolled owner, not a synthesized GUID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EfiSignature {
    pub kind: [u8; 16],
    pub owner: [u8; 16],
    pub data: Vec<u8>,
    pub first_in_list: bool,
}

impl EfiSignature {
    pub fn is_x509(&self) -> bool {
        self.kind
            == [
                0xa1, 0x59, 0xc0, 0xa5, 0xe4, 0x94, 0xa7, 0x4a, 0x87, 0xb5, 0xab, 0x15, 0x5c, 0x2b,
                0xf0, 0x72,
            ]
    }

    pub fn is_sha256(&self) -> bool {
        self.kind
            == [
                0x26, 0x16, 0xc4, 0xc1, 0x4c, 0x50, 0x92, 0x40, 0xac, 0xa9, 0x41, 0xf9, 0x36, 0x93,
                0x43, 0x28,
            ]
    }

    pub fn is_sha1(&self) -> bool {
        self.kind
            == [
                0x12, 0xa5, 0x6c, 0x82, 0x10, 0xcf, 0xc9, 0x4a, 0xb1, 0x87, 0xbe, 0x01, 0x49, 0x66,
                0x31, 0xbd,
            ]
    }

    pub fn encode(&self) -> Vec<u8> {
        [self.owner.as_slice(), &self.data].concat()
    }
}

/// Decodes EFI_SIGNATURE_LIST without efivarfs' four-byte attributes prefix.
pub(crate) fn signature_entries(mut bytes: &[u8]) -> Option<Vec<EfiSignature>> {
    let mut entries = Vec::new();
    while !bytes.is_empty() {
        let kind = bytes.get(..16)?.try_into().ok()?;
        let list_size = u32::from_le_bytes(bytes.get(16..20)?.try_into().ok()?) as usize;
        let header_size = u32::from_le_bytes(bytes.get(20..24)?.try_into().ok()?) as usize;
        let signature_size = u32::from_le_bytes(bytes.get(24..28)?.try_into().ok()?) as usize;
        let start = 28usize.checked_add(header_size)?;
        if list_size < start || signature_size <= 16 {
            return None;
        }
        let signatures = bytes.get(start..list_size)?;
        if signatures.is_empty() || signatures.len() % signature_size != 0 {
            return None;
        }
        for (index, entry) in signatures.chunks_exact(signature_size).enumerate() {
            entries.push(EfiSignature {
                kind,
                owner: entry[..16].try_into().ok()?,
                data: entry[16..].to_vec(),
                first_in_list: index == 0,
            });
        }
        bytes = bytes.get(list_size..)?;
    }
    Some(entries)
}

#[cfg(test)]
mod signature_tests {
    use super::*;

    #[test]
    fn authority_encoding_uses_efi_guid_and_utf16_lengths_without_a_nul() {
        let variable = EfiVariable {
            vendor: [0x42; 16],
            name: "A𝄞".into(),
            data: vec![0xaa, 0xbb],
        };
        let expected = hex::decode(
            "4242424242424242424242424242424203000000000000000200000000000000410034d81eddaabb",
        )
        .unwrap();
        assert_eq!(variable.encode(), expected);
        assert_eq!(EfiVariable::decode(&expected), Some(variable));
        for end in 0..expected.len() {
            assert!(EfiVariable::decode(&expected[..end]).is_none());
        }
        let mut extra = expected;
        extra.push(0);
        assert!(EfiVariable::decode(&extra).is_none());
    }

    #[test]
    fn signature_lists_preserve_each_owner_and_reject_corrupt_sizes() {
        let bytes = hex::decode("a159c0a5e494a74a87b5ab155c2bf07240000000000000001200000011111111111111111111111111111111aabb22222222222222222222222222222222ccdd").unwrap();
        let entries = signature_entries(&bytes).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries[0].is_x509());
        assert!(entries[0].first_in_list);
        assert!(!entries[1].first_in_list);
        assert_eq!(entries[0].owner, [0x11; 16]);
        assert_eq!(entries[1].owner, [0x22; 16]);
        assert_eq!(
            hex::encode(entries[1].encode()),
            "22222222222222222222222222222222ccdd"
        );
        for offset in [16, 20, 24] {
            for size in [0u32, 1, 16, 17, u32::MAX] {
                if bytes[offset..offset + 4] == size.to_le_bytes() {
                    continue;
                }
                let mut broken = bytes.clone();
                broken[offset..offset + 4].copy_from_slice(&size.to_le_bytes());
                assert!(signature_entries(&broken).is_none());
            }
        }
        let mut twice = bytes.clone();
        twice.extend_from_slice(&bytes);
        assert_eq!(signature_entries(&twice).unwrap().len(), 4);
        assert!(signature_entries(&bytes[..bytes.len() - 1]).is_none());
    }
}
