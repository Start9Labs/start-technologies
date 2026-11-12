//! Password-only access to both encrypted data LVs; TPM authorization belongs to the TLS identity.
//! Enrollment changes passphrases and replaces raw header backups in config without rotating volume
//! keys. Obsolete headers rely on Btrfs discard and NVMe DLFEAT=001b; this is neither general secure
//! erasure nor rollback protection. The caller owns enrollment eligibility, durable transition state,
//! account updates, and serialization with import and other password changes. Initial enrollment
//! requires UEFI, TPM2, and Secure Boot. Enrolled callers must supply the administrator password
//! explicitly, never retry with `DEFAULT_PASSWORD`.

use std::collections::BTreeSet;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};

use color_eyre::eyre::eyre;
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use super::main::DEFAULT_PASSWORD;
use crate::util::io::{delete_dir, delete_file, open_file, rename};
use crate::util::{GeneralBoxedGuard, GeneralGuard, Invoke, new_guid};
use crate::{Error, ErrorKind, ResultExt};

pub const HEADER_BACKUP_DIR: &str = "/media/startos/config/luks";

/// Nonsecret retirement intent to persist with the new account hash before retiring keys.
/// The caller must prevent keyslot reuse until retirement completes and the intent is cleared.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordRetirementPlan {
    volumes: [VolumeRetirement; 2],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct VolumeRetirement {
    mapper_name: String,
    uuid: String,
    slots: BTreeSet<u32>,
}

struct LuksMetadata {
    uuid: String,
    slots: BTreeSet<u32>,
}

