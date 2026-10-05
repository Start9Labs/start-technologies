use std::path::{Path, PathBuf};

use gpt::GptConfig;
use gpt::disk::LogicalBlockSize;

use crate::disk::OsPartitionInfo;
use crate::os_install::quiesce::{quiesce_disk, update_partition_table};
use crate::os_install::{partition_for, same_device};
use crate::prelude::*;

const ROOT_TARGET_BYTES: u64 = 14 * 1024 * 1024 * 1024;
/// Floor for elastic-root shrink when a protected partition is in the way.
const ROOT_MIN_BYTES: u64 = 12 * 1024 * 1024 * 1024;
/// 1 MiB at 512-byte sectors.
const PARTITION_ALIGNMENT_LBA: u64 = 2048;

/// Synchronous core of `partition()`: writes a fresh GPT to `device` placing
/// efi/bios + boot + root, then either re-creates the protected data partition
/// at its existing LBAs or fills the trailing free space with a new one.
///
/// Split out so unit tests can drive it against a sparse file without going
/// through `quiesce_disk` / `partx --update`, which need a real block device.
fn build_gpt_layout(
    device: Box<dyn gpt::DiskDevice + Send>,
    capacity: u64,
    disk_path: &Path,
    protected_partition_info: Option<(u64, u64, PathBuf)>,
    use_efi: bool,
) -> Result<(bool, Option<PathBuf>), Error> {
    let mut device = device;
    let mbr = gpt::mbr::ProtectiveMBR::with_lb_size(
        u32::try_from((capacity / 512) - 1).unwrap_or(0xFF_FF_FF_FF),
    );
    mbr.overwrite_lba0(&mut device)?;
    let mut gpt = GptConfig::new()
        .writable(true)
        .logical_block_size(LogicalBlockSize::Lb512)
        .create_from_device(device, None)?;

    gpt.update_partitions(Default::default())?;

    let efi = if use_efi {
        gpt.add_partition(
            "efi",
            100 * 1024 * 1024,
            gpt::partition_types::EFI,
            0,
            Some(PARTITION_ALIGNMENT_LBA),
        )?;
        true
    } else {
        gpt.add_partition(
            "bios-grub",
            8 * 1024 * 1024,
            gpt::partition_types::BIOS,
            0,
            Some(PARTITION_ALIGNMENT_LBA),
        )?;
        false
    };
    gpt.add_partition(
        "boot",
        2 * 1024 * 1024 * 1024,
        gpt::partition_types::LINUX_FS,
        0,
        Some(PARTITION_ALIGNMENT_LBA),
    )?;

    // Elastic root: shrink within [ROOT_MIN_BYTES, ROOT_TARGET_BYTES] if a
    // protected partition's first_lba sits inside the planned root region.
    let lb_size: u64 = (*gpt.logical_block_size()).into();
    let target_root_lba = ROOT_TARGET_BYTES.div_ceil(lb_size);
    let min_root_lba = ROOT_MIN_BYTES.div_ceil(lb_size);
    let root_size_bytes = if let Some((protect_first_lba, _, ref path)) = protected_partition_info {
        // Pick the rightmost free region before the protected partition: it's
        // the slot abutting the protected partition, so the only candidate that
        // bounds how big root can grow without overlapping. Earlier slivers
        // (e.g. the 2014-LBA gap that 2048-alignment leaves between LBA 34 and
        // the EFI partition) would over-report `available_lba` and let root
        // land past `protect_first_lba`, after which add_partition_at fails
        // with NotEnoughSpace.
        let last_free_start = gpt
            .find_free_sectors()
            .into_iter()
            .filter(|(start, _)| *start < protect_first_lba)
            .map(|(start, _)| start)
            .max()
            .ok_or_else(|| {
                Error::new(
                    eyre!("{}", t!("os-install.no-free-space-for-os-root")),
                    crate::ErrorKind::BlockDevice,
                )
            })?;
        let aligned_start = last_free_start.next_multiple_of(PARTITION_ALIGNMENT_LBA);
        let available_lba = protect_first_lba.saturating_sub(aligned_start);
        if available_lba < min_root_lba {
            let available_bytes = available_lba.saturating_mul(lb_size);
            return Err(Error::new(
                eyre!(
                    "{}",
                    t!(
                        "os-install.protected-partition-overlaps-os-root-bytes",
                        path = path.display(),
                        first_lba = protect_first_lba,
                        available_bytes = available_bytes,
                        min_bytes = ROOT_MIN_BYTES,
                    )
                ),
                crate::ErrorKind::DiskManagement,
            ));
        }
        available_lba.min(target_root_lba).saturating_mul(lb_size)
    } else {
        ROOT_TARGET_BYTES
    };
    gpt.add_partition(
        "root",
        root_size_bytes,
        match crate::ARCH {
            "x86_64" => gpt::partition_types::LINUX_ROOT_X64,
            "aarch64" => gpt::partition_types::LINUX_ROOT_ARM_64,
            _ => gpt::partition_types::LINUX_FS,
        },
        0,
        Some(PARTITION_ALIGNMENT_LBA),
    )?;

    let data_part = if let Some((first_lba, last_lba, path)) = protected_partition_info {
        // Re-create the data partition entry at the same location
        let length_lba = last_lba - first_lba + 1;
        let next_id = gpt.partitions().keys().max().map(|k| k + 1).unwrap_or(1);
        gpt.add_partition_at(
            "data",
            next_id,
            first_lba,
            length_lba,
            gpt::partition_types::LINUX_LVM,
            0,
        )?;
        Some(path)
    } else {
        // Account for alignment offset so add_partition's length check passes
        // when the largest free block isn't already 2048-aligned.
        let lb_size_bytes: u64 = (*gpt.logical_block_size()).into();
        let data_size_bytes = gpt
            .find_free_sectors()
            .iter()
            .map(|(start, length_lba)| {
                let alignment_offset = (PARTITION_ALIGNMENT_LBA
                    - (start % PARTITION_ALIGNMENT_LBA))
                    % PARTITION_ALIGNMENT_LBA;
                length_lba.saturating_sub(alignment_offset) * lb_size_bytes
            })
            .max()
            .filter(|s| *s > 0)
            .ok_or_else(|| {
                Error::new(
                    eyre!("No free space left on device"),
                    crate::ErrorKind::BlockDevice,
                )
            })?;
        gpt.add_partition(
            "data",
            data_size_bytes,
            gpt::partition_types::LINUX_LVM,
            0,
            Some(PARTITION_ALIGNMENT_LBA),
        )?;
        gpt.partitions()
            .last_key_value()
            .map(|(num, _)| partition_for(disk_path, *num))
    };

    gpt.write()?;

    Ok((efi, data_part))
}

