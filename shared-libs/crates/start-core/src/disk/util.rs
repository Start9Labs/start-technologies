use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use color_eyre::eyre::{self, eyre};
use futures::TryStreamExt;
use nom::bytes::complete::{tag, take_till1};
use nom::character::complete::multispace1;
use nom::combinator::{opt, rest};
use nom::sequence::{pair, preceded, terminated};
use nom::{AsChar, IResult, Parser};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::instrument;

use super::mount::filesystem::ReadOnly;
use super::mount::filesystem::block_dev::BlockDev;
use super::mount::guard::TmpMountGuard;
use crate::disk::OsPartitionInfo;
use crate::disk::mount::guard::GenericMountGuard;
use crate::hostname::ServerHostname;
use crate::prelude::*;
use crate::util::Invoke;
use crate::util::serde::read_json_file_bounded;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PartitionTable {
    Mbr,
    Gpt,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub logicalname: PathBuf,
    pub stable_path: PathBuf,
    pub partition_table: Option<PartitionTable>,
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub partitions: Vec<PartitionInfo>,
    pub capacity: u64,
    pub guid: Option<InternedString>,
    pub filesystem: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PartitionInfo {
    pub logicalname: PathBuf,
    pub stable_path: PathBuf,
    pub label: Option<String>,
    #[ts(type = "number")]
    pub capacity: u64,
    #[ts(type = "number | null")]
    pub used: Option<u64>,
    #[ts(type = "number | null")]
    pub available: Option<u64>,
    pub start_os: BTreeMap<String, StartOsRecoveryInfo>,
    pub legacy_backup: bool,
    pub guid: Option<InternedString>,
    pub filesystem: Option<String>,
}

pub async fn has_legacy_backup(mountpoint: impl AsRef<Path>, server_id: &str) -> bool {
    tokio::fs::metadata(
        mountpoint
            .as_ref()
            .join(super::LEGACY_BACKUP_DIR_NAME)
            .join(server_id),
    )
    .await
    .map(|m| m.is_dir())
    .unwrap_or(false)
}

/// Contains key material; client responses use `StartOsRecoveryInfo`.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupUnencryptedMetadata {
    pub hostname: ServerHostname,
    pub version: exver::Version,
    pub timestamp: DateTime<Utc>,
    pub password_hash: Option<String>,
    pub wrapped_key: Option<String>,
}
impl From<BackupUnencryptedMetadata> for StartOsRecoveryInfo {
    fn from(meta: BackupUnencryptedMetadata) -> Self {
        Self {
            hostname: meta.hostname,
            version: meta.version,
            timestamp: meta.timestamp,
            scheduled: false,
            server_id: None,
            has_system_backup: Some(true),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct StartOsRecoveryInfo {
    pub hostname: ServerHostname,
    #[ts(type = "string")]
    pub version: exver::Version,
    #[ts(type = "string")]
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub scheduled: bool,
    #[serde(default)]
    #[ts(optional)]
    pub server_id: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub has_system_backup: Option<bool>,
}

const DISK_PATH: &str = "/dev/disk/by-path";
const SYS_BLOCK_PATH: &str = "/sys/block";
/// GPT and MBR partition types reported by `lsblk -no PARTTYPE`.
const ESP_PART_TYPES: [&str; 2] = ["c12a7328-f81f-11d2-ba4b-00a0c93ec93b", "0xef"];

/// Recovery metadata contains only scalar identity and key-wrapping fields.
pub(crate) const MAX_BACKUP_RECOVERY_METADATA_BYTES: u64 = 1024 * 1024;
/// Encrypted target metadata may contain histories for many services and snapshots.
pub(crate) const MAX_BACKUP_TARGET_METADATA_BYTES: u64 = 64 * 1024 * 1024;
const MAX_BACKUP_RECOVERY_ENTRIES: usize = 1024;

lazy_static::lazy_static! {
    static ref PARTITION_REGEX: Regex = Regex::new("-part[0-9]+$").unwrap();
}

#[instrument(skip_all)]
pub async fn get_partition_table<P: AsRef<Path>>(path: P) -> Result<Option<PartitionTable>, Error> {
    Ok(String::from_utf8(
        Command::new("fdisk")
            .arg("-l")
            .arg(path.as_ref())
            .invoke(crate::ErrorKind::BlockDevice)
            .await?,
    )?
    .lines()
    .find_map(|l| l.strip_prefix("Disklabel type:"))
    .and_then(|t| match t.trim() {
        "dos" => Some(PartitionTable::Mbr),
        "gpt" => Some(PartitionTable::Gpt),
        _ => None,
    }))
}

#[instrument(skip_all)]
pub async fn get_vendor<P: AsRef<Path>>(path: P) -> Result<Option<String>, Error> {
    let vendor = tokio::fs::read_to_string(
        Path::new(SYS_BLOCK_PATH)
            .join(path.as_ref().strip_prefix("/dev").map_err(|_| {
                Error::new(
                    eyre!("{}", t!("disk.util.not-canonical-block-device")),
                    crate::ErrorKind::BlockDevice,
                )
            })?)
            .join("device")
            .join("vendor"),
    )
    .await?
    .trim()
    .to_owned();
    Ok(if vendor.is_empty() {
        None
    } else {
        Some(vendor)
    })
}

#[instrument(skip_all)]
pub async fn get_model<P: AsRef<Path>>(path: P) -> Result<Option<String>, Error> {
    let model = tokio::fs::read_to_string(
        Path::new(SYS_BLOCK_PATH)
            .join(path.as_ref().strip_prefix("/dev").map_err(|_| {
                Error::new(
                    eyre!("{}", t!("disk.util.not-canonical-block-device")),
                    crate::ErrorKind::BlockDevice,
                )
            })?)
            .join("device")
            .join("model"),
    )
    .await?
    .trim()
    .to_owned();
    Ok(if model.is_empty() { None } else { Some(model) })
}

#[instrument(skip_all)]
pub async fn get_capacity<P: AsRef<Path>>(path: P) -> Result<u64, Error> {
    let path = path.as_ref();
    let canonical = tokio::fs::canonicalize(path)
        .await
        .with_ctx(|_| (crate::ErrorKind::BlockDevice, path.display().to_string()))?;
    let name = canonical.file_name().ok_or_else(|| {
        Error::new(
            eyre!("invalid block device path: {}", path.display()),
            crate::ErrorKind::BlockDevice,
        )
    })?;
    let size_path = Path::new("/sys/class/block").join(name).join("size");
    // Sysfs counts 512-byte sectors whatever the device's logical block size.
    let sectors: u64 = tokio::fs::read_to_string(&size_path)
        .await
        .with_ctx(|_| {
            (
                crate::ErrorKind::BlockDevice,
                size_path.display().to_string(),
            )
        })?
        .trim()
        .parse()?;
    Ok(sectors * 512)
}

#[instrument(skip_all)]
pub async fn get_label<P: AsRef<Path>>(path: P) -> Result<Option<String>, Error> {
    let label = String::from_utf8(
        Command::new("lsblk")
            .arg("-no")
            .arg("label")
            .arg(path.as_ref())
            .invoke(crate::ErrorKind::BlockDevice)
            .await?,
    )?
    .trim()
    .to_owned();
    Ok(if label.is_empty() { None } else { Some(label) })
}

#[instrument(skip_all)]
pub async fn get_part_type<P: AsRef<Path>>(path: P) -> Result<Option<String>, Error> {
    let part_type = String::from_utf8(
        Command::new("lsblk")
            .arg("-no")
            .arg("parttype")
            .arg(path.as_ref())
            .invoke(crate::ErrorKind::BlockDevice)
            .await?,
    )?
    .trim()
    .to_owned();
    Ok(if part_type.is_empty() {
        None
    } else {
        Some(part_type)
    })
}

#[instrument(skip_all)]
pub async fn get_used<P: AsRef<Path>>(path: P) -> Result<u64, Error> {
    Ok(String::from_utf8(
        Command::new("df")
            .arg("--output=used")
            .arg("--block-size=1")
            .arg(path.as_ref())
            .invoke(crate::ErrorKind::Filesystem)
            .await?,
    )?
    .lines()
    .skip(1)
    .next()
    .unwrap_or_default()
    .trim()
    .parse::<u64>()?)
}

#[instrument(skip_all)]
pub async fn get_available<P: AsRef<Path>>(path: P) -> Result<u64, Error> {
    Ok(String::from_utf8(
        Command::new("df")
            .arg("--output=avail")
            .arg("--block-size=1")
            .arg(path.as_ref())
            .invoke(crate::ErrorKind::Filesystem)
            .await?,
    )?
    .lines()
    .skip(1)
    .next()
    .unwrap_or_default()
    .trim()
    .parse::<u64>()?)
}

#[instrument(skip_all)]
pub async fn get_percentage<P: AsRef<Path>>(path: P) -> Result<u64, Error> {
    Ok(String::from_utf8(
        Command::new("df")
            .arg("--output=pcent")
            .arg(path.as_ref())
            .invoke(crate::ErrorKind::Filesystem)
            .await?,
    )?
    .lines()
    .skip(1)
    .next()
    .unwrap_or_default()
    .trim()
    .strip_suffix("%")
    .unwrap()
    .parse::<u64>()?)
}

#[instrument(skip_all)]
pub async fn pvscan() -> Result<BTreeMap<PathBuf, Option<InternedString>>, Error> {
    let pvscan_out = Command::new("pvscan")
        .invoke(crate::ErrorKind::DiskManagement)
        .await?;
    let pvscan_out_str = std::str::from_utf8(&pvscan_out)?;
    Ok(parse_pvscan_output(pvscan_out_str))
}

pub async fn recovery_info(
    mountpoint: impl AsRef<Path>,
) -> Result<BTreeMap<String, StartOsRecoveryInfo>, Error> {
    recovery_info_with_limit(mountpoint.as_ref(), MAX_BACKUP_RECOVERY_ENTRIES).await
}

async fn recovery_info_with_limit(
    mountpoint: &Path,
    max_entries: usize,
) -> Result<BTreeMap<String, StartOsRecoveryInfo>, Error> {
    let backup_root = mountpoint.join(super::BACKUP_DIR_NAME);
    let mut res = BTreeMap::new();
    if tokio::fs::metadata(&backup_root).await.is_ok() {
        let mut dir = tokio::fs::read_dir(&backup_root).await?;
        let mut entry_count = 0usize;
        while let Some(entry) = dir.next_entry().await? {
            entry_count = entry_count.saturating_add(1);
            if entry_count > max_entries {
                return Err(Error::new(
                    eyre!(
                        "{}",
                        t!(
                            "disk.util.too-many-recovery-entries",
                            limit = max_entries,
                            path = backup_root.display()
                        )
                    ),
                    ErrorKind::Backup,
                ));
            }
            let server_id = entry.file_name().to_string_lossy().into_owned();
            if server_id.ends_with(".automatic") {
                let base_server_id = server_id.trim_end_matches(".automatic").to_owned();
                let metadata_path = entry.path().join("unencrypted-metadata.json");
                if tokio::fs::metadata(&metadata_path).await.is_ok() {
                    let scheduled: crate::backup::scheduled::ScheduledBackupRecoveryInfo =
                        read_json_file_bounded(&metadata_path, MAX_BACKUP_RECOVERY_METADATA_BYTES)
                            .await?;
                    res.insert(
                        server_id,
                        StartOsRecoveryInfo {
                            hostname: scheduled.hostname,
                            version: scheduled.version,
                            timestamp: scheduled.timestamp,
                            scheduled: true,
                            server_id: Some(base_server_id),
                            has_system_backup: scheduled.has_system_backup,
                        },
                    );
                }
                continue;
            }
            let backup_unencrypted_metadata_path = backup_root
                .join(&server_id)
                .join("unencrypted-metadata.json");
            if tokio::fs::metadata(&backup_unencrypted_metadata_path)
                .await
                .is_ok()
            {
                let metadata: BackupUnencryptedMetadata = read_json_file_bounded(
                    &backup_unencrypted_metadata_path,
                    MAX_BACKUP_RECOVERY_METADATA_BYTES,
                )
                .await?;
                let mut info: StartOsRecoveryInfo = metadata.into();
                info.server_id = Some(server_id.clone());
                res.insert(server_id, info);
            }
        }
    }

    Ok(res)
}

#[instrument(skip_all)]
pub async fn get_mount_source(mountpoint: impl AsRef<Path>) -> Result<Option<PathBuf>, Error> {
    let mounts_content = tokio::fs::read_to_string("/proc/mounts")
        .await
        .with_ctx(|_| (crate::ErrorKind::Filesystem, "/proc/mounts"))?;

    let mountpoint = mountpoint.as_ref();
    for line in mounts_content.lines() {
        let mut parts = line.split_whitespace();
        let source = parts.next();
        let mount = parts.next();
        if let (Some(source), Some(mount)) = (source, mount) {
            if Path::new(mount) == mountpoint {
                if let Ok(canonical) = tokio::fs::canonicalize(source).await {
                    return Ok(Some(canonical));
                }
            }
        }
    }
    Ok(None)
}

#[instrument(skip_all)]
pub async fn list(os: &OsPartitionInfo, server_id: Option<&str>) -> Result<Vec<DiskInfo>, Error> {
    struct DiskIndex {
        stable_path: PathBuf,
        parts: BTreeMap<PathBuf, PathBuf>,
        internal: bool,
    }
    let disk_guids = pvscan().await?;
    let disks = tokio_stream::wrappers::ReadDirStream::new(
        tokio::fs::read_dir(DISK_PATH)
            .await
            .with_ctx(|_| (crate::ErrorKind::Filesystem, DISK_PATH))?,
    )
    .map_err(|e| {
        Error::new(
            eyre::Error::from(e).wrap_err(DISK_PATH),
            crate::ErrorKind::Filesystem,
        )
    })
    .try_fold(
        BTreeMap::<PathBuf, DiskIndex>::new(),
        |mut disks, dir_entry| async move {
            if dir_entry.file_type().await?.is_dir() {
                return Ok(disks);
            }
            if let Some(disk_path) = dir_entry.path().file_name().and_then(|s| s.to_str()) {
                let (disk_path, part_path) = if let Some(end) = PARTITION_REGEX.find(disk_path) {
                    (
                        disk_path.strip_suffix(end.as_str()).unwrap_or_default(),
                        Some(disk_path),
                    )
                } else {
                    (disk_path, None)
                };
                let stable_path = Path::new(DISK_PATH).join(disk_path);
                let disk = tokio::fs::canonicalize(&stable_path).await.with_ctx(|_| {
                    (
                        crate::ErrorKind::Filesystem,
                        stable_path.display().to_string(),
                    )
                })?;
                let part = if let Some(part_path) = part_path {
                    let stable_part_path = Path::new(DISK_PATH).join(part_path);
                    let part = tokio::fs::canonicalize(&stable_part_path)
                        .await
                        .with_ctx(|_| {
                            (
                                crate::ErrorKind::Filesystem,
                                stable_part_path.display().to_string(),
                            )
                        })?;
                    Some((part, stable_part_path))
                } else {
                    None
                };
                let index = disks.entry(disk.clone()).or_insert_with(|| DiskIndex {
                    stable_path: stable_path.clone(),
                    parts: BTreeMap::new(),
                    internal: false,
                });
                if stable_path < index.stable_path {
                    index.stable_path = stable_path;
                }
                if let Some((part, stable_part_path)) = part {
                    if os.contains(&part) {
                        index.internal = true;
                    } else {
                        index
                            .parts
                            .entry(part)
                            .and_modify(|path| {
                                if stable_part_path < *path {
                                    *path = stable_part_path.clone();
                                }
                            })
                            .or_insert(stable_part_path);
                    }
                }
            }
            Ok(disks)
        },
    )
    .await?;

    let mut res = Vec::with_capacity(disks.len());
    for (disk, index) in disks {
        if index.internal {
            for (part, stable_part_path) in index.parts {
                let mut disk_info = disk_info(disk.clone(), index.stable_path.clone()).await;
                if let Some(g) = disk_guids.get(&part) {
                    let pi = lvm_pv_part_info(part, stable_part_path, g.clone()).await;
                    disk_info.logicalname = pi.logicalname;
                    disk_info.stable_path = pi.stable_path;
                    disk_info.capacity = pi.capacity;
                    disk_info.guid = pi.guid;
                    disk_info.filesystem = pi.filesystem;
                } else {
                    let Some(part_info) = part_info(part, stable_part_path, server_id).await else {
                        continue;
                    };
                    disk_info.logicalname = part_info.logicalname.clone();
                    disk_info.stable_path = part_info.stable_path.clone();
                    disk_info.capacity = part_info.capacity;
                    disk_info.partitions = vec![part_info];
                }
                res.push(disk_info);
            }
        } else {
            let mut disk_info = disk_info(disk, index.stable_path).await;
            disk_info.partitions = Vec::with_capacity(index.parts.len());
            if let Some(g) = disk_guids.get(&disk_info.logicalname) {
                disk_info.guid = g.clone();
                if let Some(guid) = g {
                    disk_info.filesystem = crate::disk::main::probe_package_data_fs(guid)
                        .await
                        .unwrap_or_else(|e| {
                            tracing::warn!("Failed to probe filesystem for {guid}: {e}");
                            None
                        });
                }
            } else {
                for (part, stable_part_path) in index.parts {
                    let part_info = if let Some(g) = disk_guids.get(&part) {
                        lvm_pv_part_info(part, stable_part_path, g.clone()).await
                    } else {
                        let Some(pi) = part_info(part, stable_part_path, server_id).await else {
                            continue;
                        };
                        pi
                    };
                    disk_info.partitions.push(part_info);
                }
            }
            res.push(disk_info);
        }
    }

    Ok(res)
}

async fn disk_info(disk: PathBuf, stable_path: PathBuf) -> DiskInfo {
    let partition_table = get_partition_table(&disk)
        .await
        .map_err(|e| {
            tracing::warn!(
                "{}",
                t!(
                    "disk.util.could-not-get-partition-table",
                    disk = disk.display(),
                    error = e.source
                )
            )
        })
        .unwrap_or_default();
    let vendor = get_vendor(&disk)
        .await
        .map_err(|e| {
            tracing::warn!(
                "{}",
                t!(
                    "disk.util.could-not-get-vendor",
                    disk = disk.display(),
                    error = e.source
                )
            )
        })
        .unwrap_or_default();
    let model = get_model(&disk)
        .await
        .map_err(|e| {
            tracing::warn!(
                "{}",
                t!(
                    "disk.util.could-not-get-model",
                    disk = disk.display(),
                    error = e.source
                )
            )
        })
        .unwrap_or_default();
    let capacity = get_capacity(&disk)
        .await
        .map_err(|e| {
            tracing::warn!(
                "{}",
                t!(
                    "disk.util.could-not-get-capacity",
                    disk = disk.display(),
                    error = e.source
                )
            )
        })
        .unwrap_or_default();
    DiskInfo {
        logicalname: disk,
        stable_path,
        partition_table,
        vendor,
        model,
        partitions: Vec::new(),
        capacity,
        guid: None,
        filesystem: None,
    }
}

async fn lvm_pv_part_info(
    part: PathBuf,
    stable_path: PathBuf,
    guid: Option<InternedString>,
) -> PartitionInfo {
    let capacity = get_capacity(&part)
        .await
        .map_err(|e| {
            tracing::warn!(
                "{}",
                t!(
                    "disk.util.could-not-get-capacity-part",
                    part = part.display(),
                    error = e.source
                )
            )
        })
        .unwrap_or_default();
    let filesystem = if let Some(ref guid) = guid {
        crate::disk::main::probe_package_data_fs(guid)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to probe filesystem for {guid}: {e}");
                None
            })
    } else {
        None
    };
    PartitionInfo {
        logicalname: part,
        stable_path,
        label: None,
        capacity,
        used: None,
        available: None,
        start_os: BTreeMap::new(),
        legacy_backup: false,
        guid,
        filesystem,
    }
}

async fn part_info(
    part: PathBuf,
    stable_path: PathBuf,
    server_id: Option<&str>,
) -> Option<PartitionInfo> {
    let part_type = get_part_type(&part)
        .await
        .map_err(|e| {
            tracing::warn!(
                "{}",
                t!(
                    "disk.util.could-not-get-part-type",
                    part = part.display(),
                    error = e.source
                )
            )
        })
        .unwrap_or_default();
    if part_type
        .as_deref()
        .is_some_and(|t| ESP_PART_TYPES.contains(&t))
    {
        tracing::debug!(
            "{}",
            t!("disk.util.skipping-efi-partition", part = part.display())
        );
        return None;
    }

    let label = get_label(&part)
        .await
        .map_err(|e| {
            tracing::warn!(
                "{}",
                t!(
                    "disk.util.could-not-get-label",
                    part = part.display(),
                    error = e.source
                )
            )
        })
        .unwrap_or_default();
    let capacity = get_capacity(&part)
        .await
        .map_err(|e| {
            tracing::warn!(
                "{}",
                t!(
                    "disk.util.could-not-get-capacity-part",
                    part = part.display(),
                    error = e.source
                )
            )
        })
        .unwrap_or_default();

    let mount_guard = match TmpMountGuard::mount(&BlockDev::new(&part), ReadOnly).await {
        Err(e) => {
            tracing::warn!(
                "{}",
                t!(
                    "disk.util.skipping-unmountable-partition",
                    part = part.display(),
                    error = e.source
                )
            );
            return None;
        }
        Ok(g) => g,
    };

    let used = get_used(mount_guard.path())
        .await
        .map_err(|e| {
            tracing::warn!(
                "{}",
                t!(
                    "disk.util.could-not-get-usage",
                    part = part.display(),
                    error = e.source
                )
            )
        })
        .ok();
    let available = get_available(mount_guard.path())
        .await
        .map_err(|e| {
            tracing::warn!(
                "{}",
                t!(
                    "disk.util.could-not-get-usage",
                    part = part.display(),
                    error = e.source
                )
            )
        })
        .ok();
    let start_os = match recovery_info(mount_guard.path()).await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!(
                "{}",
                t!("disk.util.error-fetching-backup-metadata", error = e)
            );
            BTreeMap::new()
        }
    };
    let legacy_backup = match server_id {
        Some(server_id) => has_legacy_backup(mount_guard.path(), server_id).await,
        None => false,
    };
    if let Err(e) = mount_guard.unmount().await {
        tracing::error!(
            "{}",
            t!(
                "disk.util.error-unmounting-partition",
                part = part.display(),
                error = e
            )
        );
    }

    Some(PartitionInfo {
        logicalname: part,
        stable_path,
        label,
        capacity,
        used,
        available,
        start_os,
        legacy_backup,
        guid: None,
        filesystem: None,
    })
}

