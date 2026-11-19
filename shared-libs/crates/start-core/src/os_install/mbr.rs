use std::fs::File;
use std::path::{Path, PathBuf};

use color_eyre::eyre::eyre;
use mbrman::{CHS, MBR, MBRPartitionEntry};

use crate::disk::OsPartitionInfo;
use crate::os_install::quiesce::{quiesce_disk, update_partition_table};
use crate::os_install::{partition_for, same_device};
use crate::prelude::*;

/// Boot end (exclusive). 2 GiB at 512-byte sectors.
const BOOT_END_SECTOR: u32 = 4196352;
/// EFI system partition, between boot and root. 100 MiB at 512-byte sectors.
const EFI_SECTORS: u32 = 100 * 1024 * 2;
/// Happy-path root size. 14 GiB at 512-byte sectors.
const ROOT_TARGET_SECTORS: u32 = 14 * 1024 * 1024 * 2;
/// Floor for elastic-root shrink. 12 GiB at 512-byte sectors.
const ROOT_MIN_SECTORS: u32 = 12 * 1024 * 1024 * 2;

fn entry(boot: u8, sys: u8, starting_lba: u32, sectors: u32) -> MBRPartitionEntry {
    MBRPartitionEntry {
        boot,
        first_chs: CHS::empty(),
        sys,
        last_chs: CHS::empty(),
        starting_lba,
        sectors,
    }
}

/// Writes boot (1), root (2) and data (3), plus an EFI system partition (4) when
/// booting with UEFI. A protected data partition keeps its sectors.
fn write_mbr_layout(
    file: &mut File,
    sectors: u32,
    protected: Option<(u32, u32, &Path)>,
    use_efi: bool,
) -> Result<(), Error> {
    let root_start = if use_efi {
        BOOT_END_SECTOR + EFI_SECTORS
    } else {
        BOOT_END_SECTOR
    };
    let root_min_end = root_start + ROOT_MIN_SECTORS;
    let root_end = match protected {
        Some((starting_lba, _, path)) if starting_lba < root_min_end => {
            return Err(Error::new(
                eyre!(
                    "{}",
                    t!(
                        "os-install.protected-partition-overlaps-os-root-sectors",
                        path = path.display(),
                        first_lba = starting_lba,
                        min_end_sector = root_min_end,
                    )
                ),
                crate::ErrorKind::DiskManagement,
            ));
        }
        Some((starting_lba, _, _)) => starting_lba.min(root_start + ROOT_TARGET_SECTORS),
        None => root_start + ROOT_TARGET_SECTORS,
    };

    let mut mbr = MBR::new_from(file, 512, rand::random())?;
    mbr[1] = entry(0x80, 0x0b, 2048, BOOT_END_SECTOR - 2048);
    mbr[2] = entry(0, 0x83, root_start, root_end - root_start);
    mbr[3] = match protected {
        Some((starting_lba, part_sectors, _)) => entry(0, 0x8e, starting_lba, part_sectors),
        None => entry(0, 0x8e, root_end, sectors - root_end),
    };
    if use_efi {
        mbr[4] = entry(0, 0xef, BOOT_END_SECTOR, EFI_SECTORS);
    }
    mbr.write_into(file)?;
    Ok(())
}

