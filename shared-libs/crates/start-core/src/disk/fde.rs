//! Password-only access to both encrypted data LVs; TPM authorization belongs to the TLS identity.
//! Enrollment replaces passwords and config header backups without rotating volume keys. Obsolete
//! headers rely on Btrfs discard and NVMe DLFEAT=001b, not general secure erasure or rollback protection.
//! Initial enrollment requires UEFI, TPM2, and Secure Boot. The caller owns eligibility, account state,
//! durable intent and exclusive keyslot ownership from planning until completion or abort. Persist
//! intent and target hash before installation; commit the new account hash and administrator slots
//! after both target slots verify; retire old slots afterward. Abort precedes that account commit.
//! A different target requires completed abort and a fresh plan. Enrolled callers never retry the
//! default password. Captured volume keys belong exclusively to a protected volatile OS-boot-lifetime
//! cache that survives daemon restarts, is unavailable after reboot, and never enters persistent
//! storage, backups, PatchDB, bootstrap state, logs, or an additional management keyslot.

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};

use color_eyre::eyre::eyre;
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use zeroize::Zeroizing;

use super::main::DEFAULT_PASSWORD;
use crate::util::io::{delete_dir, delete_file, open_file, rename};
use crate::util::{GeneralBoxedGuard, GeneralGuard, Invoke, new_guid};
use crate::{Error, ErrorKind, ResultExt};

#[cfg(target_os = "linux")]
mod keyring;

pub const HEADER_BACKUP_DIR: &str = "/media/startos/config/luks";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct VolumeId {
    mapper_name: String,
    uuid: String,
}

/// Nonsecret authority identifying administrator slots, not unrelated manually enrolled keys.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminKeyslots {
    volumes: [VolumeSlots; 2],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct VolumeSlots {
    id: VolumeId,
    slots: BTreeSet<u32>,
}

/// Persist before any keyslot mutation, paired with the caller's target account hash.
/// The caller prevents slot reuse and different-target installation until completion or abort.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordChangePlan {
    admin: AdminKeyslots,
    base_slots: [BTreeSet<u32>; 2],
    target_slots: [u32; 2],
}

impl PasswordChangePlan {
    /// Administrator authority preserved by successful abort.
    pub fn previous_admin_slots(&self) -> AdminKeyslots {
        self.admin.clone()
    }

    /// Account authority to commit after installation verifies both target slots.
    pub fn new_admin_slots(&self) -> AdminKeyslots {
        AdminKeyslots {
            volumes: std::array::from_fn(|i| VolumeSlots {
                id: self.admin.volumes[i].id.clone(),
                slots: BTreeSet::from([self.target_slots[i]]),
            }),
        }
    }
}

/// Opaque, zeroizing volume keys for the caller's protected volatile OS-boot-lifetime cache.
/// Capturing these keys does not authorize persisting them or provisioning a management keyslot.
pub struct VolumeKeys {
    volumes: [VolumeKey; 2],
}

struct VolumeKey {
    id: VolumeId,
    bytes: Zeroizing<Vec<u8>>,
}

/// Authentication material for one key installation, independent of the caller's workflow.
pub enum KeyCredential<'a> {
    Password(&'a str),
    VolumeKeys(&'a VolumeKeys),
}

impl KeyCredential<'_> {
    fn option(&self) -> &'static str {
        match self {
            Self::Password(_) => "--key-file",
            Self::VolumeKeys(_) => "--volume-key-file",
        }
    }

    fn bytes(&self, index: usize, id: &VolumeId) -> Result<&[u8], Error> {
        match self {
            Self::Password(password) => Ok(password.as_bytes()),
            Self::VolumeKeys(keys) => {
                let key = &keys.volumes[index];
                if &key.id != id {
                    return Err(metadata_error());
                }
                Ok(&key.bytes)
            }
        }
    }
}

struct LuksMetadata {
    uuid: String,
    slots: BTreeSet<u32>,
    slot_count: u32,
}

/// An activated encrypted pool with main and package-data LVs.
/// Unencrypted pools require a separate data-encryption migration.
pub struct LuksPool {
    devices: [PathBuf; 2],
    mapper_names: [String; 2],
    config_dir: PathBuf,
    cryptsetup: PathBuf,
    sync: PathBuf,
    fstrim: PathBuf,
    runtime_dir: PathBuf,
}

impl LuksPool {
    pub fn new(guid: &str, config_dir: impl AsRef<Path>) -> Result<Self, Error> {
        if !(guid.starts_with("STARTOS_") || guid.starts_with("EMBASSY_"))
            || guid.ends_with("_UNENC")
            || Path::new(guid).file_name().and_then(|s| s.to_str()) != Some(guid)
        {
            return Err(Error::new(
                eyre!("{}", t!("error.incorrect-disk")),
                ErrorKind::IncorrectDisk,
            ));
        }
        let names = ["main", "package-data"];
        Ok(Self {
            devices: names.map(|name| Path::new("/dev").join(guid).join(name)),
            mapper_names: names.map(|name| format!("{guid}_{name}")),
            config_dir: config_dir.as_ref().to_owned(),
            cryptsetup: "cryptsetup".into(),
            sync: "sync".into(),
            fstrim: "fstrim".into(),
            runtime_dir: "/run".into(),
        })
    }

    /// Identifies matching administrator slots while the unlock password is available.
    pub async fn identify_admin_slots(&self, password: &str) -> Result<AdminKeyslots, Error> {
        let (key, _cleanup) = self.password_file(password)?;
        self.identify_key_slots(&key).await
    }

    async fn identify_key_slots(&self, key: &Path) -> Result<AdminKeyslots, Error> {
        let mut volumes = Vec::new();
        for (i, device) in self.devices.iter().enumerate() {
            let metadata = self.metadata(device).await?;
            let mut slots = BTreeSet::new();
            for slot in metadata.slots {
                if self.accepts(device, key, Some(slot)).await? {
                    slots.insert(slot);
                }
            }
            require_password(!slots.is_empty())?;
            volumes.push(VolumeSlots {
                id: self.volume_id(i, metadata.uuid),
                slots,
            });
        }
        Ok(AdminKeyslots {
            volumes: volumes.try_into().ok().unwrap(),
        })
    }

