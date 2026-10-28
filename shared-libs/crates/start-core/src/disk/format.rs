use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use clap::Parser;
use gpt::disk::LogicalBlockSize;
use gpt::partition_types::LINUX_FS;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::context::RpcContext;
use crate::disk::util::{
    get_capacity, get_logical_block_size, get_mount_source, get_type, is_startos_pool_guid, pvscan,
};
use crate::disk::{LIVE_MEDIUM_PATH, OsPartitionInfo};
use crate::os_install::gpt::{
    Device, PARTITION_ALIGNMENT_LBA, fresh_gpt, largest_aligned_free_bytes,
};
use crate::os_install::partition_for;
use crate::os_install::quiesce::{list_partitions, update_partition_table};
use crate::prelude::*;
use crate::util::Invoke;
use crate::util::io::read_file_to_string;
use crate::util::serde::{WithIoFormat, display_serializable};

pub const DEFAULT_LABEL: &str = "StartOS Backups";
/// Btrfs labels reserve the final byte for NUL.
const MAX_LABEL_BYTES: usize = 255;
/// `mkfs.btrfs` refuses devices under about 109 MiB and recommends its mixed
/// mode below 1 GiB, so smaller drives are refused before anything is erased.
const MIN_CAPACITY_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Deserialize, Serialize, Parser, VisitFields)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct FormatDiskParams {
    #[arg(help = "help.arg.format-disk")]
    pub disk: PathBuf,
    #[arg(long, help = "help.arg.format-label")]
    pub label: Option<String>,
}

rpc_toolkit::reflect_ts!(FormatDiskParams);

/// Erases a whole drive into one Btrfs partition and returns that partition's node.
pub async fn format(
    ctx: RpcContext,
    FormatDiskParams { disk, label }: FormatDiskParams,
) -> Result<PathBuf, Error> {
    let label = label.unwrap_or_else(|| DEFAULT_LABEL.to_owned());
    validate_label(&label)?;
    let disk = whole_drive(&disk).await?;
    refuse_protected(&ctx.os_partitions, &disk).await?;
    let capacity = get_capacity(&disk).await?;
    check_capacity(&disk, capacity)?;
    let sector_size = get_logical_block_size(&disk).await?;
    let block_size = LogicalBlockSize::try_from(sector_size).map_err(|_| {
        refused(t!(
            "disk.format.unsupported-sector-size",
            size = sector_size
        ))
    })?;

    // Detached from the request: a dropped connection does not stop it mid-way.
    tokio::spawn(erase_and_partition(disk, capacity, block_size, label))
        .await
        .with_kind(ErrorKind::Unknown)?
}

async fn erase_and_partition(
    disk: PathBuf,
    capacity: u64,
    block_size: LogicalBlockSize,
    label: String,
) -> Result<PathBuf, Error> {
    Command::new("wipefs")
        .arg("-a")
        .arg(&disk)
        .invoke(ErrorKind::DiskManagement)
        .await?;
    let gpt_disk = disk.clone();
    let name = label.clone();
    tokio::task::spawn_blocking(move || {
        let device: Device = Box::new(
            std::fs::File::options()
                .read(true)
                .write(true)
                .open(&gpt_disk)?,
        );
        write_single_partition(device, capacity, block_size, &name)
    })
    .await
    .unwrap()?;
    update_partition_table(&disk).await?;

    let partition = partition_for(&disk, 1);
    Command::new("wipefs")
        .arg("-a")
        .arg(&partition)
        .invoke(ErrorKind::DiskManagement)
        .await?;
    create_backup_filesystem(&partition, &label).await?;
    Command::new("udevadm")
        .arg("settle")
        .invoke(ErrorKind::DiskManagement)
        .await?;
    Ok(partition)
}

async fn create_backup_filesystem(partition: &Path, label: &str) -> Result<(), Error> {
    Command::new("mkfs.btrfs")
        .arg("-q")
        .arg("-f")
        .arg("-L")
        .arg(label)
        .arg(partition)
        .invoke(ErrorKind::DiskManagement)
        .await?;
    Ok(())
}

pub fn display_formatted(
    params: WithIoFormat<FormatDiskParams>,
    partition: PathBuf,
) -> Result<(), Error> {
    if let Some(format) = params.format {
        return display_serializable(format, partition);
    }
    println!(
        "{}",
        t!("disk.format.formatted", partition = partition.display())
    );
    Ok(())
}