/// An activated, encrypted legacy pool with `main` and `package-data` LVs.
/// `_UNENC` pools require a separate data-encryption migration before using this API.
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

    /// Installs and verifies the new password on both LVs, preserving existing keys.
    /// Persist the returned plan with the account update; retirement then needs only the new password.
    /// Retries accept partial installation and an already-retired old password wherever the new one
    /// works. A replacement must differ and cannot be empty or `DEFAULT_PASSWORD`.
    pub async fn prepare_password(
        &self,
        old: &str,
        new: &str,
    ) -> Result<PasswordRetirementPlan, Error> {
        validate_change(old, new)?;
        let (old_key, _old_cleanup) = self.password_file(old)?;
        let (new_key, _new_cleanup) = self.password_file(new)?;
        let mut installed = [false; 2];
        for (i, device) in self.devices.iter().enumerate() {
            installed[i] = self.accepts(device, &new_key, None).await?;
            if !installed[i] {
                require_password(self.accepts(device, &old_key, None).await?)?;
            }
        }
        let mut volumes = Vec::new();
        for (device, mapper_name) in self.devices.iter().zip(&self.mapper_names) {
            let metadata = self.metadata(device).await?;
            let mut slots = BTreeSet::new();
            for slot in metadata.slots {
                if self.accepts(device, &old_key, Some(slot)).await? {
                    slots.insert(slot);
                }
            }
            volumes.push(VolumeRetirement {
                mapper_name: mapper_name.clone(),
                uuid: metadata.uuid,
                slots,
            });
        }
        let plan = PasswordRetirementPlan {
            volumes: volumes.try_into().ok().unwrap(),
        };
        for (device, installed) in self.devices.iter().zip(installed) {
            if !installed {
                let status = cryptsetup_status(
                    Command::new(&self.cryptsetup)
                        .args(["luksAddKey", "--batch-mode", "--key-file"])
                        .arg(&old_key)
                        .arg(device)
                        .arg(&new_key),
                )
                .await?;
                require_success("luksAddKey", status)?;
            }
        }
        self.retirement_slots(&plan, &new_key).await?;
        Ok(plan)
    }

    /// Requires the plan and new account hash to be durably committed before retiring old keys.
    /// Resumes using only the new password, including after partial key retirement or publication.
    /// Completion means the planned slots are absent and config contains durable replacement headers.
    pub async fn retire_password(
        &self,
        plan: &PasswordRetirementPlan,
        new: &str,
    ) -> Result<(), Error> {
        validate_new_password(new)?;
        let (new_key, _new_cleanup) = self.password_file(new)?;
        let slots = self.retirement_slots(plan, &new_key).await?;
        for (device, slots) in self.devices.iter().zip(slots) {
            for slot in slots {
                let status = cryptsetup_status(
                    Command::new(&self.cryptsetup)
                        .args(["luksKillSlot", "--batch-mode", "--key-file"])
                        .arg(&new_key)
                        .arg(device)
                        .arg(slot.to_string()),
                )
                .await?;
                require_success("luksKillSlot", status)?;
            }
        }
        for (device, volume) in self.devices.iter().zip(&plan.volumes) {
            if !self
                .metadata(device)
                .await?
                .slots
                .is_disjoint(&volume.slots)
            {
                return Err(metadata_error());
            }
        }
        self.verify_key(&new_key).await?;
        for (device, mapper_name) in self.devices.iter().zip(&self.mapper_names) {
            replace_header_backup_with(
                &self.cryptsetup,
                device,
                mapper_name,
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

    /// Tests both LVs without creating device mappings or trying a fallback password.
    pub async fn verify_password(&self, password: &str) -> Result<(), Error> {
        let (key, _cleanup) = self.password_file(password)?;
        self.verify_key(&key).await
    }

    async fn retirement_slots(
        &self,
        plan: &PasswordRetirementPlan,
        key: &Path,
    ) -> Result<[BTreeSet<u32>; 2], Error> {
        let mut pending = [BTreeSet::new(), BTreeSet::new()];
        for (i, volume) in plan.volumes.iter().enumerate() {
            if volume.mapper_name != self.mapper_names[i] {
                return Err(metadata_error());
            }
            let device = &self.devices[i];
            let metadata = self.metadata(device).await?;
            if volume.uuid != metadata.uuid {
                return Err(metadata_error());
            }
            let mut retained = false;
            for slot in &metadata.slots {
                if self.accepts(device, key, Some(*slot)).await? {
                    if volume.slots.contains(slot) {
                        return Err(metadata_error());
                    }
                    retained = true;
                }
            }
            require_password(retained)?;
            pending[i] = volume
                .slots
                .intersection(&metadata.slots)
                .copied()
                .collect();
        }
        Ok(pending)
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
            .filter(|value| !value.is_empty())
            .ok_or_else(metadata_error)?
            .to_owned();
        let slots = match field("Version") {
            Some("1") => luks1_slots(dump)?,
            Some("2") => {
                let json = Command::new(&self.cryptsetup)
                    .env("LC_ALL", "C")
                    .args(["luksDump", "--dump-json-metadata"])
                    .arg(device)
                    .invoke(ErrorKind::DiskManagement)
                    .await?;
                luks2_slots(&json)?
            }
            _ => return Err(metadata_error()),
        };
        Ok(LuksMetadata { uuid, slots })
    }

    fn password_file(&self, password: &str) -> Result<(PathBuf, GeneralBoxedGuard), Error> {
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
        file.write_all(password.as_bytes())
            .with_kind(ErrorKind::Filesystem)?;
        file.flush().with_kind(ErrorKind::Filesystem)?;
        Ok((path, cleanup))
    }

    async fn verify_key(&self, key: &Path) -> Result<(), Error> {
        for device in &self.devices {
            require_password(self.accepts(device, key, None).await?)?;
        }
        Ok(())
    }

    async fn accepts(&self, device: &Path, key: &Path, slot: Option<u32>) -> Result<bool, Error> {
        let mut command = Command::new(&self.cryptsetup);
        command
            .args(["open", "--test-passphrase", "--batch-mode", "--key-file"])
            .arg(key);
        if let Some(slot) = slot {
            command.arg("--key-slot").arg(slot.to_string());
        }
        let status = cryptsetup_status(command.arg(device)).await?;
        match status.code() {
            Some(0) => Ok(true),
            // cryptsetup reports rejected passphrases with exit code 2.
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

fn validate_new_password(new: &str) -> Result<(), Error> {
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
op=$1
shift
key= header= device= slot= json=0
while (($#)); do
    case $1 in
        --key-file) key=$2; shift 2 ;;
        --header-backup-file) header=$2; shift 2 ;;
        --key-slot) slot=$2; shift 2 ;;
        --dump-json-metadata) json=1; shift ;;
        --*) shift ;;
        *)
            if [[ -z $device ]]; then device=$1
            elif [[ $op = luksKillSlot ]]; then slot=$1
            else key=$1; fi
            shift ;;
    esac
done
name=${device##*/}
if [[ -n $key ]]; then
    [[ $(stat -c %a "$key") = 600 ]]
    if cmp -s "$key" "$root/old-secret"; then credential=old
    elif cmp -s "$key" "$root/new-secret"; then credential=new
    else credential=unknown; fi
    # Deliberately hostile subprocess output must never reach errors or logs.
    cat "$key" >&2
fi
fail="$root/fail-$op-$name"
case $op in
    open)
        [[ ! -f $fail ]] || exit "$(cat "$fail")"
        printf 'test %s %s\n' "$name" "$credential" >> "$root/events"
        if [[ -z $slot ]]; then
            [[ -s "$root/$name.$credential" ]] && exit 0
        elif [[ -f "$root/$name.$credential" ]]; then
            grep -Fxq "$slot" "$root/$name.$credential" && exit 0
        fi
        exit 2 ;;
    luksDump)
        [[ ! -f $fail ]] || exit "$(cat "$fail")"
        if [[ -f "$root/$name.metadata" ]]; then cat "$root/$name.metadata"; exit 0; fi
        slots=$(cat "$root/$name.old" "$root/$name.new" 2>/dev/null | sort -nu) || true
        if [[ $json = 1 ]]; then
            printf '{"keyslots":{'
            separator=
            for id in $slots; do
                printf '%s"%s":{"type":"luks2"}' "$separator" "$id"
                separator=,
            done
            printf '}}\n'
        else
            version=2
            [[ ! -f "$root/$name.version" ]] || version=$(cat "$root/$name.version")
            printf 'LUKS header information for %s\nVersion: %s\nUUID: fixture-%s\n' "$name" "$version" "$name"
            if [[ $version = 1 ]]; then
                for id in {0..7}; do
                    state=DISABLED
                    if grep -Fxq "$id" <<< "$slots"; then state=ENABLED; fi
                    printf 'Key Slot %s: %s\n' "$id" "$state"
                done
            fi
        fi ;;
    luksAddKey)
        [[ ! -f $fail ]] || exit "$(cat "$fail")"
        [[ -s "$root/$name.old" && $credential = new ]]
        [[ ! -f "$root/skip-add-$name" ]] || exit 0
        printf '4\n' > "$root/$name.new" ;;
    luksKillSlot)
        [[ ! -f $fail ]] || exit "$(cat "$fail")"
        [[ $credential = new && -s "$root/$name.new" ]]
        for credential in old new; do
            grep -Fxv "$slot" "$root/$name.$credential" > "$root/remaining" || true
            mv "$root/remaining" "$root/$name.$credential"
        done ;;
    luksHeaderBackup)
        published="$root/config/luks/${GUID:-STARTOS_TEST}_$name.luks.bak"
        if [[ -f $published ]]; then
            printf 'previous %s %s\n' "$name" "$(cat "$published")" >> "$root/events"
        fi
        printf 'partial' > "$header"
        [[ ! -f $fail ]] || exit "$(cat "$fail")"
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
            std::fs::write(root.join("old-secret"), OLD).unwrap();
            std::fs::write(root.join("new-secret"), NEW).unwrap();
            std::fs::create_dir(root.join("keys")).unwrap();
            std::fs::create_dir_all(root.join("config/luks")).unwrap();
            let mut pool = LuksPool::new(GUID, root.join("config")).unwrap();
            pool.cryptsetup = root.join("cryptsetup");
            pool.sync = root.join("sync");
            pool.fstrim = root.join("fstrim");
            pool.runtime_dir = root.join("keys");
            for name in ["main", "package-data"] {
                std::fs::write(root.join(format!("{name}.old")), "0\n").unwrap();
                std::fs::write(root.join(format!("{name}.new")), "").unwrap();
                std::fs::write(
                    root.join(format!("config/luks/{GUID}_{name}.luks.bak")),
                    "obsolete",
                )
                .unwrap();
            }
            Self { dir, pool }
        }

        fn write(&self, path: &str, data: &str) {
            std::fs::write(self.dir.path().join(path), data).unwrap();
        }

        fn remove(&self, path: &str) {
            std::fs::remove_file(self.dir.path().join(path)).unwrap();
        }

        fn events(&self) -> String {
            std::fs::read_to_string(self.dir.path().join("events")).unwrap_or_default()
        }

        fn assert_clean(&self) {
            assert_eq!(
                std::fs::read_dir(&self.pool.runtime_dir).unwrap().count(),
                0
            );
            let headers = self.pool.config_dir.join("luks");
            assert_eq!(std::fs::read_dir(headers).unwrap().count(), 2);
            let events = self.events();
            assert!(!events.contains(OLD));
            assert!(!events.contains(NEW));
        }

        fn assert_no_mutation(&self) {
            let events = self.events();
            for operation in ["luksAddKey", "luksKillSlot", "luksHeaderBackup", "fstrim"] {
                assert!(!events.contains(operation), "{events}");
            }
            self.assert_clean();
        }
    }

    #[test]
    fn unencrypted_or_invalid_pool_is_rejected() {
        for guid in ["STARTOS_TEST_UNENC", "STARTOS_TEST/../other", "other"] {
            assert!(LuksPool::new(guid, "/config").is_err());
        }
        assert!(LuksPool::new("EMBASSY_TEST", "/config").is_ok());
    }

    #[tokio::test]
    async fn enrollment_orders_validation_installation_retirement_headers_and_trim() {
        let fixture = Fixture::new();
        fixture.write("main.version", "1");
        let plan = fixture.pool.prepare_password(OLD, NEW).await.unwrap();
        assert_eq!(plan.volumes[0].slots, BTreeSet::from([0]));
        assert_eq!(plan.volumes[1].slots, BTreeSet::from([0]));
        let events = fixture.events();
        assert!(events.find("test package-data old").unwrap() < events.find("luksAddKey").unwrap());
        assert_eq!(events.matches("luksAddKey").count(), 2);
        assert!(!events.contains("luksKillSlot"));
        assert!(!events.contains("luksHeaderBackup"));
        fixture.assert_clean();
        fixture.pool.retire_password(&plan, NEW).await.unwrap();
        fixture.pool.verify_password(NEW).await.unwrap();
        assert!(fixture.pool.verify_password(OLD).await.is_err());
        let events = fixture.events();
        assert!(events.rfind("luksKillSlot").unwrap() < events.find("luksHeaderBackup").unwrap());
        assert!(events.rfind("luksHeaderBackup").unwrap() < events.find("sync -f").unwrap());
        assert!(events.find("sync -f").unwrap() < events.find("fstrim ").unwrap());
        for name in ["main", "package-data"] {
            let path = fixture
                .pool
                .config_dir
                .join(format!("luks/{GUID}_{name}.luks.bak"));
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                format!("header:{name}\n")
            );
            assert_eq!(std::fs::metadata(path).unwrap().mode() & 0o777, 0o600);
        }
        fixture.assert_clean();
    }

    #[tokio::test]
    async fn bad_credentials_are_rejected_before_any_mutation() {
        let fixture = Fixture::new();
        fixture.write("package-data.old", "");
        assert!(fixture.pool.prepare_password(OLD, NEW).await.is_err());
        fixture.assert_no_mutation();
    }

    #[tokio::test]
    async fn second_volume_failure_resumes_without_adding_duplicate_keys() {
        let fixture = Fixture::new();
        fixture.write("fail-luksAddKey-package-data", "5");
        let error = fixture.pool.prepare_password(OLD, NEW).await.unwrap_err();
        assert!(!format!("{error:?}").contains(NEW));
        assert!(!fixture.events().contains("luksKillSlot"));
        fixture.assert_clean();
        fixture.remove("fail-luksAddKey-package-data");
        let plan = fixture.pool.prepare_password(OLD, NEW).await.unwrap();
        let events = fixture.events();
        assert_eq!(
            events
                .lines()
                .filter(|l| l.contains("luksAddKey") && l.contains("/main "))
                .count(),
            1
        );
        fixture.pool.retire_password(&plan, NEW).await.unwrap();
        fixture.assert_clean();
    }

    #[tokio::test]
    async fn persisted_plan_resumes_partial_retirement_without_old_password() {
        let fixture = Fixture::new();
        fixture.write("package-data.version", "1");
        let plan = fixture.pool.prepare_password(OLD, NEW).await.unwrap();
        let persisted = serde_json::to_vec(&plan).unwrap();
        let serialized = std::str::from_utf8(&persisted).unwrap();
        assert!(!serialized.contains(OLD));
        assert!(!serialized.contains(NEW));
        drop(plan);
        fixture.remove("old-secret");
        let plan = serde_json::from_slice(&persisted).unwrap();
        fixture.write("fail-luksKillSlot-package-data", "5");
        assert!(fixture.pool.retire_password(&plan, NEW).await.is_err());
        assert!(!fixture.events().contains("luksHeaderBackup"));
        fixture.assert_clean();
        fixture.remove("fail-luksKillSlot-package-data");
        fixture.pool.retire_password(&plan, NEW).await.unwrap();
        fixture.pool.retire_password(&plan, NEW).await.unwrap();
        assert_eq!(fixture.events().matches("luksAddKey").count(), 2);
        assert_eq!(fixture.events().matches("luksKillSlot").count(), 3);
        fixture.assert_clean();
    }

    #[tokio::test]
    async fn old_key_already_retired_on_one_volume_does_not_block_installing_the_other() {
        let fixture = Fixture::new();
        fixture.write("main.new", "4\n");
        fixture.write("main.old", "");
        let plan = fixture.pool.prepare_password(OLD, NEW).await.unwrap();
        assert_eq!(fixture.events().matches("luksAddKey").count(), 1);
        fixture.pool.retire_password(&plan, NEW).await.unwrap();
        fixture.assert_clean();
    }

    #[tokio::test]
    async fn all_old_matching_slots_are_retired() {
        let fixture = Fixture::new();
        fixture.write("main.old", "0\n2\n");
        let plan = fixture.pool.prepare_password(OLD, NEW).await.unwrap();
        fixture.pool.retire_password(&plan, NEW).await.unwrap();
        assert_eq!(fixture.events().matches("luksKillSlot").count(), 3);
        fixture.assert_clean();
    }

    #[tokio::test]
    async fn operational_probe_failure_is_not_a_password_mismatch() {
        let fixture = Fixture::new();
        fixture.write("fail-open-package-data", "4");
        let error = fixture.pool.prepare_password(OLD, NEW).await.unwrap_err();
        assert_eq!(error.kind, ErrorKind::DiskManagement);
        assert!(!format!("{error:?}").contains(NEW));
        assert!(!format!("{error:?}").contains(OLD));
        fixture.assert_no_mutation();
    }

    #[tokio::test]
    async fn invalid_new_password_does_not_start_subprocesses() {
        let fixture = Fixture::new();
        let plan = fixture.pool.prepare_password(OLD, NEW).await.unwrap();
        fixture.write("events", "");
        for new in ["", DEFAULT_PASSWORD] {
            assert!(fixture.pool.prepare_password(OLD, new).await.is_err());
            assert!(fixture.pool.retire_password(&plan, new).await.is_err());
        }
        assert!(fixture.pool.prepare_password(NEW, NEW).await.is_err());
        assert!(fixture.events().is_empty());
        fixture.assert_clean();
    }

    #[tokio::test]
    async fn failed_header_copy_preserves_old_file_and_resumes_after_retirement() {
        let fixture = Fixture::new();
        let plan = fixture.pool.prepare_password(OLD, NEW).await.unwrap();
        let path = fixture
            .pool
            .config_dir
            .join(format!("luks/{GUID}_package-data.luks.bak"));
        let old_inode = std::fs::metadata(&path).unwrap().ino();
        fixture.write("fail-luksHeaderBackup-package-data", "5");
        assert!(fixture.pool.retire_password(&plan, NEW).await.is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "obsolete");
        assert_eq!(std::fs::metadata(&path).unwrap().ino(), old_inode);
        assert!(!fixture.events().contains("sync -f"));
        assert!(!fixture.events().contains("fstrim "));
        fixture.assert_clean();
        fixture.remove("fail-luksHeaderBackup-package-data");
        fixture.pool.retire_password(&plan, NEW).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "header:package-data\n"
        );
        assert_ne!(std::fs::metadata(&path).unwrap().ino(), old_inode);
        assert!(fixture.events().contains("previous package-data obsolete"));
        fixture.assert_clean();
    }

    #[tokio::test]
    async fn interrupted_header_staging_is_removed_before_completion() {
        let fixture = Fixture::new();
        let plan = fixture.pool.prepare_password(OLD, NEW).await.unwrap();
        let luks = fixture.pool.config_dir.join("luks");
        for name in ["main", "package-data"] {
            fixture.write(
                &format!("config/luks/.{GUID}_{name}.luks.bak.tmp"),
                "obsolete header",
            );
            let staging = luks.join(format!(".{GUID}_{name}.luks.bak.staging"));
            std::fs::create_dir(&staging).unwrap();
            std::fs::write(staging.join("header"), "obsolete header").unwrap();
        }
        fixture.pool.retire_password(&plan, NEW).await.unwrap();
        fixture.assert_clean();
    }

    #[tokio::test]
    async fn successful_add_is_verified_before_preparation_completes() {
        let fixture = Fixture::new();
        fixture.write("skip-add-package-data", "");
        assert!(fixture.pool.prepare_password(OLD, NEW).await.is_err());
        assert!(!fixture.events().contains("luksKillSlot"));
        assert!(!fixture.events().contains("luksHeaderBackup"));
        fixture.assert_clean();
    }

    #[tokio::test]
    async fn enrolled_administrator_password_changes_without_default_password() {
        let fixture = Fixture::new();
        let old = "previous-administrator-credential";
        fixture.write("old-secret", old);
        let plan = fixture.pool.prepare_password(old, NEW).await.unwrap();
        fixture.pool.retire_password(&plan, NEW).await.unwrap();
        fixture.pool.verify_password(NEW).await.unwrap();
        assert!(fixture.pool.verify_password(old).await.is_err());
        assert!(!fixture.events().contains(old));
        fixture.assert_clean();
    }

    #[tokio::test]
    async fn retirement_validates_both_volumes_before_deleting_slots() {
        let fixture = Fixture::new();
        let plan = fixture.pool.prepare_password(OLD, NEW).await.unwrap();
        fixture.write("package-data.new", "");
        fixture.write("events", "");
        assert!(fixture.pool.retire_password(&plan, NEW).await.is_err());
        fixture.assert_no_mutation();
    }

    #[tokio::test]
    async fn retirement_rejects_a_plan_targeting_any_current_password_slot() {
        let fixture = Fixture::new();
        let mut plan = fixture.pool.prepare_password(OLD, NEW).await.unwrap();
        fixture.write("package-data.new", "4\n5\n");
        plan.volumes[1].slots.insert(4);
        fixture.write("events", "");
        assert!(fixture.pool.retire_password(&plan, NEW).await.is_err());
        fixture.assert_no_mutation();
        fixture.pool.verify_password(NEW).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(fixture.dir.path().join("main.old")).unwrap(),
            "0\n"
        );
    }

    #[tokio::test]
    async fn retirement_rejects_a_plan_for_a_different_luks_device() {
        let fixture = Fixture::new();
        let mut plan = fixture.pool.prepare_password(OLD, NEW).await.unwrap();
        plan.volumes[1].uuid = "another-device".into();
        fixture.write("events", "");
        assert!(fixture.pool.retire_password(&plan, NEW).await.is_err());
        fixture.assert_no_mutation();
        plan.volumes[0].mapper_name = "another-pool_main".into();
        assert!(fixture.pool.retire_password(&plan, NEW).await.is_err());
        fixture.assert_no_mutation();
    }

    #[tokio::test]
    async fn malformed_metadata_fails_before_installation() {
        let fixture = Fixture::new();
        fixture.write("package-data.metadata", "not a LUKS dump");
        assert!(fixture.pool.prepare_password(OLD, NEW).await.is_err());
        fixture.assert_no_mutation();
    }

    #[test]
    fn luks_metadata_parsers_preserve_noncontiguous_enabled_slot_ids() {
        let dump = "LUKS header information for /dev/example\nVersion:        1\nUUID: example\nKey Slot 0: DISABLED\nKey Slot 1: ENABLED\n        Iterations: 123\nKey Slot 2: DISABLED\nKey Slot 3: DISABLED\nKey Slot 4: DISABLED\nKey Slot 5: DISABLED\nKey Slot 6: DISABLED\nKey Slot 7: ENABLED\n";
        assert_eq!(luks1_slots(dump).unwrap(), BTreeSet::from([1, 7]));
        assert_eq!(luks2_slots(br#"{"keyslots":{"1":{"type":"luks2"},"31":{"type":"luks2"}},"tokens":{},"segments":{}}"#).unwrap(), BTreeSet::from([1, 31]));
        for malformed in [
            "Key Slot 0: ENABLED",
            "Key Slot 8: ENABLED",
            "Key Slot 0: UNKNOWN",
        ] {
            assert!(luks1_slots(malformed).is_err());
        }
        for malformed in [
            b"{}".as_slice(),
            br#"{"keyslots":{"invalid":{}}}"#,
            br#"{"keyslots":{"01":{}}}"#,
        ] {
            assert!(luks2_slots(malformed).is_err());
        }
    }

    #[tokio::test]
    #[ignore = "requires cryptsetup; regular-file devices use a stub for fstrim"]
    async fn real_luks_keyslots_and_header_backups_retire_using_only_new_password() {
        for format in ["luks1", "luks2"] {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path();
            let config = root.join("config");
            std::fs::create_dir(&config).unwrap();
            let mut pool = LuksPool::new(GUID, &config).unwrap();
            pool.devices = [root.join("main"), root.join("package-data")];
            pool.runtime_dir = root.to_owned();
            pool.fstrim = "true".into();
            let (old_key, old_cleanup) = pool.password_file(OLD).unwrap();
            for (device, name) in pool.devices.iter().zip(&pool.mapper_names) {
                std::fs::File::create(device)
                    .unwrap()
                    .set_len(32 * 1024 * 1024)
                    .unwrap();
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
                    .arg(device)
                    .invoke(ErrorKind::DiskManagement)
                    .await
                    .unwrap();
                replace_header_backup(device, name, &config.join("luks"))
                    .await
                    .unwrap();
            }
            let plan = pool.prepare_password(OLD, NEW).await.unwrap();
            assert_eq!(plan.volumes[0].slots, BTreeSet::from([0]));
            assert_eq!(plan.volumes[1].slots, BTreeSet::from([0]));
            let persisted = serde_json::to_vec(&plan).unwrap();
            drop(plan);
            drop(old_cleanup);
            assert!(!old_key.exists());
            let (new_key, new_cleanup) = pool.password_file(NEW).unwrap();
            let status = cryptsetup_status(
                Command::new("cryptsetup")
                    .args(["luksKillSlot", "--batch-mode", "--key-file"])
                    .arg(&new_key)
                    .arg(&pool.devices[0])
                    .arg("0"),
            )
            .await
            .unwrap();
            assert!(status.success());
            drop(new_cleanup);
            let plan = serde_json::from_slice(&persisted).unwrap();
            pool.retire_password(&plan, NEW).await.unwrap();
            pool.retire_password(&plan, NEW).await.unwrap();
            pool.verify_password(NEW).await.unwrap();
            let (new_key, _new_cleanup) = pool.password_file(NEW).unwrap();
            let (old_key, _old_cleanup) = pool.password_file(OLD).unwrap();
            for (device, name) in pool.devices.iter().zip(&pool.mapper_names) {
                assert!(!pool.accepts(device, &old_key, None).await.unwrap());
                let header = config.join(format!("luks/{name}.luks.bak"));
                assert_eq!(std::fs::metadata(&header).unwrap().mode() & 0o777, 0o600);
                for (key, expected) in [(&new_key, 0), (&old_key, 2)] {
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
        }
    }

    #[tokio::test]
    async fn sync_and_trim_failures_require_retry_before_completion() {
        for tool in ["sync", "fstrim"] {
            let fixture = Fixture::new();
            let plan = fixture.pool.prepare_password(OLD, NEW).await.unwrap();
            fixture.write(&format!("fail-{tool}"), "1");
            assert!(fixture.pool.retire_password(&plan, NEW).await.is_err());
            if tool == "sync" {
                assert!(!fixture.events().contains("fstrim "));
            }
            fixture.remove(&format!("fail-{tool}"));
            fixture.pool.retire_password(&plan, NEW).await.unwrap();
            assert_eq!(fixture.events().matches("luksKillSlot").count(), 2);
            fixture.assert_clean();
        }
    }
}