fn parse_pvscan_output(pvscan_output: &str) -> BTreeMap<PathBuf, Option<InternedString>> {
    fn parse_line(line: &str) -> IResult<&str, (&str, Option<&str>)> {
        let pv_parse = preceded(
            tag("  PV "),
            terminated(take_till1(|c: char| c.is_space()), multispace1),
        );
        let vg_parse = preceded(
            opt(tag("is in exported ")),
            preceded(
                tag("VG "),
                terminated(take_till1(|c: char| c.is_space()), multispace1),
            ),
        );
        let mut parser = terminated(pair(pv_parse, opt(vg_parse)), rest);
        parser.parse(line)
    }
    let lines = pvscan_output.lines();
    let n = lines.clone().count();
    let entries = lines.take(n.saturating_sub(1));
    let mut ret = BTreeMap::new();
    for entry in entries {
        match parse_line(entry) {
            Ok((_, (pv, vg))) => {
                ret.insert(PathBuf::from(pv), vg.map(InternedString::intern));
            }
            Err(_) => {
                tracing::warn!("{}", t!("disk.util.failed-to-parse-pvscan", line = entry));
            }
        }
    }
    ret
}

#[test]
fn test_pvscan_parser() {
    let s1 = r#"  PV /dev/mapper/cryptdata   VG data            lvm2 [1.81 TiB / 0    free]
  PV /dev/sdb                                   lvm2 [931.51 GiB]
  Total: 2 [2.72 TiB] / in use: 1 [1.81 TiB] / in no VG: 1 [931.51 GiB]
"#;
    let s2 = r#"  PV /dev/sdb   VG EMBASSY_LZHJAENWGPCJJL6C6AXOD7OOOIJG7HFBV4GYRJH6HADXUCN4BRWQ   lvm2 [931.51 GiB / 0    free]
  Total: 1 [931.51 GiB] / in use: 1 [931.51 GiB] / in no VG: 0 [0   ]
"#;
    let s3 = r#"  PV /dev/mapper/cryptdata   VG data            lvm2 [1.81 TiB / 0    free]
  Total: 1 [1.81 TiB] / in use: 1 [1.81 TiB] / in no VG: 0 [0   ]
"#;
    let s4 = r#"  PV /dev/sda    is in exported VG EMBASSY_ZFHOCTYV3ZJMJW3OTFMG55LSQZLP667EDNZKDNUJKPJX5HE6S5HQ [931.51 GiB / 0    free]
  Total: 1 [931.51 GiB] / in use: 1 [931.51 GiB] / in no VG: 0 [0   ]
"#;
    println!("{:?}", parse_pvscan_output(s1));
    println!("{:?}", parse_pvscan_output(s2));
    println!("{:?}", parse_pvscan_output(s3));
    println!("{:?}", parse_pvscan_output(s4));
}