pub(crate) fn write_single_partition(
    device: Device,
    capacity: u64,
    block_size: LogicalBlockSize,
    name: &str,
) -> Result<(), Error> {
    let mut gpt = fresh_gpt(device, capacity, block_size)?;
    let size = largest_aligned_free_bytes(&gpt)?;
    gpt.add_partition(name, size, LINUX_FS, 0, Some(PARTITION_ALIGNMENT_LBA))?;
    gpt.write()?;
    Ok(())
}

fn validate_label(label: &str) -> Result<(), Error> {
    if label.is_empty() {
        return Err(refused(t!("disk.format.label-empty")));
    }
    if label.len() > MAX_LABEL_BYTES {
        return Err(refused(t!(
            "disk.format.label-too-long",
            max = MAX_LABEL_BYTES
        )));
    }
    if label.chars().any(char::is_control) {
        return Err(refused(t!("disk.format.label-control-characters")));
    }
    Ok(())
}

fn check_capacity(disk: &Path, capacity: u64) -> Result<(), Error> {
    if capacity < MIN_CAPACITY_BYTES {
        return Err(refused(t!("disk.format.too-small", disk = disk.display())));
    }
    Ok(())
}

fn refused(message: impl std::fmt::Display) -> Error {
    Error::new(eyre!("{message}"), ErrorKind::InvalidRequest)
}

/// The canonical node of what `lsblk` types as `disk`.
async fn whole_drive(path: &Path) -> Result<PathBuf, Error> {
    let disk = crate::util::io::canonicalize(path, false).await?;
    if get_type(&disk).await? != "disk" {
        return Err(Error::new(
            eyre!("{}", t!("disk.format.not-a-drive", disk = path.display())),
            ErrorKind::BlockDevice,
        ));
    }
    Ok(disk)
}

async fn canonical(path: &Path) -> PathBuf {
    tokio::fs::canonicalize(path)
        .await
        .unwrap_or_else(|_| path.to_owned())
}

async fn refuse_protected(os: &OsPartitionInfo, disk: &Path) -> Result<(), Error> {
    let mut devices = vec![disk.to_owned()];
    devices.extend(list_partitions(disk).await?);

    if devices
        .iter()
        .any(|d| os.contains(d) || os.data.as_deref() == Some(d))
    {
        return Err(refused(t!(
            "disk.format.system-drive",
            disk = disk.display()
        )));
    }
    for (pv, vg) in pvscan().await? {
        if vg.as_deref().is_some_and(is_startos_pool_guid)
            && devices.contains(&canonical(&pv).await)
        {
            return Err(refused(t!(
                "disk.format.startos-data",
                disk = disk.display()
            )));
        }
    }
    if let Ok(Some(live)) = get_mount_source(LIVE_MEDIUM_PATH).await {
        if devices.contains(&live) {
            return Err(refused(t!(
                "disk.format.live-medium",
                disk = disk.display()
            )));
        }
    }
    if let Some(by) = in_use(&devices).await? {
        return Err(refused(t!(
            "disk.format.in-use",
            disk = disk.display(),
            by = by
        )));
    }
    Ok(())
}

