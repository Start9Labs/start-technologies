//! Boot inputs pinned by the signed UKI and generation admission before PCR 15.
//!
//! Descriptors belong inside the authenticated initramfs. Public policy files
//! cannot replace these keys or select the boot mode. Selection does not stabilize
//! paths: the boot owner must open and fully read both signed verity mappings before
//! measuring filesystems or releasing bootstrap credentials, then retain the mappings
//! until every dependent mount is gone. Data-volume credentials are not handled here.

use std::path::{Path, PathBuf};

use openssl::pkey::{PKey, Public};
use serde::{Deserialize, Serialize};

use super::tpm::{self, MeasurementProfile};
use crate::os_overlay::{self, Manifest, PreparedGeneration, VerifiedGeneration};
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::util::io::write_file_private_atomic;
use crate::util::serde::Der64;

const DESCRIPTOR_VERSION: u8 = 1;
const GENERATIONS_DIR: &str = "fde/generations";
const POLICIES_DIR: &str = "policies";
const MAX_PROFILE_BYTES: usize = 8192;

/// A rootfs reference and verification keys authenticated by the containing UKI.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Descriptor {
    version: u8,
    rootfs: String,
    overlay_key: Der64<PKey<Public>>,
    policy_key: Der64<PKey<Public>>,
}

fn invalid() -> Error {
    Error::new(
        eyre!("{}", t!("fde.boot.invalid-descriptor")),
        ErrorKind::InvalidSignature,
    )
}

pub fn generation_store(config: &Path) -> PathBuf {
    config.join(GENERATIONS_DIR)
}

impl Descriptor {
    pub fn new(
        rootfs: String,
        overlay_key: PKey<Public>,
        policy_key: PKey<Public>,
    ) -> Result<Self, Error> {
        let descriptor = Self {
            version: DESCRIPTOR_VERSION,
            rootfs,
            overlay_key: Der64(overlay_key),
            policy_key: Der64(policy_key),
        };
        descriptor.validate()?;
        Ok(descriptor)
    }

    fn validate(&self) -> Result<(), Error> {
        if self.version != DESCRIPTOR_VERSION
            || Path::new(&self.rootfs)
                .file_name()
                .and_then(|name| name.to_str())
                != Some(self.rootfs.as_str())
        {
            return Err(invalid());
        }
        Ok(())
    }

    pub fn rootfs_path(&self, images: &Path) -> Result<PathBuf, Error> {
        self.validate()?;
        Ok(images.join(&self.rootfs))
    }

    /// Adds an authenticated filesystem policy before the enclosing generation publishes.
    /// Staging and rootfs must remain stable until publication completes.
    pub async fn stage_profile(
        &self,
        generation: &PreparedGeneration,
        profile: &MeasurementProfile,
    ) -> Result<PathBuf, Error> {
        self.validate()?;
        let manifest =
            os_overlay::read_manifest(generation.staging_path(), &self.overlay_key).await?;
        if profile.measurements.files
            != tpm::filesystem_pcr(&tpm::filesystem_event(
                &manifest.rootfs_sha256,
                &manifest.overlay_sha256,
            ))
            || !profile.verify(&self.policy_key).await?
        {
            return Err(invalid());
        }
        let bytes = serde_json::to_vec(profile).with_kind(ErrorKind::Serialization)?;
        if bytes.len() > MAX_PROFILE_BYTES {
            return Err(invalid());
        }
        let path = generation
            .staging_path()
            .join(POLICIES_DIR)
            .join(format!("{}.json", Guid::new()));
        write_file_private_atomic(&path, bytes).await?;
        Ok(path)
    }

    /// Selects the newest authenticated generation authorized for this pre-filesystem boot.
    /// Missing, malformed or unauthenticated policies do not make a candidate eligible.
    pub async fn select(&self, images: &Path, config: &Path) -> Result<Option<Generation>, Error> {
        let rootfs = self.rootfs_path(images)?;
        let observed = tpm::read_measurements().await?;
        for generation in
            os_overlay::verified_generations(&generation_store(config), &rootfs, &self.overlay_key)
                .await?
        {
            let expected = observed.with_filesystems(
                &generation.manifest.rootfs_sha256,
                &generation.manifest.overlay_sha256,
            )?;
            let Ok(mut policies) = tokio::fs::read_dir(generation.path.join(POLICIES_DIR)).await
            else {
                continue;
            };
            while let Some(entry) = policies.next_entry().await? {
                let Ok(bytes) =
                    os_overlay::read_generation_file(&entry.path(), MAX_PROFILE_BYTES).await
                else {
                    continue;
                };
                let Ok(profile) = serde_json::from_slice::<MeasurementProfile>(&bytes) else {
                    continue;
                };
                if profile.measurements == expected && profile.verify(&self.policy_key).await? {
                    return Ok(Some(Generation {
                        generation,
                        profile,
                    }));
                }
            }
        }
        Ok(None)
    }
}

