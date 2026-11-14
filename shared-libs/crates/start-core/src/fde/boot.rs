//! Signed filesystem inputs and boot-policy admission pinned by the UKI.
//!
//! Descriptors belong inside the authenticated initramfs. Public policy files
//! cannot replace these keys or select the boot mode. Selection does not stabilize
//! paths: the boot owner must open and fully read both signed verity mappings before
//! measuring filesystems or releasing bootstrap credentials, then retain the mappings
//! until every dependent mount is gone. Data-volume credentials are not handled here.

use std::path::{Path, PathBuf};

use openssl::pkey::{PKey, Public};
use serde::{Deserialize, Serialize};

use super::credentials::{Credentials, SEALED_CREDENTIALS_FILE, SealedCredentials};
use super::memory::InitramfsMemory;
use super::tpm::{self, MeasurementProfile};
use crate::disk::mount::guard::GenericMountGuard;
use crate::os_overlay::{self, Manifest, PreparedGeneration, VerifiedGeneration, VerityMount};
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::util::future::complete_owned_operation;
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
            let Ok(mut policies) =
                os_overlay::read_generation_directory(&generation.path.join(POLICIES_DIR)).await
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

    /// Mounts authenticated public build providers independently of live TPM state.
    /// Source admission grants neither bootstrap release nor configuration-provider authority.
    /// Cancellation retains setup; the caller reserves mountpoints through abandoned-result cleanup.
    pub async fn mount_source(
        &self,
        images: &Path,
        config: &Path,
        mountpoint: &Path,
    ) -> Result<Option<super::source::BuildSource>, Error> {
        let (descriptor, images, config, mountpoint) = (
            self.clone(),
            images.to_owned(),
            config.to_owned(),
            mountpoint.to_owned(),
        );
        complete_owned_operation(async move {
            descriptor
                .mount_source_inner(&images, &config, &mountpoint)
                .await
        })
        .await
    }

    async fn mount_source_inner(
        &self,
        images: &Path,
        config: &Path,
        mountpoint: &Path,
    ) -> Result<Option<super::source::BuildSource>, Error> {
        let rootfs = self.rootfs_path(images)?;
        let Some(generation) =
            os_overlay::select_latest(&generation_store(config), &rootfs, &self.overlay_key)
                .await?
        else {
            return Ok(None);
        };
        let verified = generation
            .mount_rootfs(&rootfs, &mountpoint.join("rootfs"))
            .await?;
        super::source::BuildSource::mount(
            verified,
            &mountpoint.join("runtime"),
            &mountpoint.join("source"),
        )
        .await
        .map(Some)
    }

    /// Opens and fully reads both signed mappings before exposing read-only mounts.
    /// The mount directory is private to the boot owner; dependent mounts must end first.
    pub async fn mount(
        &self,
        images: &Path,
        config: &Path,
        mountpoint: &Path,
    ) -> Result<Option<MountedGeneration>, Error> {
        let Some(generation) = self.select(images, config).await? else {
            return Ok(None);
        };
        let rootfs = generation
            .generation
            .mount_rootfs(&self.rootfs_path(images)?, &mountpoint.join("rootfs"))
            .await?;
        let overlay = generation
            .manifest()
            .overlay_verity
            .mount(
                &generation.generation.overlay_path(),
                &generation.path().join(os_overlay::OVERLAY_VERITY_FILE),
                &format!("startos-fde-config-{}", Guid::new()),
                &mountpoint.join("overlay"),
            )
            .await;
        match overlay {
            Ok(overlay) => Ok(Some(MountedGeneration {
                generation,
                rootfs,
                overlay,
                authority: self.policy_key.0.clone(),
            })),
            Err(error) => {
                rootfs.unmount().await.log_err();
                Err(error)
            }
        }
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

/// Fully read read-only filesystems retaining verity protection through handoff.
#[derive(Debug)]
pub struct MountedGeneration {
    generation: Generation,
    rootfs: VerityMount,
    overlay: VerityMount,
    authority: PKey<Public>,
}

impl MountedGeneration {
    pub fn generation(&self) -> &Generation {
        &self.generation
    }

    pub fn rootfs(&self) -> &Path {
        self.rootfs.path()
    }

    pub fn overlay(&self) -> &Path {
        self.overlay.path()
    }

    /// Extends PCR15 after checking that the live boot still matches this authorization.
    pub async fn measure(&self) -> Result<(), Error> {
        let manifest = self.generation.manifest();
        let expected = tpm::read_measurements()
            .await?
            .with_filesystems(&manifest.rootfs_sha256, &manifest.overlay_sha256)?;
        if expected != self.generation.profile.measurements {
            return Err(invalid());
        }
        tpm::measure_filesystems(&tpm::filesystem_event(
            &manifest.rootfs_sha256,
            &manifest.overlay_sha256,
        ))
        .await
    }

    /// Measures the protected filesystems before releasing bootstrap identities.
    pub async fn release_bootstrap(
        &self,
        memory: &InitramfsMemory,
        runtime: &Path,
    ) -> Result<Credentials, Error> {
        memory.check(runtime)?;
        let bytes = tokio::fs::read(self.overlay().join(SEALED_CREDENTIALS_FILE)).await?;
        let sealed: SealedCredentials =
            serde_json::from_slice(&bytes).with_kind(ErrorKind::Deserialization)?;
        self.measure().await?;
        sealed
            .unseal(&self.generation.profile.authorization, &self.authority)
            .await
    }

    /// Transfers protected mount lifetime to the boot owner; dependent mounts must end first.
    pub fn persist(self) -> (PathBuf, PathBuf) {
        (self.rootfs.persist(), self.overlay.persist())
    }

    pub async fn unmount(self) -> Result<(), Error> {
        let overlay = self.overlay.unmount().await;
        let rootfs = self.rootfs.unmount().await;
        overlay?;
        rootfs
    }
}

impl GenericMountGuard for MountedGeneration {
    fn path(&self) -> &Path {
        self.rootfs()
    }

    async fn unmount(self) -> Result<(), Error> {
        MountedGeneration::unmount(self).await
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use openssl::pkey::Private;

    use super::*;
    use crate::util::Invoke;

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

    #[cfg(target_os = "linux")]
    fn boot_mappings() -> Vec<std::ffi::OsString> {
        let mut names = std::fs::read_dir("/dev/mapper")
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name.as_encoded_bytes().starts_with(b"startos-fde-"))
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    #[cfg(target_os = "linux")]
    async fn mounted_fixture() -> (
        tempfile::TempDir,
        MountedGeneration,
        tpm::SealedKey,
        PKey<Public>,
    ) {
        let temp = tempfile::tempdir_in("/dev/shm").unwrap();
        let images = temp.path().join("images");
        let config = temp.path().join("config");
        let base = temp.path().join("base");
        std::fs::create_dir(&images).unwrap();
        std::fs::create_dir(&base).unwrap();
        std::fs::write(base.join("setting"), b"public default").unwrap();
        std::fs::write(base.join("public-only"), b"base content").unwrap();
        std::fs::create_dir_all(config.join("overlay")).unwrap();
        std::fs::write(config.join("overlay/unsigned"), b"old mutable state").unwrap();
        let rootfs = images.join("image.rootfs");
        let mut file = crate::util::io::create_file_mod(&rootfs, 0o600)
            .await
            .unwrap();
        crate::util::squashfs::Squashfs::from_directory(&base)
            .await
            .unwrap()
            .serialize(&mut file)
            .await
            .unwrap();
        file.sync_all().await.unwrap();
        let overlay_key = crate::util::crypto::gen_nistp256().unwrap();
        let authority = crate::util::crypto::gen_nistp256().unwrap();
        let descriptor = Descriptor::new(
            "image.rootfs".into(),
            public(&overlay_key),
            public(&authority),
        )
        .unwrap();
        let raw = temp.path().join("raw");
        std::fs::create_dir(&raw).unwrap();
        std::fs::write(raw.join("setting"), b"configuration override").unwrap();
        let snapshot =
            SealedCredentials::seal(&super::super::credentials::test_credentials(), &authority)
                .await
                .unwrap();
        let snapshot_path = raw.join(SEALED_CREDENTIALS_FILE);
        std::fs::create_dir_all(snapshot_path.parent().unwrap()).unwrap();
        std::fs::write(snapshot_path, serde_json::to_vec(&snapshot).unwrap()).unwrap();
        std::fs::write(
            temp.path().join("descriptor.json"),
            serde_json::to_vec(&descriptor).unwrap(),
        )
        .unwrap();
        std::fs::create_dir_all(generation_store(&config)).unwrap();
        let prepared =
            PreparedGeneration::import_raw(&raw, &rootfs, &generation_store(&config), &overlay_key)
                .await
                .unwrap();
        let expected = tpm::read_measurements()
            .await
            .unwrap()
            .with_filesystems(
                &prepared.manifest().rootfs_sha256,
                &prepared.manifest().overlay_sha256,
            )
            .unwrap();
        let profile = MeasurementProfile::sign(expected, &authority)
            .await
            .unwrap();
        descriptor.stage_profile(&prepared, &profile).await.unwrap();
        prepared.publish().await.unwrap();
        let sealed = tpm::seal(&[42; 32], &authority).await.unwrap();
        let mountpoint = temp.path().join("mounted");
        std::fs::create_dir(&mountpoint).unwrap();
        let before = boot_mappings();
        std::fs::write(mountpoint.join("overlay"), b"occupied").unwrap();
        assert!(
            descriptor
                .mount(&images, &config, &mountpoint)
                .await
                .is_err()
        );
        assert_eq!(boot_mappings(), before);
        assert!(
            !crate::disk::mount::util::is_mountpoint(&mountpoint.join("rootfs"))
                .await
                .unwrap()
        );
        assert_eq!(tpm::read_measurements().await.unwrap().files, [0; 32]);
        std::fs::remove_file(mountpoint.join("overlay")).unwrap();
        let mounted = descriptor
            .mount(&images, &config, &mountpoint)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            std::fs::read(mounted.rootfs().join("setting")).unwrap(),
            b"public default"
        );
        assert_eq!(
            std::fs::read(mounted.overlay().join("setting")).unwrap(),
            b"configuration override"
        );
        assert!(std::fs::write(mounted.overlay().join("setting"), b"mutable").is_err());
        (temp, mounted, sealed, public(&authority))
    }

    #[cfg(target_os = "linux")]
    struct SourceFixture {
        temp: tempfile::TempDir,
        descriptor: Descriptor,
        prepared: PreparedGeneration,
        authority: PKey<openssl::pkey::Private>,
    }

    #[cfg(target_os = "linux")]
    async fn source_fixture() -> SourceFixture {
        let image =
            std::env::var_os("FDE_SOURCE_IMAGE").expect("Provide genuine OS SquashFS image");
        let temp = tempfile::tempdir_in("/var/tmp").unwrap();
        let images = temp.path().join("images");
        let config = temp.path().join("config");
        let raw = temp.path().join("raw");
        std::fs::create_dir(&images).unwrap();
        std::fs::create_dir_all(raw.join("usr/bin")).unwrap();
        std::fs::write(raw.join("usr/bin/node"), b"configuration provider override").unwrap();
        let rootfs_name = "image.rootfs";
        let rootfs = images.join(rootfs_name);
        std::fs::copy(&image, &rootfs).unwrap();
        let key = crate::util::crypto::gen_nistp256().unwrap();
        let authority = crate::util::crypto::gen_nistp256().unwrap();
        let descriptor =
            Descriptor::new(rootfs_name.into(), public(&key), public(&authority)).unwrap();
        std::fs::create_dir_all(generation_store(&config)).unwrap();
        let prepared =
            PreparedGeneration::import_raw(&raw, &rootfs, &generation_store(&config), &key)
                .await
                .unwrap();
        SourceFixture {
            temp,
            descriptor,
            prepared,
            authority,
        }
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires genuine FDE_SOURCE_IMAGE and FDE_SOURCE_MOUNT_GATE with native mount gate on PATH"]
    async fn cancelled_source_admission_reclaims_completed_native_runtime_mount() {
        let SourceFixture {
            temp,
            descriptor,
            prepared,
            ..
        } = source_fixture().await;
        prepared.publish().await.unwrap();
        let gate = PathBuf::from(
            std::env::var_os("FDE_SOURCE_MOUNT_GATE").expect("Provide native mount gate directory"),
        );
        let images = temp.path().join("images");
        let config = temp.path().join("config");
        let mounted_at = temp.path().join("cancelled-source");
        let runtime = mounted_at.join("runtime");
        std::fs::write(gate.join("target"), runtime.as_os_str().as_encoded_bytes()).unwrap();
        tokio::process::Command::new("mkfifo")
            .arg(gate.join("release"))
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
        let before = boot_mappings();
        let pending =
            tokio::spawn(
                async move { descriptor.mount_source(&images, &config, &mounted_at).await },
            );
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            while !gate.join("entered").exists() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(
            crate::disk::mount::util::is_mountpoint(&runtime)
                .await
                .unwrap()
        );
        pending.abort();
        assert!(matches!(pending.await, Err(error) if error.is_cancelled()));
        tokio::fs::write(gate.join("release"), b"\n").await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                if boot_mappings() == before
                    && !crate::disk::mount::util::is_mountpoint(&runtime)
                        .await
                        .unwrap()
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        for name in ["source", "runtime", "rootfs"] {
            assert!(
                !crate::disk::mount::util::is_mountpoint(runtime.parent().unwrap().join(name))
                    .await
                    .unwrap()
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires software TPM, kernel verity/mounts and a genuine FDE_SOURCE_IMAGE"]
    async fn verified_build_source_uses_public_providers_and_packaged_runtime_read_only() {
        let SourceFixture {
            temp,
            descriptor,
            prepared,
            authority,
        } = source_fixture().await;
        let images = temp.path().join("images");
        let config = temp.path().join("config");
        let rootfs = descriptor.rootfs_path(&images).unwrap();
        let expected = tpm::read_measurements()
            .await
            .unwrap()
            .with_filesystems(
                &prepared.manifest().rootfs_sha256,
                &prepared.manifest().overlay_sha256,
            )
            .unwrap();
        let profile = MeasurementProfile::sign(expected, &authority)
            .await
            .unwrap();
        descriptor.stage_profile(&prepared, &profile).await.unwrap();
        let generation = prepared.publish().await.unwrap();
        let mappings = boot_mappings();
        let mounted_at = temp.path().join("verified");
        let mounted = descriptor
            .mount(&images, &config, &mounted_at)
            .await
            .unwrap()
            .unwrap();
        mounted.measure().await.unwrap();
        mounted.unmount().await.unwrap();
        let measured = tpm::read_measurements().await.unwrap();
        assert_ne!(measured.files, [0; 32]);
        assert!(descriptor.select(&images, &config).await.is_err());
        std::fs::remove_dir_all(generation.join(POLICIES_DIR)).unwrap();
        let wrong = Descriptor::new(
            "image.rootfs".into(),
            public(&authority),
            public(&authority),
        )
        .unwrap();
        assert!(
            wrong
                .mount_source(&images, &config, &mounted_at)
                .await
                .unwrap()
                .is_none()
        );
        let runtime = mounted_at.join("runtime");
        let root = mounted_at.join("source");
        std::fs::write(&root, b"occupied").unwrap();
        assert!(
            descriptor
                .mount_source(&images, &config, &mounted_at)
                .await
                .is_err()
        );
        assert_eq!(boot_mappings(), mappings);
        assert!(
            !crate::disk::mount::util::is_mountpoint(&runtime)
                .await
                .unwrap()
        );
        std::fs::remove_file(&root).unwrap();
        let source = descriptor
            .mount_source(&images, &config, &mounted_at)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(boot_mappings().len(), mappings.len() + 1);
        assert!(std::fs::write(source.path().join("unexpected"), b"write").is_err());
        let script = PathBuf::from("/usr/share/nodejs/acorn/bin/acorn");
        let output = source
            .run_script(script.clone(), Vec::new(), b"42".to_vec())
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(parsed["body"][0]["expression"]["value"], 42);
        assert_eq!(boot_mappings(), mappings);
        let source = descriptor
            .mount_source(&images, &config, &mounted_at)
            .await
            .unwrap()
            .unwrap();
        assert!(
            source
                .run_script(script, vec!["--invalid-option".into()], Vec::new())
                .await
                .is_err()
        );
        assert_eq!(boot_mappings(), mappings);
        assert!(
            !crate::disk::mount::util::is_mountpoint(&root)
                .await
                .unwrap()
        );
        assert!(
            !crate::disk::mount::util::is_mountpoint(&runtime)
                .await
                .unwrap()
        );
        for scratch in ["dev", "run", "tmp", "var/tmp"] {
            assert!(
                !crate::disk::mount::util::is_mountpoint(root.join(scratch))
                    .await
                    .unwrap()
            );
        }
        assert_eq!(tpm::read_measurements().await.unwrap(), measured);
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&rootfs)
            .unwrap()
            .write_all(b"tamper")
            .unwrap();
        assert!(
            descriptor
                .mount_source(&images, &config, &mounted_at)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(boot_mappings(), mappings);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires exclusively owned software TPM, tpm2-tools, kernel verity and mounts"]
    async fn software_tpm_mounted_generation_measures_before_unseal_and_closes() {
        let (_temp, mounted, sealed, authority) = mounted_fixture().await;
        assert!(
            tpm::unseal(
                &sealed,
                &mounted.generation().profile().authorization,
                &authority
            )
            .await
            .is_err()
        );
        mounted.measure().await.unwrap();
        assert_eq!(
            tpm::read_measurements().await.unwrap(),
            mounted.generation().profile().measurements
        );
        assert_eq!(
            &*tpm::unseal(
                &sealed,
                &mounted.generation().profile().authorization,
                &authority
            )
            .await
            .unwrap(),
            &[42; 32]
        );
        assert!(mounted.measure().await.is_err());
        mounted.unmount().await.unwrap();
        assert!(boot_mappings().is_empty());
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires isolated initramfs-like root, software TPM, kernel verity and mounts"]
    async fn software_tpm_initramfs_prepares_protected_bootstrap_handoff() {
        use std::os::unix::fs::PermissionsExt;

        use crate::util::Invoke;
        let (temp, mounted, _sealed, _authority) = mounted_fixture().await;
        mounted.unmount().await.unwrap();
        let descriptor_path = Path::new(super::super::initramfs::DESCRIPTOR_FILE);
        std::fs::create_dir_all(descriptor_path.parent().unwrap()).unwrap();
        std::fs::copy(temp.path().join("descriptor.json"), descriptor_path).unwrap();
        let root = temp.path().join("running-root");
        super::super::initramfs::prepare(temp.path(), &root)
            .await
            .unwrap();
        let runtime = Path::new(super::super::initramfs::RUNTIME);
        let handoff_path = runtime.join(super::super::initramfs::HANDOFF_FILE);
        assert_eq!(
            std::fs::metadata(&handoff_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let handoff: super::super::initramfs::BootHandoff =
            serde_json::from_slice(&std::fs::read(&handoff_path).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(&handoff.credentials).unwrap(),
            serde_json::to_value(&super::super::credentials::test_credentials()).unwrap()
        );
        assert_eq!(
            std::fs::read(handoff.rootfs.join("setting")).unwrap(),
            b"public default"
        );
        assert_eq!(
            std::fs::read(handoff.overlay.join("setting")).unwrap(),
            b"configuration override"
        );
        let working = root.join("media/startos/config/overlay");
        assert_eq!(
            std::fs::read(root.join("setting")).unwrap(),
            b"configuration override"
        );
        assert_eq!(
            std::fs::read(root.join("public-only")).unwrap(),
            b"base content"
        );
        assert!(!root.join("unsigned").exists());
        assert!(!working.join("unsigned").exists());
        assert!(!working.join("public-only").exists());
        assert_eq!(
            std::fs::read(working.join("setting")).unwrap(),
            b"configuration override"
        );
        std::fs::remove_file(working.join("setting")).unwrap();
        std::fs::write(working.join("new-setting"), b"working edit").unwrap();
        assert_eq!(
            std::fs::read(root.join("setting")).unwrap(),
            b"configuration override"
        );
        assert!(!root.join("new-setting").exists());
        assert!(std::fs::write(root.join("media/startos/current/setting"), b"changed").is_err());
        assert!(
            std::fs::write(root.join("media/startos/root/readonly-check"), b"changed").is_err()
        );
        std::fs::write(temp.path().join("volume-still-writable"), b"yes").unwrap();
        let writer = std::fs::OpenOptions::new()
            .write(true)
            .open(working.join("new-setting"))
            .unwrap();
        let readonly = nix::mount::MsFlags::MS_REMOUNT | nix::mount::MsFlags::MS_RDONLY;
        assert_eq!(
            nix::mount::mount(None::<&str>, &working, None::<&str>, readonly, None::<&str>),
            Err(nix::errno::Errno::EBUSY)
        );
        drop(writer);
        nix::mount::mount(None::<&str>, &working, None::<&str>, readonly, None::<&str>).unwrap();
        assert!(std::fs::write(working.join("new-setting"), b"blocked").is_err());
        let signing_key = crate::util::crypto::gen_nistp256().unwrap();
        let snapshot = PreparedGeneration::prepare_working(
            &handoff.overlay,
            &handoff.working_upper,
            &temp.path().join("images/image.rootfs"),
            &generation_store(&temp.path().join("config")),
            &signing_key,
        )
        .await
        .unwrap();
        let manifest = snapshot.manifest().clone();
        let published = snapshot.publish().await.unwrap();
        let next = manifest
            .overlay_verity
            .mount(
                &published.join(os_overlay::OVERLAY_FILE),
                &published.join(os_overlay::OVERLAY_VERITY_FILE),
                &format!("snapshot-{}", Guid::new()),
                &temp.path().join("snapshot-view"),
            )
            .await
            .unwrap();
        assert!(!next.path().join("setting").exists());
        assert!(!next.path().join("public-only").exists());
        assert_eq!(
            std::fs::read(next.path().join("new-setting")).unwrap(),
            b"working edit"
        );
        assert_eq!(
            std::fs::read(root.join("setting")).unwrap(),
            b"configuration override"
        );
        next.unmount().await.unwrap();
        std::fs::write(root.join("setting"), b"running edit").unwrap();
        assert!(!working.join("setting").exists());
        assert_eq!(
            std::fs::read(handoff.overlay.join("setting")).unwrap(),
            b"configuration override"
        );
        assert_eq!(unsafe { libc::prctl(libc::PR_GET_DUMPABLE, 0, 0, 0, 0) }, 0);
        let options = tokio::process::Command::new("findmnt")
            .args(["-n", "-o", "OPTIONS", "--target"])
            .arg(runtime)
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
        assert!(
            String::from_utf8(options)
                .unwrap()
                .trim()
                .split(',')
                .any(|option| option == "noswap")
        );
        assert!(
            super::super::initramfs::prepare(temp.path(), &root)
                .await
                .is_err()
        );
        crate::disk::mount::util::unmount_all_under(&root, false)
            .await
            .unwrap();
        for path in [&handoff.overlay, &handoff.rootfs] {
            tokio::process::Command::new("umount")
                .arg(path)
                .invoke(ErrorKind::Filesystem)
                .await
                .unwrap();
        }
        for name in boot_mappings() {
            os_overlay::VerityImage::close(name.to_str().unwrap())
                .await
                .unwrap();
        }
        tokio::process::Command::new("umount")
            .arg(runtime)
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
        assert!(boot_mappings().is_empty());
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires isolated initramfs-like root, software TPM, kernel verity and mounts"]
    async fn software_tpm_initramfs_failure_publishes_no_handoff() {
        let (temp, mounted, _sealed, _authority) = mounted_fixture().await;
        mounted.unmount().await.unwrap();
        let descriptor_path = Path::new(super::super::initramfs::DESCRIPTOR_FILE);
        std::fs::create_dir_all(descriptor_path.parent().unwrap()).unwrap();
        std::fs::write(descriptor_path, b"invalid descriptor").unwrap();
        assert!(
            super::super::initramfs::prepare(temp.path(), &temp.path().join("running-root"))
                .await
                .is_err()
        );
        assert!(boot_mappings().is_empty());
        assert!(
            !Path::new(super::super::initramfs::RUNTIME)
                .join(super::super::initramfs::HANDOFF_FILE)
                .exists()
        );
        assert_eq!(tpm::read_measurements().await.unwrap().files, [0; 32]);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires isolated initramfs-like root, software TPM, kernel verity and mounts"]
    async fn software_tpm_root_composition_failure_unwinds_before_measurement() {
        let (temp, mounted, _sealed, _authority) = mounted_fixture().await;
        mounted.unmount().await.unwrap();
        let descriptor_path = Path::new(super::super::initramfs::DESCRIPTOR_FILE);
        std::fs::create_dir_all(descriptor_path.parent().unwrap()).unwrap();
        std::fs::copy(temp.path().join("descriptor.json"), descriptor_path).unwrap();
        let overlay = temp.path().join("config/overlay");
        std::fs::remove_dir_all(&overlay).unwrap();
        std::fs::write(&overlay, b"occupied").unwrap();
        let root = temp.path().join("running-root");
        assert!(
            super::super::initramfs::prepare(temp.path(), &root)
                .await
                .is_err()
        );
        assert!(
            !crate::disk::mount::util::is_mountpoint(&root)
                .await
                .unwrap()
        );
        assert!(boot_mappings().is_empty());
        assert!(
            !Path::new(super::super::initramfs::RUNTIME)
                .join(super::super::initramfs::HANDOFF_FILE)
                .exists()
        );
        assert_eq!(std::fs::read(overlay).unwrap(), b"occupied");
        assert_eq!(tpm::read_measurements().await.unwrap().files, [0; 32]);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires exclusively owned software TPM, tpm2-tools, kernel verity and mounts"]
    async fn software_tpm_mounted_generation_rejects_boot_change_before_measurement() {
        use crate::util::Invoke;
        let (_temp, mounted, _sealed, _authority) = mounted_fixture().await;
        tokio::process::Command::new("tpm2_pcrextend")
            .arg(format!("4:sha256={}", hex::encode([1; 32])))
            .invoke(ErrorKind::Bios)
            .await
            .unwrap();
        assert!(mounted.measure().await.is_err());
        assert_eq!(tpm::read_measurements().await.unwrap().files, [0; 32]);
        mounted.unmount().await.unwrap();
        assert!(boot_mappings().is_empty());
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
