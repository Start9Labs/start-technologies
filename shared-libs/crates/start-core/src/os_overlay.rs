//! Signed immutable configuration overlays bound to a complete rootfs image.
//!
//! Writers must serialize preparation, publication and pruning for each store. The
//! configuration sources and rootfs must remain quiescent during preparation.
//! Raw imports retain root-deletion markers. Working snapshots combine the raw
//! config-only lower with its RAM upper: upper deletions remove overrides, while
//! untouched lower root-deletion markers survive unrelated edits. The public
//! rootfs is excluded from configuration snapshots.
//!
//! Selection authenticates full file contents. Callers must keep verified contents
//! stable through use, using the signed dm-verity roots or a RAM copy; a returned
//! path is not protection against subsequent writes. Timestamps select among valid generations
//! and provide no antirollback guarantee. Signatures do not encrypt secrets.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use openssl::hash::MessageDigest;
use openssl::pkey::{HasPublic, PKey, PKeyRef, Private, Public};
use openssl::sign::{Signer, Verifier};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::disk::mount::filesystem::block_dev::BlockDev;
use crate::disk::mount::filesystem::{FileSystem, ReadOnly};
use crate::disk::mount::guard::{GenericMountGuard, MountGuard};
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::s9pk::merkle_archive::source::DynFileSource;
pub use crate::util::crypto::gen_nistp256 as generate_signing_key;
use crate::util::io::{create_file_mod, open_file};
use crate::util::squashfs::Squashfs;
use crate::util::{HashWriter, Invoke};

const DOMAIN: &[u8] = b"StartOS configuration overlay\0";
pub const MANIFEST_VERSION: u32 = 1;
const MANIFEST_LEN: usize = DOMAIN.len() + 4 + 32 + 32 + 40 + 40;
const GENERATION_PREFIX: &str = "generation-";
pub const OVERLAY_FILE: &str = "overlay.squashfs";
pub const MANIFEST_FILE: &str = "manifest.bin";
pub const SIGNATURE_FILE: &str = "manifest.sig";
pub const ROOTFS_VERITY_FILE: &str = "rootfs.verity";
pub const OVERLAY_VERITY_FILE: &str = "overlay.verity";
pub const VERITY_BLOCK_SIZE: u64 = 4096;

/// dm-verity format 1, SHA-256, 4096-byte data/hash blocks, empty salt, no superblock.
///
/// Runtime mappings must use these fixed parameters, this signed block count and
/// root hash. The caller owns mapping lifetime and fail-closed corruption handling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerityImage {
    pub root_hash: [u8; 32],
    pub data_blocks: u64,
}

fn verity_command() -> tokio::process::Command {
    let mut command = tokio::process::Command::new("veritysetup");
    command.env("LC_ALL", "C").kill_on_drop(true);
    command
}

fn verity_options(command: &mut tokio::process::Command, blocks: u64) {
    command
        .args(["--no-superblock", "--format=1", "--hash=sha256", "--salt=-"])
        .arg(format!("--data-block-size={VERITY_BLOCK_SIZE}"))
        .arg(format!("--hash-block-size={VERITY_BLOCK_SIZE}"))
        .arg(format!("--data-blocks={blocks}"));
}

impl VerityImage {
    /// Builds a separate regular-file hash tree for a stable, block-aligned image.
    /// The hash-tree destination must be fresh and outside the data image.
    pub async fn format(data: &Path, tree: &Path) -> Result<Self, Error> {
        let len = regular_file(data).await?.metadata().await?.len();
        if len == 0 || len % VERITY_BLOCK_SIZE != 0 {
            return Err(invalid_signature());
        }
        tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(tree)
            .await?;
        let data_blocks = len / VERITY_BLOCK_SIZE;
        let mut command = verity_command();
        command.arg("format").arg(data).arg(tree);
        verity_options(&mut command, data_blocks);
        let output = command.invoke(ErrorKind::Filesystem).await?;
        let text = std::str::from_utf8(&output).with_kind(ErrorKind::Utf8)?;
        let root = text
            .lines()
            .find_map(|line| line.strip_prefix("Root hash:").map(str::trim))
            .ok_or_else(invalid_signature)?;
        let root_hash = hex::decode(root)
            .with_kind(ErrorKind::Deserialization)?
            .try_into()
            .map_err(|_| invalid_signature())?;
        let image = Self {
            root_hash,
            data_blocks,
        };
        // A single data block still requires a nonempty kernel hash device.
        if regular_file(tree).await?.metadata().await?.len() == 0 {
            tokio::fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(tree)
                .await?
                .set_len(VERITY_BLOCK_SIZE)
                .await?;
        }
        image.verify(data, tree).await?;
        regular_file(tree).await?.sync_all().await?;
        Ok(image)
    }

    async fn check_images(&self, data: &Path, tree: &Path) -> Result<(), Error> {
        let len = regular_file(data).await?.metadata().await?.len();
        if self.data_blocks == 0 || self.data_blocks.checked_mul(VERITY_BLOCK_SIZE) != Some(len) {
            return Err(invalid_signature());
        }
        if regular_file(tree).await?.metadata().await?.len() < VERITY_BLOCK_SIZE {
            return Err(invalid_signature());
        }
        Ok(())
    }

    /// Checks the complete data image and hash tree against this signed root.
    pub async fn verify(&self, data: &Path, tree: &Path) -> Result<(), Error> {
        self.check_images(data, tree).await?;
        let mut command = verity_command();
        command
            .arg("verify")
            .arg(data)
            .arg(tree)
            .arg(hex::encode(self.root_hash));
        verity_options(&mut command, self.data_blocks);
        command.invoke(ErrorKind::InvalidSignature).await?;
        Ok(())
    }

    /// Opens and fully reads a read-only mapping against the signed verity root.
    /// Later corruption returns I/O errors. The caller owns the name until all mounts
    /// are unmounted and must close it; failed initial reads close their new mapping.
    pub async fn open(
        &self,
        data: &Path,
        tree: &Path,
        name: &str,
    ) -> Result<BlockDev<PathBuf>, Error> {
        if Path::new(name).file_name().and_then(|name| name.to_str()) != Some(name) {
            return Err(Error::new(
                eyre!(t!("error.invalid-request")),
                ErrorKind::InvalidRequest,
            ));
        }
        self.check_images(data, tree).await?;
        let mut command = verity_command();
        command.arg("open");
        verity_options(&mut command, self.data_blocks);
        command
            .arg("--")
            .arg(data)
            .arg(name)
            .arg(tree)
            .arg(hex::encode(self.root_hash));
        command.invoke(ErrorKind::Filesystem).await?;
        let device = Path::new("/dev/mapper").join(name);
        let result = async {
            let read =
                tokio::io::copy(&mut open_file(&device).await?, &mut tokio::io::sink()).await?;
            if Some(read) != self.data_blocks.checked_mul(VERITY_BLOCK_SIZE) {
                return Err(invalid_signature());
            }
            Ok(BlockDev::new(device))
        }
        .await;
        if result.is_err() {
            Self::close(name).await.log_err();
        }
        result
    }