/// Authenticated selection at verification time, not a live mount or unseal permission.
#[derive(Debug)]
pub struct Generation {
    generation: VerifiedGeneration,
    profile: MeasurementProfile,
}

impl Generation {
    pub fn path(&self) -> &Path {
        &self.generation.path
    }

    pub fn manifest(&self) -> &Manifest {
        &self.generation.manifest
    }

    pub fn profile(&self) -> &MeasurementProfile {
        &self.profile
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use openssl::pkey::Private;

    use super::*;

    fn public(key: &PKey<Private>) -> PKey<Public> {
        PKey::public_key_from_der(&key.public_key_to_der().unwrap()).unwrap()
    }

    #[test]
    fn descriptor_pins_keys_and_a_single_image_without_public_mode_overrides() {
        let overlay = crate::util::crypto::gen_nistp256().unwrap();
        let policy = crate::util::crypto::gen_nistp256().unwrap();
        for name in [
            "",
            ".",
            "..",
            "/image.rootfs",
            "../image.rootfs",
            "images/image.rootfs",
        ] {
            assert!(Descriptor::new(name.into(), public(&overlay), public(&policy)).is_err());
        }
        let descriptor =
            Descriptor::new("image.rootfs".into(), public(&overlay), public(&policy)).unwrap();
        let json = serde_json::to_value(&descriptor).unwrap();
        let decoded: Descriptor = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(
            decoded.rootfs_path(Path::new("/images")).unwrap(),
            Path::new("/images/image.rootfs")
        );
        assert!(decoded.overlay_key.public_eq(&overlay));
        assert!(decoded.policy_key.public_eq(&policy));
        let mut extra = json.clone();
        extra["enabled"] = serde_json::json!(false);
        assert!(serde_json::from_value::<Descriptor>(extra).is_err());
        let mut version = json;
        version["version"] = serde_json::json!(2);
        let unsupported: Descriptor = serde_json::from_value(version).unwrap();
        assert!(unsupported.rootfs_path(Path::new("/images")).is_err());
    }

    async fn prepare(
        config: &Path,
        rootfs: &Path,
        key: &PKey<Private>,
        content: &[u8],
    ) -> PreparedGeneration {
        let raw = tempfile::tempdir().unwrap();
        tokio::fs::write(raw.path().join("setting"), content)
            .await
            .unwrap();
        let store = generation_store(config);
        tokio::fs::create_dir_all(&store).await.unwrap();
        PreparedGeneration::import_raw(raw.path(), rootfs, &store, key)
            .await
            .unwrap()
    }

    fn timestamp(path: &Path, value: u64) {
        std::fs::File::open(path)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(value))
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "requires exclusively owned software TPM, tpm2-tools and veritysetup; no kernel mappings"]
    async fn software_tpm_selects_only_compatible_generations() {
        let directory = tempfile::tempdir().unwrap();
        let images = directory.path().join("images");
        let config = directory.path().join("config");
        tokio::fs::create_dir_all(&images).await.unwrap();
        let overlay_key = crate::util::crypto::gen_nistp256().unwrap();
        let authority = crate::util::crypto::gen_nistp256().unwrap();
        let descriptor = Descriptor::new(
            "image.rootfs".into(),
            public(&overlay_key),
            public(&authority),
        )
        .unwrap();
        let rootfs = descriptor.rootfs_path(&images).unwrap();
        tokio::fs::write(&rootfs, vec![0x42; 8192]).await.unwrap();
        let observed = tpm::read_measurements().await.unwrap();

        let first = prepare(&config, &rootfs, &overlay_key, b"first").await;
        let expected = observed
            .with_filesystems(
                &first.manifest().rootfs_sha256,
                &first.manifest().overlay_sha256,
            )
            .unwrap();
        let first_profile = MeasurementProfile::sign(expected, &authority)
            .await
            .unwrap();
        descriptor
            .stage_profile(&first, &first_profile)
            .await
            .unwrap();
        let first = first.publish().await.unwrap();
        timestamp(&first, 1);

        let other_boot = prepare(&config, &rootfs, &overlay_key, b"another boot").await;
        let mut expected = observed
            .with_filesystems(
                &other_boot.manifest().rootfs_sha256,
                &other_boot.manifest().overlay_sha256,
            )
            .unwrap();
        expected.applications[0] ^= 1;
        let other_profile = MeasurementProfile::sign(expected, &authority)
            .await
            .unwrap();
        descriptor
            .stage_profile(&other_boot, &other_profile)
            .await
            .unwrap();
        let other_boot = other_boot.publish().await.unwrap();
        timestamp(&other_boot, 2);
        assert_eq!(
            os_overlay::select_latest(&generation_store(&config), &rootfs, &public(&overlay_key))
                .await
                .unwrap()
                .unwrap()
                .path,
            other_boot
        );
        assert_eq!(
            descriptor
                .select(&images, &config)
                .await
                .unwrap()
                .unwrap()
                .path(),
            first
        );

        let substituted = prepare(
            &config,
            &rootfs,
            &overlay_key,
            b"unsigned profile substitution",
        )
        .await;
        let mut substituted_profile = first_profile.clone();
        substituted_profile.measurements = observed
            .with_filesystems(
                &substituted.manifest().rootfs_sha256,
                &substituted.manifest().overlay_sha256,
            )
            .unwrap();
        assert!(
            tpm::verify_authorization(&substituted_profile.authorization, &public(&authority))
                .unwrap()
        );
        assert!(
            descriptor
                .stage_profile(&substituted, &substituted_profile)
                .await
                .is_err()
        );
        let policy_path = substituted
            .staging_path()
            .join(POLICIES_DIR)
            .join("tampered.json");
        write_file_private_atomic(
            &policy_path,
            serde_json::to_vec(&substituted_profile).unwrap(),
        )
        .await
        .unwrap();
        let substituted = substituted.publish().await.unwrap();
        timestamp(&substituted, 3);
        assert_eq!(
            descriptor
                .select(&images, &config)
                .await
                .unwrap()
                .unwrap()
                .path(),
            first
        );

        let missing = prepare(&config, &rootfs, &overlay_key, b"missing policy")
            .await
            .publish()
            .await
            .unwrap();
        timestamp(&missing, 4);
        assert_eq!(
            descriptor
                .select(&images, &config)
                .await
                .unwrap()
                .unwrap()
                .path(),
            first
        );
        tokio::fs::create_dir_all(missing.join(POLICIES_DIR))
            .await
            .unwrap();
        tokio::fs::write(
            missing.join(POLICIES_DIR).join("malformed.json"),
            b"not JSON",
        )
        .await
        .unwrap();
        tokio::fs::write(
            missing.join(POLICIES_DIR).join("oversized.json"),
            vec![0; MAX_PROFILE_BYTES + 1],
        )
        .await
        .unwrap();
        assert_eq!(
            descriptor
                .select(&images, &config)
                .await
                .unwrap()
                .unwrap()
                .path(),
            first
        );

        let malformed_signature =
            prepare(&config, &rootfs, &overlay_key, b"malformed signature").await;
        let expected = observed
            .with_filesystems(
                &malformed_signature.manifest().rootfs_sha256,
                &malformed_signature.manifest().overlay_sha256,
            )
            .unwrap();
        let mut malformed_profile = MeasurementProfile::sign(expected, &authority)
            .await
            .unwrap();
        malformed_profile.authorization.signature = vec![0];
        assert!(
            descriptor
                .stage_profile(&malformed_signature, &malformed_profile)
                .await
                .is_err()
        );
        write_file_private_atomic(
            malformed_signature
                .staging_path()
                .join(POLICIES_DIR)
                .join("malformed-signature.json"),
            serde_json::to_vec(&malformed_profile).unwrap(),
        )
        .await
        .unwrap();
        let malformed_signature = malformed_signature.publish().await.unwrap();
        timestamp(&malformed_signature, 5);
        assert_eq!(
            descriptor
                .select(&images, &config)
                .await
                .unwrap()
                .unwrap()
                .path(),
            first
        );

        let staging = prepare(&config, &rootfs, &overlay_key, b"unpublished").await;
        let expected = observed
            .with_filesystems(
                &staging.manifest().rootfs_sha256,
                &staging.manifest().overlay_sha256,
            )
            .unwrap();
        let profile = MeasurementProfile::sign(expected, &authority)
            .await
            .unwrap();
        descriptor.stage_profile(&staging, &profile).await.unwrap();
        let selected = descriptor.select(&images, &config).await.unwrap().unwrap();
        assert_eq!(selected.path(), first);
        assert_eq!(
            selected.profile().measurements.files,
            first_profile.measurements.files
        );

        let wrong_key = crate::util::crypto::gen_nistp256().unwrap();
        let wrong = Descriptor::new(
            "image.rootfs".into(),
            public(&overlay_key),
            public(&wrong_key),
        )
        .unwrap();
        assert!(wrong.select(&images, &config).await.unwrap().is_none());
        let wrong = Descriptor::new(
            "image.rootfs".into(),
            public(&wrong_key),
            public(&authority),
        )
        .unwrap();
        assert!(wrong.select(&images, &config).await.unwrap().is_none());

        tokio::fs::write(&rootfs, vec![0x43; 8192]).await.unwrap();
        assert!(descriptor.select(&images, &config).await.unwrap().is_none());
        tokio::fs::write(&rootfs, vec![0x42; 8192]).await.unwrap();
        let manifest = selected.manifest();
        tpm::measure_filesystems(&tpm::filesystem_event(
            &manifest.rootfs_sha256,
            &manifest.overlay_sha256,
        ))
        .await
        .unwrap();
        assert!(descriptor.select(&images, &config).await.is_err());
    }
}