pub async fn partition(
    disk_path: &Path,
    capacity: u64,
    protect: Option<&Path>,
    use_efi: bool,
) -> Result<OsPartitionInfo, Error> {
    // Guard: cannot protect the whole disk
    if let Some(p) = protect {
        if p == disk_path {
            return Err(Error::new(
                eyre!(
                    "Cannot protect the entire disk {}; must specify a partition",
                    disk_path.display()
                ),
                crate::ErrorKind::InvalidRequest,
            ));
        }
    }

    quiesce_disk(disk_path).await?;

    let disk_path = disk_path.to_owned();
    let disk_path_clone = disk_path.clone();
    let protect = protect.map(|p| p.to_owned());
    let (efi, data_part) = tokio::task::spawn_blocking(move || {
        let disk_path = disk_path_clone;

        let protected_partition_info: Option<(u64, u64, PathBuf)> =
            if let Some(ref protect_path) = protect {
                let existing_gpt = GptConfig::new()
                    .writable(false)
                    .logical_block_size(LogicalBlockSize::Lb512)
                    .open_from_device(Box::new(
                        std::fs::File::options().read(true).open(&disk_path)?,
                    ))?;
                let info = existing_gpt
                    .partitions()
                    .iter()
                    .find(|(num, _)| same_device(&partition_for(&disk_path, **num), protect_path))
                    .map(|(_, p)| (p.first_lba, p.last_lba, protect_path.clone()));
                if info.is_none() {
                    return Err(Error::new(
                        eyre!(
                            "Protected partition {} not found in GPT on {}",
                            protect_path.display(),
                            disk_path.display()
                        ),
                        crate::ErrorKind::NotFound,
                    ));
                }
                info
            } else {
                None
            };

        let device = Box::new(
            std::fs::File::options()
                .read(true)
                .write(true)
                .open(&disk_path)?,
        );

        build_gpt_layout(
            device,
            capacity,
            &disk_path,
            protected_partition_info,
            use_efi,
        )
    })
    .await
    .unwrap()?;

    update_partition_table(&disk_path).await?;

    let mut extra_boot = std::collections::BTreeMap::new();
    let bios;
    if efi {
        extra_boot.insert("efi".to_string(), partition_for(&disk_path, 1));
        bios = None;
    } else {
        bios = Some(partition_for(&disk_path, 1));
    }
    Ok(OsPartitionInfo {
        bios,
        boot: partition_for(&disk_path, 2),
        root: partition_for(&disk_path, 3),
        extra_boot,
        data: data_part,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gpt::GptConfig;
    use gpt::disk::LogicalBlockSize;

    use super::*;

    /// Reproduce Embassy 0.3.5.1's GPT layout so we can test the preserve path
    /// against the geometry that 0.3.5.1 actually wrote: efi(100M) + boot(1G) +
    /// root(15G) + data filling the rest, all without 1 MiB partition
    /// alignment. Returns the data partition's first/last LBA.
    fn write_0_3_5_1_layout(disk_path: &Path, capacity: u64) -> (u64, u64) {
        let mut device = Box::new(
            std::fs::File::options()
                .read(true)
                .write(true)
                .open(disk_path)
                .unwrap(),
        );
        let mbr = gpt::mbr::ProtectiveMBR::with_lb_size(
            u32::try_from((capacity / 512) - 1).unwrap_or(0xFF_FF_FF_FF),
        );
        mbr.overwrite_lba0(&mut device).unwrap();
        let mut gpt = GptConfig::new()
            .writable(true)
            .logical_block_size(LogicalBlockSize::Lb512)
            .create_from_device(device, None)
            .unwrap();
        gpt.update_partitions(Default::default()).unwrap();
        gpt.add_partition("efi", 100 * 1024 * 1024, gpt::partition_types::EFI, 0, None)
            .unwrap();
        gpt.add_partition(
            "boot",
            1024 * 1024 * 1024,
            gpt::partition_types::LINUX_FS,
            0,
            None,
        )
        .unwrap();
        gpt.add_partition(
            "root",
            15 * 1024 * 1024 * 1024,
            gpt::partition_types::LINUX_ROOT_X64,
            0,
            None,
        )
        .unwrap();
        let data_size_bytes = gpt
            .find_free_sectors()
            .iter()
            .map(|(_, s)| *s * 512)
            .min()
            .unwrap();
        gpt.add_partition(
            "data",
            data_size_bytes,
            gpt::partition_types::LINUX_LVM,
            0,
            None,
        )
        .unwrap();
        let data = gpt.partitions().get(&4).unwrap();
        let (f, l) = (data.first_lba, data.last_lba);
        gpt.write().unwrap();
        (f, l)
    }

    fn make_sparse(path: &Path, capacity: u64) {
        let f = std::fs::File::create(path).unwrap();
        f.set_len(capacity).unwrap();
    }

    /// Regression test for https://github.com/Start9Labs/start-technologies/pull/3193
    /// preserve path on 0.3.5.1 disks: the .min() that was originally used to
    /// pick the free region before the protected data partition picked the
    /// 2014-LBA sliver between LBA 34 and the new 2048-aligned EFI start, so
    /// `available_lba` was over-reported and root landed 2014 LBAs past
    /// `protect_first_lba`, after which add_partition_at("data", ...) returned
    /// NotEnoughSpace.
    #[test]
    fn preserve_data_partition_from_0_3_5_1_layout() {
        for size_gb in [32u64, 64, 128, 256, 512] {
            let dir = std::env::temp_dir();
            let path = dir.join(format!(
                "startos-gpt-test-{}-{}.img",
                std::process::id(),
                size_gb
            ));
            let capacity = size_gb * 1024 * 1024 * 1024;
            make_sparse(&path, capacity);

            let (old_data_first, old_data_last) = write_0_3_5_1_layout(&path, capacity);
            let protected = (old_data_first, old_data_last, PathBuf::from("/dev/sda4"));

            let device = Box::new(
                std::fs::File::options()
                    .read(true)
                    .write(true)
                    .open(&path)
                    .unwrap(),
            );
            let result = build_gpt_layout(
                device,
                capacity,
                Path::new("/dev/sda"),
                Some(protected),
                true,
            );
            assert!(
                result.is_ok(),
                "build_gpt_layout({} GiB) failed: {:?}",
                size_gb,
                result.err()
            );

            // Re-open and inspect the produced GPT.
            let dev = Box::new(std::fs::File::options().read(true).open(&path).unwrap());
            let new_gpt = GptConfig::new()
                .writable(false)
                .logical_block_size(LogicalBlockSize::Lb512)
                .open_from_device(dev)
                .unwrap();
            let parts = new_gpt.partitions();
            assert_eq!(
                parts.len(),
                4,
                "expected 4 partitions on {} GiB disk",
                size_gb
            );
            let data = parts.get(&4).expect("partition 4 (data) missing");
            assert_eq!(
                data.first_lba, old_data_first,
                "data partition first_lba moved on {} GiB disk",
                size_gb
            );
            assert_eq!(
                data.last_lba, old_data_last,
                "data partition last_lba moved on {} GiB disk",
                size_gb
            );
            let root = parts.get(&3).expect("partition 3 (root) missing");
            assert!(
                root.last_lba < data.first_lba,
                "root.last_lba {} >= data.first_lba {} on {} GiB disk",
                root.last_lba,
                data.first_lba,
                size_gb
            );

            std::fs::remove_file(&path).ok();
        }
    }
}