    /// Mounts a fully read mapping as read-only SquashFS; the guard owns both resources.
    pub async fn mount(
        &self,
        data: &Path,
        tree: &Path,
        name: &str,
        mountpoint: &Path,
    ) -> Result<VerityMount, Error> {
        let device = self.open(data, tree, name).await?;
        match MountGuard::mount(&SquashfsDevice(device), mountpoint, ReadOnly).await {
            Ok(mount) => Ok(VerityMount {
                mount,
                mapping: Some(name.to_owned()),
            }),
            Err(error) => {
                Self::close(name).await.log_err();
                Err(error)
            }
        }
    }

    /// Requires every mount using the mapping to be unmounted first.
    pub async fn close(name: &str) -> Result<(), Error> {
        verity_command()
            .arg("close")
            .arg("--")
            .arg(name)
            .invoke(ErrorKind::Filesystem)
            .await?;
        Ok(())
    }
}

struct SquashfsDevice(BlockDev<PathBuf>);

impl FileSystem for SquashfsDevice {
    fn mount_type(&self) -> Option<impl AsRef<str>> {
        Some("squashfs")
    }

    async fn source(&self) -> Result<Option<impl AsRef<Path>>, Error> {
        self.0.source().await
    }

    async fn source_hash(&self) -> Result<digest::Output<Sha256>, Error> {
        self.0.source_hash().await
    }
}

/// A read-only mount retaining its verified mapping until successful unmount.
/// Failed teardown retains mapping protection.
#[derive(Debug)]
pub struct VerityMount {
    mount: MountGuard,
    mapping: Option<String>,
}

impl VerityMount {
    /// Transfers mount and mapping lifetime to the caller without removing protection.
    pub fn persist(mut self) -> PathBuf {
        self.mapping.take();
        self.mount.take().persist()
    }
}

impl GenericMountGuard for VerityMount {
    fn path(&self) -> &Path {
        self.mount.path()
    }

    async fn unmount(mut self) -> Result<(), Error> {
        let name = self.mapping.take().unwrap();
        self.mount.take().unmount(false).await?;
        VerityImage::close(&name).await
    }
}

impl Drop for VerityMount {
    fn drop(&mut self) {
        if let Some(name) = self.mapping.take() {
            let mount = self.mount.take();
            tokio::spawn(async move {
                match mount.unmount(false).await {
                    Ok(()) => VerityImage::close(&name).await.log_err(),
                    Err(error) => Err::<(), _>(error).log_err(),
                };
            });
        }
    }
}

/// The signed binding between complete rootfs and overlay file contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub rootfs_sha256: [u8; 32],
    pub overlay_sha256: [u8; 32],
    pub rootfs_verity: VerityImage,
    pub overlay_verity: VerityImage,
}

fn invalid_signature() -> Error {
    Error::new(
        eyre!(t!("error.invalid-signature")),
        ErrorKind::InvalidSignature,
    )
}

fn require_p256<T: HasPublic>(key: &PKeyRef<T>) -> Result<(), Error> {
    if key
        .ec_key()
        .with_kind(ErrorKind::OpenSsl)?
        .group()
        .curve_name()
        != Some(crate::util::crypto::NIST_P256_CURVE)
    {
        return Err(invalid_signature());
    }
    Ok(())
}

impl Manifest {
    /// Canonical signed bytes: domain, big-endian version, full-file SHA-256 digests,
    /// then rootfs/overlay verity roots with their big-endian block counts.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(MANIFEST_LEN);
        bytes.extend_from_slice(DOMAIN);
        bytes.extend_from_slice(&MANIFEST_VERSION.to_be_bytes());
        bytes.extend_from_slice(&self.rootfs_sha256);
        bytes.extend_from_slice(&self.overlay_sha256);
        for image in [&self.rootfs_verity, &self.overlay_verity] {
            bytes.extend_from_slice(&image.root_hash);
            bytes.extend_from_slice(&image.data_blocks.to_be_bytes());
        }
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let header = DOMAIN.len() + 4;
        if bytes.len() != MANIFEST_LEN
            || !bytes.starts_with(DOMAIN)
            || bytes[DOMAIN.len()..header] != MANIFEST_VERSION.to_be_bytes()
        {
            return Err(invalid_signature());
        }
        Ok(Self {
            rootfs_sha256: bytes[header..header + 32].try_into().unwrap(),
            overlay_sha256: bytes[header + 32..header + 64].try_into().unwrap(),
            rootfs_verity: VerityImage {
                root_hash: bytes[header + 64..header + 96].try_into().unwrap(),
                data_blocks: u64::from_be_bytes(
                    bytes[header + 96..header + 104].try_into().unwrap(),
                ),
            },
            overlay_verity: VerityImage {
                root_hash: bytes[header + 104..header + 136].try_into().unwrap(),
                data_blocks: u64::from_be_bytes(bytes[header + 136..].try_into().unwrap()),
            },
        })
    }

    /// Returns an ASN.1 DER ECDSA signature over the canonical manifest, using SHA-256.
    pub fn sign(&self, key: &PKeyRef<Private>) -> Result<Vec<u8>, Error> {
        require_p256(key)?;
        Signer::new(MessageDigest::sha256(), key)
            .with_kind(ErrorKind::OpenSsl)?
            .sign_oneshot_to_vec(&self.to_bytes())
            .with_kind(ErrorKind::OpenSsl)
    }

    pub fn verify(&self, signature: &[u8], key: &PKeyRef<Public>) -> Result<(), Error> {
        require_p256(key)?;
        let valid = Verifier::new(MessageDigest::sha256(), key)
            .with_kind(ErrorKind::OpenSsl)?
            .verify_oneshot(signature, &self.to_bytes())
            .unwrap_or(false);
        if !valid {
            return Err(invalid_signature());
        }
        Ok(())
    }
}

