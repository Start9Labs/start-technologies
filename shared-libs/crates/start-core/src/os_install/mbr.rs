use std::path::{Path, PathBuf};

use color_eyre::eyre::eyre;
use mbrman::{CHS, MBR, MBRPartitionEntry};

use crate::disk::OsPartitionInfo;
use crate::os_install::partition_for;
use crate::os_install::quiesce::{quiesce_disk, update_partition_table};
use crate::prelude::*;

/// Boot end (exclusive). 2 GiB at 512-byte sectors.
const BOOT_END_SECTOR: u32 = 4196352;
/// Happy-path root end. 14 GiB of root after boot.
const ROOT_TARGET_END_SECTOR: u32 = 33556480;
/// Floor for elastic-root shrink. 12 GiB of root after boot.
const ROOT_MIN_END_SECTOR: u32 = BOOT_END_SECTOR + 12 * 1024 * 1024 * 2;

pub async fn partition(
    disk_path: &Path,
    capacity: u64,
    protect: Option<&Path>,
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
    let sectors = (capacity / 512) as u32;
    let data_part = tokio::task::spawn_blocking(move || {
        let disk_path = disk_path_clone;

        // If protecting a partition, read its location from the existing MBR
        let protected_partition_info: Option<(u32, u32, PathBuf)> =
            if let Some(ref protect_path) = protect {
                let mut file = std::fs::File::options().read(true).open(&disk_path)?;
                let existing_mbr = MBR::read_from(&mut file, 512)?;
                // Find the partition matching the protected path (check partitions 1-4)
                let info = (1..=4u32)
                    .find(|&idx| partition_for(&disk_path, idx) == *protect_path)
                    .and_then(|idx| {
                        let entry = &existing_mbr[idx as usize];
                        if entry.sectors > 0 {
                            Some((entry.starting_lba, entry.sectors, protect_path.clone()))
                        } else {
                            None
                        }
                    });
                if info.is_none() {
                    return Err(Error::new(
                        eyre!(
                            "Protected partition {} not found in MBR on {}",
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

        // Layout: boot 2048..BOOT_END_SECTOR, root BOOT_END_SECTOR..root_end_sector,
        // data root_end_sector..end. root shrinks toward ROOT_MIN_END_SECTOR if a
        // protected partition sits in the way.
        let root_end_sector: u32 =
            if let Some((starting_lba, _, ref path)) = protected_partition_info {
                if starting_lba < ROOT_MIN_END_SECTOR {
                    return Err(Error::new(
                        eyre!(
                            "{}",
                            t!(
                                "os-install.protected-partition-overlaps-os-root-sectors",
                                path = path.display(),
                                first_lba = starting_lba,
                                min_end_sector = ROOT_MIN_END_SECTOR,
                            )
                        ),
                        crate::ErrorKind::DiskManagement,
                    ));
                }
                std::cmp::min(starting_lba, ROOT_TARGET_END_SECTOR)
            } else {
                ROOT_TARGET_END_SECTOR
            };

        let mut file = std::fs::File::options()
            .read(true)
            .write(true)
            .open(&disk_path)?;
        let mut mbr = MBR::new_from(&mut file, 512, rand::random())?;

        mbr[1] = MBRPartitionEntry {
            boot: 0x80,
            first_chs: CHS::empty(),
            sys: 0x0b,
            last_chs: CHS::empty(),
            starting_lba: 2048,
            sectors: BOOT_END_SECTOR - 2048,
        };
        mbr[2] = MBRPartitionEntry {
            boot: 0,
            first_chs: CHS::empty(),
            sys: 0x83,
            last_chs: CHS::empty(),
            starting_lba: BOOT_END_SECTOR,
            sectors: root_end_sector - BOOT_END_SECTOR,
        };

        let data_part = if let Some((starting_lba, part_sectors, path)) = protected_partition_info {
            // Re-create the data partition entry at the same location
            mbr[3] = MBRPartitionEntry {
                boot: 0,
                first_chs: CHS::empty(),
                sys: 0x8e,
                last_chs: CHS::empty(),
                starting_lba,
                sectors: part_sectors,
            };
            Some(path)
        } else {
            mbr[3] = MBRPartitionEntry {
                boot: 0,
                first_chs: CHS::empty(),
                sys: 0x8e,
                last_chs: CHS::empty(),
                starting_lba: root_end_sector,
                sectors: sectors - root_end_sector,
            };
            Some(partition_for(&disk_path, 3))
        };
        mbr.write_into(&mut file)?;

        Ok::<_, Error>(data_part)
    })
    .await
    .unwrap()?;

    update_partition_table(&disk_path).await?;

    Ok(OsPartitionInfo {
        bios: None,
        boot: partition_for(&disk_path, 1),
        root: partition_for(&disk_path, 2),
        extra_boot: Default::default(),
        data: data_part,
    })
}