pub async fn partition(
    disk_path: &Path,
    capacity: u64,
    protect: Option<&Path>,
    use_efi: bool,
) -> Result<OsPartitionInfo, Error> {
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

        let protected_partition_info: Option<(u32, u32, PathBuf)> =
            if let Some(ref protect_path) = protect {
                let mut file = std::fs::File::options().read(true).open(&disk_path)?;
                let existing_mbr = MBR::read_from(&mut file, 512)?;
                let info = (1..=4u32)
                    .find(|&idx| same_device(&partition_for(&disk_path, idx), protect_path))
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

        let mut file = std::fs::File::options()
            .read(true)
            .write(true)
            .open(&disk_path)?;
        write_mbr_layout(
            &mut file,
            sectors,
            protected_partition_info
                .as_ref()
                .map(|(start, len, path)| (*start, *len, path.as_path())),
            use_efi,
        )?;

        Ok::<_, Error>(match protected_partition_info {
            Some((_, _, path)) => path,
            None => partition_for(&disk_path, 3),
        })
    })
    .await
    .unwrap()?;

    update_partition_table(&disk_path).await?;

    let mut extra_boot = std::collections::BTreeMap::new();
    if use_efi {
        extra_boot.insert("efi".to_string(), partition_for(&disk_path, 4));
    }
    Ok(OsPartitionInfo {
        bios: None,
        boot: partition_for(&disk_path, 1),
        root: partition_for(&disk_path, 2),
        extra_boot,
        data: Some(data_part),
    })
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Seek, SeekFrom, Write};

    use super::*;

    /// 64 GiB at 512-byte sectors.
    const DISK_SECTORS: u32 = 134217728;
    /// Data partition of a StartOS 0.3.4.1 legacy install: after a 1 GiB boot and a
    /// 15 GiB root, to the last sector.
    const DATA_START: u32 = 33556480;
    const DATA_SECTORS: u32 = DISK_SECTORS - DATA_START;

    /// Writes a 0.3.4.1 legacy-install MBR to a sparse image, with marker bytes at
    /// both ends of the data partition.
    fn legacy_0_3_4_1_disk(name: &str) -> (PathBuf, File) {
        let path = std::env::temp_dir().join(format!(
            "startos-mbr-test-{}-{name}.img",
            std::process::id()
        ));
        let mut file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .unwrap();
        file.set_len(u64::from(DISK_SECTORS) * 512).unwrap();
        let mut mbr = MBR::new_from(&mut file, 512, [1, 2, 3, 4]).unwrap();
        mbr[1] = entry(0x80, 0x0b, 2048, 2097152);
        mbr[2] = entry(0, 0x83, 2099200, 31457280);
        mbr[3] = entry(0, 0x8e, DATA_START, DATA_SECTORS);
        mbr.write_into(&mut file).unwrap();
        for sector in [DATA_START, DISK_SECTORS - 1] {
            file.seek(SeekFrom::Start(u64::from(sector) * 512)).unwrap();
            file.write_all(&[0xa5; 512]).unwrap();
        }
        (path, file)
    }

    fn data_markers(file: &mut File) -> Vec<u8> {
        let mut bytes = vec![0; 1024];
        for (i, sector) in [DATA_START, DISK_SECTORS - 1].into_iter().enumerate() {
            file.seek(SeekFrom::Start(u64::from(sector) * 512)).unwrap();
            file.read_exact(&mut bytes[i * 512..(i + 1) * 512]).unwrap();
        }
        bytes
    }

    fn rewrite(name: &str, protected_start: u32, use_efi: bool) -> Result<MBR, Error> {
        let (path, mut file) = legacy_0_3_4_1_disk(name);
        let res = write_mbr_layout(
            &mut file,
            DISK_SECTORS,
            Some((
                protected_start,
                DISK_SECTORS - protected_start,
                Path::new("/dev/sda3"),
            )),
            use_efi,
        )
        .map(|()| {
            assert_eq!(data_markers(&mut file), vec![0xa5; 1024]);
            MBR::read_from(&mut file, 512).unwrap()
        });
        std::fs::remove_file(&path).ok();
        res
    }

    #[test]
    fn preserve_under_uefi_adds_efi_partition_and_keeps_data() {
        let mbr = rewrite("uefi", DATA_START, true).unwrap();
        assert_eq!(mbr[1], entry(0x80, 0x0b, 2048, 4194304));
        assert_eq!(mbr[4], entry(0, 0xef, 4196352, 204800));
        assert_eq!(mbr[2], entry(0, 0x83, 4401152, DATA_START - 4401152));
        assert_eq!(mbr[3], entry(0, 0x8e, DATA_START, DATA_SECTORS));
    }

    #[test]
    fn preserve_under_legacy_bios_keeps_three_partitions() {
        let mbr = rewrite("bios", DATA_START, false).unwrap();
        assert_eq!(mbr[1], entry(0x80, 0x0b, 2048, 4194304));
        assert_eq!(mbr[2], entry(0, 0x83, 4196352, DATA_START - 4196352));
        assert_eq!(mbr[3], entry(0, 0x8e, DATA_START, DATA_SECTORS));
        assert_eq!(mbr[4].sectors, 0);
    }

    #[test]
    fn efi_partition_comes_out_of_root_minimum() {
        let legacy_min_end = BOOT_END_SECTOR + ROOT_MIN_SECTORS;
        assert!(rewrite("bios-min", legacy_min_end, false).is_ok());
        let err = rewrite("uefi-min", legacy_min_end, true).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::DiskManagement));
        assert!(rewrite("uefi-min-ok", legacy_min_end + EFI_SECTORS, true).is_ok());
    }
}