async fn regular_file(path: &Path) -> Result<tokio::fs::File, Error> {
    let file = tokio::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .await
        .with_kind(ErrorKind::Filesystem)?;
    if !file.metadata().await?.is_file() {
        return Err(invalid_signature());
    }
    Ok(file)
}

async fn digest_file(path: &Path) -> Result<[u8; 32], Error> {
    let mut file = regular_file(path).await?;
    let mut writer = HashWriter::new(Sha256::new(), tokio::io::sink());
    tokio::io::copy(&mut file, &mut writer).await?;
    Ok(writer.finish().0.finalize().into())
}

pub(crate) async fn read_generation_file(path: &Path, max: usize) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    regular_file(path)
        .await?
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() > max {
        return Err(invalid_signature());
    }
    Ok(bytes)
}

pub(crate) async fn read_manifest(path: &Path, key: &PKeyRef<Public>) -> Result<Manifest, Error> {
    let manifest = Manifest::from_bytes(
        &read_generation_file(&path.join(MANIFEST_FILE), MANIFEST_LEN).await?,
    )?;
    let signature = read_generation_file(&path.join(SIGNATURE_FILE), 72).await?;
    manifest.verify(&signature, key)?;
    Ok(manifest)
}

async fn verify_generation(
    path: &Path,
    rootfs: &Path,
    rootfs_sha256: &[u8; 32],
    key: &PKeyRef<Public>,
) -> Result<Manifest, Error> {
    let manifest = read_manifest(path, key).await?;
    if &manifest.rootfs_sha256 != rootfs_sha256
        || manifest.overlay_sha256 != digest_file(&path.join(OVERLAY_FILE)).await?
    {
        return Err(invalid_signature());
    }
    manifest
        .rootfs_verity
        .verify(rootfs, &path.join(ROOTFS_VERITY_FILE))
        .await?;
    manifest
        .overlay_verity
        .verify(&path.join(OVERLAY_FILE), &path.join(OVERLAY_VERITY_FILE))
        .await?;
    Ok(manifest)
}

/// A hidden, unpublished generation. Abandonment leaves it invisible to selection.
#[must_use]
pub struct PreparedGeneration {
    staging: PathBuf,
    destination: PathBuf,
    rootfs: PathBuf,
    manifest: Manifest,
    public_key: PKey<Public>,
}

impl PreparedGeneration {
    /// Imports a quiescent raw legacy configuration tree, retaining root-deletion markers.
    /// The durable store must be outside all source trees; rootfs must be block-aligned.
    pub async fn import_raw(
        raw: &Path,
        rootfs: &Path,
        store: &Path,
        key: &PKeyRef<Private>,
    ) -> Result<Self, Error> {
        Self::prepare_snapshot(
            Squashfs::from_directory(raw).await?.into_dyn(),
            rootfs,
            store,
            key,
        )
        .await
    }

    /// Commits a quiescent config-only lower plus RAM upper, not the public rootfs.
    ///
    /// The working mount must disable index, metacopy and redirect_dir. Upper
    /// whiteouts remove overrides; untouched lower root-deletion markers survive.
    /// Sources must remain stable through preparation; rootfs must be block-aligned.
    pub async fn prepare_working(
        lower: &Path,
        upper: &Path,
        rootfs: &Path,
        store: &Path,
        key: &PKeyRef<Private>,
    ) -> Result<Self, Error> {
        Self::prepare_snapshot(
            Squashfs::from_working_overlay(lower, upper).await?,
            rootfs,
            store,
            key,
        )
        .await
    }

    async fn prepare_snapshot(
        snapshot: Squashfs<DynFileSource>,
        rootfs: &Path,
        store: &Path,
        key: &PKeyRef<Private>,
    ) -> Result<Self, Error> {
        require_p256(key)?;
        let id = Guid::new();
        let staging = store.join(format!(".staging-{id}"));
        tokio::fs::DirBuilder::new()
            .mode(0o700)
            .create(&staging)
            .await
            .with_kind(ErrorKind::Filesystem)?;
        let mut overlay = create_file_mod(staging.join(OVERLAY_FILE), 0o600).await?;
        snapshot.serialize(&mut overlay).await?;
        overlay.sync_all().await?;
        let manifest = Manifest {
            rootfs_sha256: digest_file(rootfs).await?,
            overlay_sha256: digest_file(&staging.join(OVERLAY_FILE)).await?,
            rootfs_verity: VerityImage::format(rootfs, &staging.join(ROOTFS_VERITY_FILE)).await?,
            overlay_verity: VerityImage::format(
                &staging.join(OVERLAY_FILE),
                &staging.join(OVERLAY_VERITY_FILE),
            )
            .await?,
        };
        for (name, bytes) in [
            (MANIFEST_FILE, manifest.to_bytes()),
            (SIGNATURE_FILE, manifest.sign(key)?),
        ] {
            let mut file = create_file_mod(staging.join(name), 0o600).await?;
            file.write_all(&bytes).await?;
            file.sync_all().await?;
        }
        open_file(staging.join(OVERLAY_FILE))
            .await?
            .sync_all()
            .await?;
        open_file(&staging).await?.sync_all().await?;
        open_file(store).await?.sync_all().await?;
        Ok(Self {
            staging,
            destination: store.join(format!("{GENERATION_PREFIX}{id}")),
            rootfs: rootfs.to_owned(),
            manifest,
            public_key: PKey::public_key_from_der(
                &key.public_key_to_der().with_kind(ErrorKind::OpenSsl)?,
            )
            .with_kind(ErrorKind::OpenSsl)?,
        })
    }