#[tokio::test]
async fn recovery_info_rejects_oversized_scheduled_and_legacy_metadata() {
    let root = PathBuf::from(format!(
        "/tmp/start-core-recovery-metadata-{}",
        std::process::id()
    ));
    let backup_root = root.join(super::BACKUP_DIR_NAME);
    let oversized = vec![b' '; MAX_BACKUP_RECOVERY_METADATA_BYTES as usize + 1];

    let scheduled = backup_root.join("server.automatic");
    tokio::fs::create_dir_all(&scheduled).await.unwrap();
    tokio::fs::write(scheduled.join("unencrypted-metadata.json"), &oversized)
        .await
        .unwrap();
    let error = recovery_info(&root).await.unwrap_err();
    assert_eq!(error.kind, ErrorKind::Filesystem);
    assert!(error.to_string().contains("size limit"));

    tokio::fs::remove_dir_all(&backup_root).await.unwrap();
    let legacy = backup_root.join("server");
    tokio::fs::create_dir_all(&legacy).await.unwrap();
    tokio::fs::write(legacy.join("unencrypted-metadata.json"), &oversized)
        .await
        .unwrap();
    let error = recovery_info(&root).await.unwrap_err();
    assert_eq!(error.kind, ErrorKind::Filesystem);
    assert!(error.to_string().contains("size limit"));

    tokio::fs::remove_dir_all(root).await.unwrap();
}