/// The mountpoint, swap, or holder device keeping one of them busy.
async fn in_use(devices: &[PathBuf]) -> Result<Option<String>, Error> {
    let devices: BTreeSet<&Path> = devices.iter().map(PathBuf::as_path).collect();
    for (table, usage) in [("/proc/mounts", None), ("/proc/swaps", Some("swap"))] {
        let content = read_file_to_string(table).await?;
        for line in content.lines() {
            let mut fields = line.split_whitespace();
            let Some(source) = fields.next().filter(|s| s.starts_with('/')) else {
                continue;
            };
            if devices.contains(canonical(Path::new(source)).await.as_path()) {
                return Ok(Some(
                    usage.or_else(|| fields.next()).unwrap_or(table).to_owned(),
                ));
            }
        }
    }
    for dev in devices {
        let Some(name) = dev.file_name() else {
            continue;
        };
        let holders = Path::new("/sys/class/block").join(name).join("holders");
        let Ok(mut entries) = tokio::fs::read_dir(&holders).await else {
            continue;
        };
        if let Some(holder) = entries.next_entry().await? {
            return Ok(Some(
                Path::new("/dev")
                    .join(holder.file_name())
                    .display()
                    .to_string(),
            ));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use gpt::GptConfig;

    use super::*;

    #[test]
    fn labels_are_validated_before_formatting() {
        for label in [
            DEFAULT_LABEL,
            "1234567890123456",
            "éééééééé",
            "Mum’s Backups!",
            "Family’s Backups",
            &"a".repeat(255),
            &format!("{}a", "é".repeat(127)),
        ] {
            assert!(validate_label(label).is_ok(), "{label:?}");
        }
        for label in [
            "",
            &"a".repeat(256),
            &"é".repeat(128),
            "bad\0label",
            "bad\nlabel",
            "bad\tlabel",
            "bad\u{7f}label",
            "bad\u{85}label",
        ] {
            assert!(validate_label(label).is_err(), "{label:?}");
        }
    }

    #[test]
    fn drives_under_one_gib_are_refused() {
        let disk = Path::new("/dev/sdz");
        for capacity in [0, 114_294_784, MIN_CAPACITY_BYTES - 1] {
            assert!(check_capacity(disk, capacity).is_err(), "{capacity}");
        }
        for capacity in [MIN_CAPACITY_BYTES, 8 * MIN_CAPACITY_BYTES] {
            assert!(check_capacity(disk, capacity).is_ok(), "{capacity}");
        }
    }

    #[tokio::test]
    async fn backup_filesystem_is_btrfs_with_the_requested_label() {
        let image = tempfile::NamedTempFile::new().unwrap();
        image.as_file().set_len(256 * 1024 * 1024).unwrap();
        let label = format!("{}a", "é".repeat(127));
        validate_label(&label).unwrap();
        create_backup_filesystem(image.path(), &label)
            .await
            .unwrap();

        Command::new("btrfs")
            .arg("check")
            .arg("--readonly")
            .arg(image.path())
            .invoke(ErrorKind::DiskManagement)
            .await
            .unwrap();
        let actual_label = Command::new("btrfs")
            .arg("filesystem")
            .arg("label")
            .arg(image.path())
            .invoke(ErrorKind::DiskManagement)
            .await
            .unwrap();
        assert_eq!(String::from_utf8(actual_label).unwrap().trim(), label);
    }

    #[test]
    fn single_partition_spans_the_disk() {
        for (block_size, size_gb) in [LogicalBlockSize::Lb512, LogicalBlockSize::Lb4096]
            .into_iter()
            .flat_map(|block_size| [8u64, 64, 2048].map(|size| (block_size, size)))
        {
            let path = std::env::temp_dir().join(format!(
                "startos-format-test-{}-{size_gb}-{block_size}.img",
                std::process::id(),
            ));
            let capacity = size_gb * 1024 * 1024 * 1024;
            std::fs::File::create(&path)
                .unwrap()
                .set_len(capacity)
                .unwrap();

            let device: Device = Box::new(
                std::fs::File::options()
                    .read(true)
                    .write(true)
                    .open(&path)
                    .unwrap(),
            );
            write_single_partition(device, capacity, block_size, DEFAULT_LABEL).unwrap();

            let dev = Box::new(std::fs::File::options().read(true).open(&path).unwrap());
            let gpt = GptConfig::new()
                .writable(false)
                .logical_block_size(block_size)
                .open_from_device(dev)
                .unwrap();
            let parts = gpt.partitions();
            assert_eq!(parts.len(), 1, "{size_gb} GiB");
            let part = parts.get(&1).unwrap();
            assert_eq!(part.name, DEFAULT_LABEL);
            assert_eq!(part.part_type_guid, LINUX_FS);
            assert_eq!(part.first_lba, PARTITION_ALIGNMENT_LBA, "{size_gb} GiB");
            let reserved_end_sectors = match block_size {
                LogicalBlockSize::Lb512 => 34,
                LogicalBlockSize::Lb4096 => 6,
            };
            assert_eq!(
                part.last_lba,
                capacity / u64::from(block_size) - reserved_end_sectors,
                "{size_gb} GiB, {block_size}-byte sectors",
            );

            std::fs::remove_file(&path).ok();
        }
    }
}