    /// Additional TPM authorization files may be added here before publication.
    ///
    /// The caller must leave the signed artifacts intact and finish all writes
    /// before publishing. Additional files must be regular files or directories;
    /// their authentication belongs to the caller's TPM policy protocol.
    pub fn staging_path(&self) -> &Path {
        &self.staging
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Syncs all staged files and atomically publishes the complete generation.
    ///
    /// Returns after the store directory is synced. An I/O failure after rename
    /// can leave a visible generation; callers must reconcile before retrying.
    /// Pruning is a separate operation and cannot change this method's result.
    pub async fn publish(self) -> Result<PathBuf, Error> {
        verify_generation(
            &self.staging,
            &self.rootfs,
            &digest_file(&self.rootfs).await?,
            &self.public_key,
        )
        .await?;
        sync_tree(&self.staging).await?;
        // Publication time, rather than preparation time, orders generations.
        open_file(&self.staging)
            .await?
            .into_std()
            .await
            .set_modified(SystemTime::now())?;
        open_file(&self.staging).await?.sync_all().await?;
        tokio::fs::rename(&self.staging, &self.destination)
            .await
            .with_kind(ErrorKind::Filesystem)?;
        open_file(self.destination.parent().unwrap())
            .await?
            .sync_all()
            .await?;
        Ok(self.destination)
    }
}

fn sync_tree(path: &Path) -> impl Future<Output = Result<(), Error>> + Send {
    async move {
        let mut entries = tokio::fs::read_dir(path).await?;
        while let Some(entry) = entries.next_entry().await? {
            let kind = entry.file_type().await?;
            if kind.is_dir() {
                Box::pin(sync_tree(&entry.path())).await?;
            } else if kind.is_file() {
                regular_file(&entry.path()).await?.sync_all().await?;
            } else {
                return Err(invalid_signature());
            }
        }
        open_file(path).await?.sync_all().await?;
        Ok(())
    }
}

fn is_generation(name: &std::ffi::OsStr) -> bool {
    name.to_str()
        .and_then(|name| name.strip_prefix(GENERATION_PREFIX))
        .is_some_and(|id| Guid::from(id).is_some())
}

/// Authenticated file contents at verification time; callers own their stability.
#[derive(Debug)]
pub struct VerifiedGeneration {
    pub path: PathBuf,
    pub manifest: Manifest,
}

impl VerifiedGeneration {
    pub fn overlay_path(&self) -> PathBuf {
        self.path.join(OVERLAY_FILE)
    }
}

/// Selects the newest valid complete generation by directory modification time.
///
/// Hidden staging directories and invalid candidates are ignored. Equal timestamps
/// use the directory name as a deterministic tie-breaker. The rootfs is hashed in
/// full once; every candidate's overlay is hashed in full before it can be selected.
pub async fn select_latest(
    store: &Path,
    rootfs: &Path,
    key: &PKeyRef<Public>,
) -> Result<Option<VerifiedGeneration>, Error> {
    Ok(verified_generations(store, rootfs, key)
        .await?
        .into_iter()
        .next())
}

/// Returns complete authenticated generations, newest first with deterministic ties.
/// Returned paths require the same lifetime protection as `select_latest`.
pub async fn verified_generations(
    store: &Path,
    rootfs: &Path,
    key: &PKeyRef<Public>,
) -> Result<Vec<VerifiedGeneration>, Error> {
    require_p256(key)?;
    let rootfs_sha256 = digest_file(rootfs).await?;
    let mut entries = tokio::fs::read_dir(store).await?;
    let mut verified = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        if !is_generation(&entry.file_name()) || !entry.file_type().await?.is_dir() {
            continue;
        }
        let path = entry.path();
        let Ok(manifest) = verify_generation(&path, rootfs, &rootfs_sha256, key).await else {
            continue;
        };
        let timestamp = entry.metadata().await?.modified()?;
        verified.push((timestamp, VerifiedGeneration { path, manifest }));
    }
    verified.sort_by(|(left_time, left), (right_time, right)| {
        (right_time, &right.path).cmp(&(left_time, &left.path))
    });
    Ok(verified
        .into_iter()
        .map(|(_, generation)| generation)
        .collect())
}