    /// Tests every owned administrator slot without trying unrelated slots.
    pub async fn accepts_admin_password(
        &self,
        admin: &AdminKeyslots,
        password: &str,
    ) -> Result<bool, Error> {
        let (key, _cleanup) = self.password_file(password)?;
        for (index, volume) in admin.volumes.iter().enumerate() {
            let metadata = self.bound_metadata(index, &volume.id).await?;
            if volume.slots.is_empty() || !volume.slots.is_subset(&metadata.slots) {
                return Err(metadata_error());
            }
            for slot in &volume.slots {
                if !self
                    .accepts(&self.devices[index], &key, Some(*slot))
                    .await?
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// Captures administrator authority and actual volume keys during password unlock.
    /// The caller retains keys only in protected volatile storage for this OS boot.
    pub async fn capture_unlock_keys(
        &self,
        password: &str,
    ) -> Result<(AdminKeyslots, VolumeKeys), Error> {
        let (password, _cleanup) = self.password_file(password)?;
        let admin = self.identify_key_slots(&password).await?;
        let mut volumes = Vec::new();
        for (i, device) in self.devices.iter().enumerate() {
            let (output, _cleanup) = self.secret_file(&[])?;
            delete_file(&output).await?;
            let status = cryptsetup_status(
                Command::new(&self.cryptsetup)
                    .args([
                        "luksDump",
                        "--batch-mode",
                        "--dump-volume-key",
                        "--volume-key-file",
                    ])
                    .arg(&output)
                    .arg("--key-file")
                    .arg(&password)
                    .arg(device),
            )
            .await?;
            require_success("luksDump --dump-volume-key", status)?;
            let mut bytes = Zeroizing::new(Vec::new());
            std::fs::File::open(&output)
                .with_kind(ErrorKind::Filesystem)?
                .read_to_end(&mut bytes)
                .with_kind(ErrorKind::Filesystem)?;
            if bytes.is_empty() {
                return Err(metadata_error());
            }
            volumes.push(VolumeKey {
                id: admin.volumes[i].id.clone(),
                bytes,
            });
        }
        Ok((
            admin,
            VolumeKeys {
                volumes: volumes.try_into().ok().unwrap(),
            },
        ))
    }

    pub(crate) fn protect_secret_handling(
        &self,
    ) -> Result<crate::fde::memory::ProcessSecretMemory, Error> {
        crate::fde::memory::require_tmpfs(&self.runtime_dir)?;
        crate::fde::memory::ProcessSecretMemory::protect()
    }

    fn volume_id(&self, index: usize, uuid: String) -> VolumeId {
        VolumeId {
            mapper_name: self.mapper_names[index].clone(),
            uuid,
        }
    }

    /// Retains captured keys in the OS root user keyring until reboot.
    /// Requires initial-user-namespace root credentials and protected transient copies.
    /// Hardens the entire root user keyring to owner-only access.
    pub async fn cache_unlock_keys(&self, keys: &VolumeKeys) -> Result<(), Error> {
        for (index, key) in keys.volumes.iter().enumerate() {
            self.bound_metadata(index, &key.id).await?;
        }
        #[cfg(target_os = "linux")]
        {
            let ring = keyring::RootKeyring::open()?;
            for key in &keys.volumes {
                ring.store(&key.id, &key.bytes)?;
            }
            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        Err(metadata_error())
    }

    /// Loads both current volume identities' boot-lifetime keys; absent keys return `None`.
    /// Requires initial-user-namespace root credentials and protected transient copies.
    /// Hardens the entire root user keyring to owner-only access.
    pub async fn cached_unlock_keys(&self) -> Result<Option<VolumeKeys>, Error> {
        #[cfg(target_os = "linux")]
        {
            let mut ids = Vec::new();
            for (index, device) in self.devices.iter().enumerate() {
                ids.push(self.volume_id(index, self.metadata(device).await?.uuid));
            }
            let ring = keyring::RootKeyring::open()?;
            let mut volumes = Vec::new();
            for id in ids {
                let Some(bytes) = ring.load(&id)? else {
                    return Ok(None);
                };
                volumes.push(VolumeKey { id, bytes });
            }
            Ok(Some(VolumeKeys {
                volumes: volumes.try_into().ok().unwrap(),
            }))
        }
        #[cfg(not(target_os = "linux"))]
        Err(metadata_error())
    }

    /// Reads a complete base inventory and reserves free target IDs without mutating either LV.
    /// Persist this plan and target hash before installation; retain the exclusive owner until cleared.
    pub async fn plan_password_change(
        &self,
        admin: &AdminKeyslots,
    ) -> Result<PasswordChangePlan, Error> {
        let mut base_slots = [BTreeSet::new(), BTreeSet::new()];
        let mut target_slots = [0; 2];
        for (i, volume) in admin.volumes.iter().enumerate() {
            let metadata = self.bound_metadata(i, &volume.id).await?;
            if volume.slots.is_empty() || !volume.slots.is_subset(&metadata.slots) {
                return Err(metadata_error());
            }
            target_slots[i] = (0..metadata.slot_count)
                .find(|slot| !metadata.slots.contains(slot))
                .ok_or_else(metadata_error)?;
            base_slots[i] = metadata.slots;
        }
        Ok(PasswordChangePlan {
            admin: admin.clone(),
            base_slots,
            target_slots,
        })
    }

    /// Installs the persisted plan's same target password, preserving every base slot.
    /// A different target requires successful abort before planning another installation.
    pub async fn install_password(
        &self,
        plan: &PasswordChangePlan,
        new: &str,
        credential: KeyCredential<'_>,
    ) -> Result<(), Error> {
        validate_new_password(new)?;
        if let KeyCredential::Password(old) = &credential {
            validate_change(old, new)?;
        }
        let (new_key, _cleanup) = self.password_file(new)?;
        let metadata = self.change_metadata(plan).await?;
        let mut credentials = Vec::new();
        for (i, metadata) in metadata.iter().enumerate() {
            if !plan.base_slots[i].is_subset(&metadata.slots) {
                return Err(metadata_error());
            }
            let key = self
                .authenticate_credential(i, &plan.admin.volumes[i].id, &credential)
                .await?;
            for slot in &plan.admin.volumes[i].slots {
                if self
                    .accepts(&self.devices[i], &new_key, Some(*slot))
                    .await?
                {
                    return Err(metadata_error());
                }
            }
            if metadata.slots.contains(&plan.target_slots[i]) {
                require_password(
                    self.accepts(&self.devices[i], &new_key, Some(plan.target_slots[i]))
                        .await?,
                )?;
            }
            credentials.push(key);
        }
        for (i, metadata) in metadata.iter().enumerate() {
            if !metadata.slots.contains(&plan.target_slots[i]) {
                self.add_and_verify(
                    &self.devices[i],
                    plan.target_slots[i],
                    &new_key,
                    &credentials[i].0,
                    credential.option(),
                )
                .await?;
            }
        }
        self.verify_targets(plan, &new_key).await
    }

    async fn add_and_verify(
        &self,
        device: &Path,
        slot: u32,
        new_key: &Path,
        credential: &Path,
        option: &str,
    ) -> Result<(), Error> {
        let status = cryptsetup_status(
            Command::new(&self.cryptsetup)
                .args(["luksAddKey", "--batch-mode", "--key-slot"])
                .arg(slot.to_string())
                .arg(option)
                .arg(credential)
                .arg(device)
                .arg(new_key),
        )
        .await?;
        require_success("luksAddKey", status)?;
        require_password(self.accepts(device, new_key, Some(slot)).await?)
    }

    /// Aborts an uncommitted installation using captured volume keys, preserving all base slots.
    /// Account commit or administrator-slot retirement makes abort invalid.
    pub async fn abort_password_change(
        &self,
        plan: &PasswordChangePlan,
        keys: &VolumeKeys,
    ) -> Result<(), Error> {
        let credential = KeyCredential::VolumeKeys(keys);
        let metadata = self.change_metadata(plan).await?;
        let mut credentials = Vec::new();
        for (i, metadata) in metadata.iter().enumerate() {
            if !plan.base_slots[i].is_subset(&metadata.slots) {
                return Err(metadata_error());
            }
            credentials.push(
                self.authenticate_credential(i, &plan.admin.volumes[i].id, &credential)
                    .await?,
            );
        }
        for (i, metadata) in metadata.iter().enumerate() {
            if metadata.slots.contains(&plan.target_slots[i]) {
                self.kill_slot(
                    &self.devices[i],
                    plan.target_slots[i],
                    &credentials[i].0,
                    credential.option(),
                )
                .await?;
            }
        }
        for (i, metadata) in self.change_metadata(plan).await?.iter().enumerate() {
            if metadata.slots != plan.base_slots[i] {
                return Err(metadata_error());
            }
        }
        self.publish_headers().await
    }

    /// Retires old administrator slots after durable account commit, using only the new password.
    /// Skips absent old slots on retry and preserves every unrelated base slot.
    pub async fn retire_password(&self, plan: &PasswordChangePlan, new: &str) -> Result<(), Error> {
        validate_new_password(new)?;
        let (key, _cleanup) = self.password_file(new)?;
        let metadata = self.retirement_metadata(plan).await?;
        self.verify_targets(plan, &key).await?;
        for (i, metadata) in metadata.iter().enumerate() {
            for slot in plan.admin.volumes[i].slots.intersection(&metadata.slots) {
                if self.accepts(&self.devices[i], &key, Some(*slot)).await? {
                    return Err(metadata_error());
                }
            }
        }
        self.remove_old_admin_slots(plan, &metadata, [&key, &key], "--key-file")
            .await?;
        self.verify_targets(plan, &key).await?;
        self.publish_headers().await
    }

    /// Finishes a durably committed account change without its old plaintext password.
    /// The caller commits only after both target slots verify and retains exclusive slot ownership.
    pub async fn retire_password_with_keys(
        &self,
        plan: &PasswordChangePlan,
        keys: &VolumeKeys,
    ) -> Result<(), Error> {
        let metadata = self.retirement_metadata(plan).await?;
        let credential = KeyCredential::VolumeKeys(keys);
        let mut files = Vec::new();
        for (index, volume) in plan.admin.volumes.iter().enumerate() {
            files.push(
                self.authenticate_credential(index, &volume.id, &credential)
                    .await?,
            );
        }
        self.remove_old_admin_slots(
            plan,
            &metadata,
            [&files[0].0, &files[1].0],
            credential.option(),
        )
        .await?;
        self.publish_headers().await
    }

    async fn retirement_metadata(
        &self,
        plan: &PasswordChangePlan,
    ) -> Result<[LuksMetadata; 2], Error> {
        let metadata = self.change_metadata(plan).await?;
        for (index, volume) in metadata.iter().enumerate() {
            let manual: BTreeSet<_> = plan.base_slots[index]
                .difference(&plan.admin.volumes[index].slots)
                .copied()
                .collect();
            if !manual.is_subset(&volume.slots) || !volume.slots.contains(&plan.target_slots[index])
            {
                return Err(metadata_error());
            }
        }
        Ok(metadata)
    }

    async fn remove_old_admin_slots(
        &self,
        plan: &PasswordChangePlan,
        metadata: &[LuksMetadata; 2],
        keys: [&Path; 2],
        option: &str,
    ) -> Result<(), Error> {
        for (index, volume) in metadata.iter().enumerate() {
            for slot in plan.admin.volumes[index].slots.intersection(&volume.slots) {
                self.kill_slot(&self.devices[index], *slot, keys[index], option)
                    .await?;
            }
        }
        for (i, metadata) in self.change_metadata(plan).await?.iter().enumerate() {
            let mut expected: BTreeSet<_> = plan.base_slots[i]
                .difference(&plan.admin.volumes[i].slots)
                .copied()
                .collect();
            expected.insert(plan.target_slots[i]);
            if metadata.slots != expected {
                return Err(metadata_error());
            }
        }
        Ok(())
    }

    async fn authenticate_credential(
        &self,
        index: usize,
        id: &VolumeId,
        credential: &KeyCredential<'_>,
    ) -> Result<(PathBuf, GeneralBoxedGuard), Error> {
        let file = self.secret_file(credential.bytes(index, id)?)?;
        require_password(
            self.test_key(&self.devices[index], &file.0, None, credential.option())
                .await?,
        )?;
        Ok(file)
    }

    async fn kill_slot(
        &self,
        device: &Path,
        slot: u32,
        key: &Path,
        option: &str,
    ) -> Result<(), Error> {
        let status = cryptsetup_status(
            Command::new(&self.cryptsetup)
                .args(["luksKillSlot", "--batch-mode"])
                .arg(option)
                .arg(key)
                .arg(device)
                .arg(slot.to_string()),
        )
        .await?;
        require_success("luksKillSlot", status)
    }

    async fn verify_targets(&self, plan: &PasswordChangePlan, key: &Path) -> Result<(), Error> {
        for (i, device) in self.devices.iter().enumerate() {
            require_password(
                self.accepts(device, key, Some(plan.target_slots[i]))
                    .await?,
            )?;
        }
        Ok(())
    }

    async fn change_metadata(&self, plan: &PasswordChangePlan) -> Result<[LuksMetadata; 2], Error> {
        let mut result = Vec::new();
        for (i, volume) in plan.admin.volumes.iter().enumerate() {
            let metadata = self.bound_metadata(i, &volume.id).await?;
            if volume.slots.is_empty()
                || !volume.slots.is_subset(&plan.base_slots[i])
                || plan.base_slots[i].contains(&plan.target_slots[i])
                || plan.target_slots[i] >= metadata.slot_count
            {
                return Err(metadata_error());
            }
            let mut allowed = plan.base_slots[i].clone();
            allowed.insert(plan.target_slots[i]);
            if !metadata.slots.is_subset(&allowed) {
                return Err(metadata_error());
            }
            result.push(metadata);
        }
        Ok(result.try_into().ok().unwrap())
    }

    async fn publish_headers(&self) -> Result<(), Error> {
        for (device, name) in self.devices.iter().zip(&self.mapper_names) {
            replace_header_backup_with(
                &self.cryptsetup,
                device,
                name,
                &self.config_dir.join("luks"),
            )
            .await?;
        }
        Command::new(&self.sync)
            .arg("-f")
            .arg(&self.config_dir)
            .invoke(ErrorKind::Filesystem)
            .await?;
        Command::new(&self.fstrim)
            .arg(&self.config_dir)
            .invoke(ErrorKind::Filesystem)
            .await?;
        Ok(())
    }

    /// Tests both LVs without creating mappings or trying a fallback password.
    pub async fn verify_password(&self, password: &str) -> Result<(), Error> {
        let (key, _cleanup) = self.password_file(password)?;
        for device in &self.devices {
            require_password(self.accepts(device, &key, None).await?)?;
        }
        Ok(())
    }

    async fn bound_metadata(&self, index: usize, id: &VolumeId) -> Result<LuksMetadata, Error> {
        if id.mapper_name != self.mapper_names[index] {
            return Err(metadata_error());
        }
        let metadata = self.metadata(&self.devices[index]).await?;
        if id.uuid != metadata.uuid {
            return Err(metadata_error());
        }
        Ok(metadata)
    }

    async fn metadata(&self, device: &Path) -> Result<LuksMetadata, Error> {
        let dump = Command::new(&self.cryptsetup)
            .env("LC_ALL", "C")
            .arg("luksDump")
            .arg(device)
            .invoke(ErrorKind::DiskManagement)
            .await?;
        let dump = std::str::from_utf8(&dump).map_err(|_| metadata_error())?;
        let field = |name: &str| {
            dump.lines().find_map(|line| {
                let (key, value) = line.split_once(':')?;
                (key.trim() == name).then(|| value.trim())
            })
        };
        let uuid = field("UUID")
            .filter(|s| !s.is_empty())
            .ok_or_else(metadata_error)?
            .to_owned();
        let (slots, slot_count) = match field("Version") {
            Some("1") => (luks1_slots(dump)?, 8),
            Some("2") => {
                let json = Command::new(&self.cryptsetup)
                    .env("LC_ALL", "C")
                    .args(["luksDump", "--dump-json-metadata"])
                    .arg(device)
                    .invoke(ErrorKind::DiskManagement)
                    .await?;
                (luks2_slots(&json)?, 32)
            }
            _ => return Err(metadata_error()),
        };
        Ok(LuksMetadata {
            uuid,
            slots,
            slot_count,
        })
    }

    fn password_file(&self, password: &str) -> Result<(PathBuf, GeneralBoxedGuard), Error> {
        self.secret_file(password.as_bytes())
    }

    fn secret_file(&self, bytes: &[u8]) -> Result<(PathBuf, GeneralBoxedGuard), Error> {
        let path = self
            .runtime_dir
            .join(format!("startos-luks-{}", new_guid()));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .with_kind(ErrorKind::Filesystem)?;
        let cleanup = GeneralBoxedGuard::new({
            let path = path.clone();
            move || {
                let _ = std::fs::remove_file(path);
            }
        });
        file.write_all(bytes).with_kind(ErrorKind::Filesystem)?;
        file.flush().with_kind(ErrorKind::Filesystem)?;
        Ok((path, cleanup))
    }

    async fn accepts(&self, device: &Path, key: &Path, slot: Option<u32>) -> Result<bool, Error> {
        self.test_key(device, key, slot, "--key-file").await
    }

    async fn test_key(
        &self,
        device: &Path,
        key: &Path,
        slot: Option<u32>,
        option: &str,
    ) -> Result<bool, Error> {
        let mut command = Command::new(&self.cryptsetup);
        command
            .args(["open", "--test-passphrase", "--batch-mode"])
            .arg(option)
            .arg(key);
        if let Some(slot) = slot {
            command.arg("--key-slot").arg(slot.to_string());
        }
        let status = cryptsetup_status(command.arg(device)).await?;
        match status.code() {
            Some(0) => Ok(true),
            Some(2) => Ok(false),
            _ => {
                require_success("open --test-passphrase", status)?;
                unreachable!()
            }
        }
    }
}

fn validate_change(old: &str, new: &str) -> Result<(), Error> {
    validate_new_password(new)?;
    if new == old {
        return Err(Error::new(
            eyre!("{}", t!("error.invalid-request")),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(())
}

pub(crate) fn validate_new_password(new: &str) -> Result<(), Error> {
    if new.is_empty() || new == DEFAULT_PASSWORD {
        return Err(Error::new(
            eyre!("{}", t!("error.invalid-request")),
            ErrorKind::InvalidRequest,
        ));
    }
    Ok(())
}

fn metadata_error() -> Error {
    Error::new(
        eyre!("{}", t!("error.disk-management")),
        ErrorKind::DiskManagement,
    )
}

fn luks1_slots(dump: &str) -> Result<BTreeSet<u32>, Error> {
    let mut seen = BTreeSet::new();
    let mut enabled = BTreeSet::new();
    for line in dump.lines() {
        if let Some(line) = line.trim().strip_prefix("Key Slot ") {
            let (slot, state) = line.split_once(':').ok_or_else(metadata_error)?;
            let slot: u32 = slot.parse().map_err(|_| metadata_error())?;
            if slot >= 8 || !seen.insert(slot) {
                return Err(metadata_error());
            }
            match state.trim() {
                "ENABLED" => {
                    enabled.insert(slot);
                }
                "DISABLED" => {}
                _ => return Err(metadata_error()),
            }
        }
    }
    if seen.len() != 8 {
        return Err(metadata_error());
    }
    Ok(enabled)
}

fn luks2_slots(json: &[u8]) -> Result<BTreeSet<u32>, Error> {
    let metadata: serde_json::Value = serde_json::from_slice(json).map_err(|_| metadata_error())?;
    metadata
        .get("keyslots")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(metadata_error)?
        .keys()
        .map(|key| {
            let slot: u32 = key.parse().map_err(|_| metadata_error())?;
            if slot.to_string() != *key {
                return Err(metadata_error());
            }
            Ok(slot)
        })
        .collect()
}

fn require_password(accepted: bool) -> Result<(), Error> {
    if accepted {
        Ok(())
    } else {
        Err(Error::new(
            eyre!("{}", t!("error.incorrect-password")),
            ErrorKind::IncorrectPassword,
        ))
    }
}

fn require_success(operation: &str, status: ExitStatus) -> Result<(), Error> {
    if status.success() {
        Ok(())
    } else {
        Err(Error::new(
            eyre!("{} ({operation}: {status})", t!("error.disk-management")),
            ErrorKind::DiskManagement,
        ))
    }
}

// Preserve exit codes without including credential-bearing subprocess output in errors or logs.
async fn cryptsetup_status(command: &mut Command) -> Result<ExitStatus, Error> {
    Ok(command
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await
        .with_kind(ErrorKind::DiskManagement)?
        .status)
}

/// Publishes a synced raw header by atomic replacement, then syncs its containing directory.
/// Removes interrupted staging headers, including the legacy temporary backup.
/// The caller serializes header publication with passphrase changes for this device.
pub async fn replace_header_backup(
    device: &Path,
    mapper_name: &str,
    directory: &Path,
) -> Result<(), Error> {
    replace_header_backup_with(Path::new("cryptsetup"), device, mapper_name, directory).await
}

async fn replace_header_backup_with(
    cryptsetup: &Path,
    device: &Path,
    mapper_name: &str,
    directory: &Path,
) -> Result<(), Error> {
    if Path::new(mapper_name).file_name().and_then(|s| s.to_str()) != Some(mapper_name) {
        return Err(Error::new(
            eyre!("{}", t!("error.invalid-request")),
            ErrorKind::InvalidRequest,
        ));
    }
    tokio::fs::create_dir_all(directory).await?;
    let staging = directory.join(format!(".{mapper_name}.luks.bak.staging"));
    delete_dir(&staging).await?;
    delete_file(directory.join(format!(".{mapper_name}.luks.bak.tmp"))).await?;
    let cleanup = GeneralGuard::new(|| std::fs::remove_dir_all(&staging));
    tokio::fs::DirBuilder::new()
        .mode(0o700)
        .create(&staging)
        .await?;
    let header = staging.join("header");
    let status = cryptsetup_status(
        Command::new(cryptsetup)
            .args(["luksHeaderBackup", "--header-backup-file"])
            .arg(&header)
            .arg(device),
    )
    .await?;
    require_success("luksHeaderBackup", status)?;
    let file = open_file(&header).await?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .await?;
    file.sync_all().await?;
    rename(&header, directory.join(format!("{mapper_name}.luks.bak"))).await?;
    delete_dir(&staging).await?;
    cleanup.drop_without_action();
    open_file(directory).await?.sync_all().await?;
    if let Some(parent) = directory.parent() {
        open_file(parent).await?.sync_all().await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::MetadataExt;

    use super::*;

    const OLD: &str = DEFAULT_PASSWORD;
    const NEW: &str = "administrator\ncredential-🔒";
    const OTHER: &str = "different-administrator-credential";
    const MANUAL: &str = "manual-unrelated-credential";
    const GUID: &str = "STARTOS_TEST";
    const FIXTURE: &str = r##"#!/bin/bash
set -eu
root=$(dirname "$0")
tool=${0##*/}
printf '%s' "$tool" >> "$root/events"
printf ' %s' "$@" >> "$root/events"
printf '\n' >> "$root/events"
if [[ $tool != cryptsetup ]]; then
    [[ ! -f "$root/fail-$tool" ]] || exit "$(cat "$root/fail-$tool")"
    exit 0
fi
op=$1; shift
key= auth= volume= header= device= slot= json=0 dump=0
while (($#)); do
    case $1 in
        --key-file) auth=$2; shift 2 ;;
        --volume-key-file) volume=$2; shift 2 ;;
        --header-backup-file) header=$2; shift 2 ;;
        --key-slot) slot=$2; shift 2 ;;
        --dump-json-metadata) json=1; shift ;;
        --dump-volume-key) dump=1; shift ;;
        --*) shift ;;
        *)
            if [[ -z $device ]]; then device=$1
            elif [[ $op = luksKillSlot ]]; then slot=$1
            else key=$1; fi
            shift ;;
    esac
done
name=${device##*/}
classify() {
    [[ $(stat -c %a "$1") = 600 ]]
    cat "$1" >&2
    credential=unknown
    for kind in old new alternate manual; do
        if cmp -s "$1" "$root/$kind-secret"; then credential=$kind; fi
    done
}
valid=0
if [[ -n $auth ]]; then
    classify "$auth"
    [[ ! -s "$root/$name.$credential" ]] || valid=1
fi
if [[ -n $volume && $dump = 0 ]]; then
    [[ $(stat -c %a "$volume") = 600 ]]
    cat "$volume" >&2
    if cmp -s "$volume" "$root/$name.volume"; then valid=1; credential=volume; fi
fi
fail="$root/fail-$op-$name"
[[ ! -f $fail ]] || exit "$(cat "$fail")"
case $op in
    open)
        [[ $valid = 1 ]] || exit 2
        [[ $credential != volume ]] || exit 0
        if [[ -n $slot ]]; then grep -Fxq "$slot" "$root/$name.$credential" || exit 2; fi ;;
    luksDump)
        if [[ $dump = 1 ]]; then
            [[ $valid = 1 && ! -e $volume ]]
            cat "$root/$name.volume" > "$volume"
            chmod 600 "$volume"
            exit 0
        fi
        if [[ -f "$root/$name.metadata" ]]; then cat "$root/$name.metadata"; exit 0; fi
        slots=$(cat "$root/$name.old" "$root/$name.new" "$root/$name.alternate" "$root/$name.manual" | sort -nu)
        if [[ $json = 1 ]]; then
            printf '{"keyslots":{'; separator=
            for id in $slots; do printf '%s"%s":{"type":"luks2"}' "$separator" "$id"; separator=,; done
            printf '}}\n'
        else
            version=2
            [[ ! -f "$root/$name.version" ]] || version=$(cat "$root/$name.version")
            printf 'Version: %s\nUUID: fixture-%s\n' "$version" "$name"
            if [[ $version = 1 ]]; then
                for id in {0..7}; do
                    state=DISABLED
                    if grep -Fxq "$id" <<< "$slots"; then state=ENABLED; fi
                    printf 'Key Slot %s: %s\n' "$id" "$state"
                done
            fi
        fi ;;
    luksAddKey)
        [[ $valid = 1 && -n $slot ]]
        ! cat "$root/$name.old" "$root/$name.new" "$root/$name.alternate" "$root/$name.manual" | grep -Fxq "$slot"
        [[ ! -f "$root/skip-add-$name" ]] || exit 0
        classify "$key"
        [[ $credential != unknown ]]
        printf '%s\n' "$slot" >> "$root/$name.$credential" ;;
    luksKillSlot)
        [[ $valid = 1 ]]
        [[ ! -f "$root/skip-kill-$name" ]] || exit 0
        for credential in old new alternate manual; do
            grep -Fxv "$slot" "$root/$name.$credential" > "$root/remaining" || true
            mv "$root/remaining" "$root/$name.$credential"
        done ;;
    luksHeaderBackup)
        printf 'partial' > "$header"
        [[ ! -f "$root/fail-header-$name" ]] || exit 5
        printf 'header:%s\n' "$name" > "$header" ;;
    *) exit 9 ;;