#[tokio::test]
async fn recovery_info_rejects_entry_limit_plus_one() {
    let root = PathBuf::from(format!(
        "/tmp/start-core-recovery-entries-{}",
        std::process::id()
    ));
    let backup_root = root.join(super::BACKUP_DIR_NAME);
    tokio::fs::create_dir_all(&backup_root).await.unwrap();
    for entry in ["one", "two", "three"] {
        tokio::fs::create_dir(backup_root.join(entry))
            .await
            .unwrap();
    }

    let error = recovery_info_with_limit(&root, 2).await.unwrap_err();
    assert_eq!(error.kind, ErrorKind::Backup);
    assert!(
        error.to_string().contains(
            t!(
                "disk.util.too-many-recovery-entries",
                limit = 2,
                path = backup_root.display()
            )
            .as_ref()
        )
    );

    tokio::fs::remove_dir_all(root).await.unwrap();
}

#[tokio::test]
async fn recovery_info_marks_scheduled_backups() {
    let root = PathBuf::from(format!(
        "/tmp/start-core-scheduled-recovery-info-{}",
        std::process::id()
    ));
    let scheduled = root.join(super::BACKUP_DIR_NAME).join("server.automatic");
    tokio::fs::create_dir_all(&scheduled).await.unwrap();
    tokio::fs::write(
        scheduled.join("unencrypted-metadata.json"),
        serde_json::to_vec(&serde_json::json!({
            "targetInstanceId": "target",
            "hostname": "server",
            "version": "0.4.0",
            "timestamp": "2026-01-01T00:00:00Z",
            "passwordHash": "hash",
            "wrappedKey": "key",
            "hasSystemBackup": true
        }))
        .unwrap(),
    )
    .await
    .unwrap();
    let service_only = root
        .join(super::BACKUP_DIR_NAME)
        .join("service-only.automatic");
    tokio::fs::create_dir_all(&service_only).await.unwrap();
    tokio::fs::write(
        service_only.join("unencrypted-metadata.json"),
        serde_json::to_vec(&serde_json::json!({
            "targetInstanceId": "target",
            "hostname": "service-only",
            "version": "0.4.0",
            "timestamp": "2026-01-01T00:00:00Z",
            "passwordHash": "hash",
            "wrappedKey": "key",
            "hasSystemBackup": false
        }))
        .unwrap(),
    )
    .await
    .unwrap();

    let info = recovery_info(&root).await.unwrap();
    assert!(info["server.automatic"].scheduled);
    assert_eq!(
        info["server.automatic"].server_id.as_deref(),
        Some("server")
    );
    assert_eq!(info["server.automatic"].has_system_backup, Some(true));
    assert!(info["service-only.automatic"].scheduled);
    assert_eq!(
        info["service-only.automatic"].has_system_backup,
        Some(false)
    );

    tokio::fs::remove_dir_all(root).await.unwrap();
}