/// Removes other published generation directories after a durable publication.
///
/// Retains by generation directory name. The caller must retain every generation
/// still needed by a mounted root or recovery policy. Hidden staging directories
/// are left alone, including active preparations. Serialize pruning with writers
/// and readers that still need paths.
pub async fn prune(store: &Path, retain: &[PathBuf]) -> Result<(), Error> {
    let mut entries = tokio::fs::read_dir(store).await?;
    while let Some(entry) = entries.next_entry().await? {
        if is_generation(&entry.file_name())
            && entry.file_type().await?.is_dir()
            && !retain
                .iter()
                .any(|path| path.file_name() == Some(entry.file_name().as_os_str()))
        {
            tokio::fs::remove_dir_all(entry.path()).await?;
        }
    }
    open_file(store).await?.sync_all().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
    use std::time::Duration;

    use openssl::ec::{EcGroup, EcKey};
    use openssl::nid::Nid;

    use super::*;

    fn public_key(key: &PKeyRef<Private>) -> PKey<Public> {
        PKey::public_key_from_pem(&key.public_key_to_pem().unwrap()).unwrap()
    }

    #[test]
    fn p256_key_export_and_manifest_signature() {
        let key = generate_signing_key().unwrap();
        let key = PKey::private_key_from_pem(&key.private_key_to_pem_pkcs8().unwrap()).unwrap();
        let public = public_key(&key);
        let manifest = Manifest {
            rootfs_sha256: [1; 32],
            overlay_sha256: [2; 32],
            rootfs_verity: VerityImage {
                root_hash: [3; 32],
                data_blocks: 2,
            },
            overlay_verity: VerityImage {
                root_hash: [4; 32],
                data_blocks: 3,
            },
        };
        let signature = manifest.sign(&key).unwrap();
        manifest.verify(&signature, &public).unwrap();
        assert_eq!(
            Manifest::from_bytes(&manifest.to_bytes()).unwrap(),
            manifest
        );
        assert!(
            manifest
                .verify(&signature, &public_key(&generate_signing_key().unwrap()))
                .is_err()
        );
        assert!(
            manifest
                .verify(&signature[..signature.len() - 1], &public)
                .is_err()
        );
        let mut changed = manifest.clone();
        changed.rootfs_sha256[0] ^= 1;
        assert!(changed.verify(&signature, &public).is_err());
        changed = manifest.clone();
        changed.overlay_sha256[0] ^= 1;
        assert!(changed.verify(&signature, &public).is_err());
        changed = manifest.clone();
        changed.rootfs_verity.root_hash[0] ^= 1;
        assert!(changed.verify(&signature, &public).is_err());
        changed = manifest.clone();
        changed.overlay_verity.data_blocks += 1;
        assert!(changed.verify(&signature, &public).is_err());

        let group = EcGroup::from_curve_name(Nid::SECP384R1).unwrap();
        let wrong_curve = PKey::from_ec_key(EcKey::generate(&group).unwrap()).unwrap();
        assert!(manifest.sign(&wrong_curve).is_err());
        assert!(
            manifest
                .verify(&signature, &public_key(&wrong_curve))
                .is_err()
        );
    }

    #[test]
    fn domain_version_and_noncanonical_manifest_rejected() {
        let manifest = Manifest {
            rootfs_sha256: [1; 32],
            overlay_sha256: [2; 32],
            rootfs_verity: VerityImage {
                root_hash: [3; 32],
                data_blocks: 2,
            },
            overlay_verity: VerityImage {
                root_hash: [4; 32],
                data_blocks: 3,
            },
        };
        let mut wrong_domain = manifest.to_bytes();
        wrong_domain[0] ^= 1;
        assert!(Manifest::from_bytes(&wrong_domain).is_err());
        let mut wrong_version = manifest.to_bytes();
        wrong_version[DOMAIN.len()..DOMAIN.len() + 4].copy_from_slice(&2u32.to_be_bytes());
        assert!(Manifest::from_bytes(&wrong_version).is_err());
        let mut trailing_data = manifest.to_bytes();
        trailing_data.push(0);
        assert!(Manifest::from_bytes(&trailing_data).is_err());
        assert!(Manifest::from_bytes(&[]).is_err());
        assert!(Manifest::from_bytes(&manifest.to_bytes()[..MANIFEST_LEN - 1]).is_err());
    }

    fn image_bytes(content: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0; 8192];
        bytes[..content.len()].copy_from_slice(content);
        bytes
    }

    async fn signed_candidate(
        store: &Path,
        rootfs: &Path,
        key: &PKeyRef<Private>,
        timestamp: u64,
    ) -> PathBuf {
        let path = store.join(format!("{GENERATION_PREFIX}{}", Guid::new()));
        fs::create_dir(&path).unwrap();
        fs::write(
            path.join(OVERLAY_FILE),
            image_bytes(b"complete overlay content"),
        )
        .unwrap();
        let manifest = Manifest {
            rootfs_sha256: digest_file(rootfs).await.unwrap(),
            overlay_sha256: digest_file(&path.join(OVERLAY_FILE)).await.unwrap(),
            rootfs_verity: VerityImage::format(rootfs, &path.join(ROOTFS_VERITY_FILE))
                .await
                .unwrap(),
            overlay_verity: VerityImage::format(
                &path.join(OVERLAY_FILE),
                &path.join(OVERLAY_VERITY_FILE),
            )
            .await
            .unwrap(),
        };
        fs::write(path.join(MANIFEST_FILE), manifest.to_bytes()).unwrap();
        fs::write(path.join(SIGNATURE_FILE), manifest.sign(key).unwrap()).unwrap();
        fs::File::open(&path)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(timestamp))
            .unwrap();
        path
    }

    #[tokio::test]
    async fn selection_hashes_actual_files_and_binds_rootfs() {
        let temp = tempfile::tempdir().unwrap();
        let rootfs = temp.path().join("rootfs");
        fs::write(&rootfs, image_bytes(b"full rootfs")).unwrap();
        let store = temp.path().join("store");
        fs::create_dir(&store).unwrap();
        let key = generate_signing_key().unwrap();
        let public = public_key(&key);
        let path = signed_candidate(&store, &rootfs, &key, 1).await;
        assert_eq!(
            select_latest(&store, &rootfs, &public)
                .await
                .unwrap()
                .unwrap()
                .path,
            path
        );
        fs::write(&rootfs, b"changed rootfs").unwrap();
        assert!(
            select_latest(&store, &rootfs, &public)
                .await
                .unwrap()
                .is_none()
        );
        fs::write(&rootfs, image_bytes(b"full rootfs")).unwrap();
        fs::write(path.join(OVERLAY_FILE), b"tampered overlay content").unwrap();
        assert!(
            select_latest(&store, &rootfs, &public)
                .await
                .unwrap()
                .is_none()
        );
        fs::write(
            path.join(OVERLAY_FILE),
            image_bytes(b"complete overlay content"),
        )
        .unwrap();
        fs::rename(path.join(OVERLAY_FILE), path.join("elsewhere")).unwrap();
        symlink("elsewhere", path.join(OVERLAY_FILE)).unwrap();
        assert!(
            select_latest(&store, &rootfs, &public)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn timestamp_selection_ignores_newer_invalid_and_hidden_candidates() {
        let temp = tempfile::tempdir().unwrap();
        let rootfs = temp.path().join("rootfs");
        fs::write(&rootfs, image_bytes(b"rootfs")).unwrap();
        let store = temp.path().join("store");
        fs::create_dir(&store).unwrap();
        let key = generate_signing_key().unwrap();
        let public = public_key(&key);
        let older = signed_candidate(&store, &rootfs, &key, 10).await;
        let newest = signed_candidate(&store, &rootfs, &key, 20).await;
        let incomplete = signed_candidate(&store, &rootfs, &key, 30).await;
        fs::remove_file(incomplete.join(SIGNATURE_FILE)).unwrap();
        let tampered = signed_candidate(&store, &rootfs, &key, 40).await;
        fs::write(tampered.join(OVERLAY_FILE), b"tampered").unwrap();
        let hidden = signed_candidate(&store, &rootfs, &key, 50).await;
        let hidden_destination = store.join(".staging-crashed");
        fs::rename(hidden, &hidden_destination).unwrap();
        let symlink_path = store.join(format!("{GENERATION_PREFIX}{}", Guid::new()));
        symlink(&newest, &symlink_path).unwrap();
        assert_eq!(
            select_latest(&store, &rootfs, &public)
                .await
                .unwrap()
                .unwrap()
                .path,
            newest
        );
        assert_eq!(
            verified_generations(&store, &rootfs, &public)
                .await
                .unwrap()
                .into_iter()
                .map(|generation| generation.path)
                .collect::<Vec<_>>(),
            vec![newest.clone(), older.clone()]
        );
        prune(&store, &[older.clone(), newest.clone()])
            .await
            .unwrap();
        assert!(older.exists());
        assert!(newest.exists());
        assert!(!incomplete.exists());
        assert!(!tampered.exists());
        assert!(hidden_destination.exists());
        assert!(
            fs::symlink_metadata(symlink_path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[tokio::test]
    async fn verity_rejects_changed_data_tree_and_parameters() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let tree = temp.path().join("tree");
        fs::write(&data, image_bytes(b"verity data")).unwrap();
        let image = VerityImage::format(&data, &tree).await.unwrap();
        image.verify(&data, &tree).await.unwrap();
        let mut wrong = image.clone();
        wrong.root_hash[0] ^= 1;
        assert!(wrong.verify(&data, &tree).await.is_err());
        wrong = image.clone();
        wrong.data_blocks -= 1;
        assert!(wrong.verify(&data, &tree).await.is_err());
        fs::write(&data, image_bytes(b"changed data")).unwrap();
        assert!(image.verify(&data, &tree).await.is_err());
        fs::write(&data, image_bytes(b"verity data")).unwrap();
        let mut hash_tree = fs::read(&tree).unwrap();
        hash_tree[0] ^= 1;
        fs::write(&tree, hash_tree).unwrap();
        assert!(image.verify(&data, &tree).await.is_err());
        fs::write(&data, b"not block aligned").unwrap();
        assert!(
            VerityImage::format(&data, &temp.path().join("unaligned"))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn single_block_verity_tree_is_a_nonempty_kernel_hash_device() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let tree = temp.path().join("tree");
        fs::write(&data, vec![42; VERITY_BLOCK_SIZE as usize]).unwrap();
        let image = VerityImage::format(&data, &tree).await.unwrap();
        assert_eq!(image.data_blocks, 1);
        assert_eq!(fs::metadata(&data).unwrap().len(), VERITY_BLOCK_SIZE);
        assert_eq!(fs::metadata(&tree).unwrap().len(), VERITY_BLOCK_SIZE);
        assert!(fs::read(&tree).unwrap().iter().all(|byte| *byte == 0));
        image.verify(&data, &tree).await.unwrap();
        fs::write(&tree, []).unwrap();
        assert!(image.verify(&data, &tree).await.is_err());
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires CAP_SYS_ADMIN, CAP_MKNOD and loop devices"]
    async fn mounted_squashfs_overlay_honors_whiteouts_and_opaque_directories() {
        use crate::disk::mount::filesystem::ReadOnly;
        use crate::disk::mount::guard::MountGuard;

        async fn mount_view(
            lower: &[&Path],
            upper: &Path,
            work: &Path,
            view: &Path,
        ) -> crate::util::GeneralBoxedGuard {
            for path in [upper, work, view] {
                fs::create_dir(path).unwrap();
            }
            let lower = lower
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(":");
            tokio::process::Command::new("mount").args(["-t", "overlay", "overlay", "-o"])
                .arg(format!("lowerdir={lower},upperdir={},workdir={},index=off,metacopy=off,redirect_dir=off", upper.display(), work.display()))
                .arg(view).invoke(ErrorKind::Filesystem).await.unwrap();
            let view = view.to_owned();
            crate::util::GeneralBoxedGuard::new(move || {
                let _ = std::process::Command::new("umount").arg(view).output();
            })
        }

        async fn mount_image(
            image: &Path,
            tree: &Path,
            verity: &VerityImage,
            at: &Path,
        ) -> (MountGuard, crate::util::GeneralBoxedGuard) {
            let name = format!("overlay-test-{}", Guid::new());
            let device = verity.open(image, tree, &name).await.unwrap();
            let mapping = crate::util::GeneralBoxedGuard::new(move || {
                let _ = std::process::Command::new("veritysetup")
                    .arg("close")
                    .arg(name)
                    .output();
            });
            let mount = MountGuard::mount(&device, at, ReadOnly).await.unwrap();
            (mount, mapping)
        }

        fn assert_root(root: &Path, added: &[u8]) {
            assert!(!root.join("deleted").exists());
            assert!(!root.join("opaque/default").exists());
            assert_eq!(
                fs::read(root.join("opaque/replacement")).unwrap(),
                b"saved config"
            );
            assert_eq!(fs::read(root.join("override")).unwrap(), b"public default");
            assert!(!root.join("recreated/old").exists());
            assert_eq!(
                fs::read(root.join("recreated/default")).unwrap(),
                b"public child"
            );
            assert_eq!(
                fs::read(root.join("recreated/new")).unwrap(),
                b"new config child"
            );
            assert_eq!(fs::read(root.join("added")).unwrap(), added);
        }

        let temp = tempfile::tempdir_in("/dev/shm").unwrap();
        tokio::process::Command::new("mount")
            .args(["-t", "tmpfs", "tmpfs"])
            .arg(temp.path())
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
        let mount_path = temp.path().to_owned();
        let _tmpfs = crate::util::GeneralBoxedGuard::new(move || {
            let _ = std::process::Command::new("umount")
                .arg(mount_path)
                .output();
        });
        let store = temp.path().join("store");
        let raw = temp.path().join("raw");
        let base = temp.path().join("base");
        for path in [&store, &raw, &base] {
            fs::create_dir(path).unwrap();
        }
        fs::write(base.join("deleted"), b"must stay deleted").unwrap();
        fs::write(base.join("override"), b"public default").unwrap();
        fs::create_dir(base.join("opaque")).unwrap();
        fs::write(base.join("opaque/default"), b"must stay hidden").unwrap();
        fs::create_dir(base.join("recreated")).unwrap();
        fs::write(base.join("recreated/default"), b"public child").unwrap();
        fs::write(raw.join("override"), b"local override").unwrap();
        nix::sys::stat::mknod(
            &raw.join("deleted"),
            nix::sys::stat::SFlag::S_IFCHR,
            nix::sys::stat::Mode::S_IRUSR,
            nix::sys::stat::makedev(0, 0),
        )
        .unwrap();
        fs::create_dir(raw.join("opaque")).unwrap();
        xattr::set(raw.join("opaque"), "trusted.overlay.opaque", b"y").unwrap();
        fs::write(raw.join("opaque/replacement"), b"saved config").unwrap();
        fs::create_dir(raw.join("recreated")).unwrap();
        fs::write(raw.join("recreated/old"), b"removed config child").unwrap();
        let rootfs = temp.path().join("rootfs");
        let mut rootfs_file = create_file_mod(&rootfs, 0o600).await.unwrap();
        Squashfs::from_directory(&base)
            .await
            .unwrap()
            .serialize(&mut rootfs_file)
            .await
            .unwrap();
        rootfs_file.sync_all().await.unwrap();
        let key = generate_signing_key().unwrap();
        let prepared = PreparedGeneration::import_raw(&raw, &rootfs, &store, &key)
            .await
            .unwrap();
        let manifest1 = prepared.manifest().clone();
        let generation1 = prepared.publish().await.unwrap();
        assert_eq!(manifest1.rootfs_verity.data_blocks, 1);
        assert_eq!(manifest1.overlay_verity.data_blocks, 1);
        let base_mount = temp.path().join("base-mount");
        let (public, public_verity) = mount_image(
            &rootfs,
            &generation1.join(ROOTFS_VERITY_FILE),
            &manifest1.rootfs_verity,
            &base_mount,
        )
        .await;
        let lower1 = temp.path().join("lower1");
        let (lower1_guard, lower1_verity) = mount_image(
            &generation1.join(OVERLAY_FILE),
            &generation1.join(OVERLAY_VERITY_FILE),
            &manifest1.overlay_verity,
            &lower1,
        )
        .await;
        let running1 = temp.path().join("root1");
        let running1_guard = mount_view(
            &[&lower1, &base_mount],
            &temp.path().join("runtime-upper1"),
            &temp.path().join("runtime-work1"),
            &running1,
        )
        .await;
        let upper1 = temp.path().join("upper1");
        let view1 = temp.path().join("view1");
        let view1_guard = mount_view(&[&lower1], &upper1, &temp.path().join("work1"), &view1).await;
        fs::remove_file(view1.join("override")).unwrap();
        fs::set_permissions(view1.join("opaque"), fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(view1.join("added"), b"first commit").unwrap();
        fs::remove_dir_all(view1.join("recreated")).unwrap();
        fs::create_dir(view1.join("recreated")).unwrap();
        fs::write(view1.join("recreated/new"), b"new config child").unwrap();
        let prepared = PreparedGeneration::prepare_working(&lower1, &upper1, &rootfs, &store, &key)
            .await
            .unwrap();
        let manifest2 = prepared.manifest().clone();
        let generation2 = prepared.publish().await.unwrap();
        let lower2 = temp.path().join("lower2");
        let (lower2_guard, lower2_verity) = mount_image(
            &generation2.join(OVERLAY_FILE),
            &generation2.join(OVERLAY_VERITY_FILE),
            &manifest2.overlay_verity,
            &lower2,
        )
        .await;
        let running2 = temp.path().join("root2");
        let running2_guard = mount_view(
            &[&lower2, &base_mount],
            &temp.path().join("runtime-upper2"),
            &temp.path().join("runtime-work2"),
            &running2,
        )
        .await;
        assert_root(&running2, b"first commit");
        assert_eq!(
            fs::metadata(running2.join("opaque")).unwrap().mode() & 0o7777,
            0o700
        );
        assert_eq!(
            fs::read(running1.join("override")).unwrap(),
            b"local override"
        );
        let upper2 = temp.path().join("upper2");
        let view2 = temp.path().join("view2");
        let view2_guard = mount_view(&[&lower2], &upper2, &temp.path().join("work2"), &view2).await;
        fs::write(view2.join("added"), b"second commit").unwrap();
        fs::set_permissions(view2.join("opaque"), fs::Permissions::from_mode(0o750)).unwrap();
        let prepared = PreparedGeneration::prepare_working(&lower2, &upper2, &rootfs, &store, &key)
            .await
            .unwrap();
        let manifest3 = prepared.manifest().clone();
        let generation3 = prepared.publish().await.unwrap();
        let lower3 = temp.path().join("lower3");
        let (lower3_guard, lower3_verity) = mount_image(
            &generation3.join(OVERLAY_FILE),
            &generation3.join(OVERLAY_VERITY_FILE),
            &manifest3.overlay_verity,
            &lower3,
        )
        .await;
        let running3 = temp.path().join("root3");
        let running3_guard = mount_view(
            &[&lower3, &base_mount],
            &temp.path().join("runtime-upper3"),
            &temp.path().join("runtime-work3"),
            &running3,
        )
        .await;
        assert_root(&running3, b"second commit");
        assert_root(&running2, b"first commit");
        assert_eq!(
            fs::metadata(running3.join("opaque")).unwrap().mode() & 0o7777,
            0o750
        );
        drop(running3_guard);
        drop(view2_guard);
        drop(running2_guard);
        drop(view1_guard);
        drop(running1_guard);
        lower3_guard.unmount(false).await.unwrap();
        lower2_guard.unmount(false).await.unwrap();
        lower1_guard.unmount(false).await.unwrap();
        public.unmount(false).await.unwrap();
        drop(lower3_verity);
        drop(lower2_verity);
        drop(lower1_verity);
        drop(public_verity);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires CAP_SYS_ADMIN and loop devices"]
    async fn verity_mount_retains_protection_through_handoff_and_cleans_failures() {
        let temp = tempfile::tempdir_in("/dev/shm").unwrap();
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("setting"), b"verified setting").unwrap();
        let image = temp.path().join("image");
        let mut file = create_file_mod(&image, 0o600).await.unwrap();
        Squashfs::from_directory(&source)
            .await
            .unwrap()
            .serialize(&mut file)
            .await
            .unwrap();
        file.sync_all().await.unwrap();
        let tree = temp.path().join("tree");
        let verity = VerityImage::format(&image, &tree).await.unwrap();
        let name = format!("verity-mount-{}", Guid::new());
        let device = PathBuf::from(format!("/dev/mapper/{name}"));
        let at = temp.path().join("mount");
        let mount = verity.mount(&image, &tree, &name, &at).await.unwrap();
        assert_eq!(
            fs::read(mount.path().join("setting")).unwrap(),
            b"verified setting"
        );
        assert!(fs::write(mount.path().join("setting"), b"changed").is_err());
        mount.unmount().await.unwrap();
        assert!(!device.exists());

        let mount = verity.mount(&image, &tree, &name, &at).await.unwrap();
        assert_eq!(mount.persist(), at);
        assert!(device.exists());
        assert!(VerityImage::close(&name).await.is_err());
        assert_eq!(fs::read(at.join("setting")).unwrap(), b"verified setting");
        tokio::process::Command::new("umount")
            .arg(&at)
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
        VerityImage::close(&name).await.unwrap();
        assert!(!device.exists());

        let mount = verity.mount(&image, &tree, &name, &at).await.unwrap();
        let dependent = temp.path().join("dependent");
        fs::create_dir(&dependent).unwrap();
        tokio::process::Command::new("mount")
            .arg("--bind")
            .arg(&at)
            .arg(&dependent)
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
        assert!(mount.unmount().await.is_err());
        assert!(device.exists());
        assert_eq!(
            fs::read(dependent.join("setting")).unwrap(),
            b"verified setting"
        );
        tokio::process::Command::new("umount")
            .arg(&dependent)
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
        VerityImage::close(&name).await.unwrap();
        assert!(!device.exists());

        let not_directory = temp.path().join("not-directory");
        fs::write(&not_directory, b"occupied").unwrap();
        assert!(
            verity
                .mount(&image, &tree, &name, &not_directory)
                .await
                .is_err()
        );
        assert!(!device.exists());
        fs::write(&image, image_bytes(b"not SquashFS")).unwrap();
        let other_tree = temp.path().join("other-tree");
        let not_squashfs = VerityImage::format(&image, &other_tree).await.unwrap();
        assert!(
            not_squashfs
                .mount(&image, &other_tree, &name, &at)
                .await
                .is_err()
        );
        assert!(!device.exists());
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires CAP_SYS_ADMIN and loop devices"]
    async fn verity_mapping_reads_all_blocks_and_closes_failed_opens() {
        use crate::disk::mount::filesystem::FileSystem;

        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let tree = temp.path().join("tree");
        let original = image_bytes(b"fully verified image");
        fs::write(&data, &original).unwrap();
        let verity = VerityImage::format(&data, &tree).await.unwrap();
        let name = format!("overlay-test-{}", Guid::new());
        let device = PathBuf::from(format!("/dev/mapper/{name}"));
        let mut corrupt = original.clone();
        corrupt[VERITY_BLOCK_SIZE as usize] ^= 1;
        fs::write(&data, &corrupt).unwrap();
        assert!(verity.open(&data, &tree, &name).await.is_err());
        assert!(!device.exists());

        fs::write(&data, &original).unwrap();
        let mapping = verity.open(&data, &tree, &name).await.unwrap();
        assert_eq!(mapping.source().await.unwrap().unwrap().as_ref(), device);
        fs::write(&data, &corrupt).unwrap();
        let mut command = tokio::process::Command::new("dd");
        command.arg(format!("if={}", device.display())).args([
            "of=/dev/null",
            "bs=4096",
            "iflag=direct",
            "status=none",
        ]);
        assert!(command.invoke(ErrorKind::Filesystem).await.is_err());
        VerityImage::close(&name).await.unwrap();
        assert!(!device.exists());
    }

    async fn squashfs_tools_available() -> bool {
        for name in ["unsquashfs", "veritysetup"] {
            match tokio::process::Command::new(name)
                .arg("-version")
                .output()
                .await
            {
                Ok(_) => (),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    log::warn!("Skipping overlay SquashFS interoperability test: {name} missing");
                    return false;
                }
                Err(error) => panic!("{name}: {error}"),
            }
        }
        true
    }

    #[tokio::test]
    async fn prepared_generation_is_invisible_and_publication_is_coherent() {
        if !squashfs_tools_available().await {
            return;
        }
        #[cfg(target_os = "linux")]
        let temp = tempfile::tempdir_in("/dev/shm").unwrap();
        #[cfg(not(target_os = "linux"))]
        let temp = tempfile::tempdir().unwrap();
        let rootfs = temp.path().join("rootfs");
        fs::write(&rootfs, image_bytes(b"rootfs")).unwrap();
        let store = temp.path().join("store");
        let working = temp.path().join("working");
        fs::create_dir(&store).unwrap();
        fs::create_dir(&working).unwrap();
        fs::create_dir(working.join("opaque")).unwrap();
        xattr::set(working.join("opaque"), "user.overlay.opaque", b"y").unwrap();
        fs::write(working.join("whiteout"), []).unwrap();
        xattr::set(working.join("whiteout"), "user.overlay.whiteout", b"y").unwrap();
        fs::write(working.join("config"), b"config contents").unwrap();
        fs::set_permissions(working.join("config"), fs::Permissions::from_mode(0o640)).unwrap();
        xattr::set(working.join("config"), "user.overlay-test", b"xattr value").unwrap();
        symlink("config", working.join("link")).unwrap();
        let key = generate_signing_key().unwrap();
        let public = public_key(&key);
        let prepared = PreparedGeneration::import_raw(&working, &rootfs, &store, &key)
            .await
            .unwrap();
        let abandoned_path = prepared.staging_path().to_owned();
        drop(prepared);
        assert!(abandoned_path.exists());
        assert!(
            select_latest(&store, &rootfs, &public)
                .await
                .unwrap()
                .is_none()
        );
        let prepared = PreparedGeneration::import_raw(&working, &rootfs, &store, &key)
            .await
            .unwrap();
        let staging_path = prepared.staging_path().to_owned();
        let manifest = prepared.manifest().clone();
        fs::create_dir(staging_path.join("tpm")).unwrap();
        fs::write(
            staging_path.join("tpm/policy.sig"),
            b"parent-owned TPM authorization",
        )
        .unwrap();
        assert!(
            select_latest(&store, &rootfs, &public)
                .await
                .unwrap()
                .is_none()
        );
        let path = prepared.publish().await.unwrap();
        assert!(!staging_path.exists());
        assert_eq!(
            fs::read(path.join("tpm/policy.sig")).unwrap(),
            b"parent-owned TPM authorization"
        );
        let selected = select_latest(&store, &rootfs, &public)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(selected.path, path);
        assert_eq!(selected.manifest, manifest);
        let extracted = temp.path().join("extracted");
        tokio::process::Command::new("unsquashfs")
            .arg("-no-progress")
            .arg("-d")
            .arg(&extracted)
            .arg(selected.overlay_path())
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
        assert_eq!(
            fs::read(extracted.join("config")).unwrap(),
            b"config contents"
        );
        assert_eq!(
            fs::metadata(extracted.join("config")).unwrap().mode() & 0o7777,
            0o640
        );
        let source_meta = fs::metadata(working.join("config")).unwrap();
        let extracted_meta = fs::metadata(extracted.join("config")).unwrap();
        assert_eq!(
            (extracted_meta.uid(), extracted_meta.gid()),
            (source_meta.uid(), source_meta.gid())
        );
        assert_eq!(
            fs::read_link(extracted.join("link")).unwrap(),
            Path::new("config")
        );
        assert_eq!(
            xattr::get(extracted.join("config"), "user.overlay-test")
                .unwrap()
                .unwrap(),
            b"xattr value"
        );
        assert_eq!(
            xattr::get(extracted.join("opaque"), "user.overlay.opaque")
                .unwrap()
                .unwrap(),
            b"y"
        );
        assert_eq!(
            xattr::get(extracted.join("whiteout"), "user.overlay.whiteout")
                .unwrap()
                .unwrap(),
            b"y"
        );
        prune(&store, &[path.clone()]).await.unwrap();
        assert!(path.exists());
        assert!(abandoned_path.exists());
    }
}