esac
"##;

    struct Fixture {
        dir: tempfile::TempDir,
        pool: LuksPool,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            for tool in ["cryptsetup", "sync", "fstrim"] {
                let path = root.join(tool);
                std::fs::write(&path, FIXTURE).unwrap();
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
            }
            for (name, secret) in [
                ("old", OLD),
                ("new", NEW),
                ("alternate", OTHER),
                ("manual", MANUAL),
            ] {
                std::fs::write(root.join(format!("{name}-secret")), secret).unwrap();
            }
            std::fs::create_dir(root.join("keys")).unwrap();
            std::fs::create_dir_all(root.join("config/luks")).unwrap();
            let mut pool = LuksPool::new(GUID, root.join("config")).unwrap();
            pool.cryptsetup = root.join("cryptsetup");
            pool.sync = root.join("sync");
            pool.fstrim = root.join("fstrim");
            pool.runtime_dir = root.join("keys");
            for name in ["main", "package-data"] {
                for kind in ["old", "new", "alternate", "manual"] {
                    std::fs::write(
                        root.join(format!("{name}.{kind}")),
                        if kind == "old" {
                            "0\n"
                        } else if kind == "manual" {
                            "5\n"
                        } else {
                            ""
                        },
                    )
                    .unwrap();
                }
                std::fs::write(
                    root.join(format!("{name}.volume")),
                    format!("volume-key-sensitive-{name}"),
                )
                .unwrap();
                std::fs::write(
                    root.join(format!("config/luks/{GUID}_{name}.luks.bak")),
                    "obsolete",
                )
                .unwrap();
            }
            Self { dir, pool }
        }
        fn write(&self, name: &str, data: &str) {
            std::fs::write(self.dir.path().join(name), data).unwrap();
        }
        fn remove(&self, name: &str) {
            std::fs::remove_file(self.dir.path().join(name)).unwrap();
        }
        fn events(&self) -> String {
            std::fs::read_to_string(self.dir.path().join("events")).unwrap_or_default()
        }
        async fn plan(&self) -> PasswordChangePlan {
            let admin = self.pool.identify_admin_slots(OLD).await.unwrap();
            self.pool.plan_password_change(&admin).await.unwrap()
        }
        async fn installed(&self) -> PasswordChangePlan {
            let plan = self.plan().await;
            self.pool
                .install_password(&plan, NEW, KeyCredential::Password(OLD))
                .await
                .unwrap();
            plan
        }
        fn assert_clean(&self) {
            assert_eq!(
                std::fs::read_dir(&self.pool.runtime_dir).unwrap().count(),
                0
            );
            assert_eq!(
                std::fs::read_dir(self.pool.config_dir.join("luks"))
                    .unwrap()
                    .count(),
                2
            );
            let events = self.events();
            for secret in [OLD, NEW, OTHER, MANUAL, "volume-key-sensitive"] {
                assert!(!events.contains(secret));
            }
        }
        fn assert_no_mutation(&self) {
            for op in ["luksAddKey", "luksKillSlot", "luksHeaderBackup", "fstrim"] {
                assert!(!self.events().contains(op));
            }
            self.assert_clean();
        }
    }

    #[test]
    fn unencrypted_or_invalid_pool_is_rejected() {
        for guid in ["STARTOS_TEST_UNENC", "STARTOS_TEST/../other", "other"] {
            assert!(LuksPool::new(guid, "/config").is_err());
        }
    }

    #[tokio::test]
    async fn planning_is_nonsecret_and_precedes_every_mutation() {
        let fixture = Fixture::new();
        fixture.write("main.version", "1");
        let plan = fixture.plan().await;
        assert_eq!(
            plan.base_slots,
            [BTreeSet::from([0, 5]), BTreeSet::from([0, 5])]
        );
        assert_eq!(plan.target_slots, [1, 1]);
        let json = serde_json::to_string(&plan).unwrap();
        for secret in [OLD, NEW, "volume-key-sensitive"] {
            assert!(!json.contains(secret));
        }
        assert_eq!(plan, serde_json::from_str(&json).unwrap());
        fixture.assert_no_mutation();
    }

    #[tokio::test]
    async fn captured_volume_keys_install_without_old_plaintext_and_preserve_manual_slots() {
        let fixture = Fixture::new();
        let (admin, keys) = fixture.pool.capture_unlock_keys(OLD).await.unwrap();
        let plan = fixture.pool.plan_password_change(&admin).await.unwrap();
        fixture.remove("old-secret");
        fixture
            .pool
            .install_password(&plan, NEW, KeyCredential::VolumeKeys(&keys))
            .await
            .unwrap();
        fixture.pool.retire_password(&plan, NEW).await.unwrap();
        assert_eq!(
            fixture.pool.identify_admin_slots(NEW).await.unwrap(),
            plan.new_admin_slots()
        );
        fixture.pool.verify_password(MANUAL).await.unwrap();
        assert!(fixture.pool.verify_password(OLD).await.is_err());
        fixture.assert_clean();
    }

    #[tokio::test]
    async fn partial_a_install_requires_abort_before_different_b_and_preserves_base() {
        let fixture = Fixture::new();
        let (admin, keys) = fixture.pool.capture_unlock_keys(OLD).await.unwrap();
        let plan = fixture.pool.plan_password_change(&admin).await.unwrap();
        fixture.write("fail-luksAddKey-package-data", "5");
        assert!(
            fixture
                .pool
                .install_password(&plan, NEW, KeyCredential::VolumeKeys(&keys))
                .await
                .is_err()
        );
        fixture.remove("fail-luksAddKey-package-data");
        assert!(
            fixture
                .pool
                .install_password(&plan, OTHER, KeyCredential::VolumeKeys(&keys))
                .await
                .is_err()
        );
        fixture
            .pool
            .abort_password_change(&plan, &keys)
            .await
            .unwrap();
        fixture
            .pool
            .abort_password_change(&plan, &keys)
            .await
            .unwrap();
        assert!(fixture.pool.verify_password(NEW).await.is_err());
        fixture.pool.verify_password(OLD).await.unwrap();
        fixture.pool.verify_password(MANUAL).await.unwrap();
        let plan_b = fixture.pool.plan_password_change(&admin).await.unwrap();
        fixture
            .pool
            .install_password(&plan_b, OTHER, KeyCredential::VolumeKeys(&keys))
            .await
            .unwrap();
        fixture.pool.verify_password(OLD).await.unwrap();
        fixture.pool.retire_password(&plan_b, OTHER).await.unwrap();
        assert!(fixture.pool.verify_password(NEW).await.is_err());
        fixture.pool.verify_password(OTHER).await.unwrap();
        fixture.pool.verify_password(MANUAL).await.unwrap();
        fixture.assert_clean();
    }

    #[tokio::test]
    async fn serialized_intent_resumes_partial_retirement_using_only_new_password() {
        let fixture = Fixture::new();
        fixture.write("package-data.version", "1");
        let plan = fixture.installed().await;
        let json = serde_json::to_vec(&plan).unwrap();
        drop(plan);
        fixture.remove("old-secret");
        let plan = serde_json::from_slice(&json).unwrap();
        fixture.write("fail-luksKillSlot-package-data", "5");
        assert!(fixture.pool.retire_password(&plan, NEW).await.is_err());
        assert!(!fixture.events().contains("luksHeaderBackup"));
        fixture.remove("fail-luksKillSlot-package-data");
        fixture.pool.retire_password(&plan, NEW).await.unwrap();
        fixture.pool.retire_password(&plan, NEW).await.unwrap();
        assert_eq!(fixture.events().matches("luksKillSlot").count(), 3);
        fixture.pool.verify_password(MANUAL).await.unwrap();
        fixture.assert_clean();
    }

    #[tokio::test]
    async fn retries_same_installation_do_not_reuse_or_duplicate_slots() {
        let fixture = Fixture::new();
        let plan = fixture.plan().await;
        fixture.write("fail-luksAddKey-package-data", "5");
        assert!(
            fixture
                .pool
                .install_password(&plan, NEW, KeyCredential::Password(OLD))
                .await
                .is_err()
        );
        fixture.remove("fail-luksAddKey-package-data");
        fixture
            .pool
            .install_password(&plan, NEW, KeyCredential::Password(OLD))
            .await
            .unwrap();
        fixture
            .pool
            .install_password(&plan, NEW, KeyCredential::Password(OLD))
            .await
            .unwrap();
        assert_eq!(fixture.events().matches("luksAddKey").count(), 3);
        fixture.assert_clean();
    }

    #[tokio::test]
    async fn all_credentials_and_volume_bindings_validate_before_any_install() {
        let fixture = Fixture::new();
        let plan = fixture.plan().await;
        fixture.write("package-data.old", "");
        fixture.write("events", "");
        assert!(
            fixture
                .pool
                .install_password(&plan, NEW, KeyCredential::Password(OLD))
                .await
                .is_err()
        );
        fixture.assert_no_mutation();
        fixture.write("package-data.old", "0\n");
        let (_, mut keys) = fixture.pool.capture_unlock_keys(OLD).await.unwrap();
        keys.volumes.swap(0, 1);
        fixture.write("events", "");
        assert!(
            fixture
                .pool
                .install_password(&plan, NEW, KeyCredential::VolumeKeys(&keys))
                .await
                .is_err()
        );
        fixture.assert_no_mutation();
    }

    #[tokio::test]
    async fn both_target_slots_must_verify_before_retirement_and_current_keys_are_never_deleted() {
        let fixture = Fixture::new();
        let mut plan = fixture.installed().await;
        fixture.write("package-data.new", "");
        fixture.write("events", "");
        assert!(fixture.pool.retire_password(&plan, NEW).await.is_err());
        fixture.assert_no_mutation();
        fixture.write("package-data.new", "1\n0\n");
        assert!(fixture.pool.retire_password(&plan, NEW).await.is_err());
        fixture.assert_no_mutation();
        fixture.write("package-data.new", "1\n");
        plan.target_slots[0] = 0;
        assert!(fixture.pool.retire_password(&plan, NEW).await.is_err());
        fixture.assert_no_mutation();
    }

    #[tokio::test]
    async fn abort_refuses_missing_base_slots_and_resumes_second_volume_failure() {
        let fixture = Fixture::new();
        let (_, keys) = fixture.pool.capture_unlock_keys(OLD).await.unwrap();
        let plan = fixture.installed().await;
        fixture.write("main.old", "");
        fixture.write("events", "");
        assert!(
            fixture
                .pool
                .abort_password_change(&plan, &keys)
                .await
                .is_err()
        );
        fixture.assert_no_mutation();
        fixture.write("main.old", "0\n");
        fixture.write("fail-luksKillSlot-package-data", "5");
        assert!(
            fixture
                .pool
                .abort_password_change(&plan, &keys)
                .await
                .is_err()
        );
        fixture.remove("fail-luksKillSlot-package-data");
        fixture
            .pool
            .abort_password_change(&plan, &keys)
            .await
            .unwrap();
        fixture.pool.verify_password(OLD).await.unwrap();
        fixture.pool.verify_password(MANUAL).await.unwrap();
        fixture.assert_clean();
    }

    #[tokio::test]
    async fn command_success_without_mutation_does_not_claim_completion() {
        let fixture = Fixture::new();
        let plan = fixture.plan().await;
        fixture.write("skip-add-package-data", "");
        assert!(
            fixture
                .pool
                .install_password(&plan, NEW, KeyCredential::Password(OLD))
                .await
                .is_err()
        );
        fixture.remove("skip-add-package-data");
        fixture
            .pool
            .install_password(&plan, NEW, KeyCredential::Password(OLD))
            .await
            .unwrap();
        fixture.write("skip-kill-package-data", "");
        assert!(fixture.pool.retire_password(&plan, NEW).await.is_err());
        assert!(!fixture.events().contains("luksHeaderBackup"));
        fixture.assert_clean();
    }

    #[tokio::test]
    async fn stale_inventory_identity_or_unexpected_slot_reuse_is_rejected() {
        let fixture = Fixture::new();
        let mut plan = fixture.plan().await;
        plan.admin.volumes[1].id.uuid = "other-device".into();
        fixture.write("events", "");
        assert!(
            fixture
                .pool
                .install_password(&plan, NEW, KeyCredential::Password(OLD))
                .await
                .is_err()
        );
        fixture.assert_no_mutation();
        plan.admin.volumes[1].id.uuid = "fixture-package-data".into();
        fixture.write("main.manual", "5\n6\n");
        assert!(
            fixture
                .pool
                .install_password(&plan, NEW, KeyCredential::Password(OLD))
                .await
                .is_err()
        );
        fixture.assert_no_mutation();
    }

    #[tokio::test]
    async fn invalid_new_password_and_probe_errors_are_not_mutations_or_secret_leaks() {
        let fixture = Fixture::new();
        let plan = fixture.plan().await;
        fixture.write("events", "");
        for new in ["", OLD] {
            assert!(
                fixture
                    .pool
                    .install_password(&plan, new, KeyCredential::Password(OLD))
                    .await
                    .is_err()
            );
            assert!(fixture.pool.retire_password(&plan, new).await.is_err());
        }
        fixture.write("fail-open-package-data", "4");
        let error = fixture
            .pool
            .install_password(&plan, NEW, KeyCredential::Password(OLD))
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::DiskManagement);
        for secret in [OLD, NEW] {
            assert!(!format!("{error:?}").contains(secret));
        }
        fixture.assert_no_mutation();
    }

    #[tokio::test]
    async fn headers_replace_atomically_and_sync_trim_failures_remain_retryable() {
        let fixture = Fixture::new();
        let plan = fixture.installed().await;
        let path = fixture
            .pool
            .config_dir
            .join(format!("luks/{GUID}_package-data.luks.bak"));
        let inode = std::fs::metadata(&path).unwrap().ino();
        fixture.write("fail-header-package-data", "");
        assert!(fixture.pool.retire_password(&plan, NEW).await.is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "obsolete");
        assert_eq!(std::fs::metadata(&path).unwrap().ino(), inode);
        fixture.remove("fail-header-package-data");
        for tool in ["sync", "fstrim"] {
            fixture.write(&format!("fail-{tool}"), "1");
            assert!(fixture.pool.retire_password(&plan, NEW).await.is_err());
            fixture.remove(&format!("fail-{tool}"));
        }
        fixture.pool.retire_password(&plan, NEW).await.unwrap();
        assert_ne!(std::fs::metadata(&path).unwrap().ino(), inode);
        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
        let events = fixture.events();
        assert!(events.rfind("luksKillSlot").unwrap() < events.find("luksHeaderBackup").unwrap());
        assert!(events.find("sync -f").unwrap() < events.find("fstrim ").unwrap());
        fixture.assert_clean();
    }

    #[test]
    fn metadata_parsers_preserve_enabled_slot_ids_and_reject_malformed_metadata() {
        let dump = "Key Slot 0: DISABLED\nKey Slot 1: ENABLED\nKey Slot 2: DISABLED\nKey Slot 3: DISABLED\nKey Slot 4: DISABLED\nKey Slot 5: DISABLED\nKey Slot 6: DISABLED\nKey Slot 7: ENABLED\n";
        assert_eq!(luks1_slots(dump).unwrap(), BTreeSet::from([1, 7]));
        assert_eq!(
            luks2_slots(br#"{"keyslots":{"1":{},"31":{}}}"#).unwrap(),
            BTreeSet::from([1, 31])
        );
        assert!(luks1_slots("Key Slot 0: ENABLED").is_err());
        assert!(luks2_slots(br#"{"keyslots":{"bad":{}}}"#).is_err());
    }

    fn real_pool(root: &Path) -> LuksPool {
        let mut pool = LuksPool::new(GUID, root.join("config")).unwrap();
        pool.devices = [root.join("main"), root.join("package-data")];
        pool.runtime_dir = root.to_owned();
        pool.fstrim = "true".into();
        pool
    }

    async fn format_real_pool(root: &Path, format: &str) -> LuksPool {
        std::fs::create_dir(root.join("config")).unwrap();
        let pool = real_pool(root);
        let (old_key, _old_cleanup) = pool.password_file(OLD).unwrap();
        let (manual_key, _manual_cleanup) = pool.password_file(MANUAL).unwrap();
        for device in &pool.devices {
            std::fs::File::create(device)
                .unwrap()
                .set_len(32 * 1024 * 1024)
                .unwrap();
            require_success(
                "luksFormat",
                cryptsetup_status(
                    Command::new("cryptsetup")
                        .args([
                            "luksFormat",
                            "--batch-mode",
                            "--type",
                            format,
                            "--pbkdf",
                            "pbkdf2",
                            "--pbkdf-force-iterations",
                            "1000",
                            "--key-file",
                        ])
                        .arg(&old_key)
                        .arg(device),
                )
                .await
                .unwrap(),
            )
            .unwrap();
            pool.add_and_verify(device, 5, &manual_key, &old_key, "--key-file")
                .await
                .unwrap();
        }
        pool
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "controlled subprocess for the managed LUKS/kernel-keyring fixture"]
    async fn cache_real_pool_keys_process() {
        let root = PathBuf::from(std::env::var_os("FDE_CACHE_POOL").unwrap());
        let pool = real_pool(&root);
        let (admin, keys) = pool.capture_unlock_keys(OLD).await.unwrap();
        pool.cache_unlock_keys(&keys).await.unwrap();
        std::fs::write(root.join("admin.json"), serde_json::to_vec(&admin).unwrap()).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires native cryptsetup and OS-root keyrings in an isolated managed guest"]
    async fn real_luks_cached_keys_reset_after_unlock_process_exits() {
        for format in ["luks1", "luks2"] {
            let directory = tempfile::tempdir_in("/dev/shm").unwrap();
            let root = directory.path();
            let pool = format_real_pool(root, format).await;
            assert!(pool.cached_unlock_keys().await.unwrap().is_none());
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "disk::fde::tests::cache_real_pool_keys_process",
                ])
                .env("FDE_CACHE_POOL", root)
                .invoke(ErrorKind::DiskManagement)
                .await
                .unwrap();
            let admin =
                serde_json::from_slice(&std::fs::read(root.join("admin.json")).unwrap()).unwrap();
            let keys = pool.cached_unlock_keys().await.unwrap().unwrap();
            let plan = pool.plan_password_change(&admin).await.unwrap();
            pool.install_password(&plan, NEW, KeyCredential::VolumeKeys(&keys))
                .await
                .unwrap();
            pool.verify_password(NEW).await.unwrap();
            pool.verify_password(MANUAL).await.unwrap();
            let ring = keyring::RootKeyring::open().unwrap();
            for key in keys.volumes {
                ring.remove(&key.id).unwrap();
            }
            assert!(pool.cached_unlock_keys().await.unwrap().is_none());
        }
    }

    #[tokio::test]
    #[ignore = "requires native cryptsetup, RAM-backed fixtures and protected process memory"]
    async fn real_luks_account_changes_resume_install_and_committed_retirement() {
        use crate::account::password::change_encrypted_password;
        use crate::account::{AccountInfo, LuksCredentials};
        use crate::db::model::Database;
        use crate::prelude::{PatchDb, TypedPatchDb};
        use crate::util::sync::SyncRwLock;

        for format in ["luks1", "luks2"] {
            let directory = tempfile::tempdir_in("/dev/shm").unwrap();
            let root = directory.path();
            let mut pool = format_real_pool(root, format).await;
            let memory = pool.protect_secret_handling().unwrap();
            let (admin, keys) = pool.capture_unlock_keys(OLD).await.unwrap();
            let initial_account =
                AccountInfo::new(OLD, std::time::SystemTime::now(), None).unwrap();
            let raw = PatchDb::open(root.join("account.db")).await.unwrap();
            let mut initial = Database::init(&initial_account, false, None, None).unwrap();
            initial.private.luks_credentials = Some(LuksCredentials::Stable { admin });
            let db = TypedPatchDb::<Database>::load_or_init(raw, || async { Ok(initial) })
                .await
                .unwrap();
            let account = SyncRwLock::new(initial_account);
            let wrapper = root.join("cryptsetup-account-fixture");
            std::fs::write(&wrapper, format!("#!/bin/sh\ncase \"$*\" in *package-data*) if [ \"$1\" = luksAddKey ] && [ -e '{0}/fail-add' ]; then exit 5; fi; if [ \"$1\" = luksKillSlot ] && [ -e '{0}/fail-kill' ]; then exit 5; fi;; esac\nexec /usr/sbin/cryptsetup \"$@\"\n", root.display())).unwrap();
            std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
            pool.cryptsetup = wrapper;
            std::fs::write(root.join("fail-add"), "").unwrap();
            assert!(
                change_encrypted_password(&db, &account, &pool, &keys, NEW)
                    .await
                    .is_err()
            );
            let snapshot = db.peek().await;
            crate::auth::check_password(&snapshot.as_private().as_password().de().unwrap(), OLD)
                .unwrap();
            assert!(matches!(
                snapshot.as_private().as_luks_credentials().de().unwrap(),
                Some(LuksCredentials::Installing { .. })
            ));
            drop(db);
            let db = TypedPatchDb::<Database>::load(
                PatchDb::open(root.join("account.db")).await.unwrap(),
            )
            .await
            .unwrap();
            let account = SyncRwLock::new(AccountInfo::load(&db.peek().await).unwrap());
            std::fs::remove_file(root.join("fail-add")).unwrap();
            std::fs::write(root.join("fail-kill"), "").unwrap();
            assert!(
                change_encrypted_password(&db, &account, &pool, &keys, OTHER)
                    .await
                    .is_err()
            );
            let snapshot = db.peek().await;
            crate::auth::check_password(&snapshot.as_private().as_password().de().unwrap(), OTHER)
                .unwrap();
            crate::auth::check_password(&account.read().password, OTHER).unwrap();
            assert!(matches!(
                snapshot.as_private().as_luks_credentials().de().unwrap(),
                Some(LuksCredentials::Retiring { .. })
            ));
            drop(db);
            let db = TypedPatchDb::<Database>::load(
                PatchDb::open(root.join("account.db")).await.unwrap(),
            )
            .await
            .unwrap();
            let account = SyncRwLock::new(AccountInfo::load(&db.peek().await).unwrap());
            std::fs::remove_file(root.join("fail-kill")).unwrap();
            change_encrypted_password(&db, &account, &pool, &keys, NEW)
                .await
                .unwrap();
            let snapshot = db.peek().await;
            crate::auth::check_password(&snapshot.as_private().as_password().de().unwrap(), NEW)
                .unwrap();
            let Some(LuksCredentials::Stable { admin }) =
                snapshot.as_private().as_luks_credentials().de().unwrap()
            else {
                panic!("retirement did not finish")
            };
            assert!(pool.accepts_admin_password(&admin, NEW).await.unwrap());
            pool.verify_password(NEW).await.unwrap();
            pool.verify_password(MANUAL).await.unwrap();
            assert!(pool.verify_password(OLD).await.is_err());
            assert!(pool.verify_password(OTHER).await.is_err());
            memory.check().unwrap();
        }
    }

    #[tokio::test]
    #[ignore = "requires cryptsetup; regular-file devices use a stub for fstrim"]
    async fn real_luks_keyslots_and_header_backups_retire_using_only_new_password() {
        for format in ["luks1", "luks2"] {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path();
            let mut pool = format_real_pool(root, format).await;
            let config = &pool.config_dir;
            let (admin, keys) = pool.capture_unlock_keys(OLD).await.unwrap();
            let fingerprints = keys.volumes.each_ref().map(|key| blake3::hash(&key.bytes));
            assert_eq!(admin.volumes[0].slots, BTreeSet::from([0]));
            let plan_a = pool.plan_password_change(&admin).await.unwrap();
            let wrapper = root.join("cryptsetup-fixture");
            std::fs::write(&wrapper, format!("#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{0}/arguments'\ncase \"$*\" in *package-data*) if [ \"$1\" = luksAddKey ] && [ -e '{0}/fail-add' ]; then exit 5; fi; if [ \"$1\" = luksKillSlot ] && [ -e '{0}/fail-kill' ]; then exit 5; fi;; esac\nexec /usr/sbin/cryptsetup \"$@\"\n", root.display())).unwrap();
            std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
            pool.cryptsetup = wrapper;
            std::fs::write(root.join("fail-add"), "").unwrap();
            assert!(
                pool.install_password(&plan_a, NEW, KeyCredential::VolumeKeys(&keys))
                    .await
                    .is_err()
            );
            std::fs::remove_file(root.join("fail-add")).unwrap();
            assert!(
                pool.install_password(&plan_a, OTHER, KeyCredential::VolumeKeys(&keys))
                    .await
                    .is_err()
            );
            pool.abort_password_change(&plan_a, &keys).await.unwrap();
            pool.abort_password_change(&plan_a, &keys).await.unwrap();
            assert!(pool.verify_password(NEW).await.is_err());
            pool.verify_password(OLD).await.unwrap();
            pool.verify_password(MANUAL).await.unwrap();
            let plan_b = pool.plan_password_change(&admin).await.unwrap();
            pool.install_password(&plan_b, OTHER, KeyCredential::VolumeKeys(&keys))
                .await
                .unwrap();
            pool.verify_password(OLD).await.unwrap();
            let persisted = serde_json::to_vec(&plan_b).unwrap();
            let new_admin = plan_b.new_admin_slots();
            drop(plan_b);
            drop(keys);
            let plan_b = serde_json::from_slice(&persisted).unwrap();
            std::fs::write(root.join("fail-kill"), "").unwrap();
            assert!(pool.retire_password(&plan_b, OTHER).await.is_err());
            std::fs::remove_file(root.join("fail-kill")).unwrap();
            pool.retire_password(&plan_b, OTHER).await.unwrap();
            pool.retire_password(&plan_b, OTHER).await.unwrap();
            pool.verify_password(OTHER).await.unwrap();
            pool.verify_password(MANUAL).await.unwrap();
            assert!(pool.verify_password(OLD).await.is_err());
            assert!(pool.verify_password(NEW).await.is_err());
            let (observed, recaptured) = pool.capture_unlock_keys(OTHER).await.unwrap();
            assert_eq!(observed, new_admin);
            assert!(
                recaptured
                    .volumes
                    .iter()
                    .zip(fingerprints)
                    .all(|(key, fingerprint)| blake3::hash(&key.bytes) == fingerprint)
            );
            let arguments = std::fs::read_to_string(root.join("arguments")).unwrap();
            for secret in [OLD, NEW, OTHER, MANUAL] {
                assert!(!arguments.contains(secret));
            }
            for (device, name) in pool.devices.iter().zip(&pool.mapper_names) {
                assert_eq!(
                    pool.metadata(device).await.unwrap().slots,
                    BTreeSet::from([1, 5])
                );
                let header = config.join(format!("luks/{name}.luks.bak"));
                for (password, expected) in [(OTHER, 0), (OLD, 2), (MANUAL, 0), (NEW, 2)] {
                    let (key, _cleanup) = pool.password_file(password).unwrap();
                    let status = cryptsetup_status(
                        Command::new("cryptsetup")
                            .args(["open", "--test-passphrase", "--batch-mode", "--header"])
                            .arg(&header)
                            .arg("--key-file")
                            .arg(key)
                            .arg(device),
                    )
                    .await
                    .unwrap();
                    assert_eq!(status.code(), Some(expected), "{format}: {name}");
                }
            }
            assert!(!std::fs::read_dir(root).unwrap().any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("startos-luks-")
            }));
        }
    }
}
