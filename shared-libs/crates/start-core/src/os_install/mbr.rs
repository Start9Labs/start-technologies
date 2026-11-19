use std::fs::File;
use std::path::Path;

use color_eyre::eyre::eyre;
use mbrman::{CHS, MBR, MBRPartitionEntry};

use crate::disk::OsPartitionInfo;
use crate::os_install::quiesce::{forget_partitions, quiesce_disk, update_partition_table};
use crate::os_install::{partition_for, same_device};
use crate::prelude::*;

/// Sectors per MiB.
const MIB: u32 = 2048;
const BOOT_START: u32 = MIB;
const BOOT_SECTORS: u32 = 2048 * MIB;
const BOOT_END: u32 = BOOT_START + BOOT_SECTORS;
const EFI_SECTORS: u32 = 100 * MIB;
const ROOT_TARGET_SECTORS: u32 = 14 * 1024 * MIB;
const ROOT_MIN_SECTORS: u32 = 12 * 1024 * MIB;

/// A partition the new table keeps at its exact start and size.
#[derive(Clone, Copy)]
struct Protected<'a> {
    path: &'a Path,
    slot: usize,
    start: u32,
    sectors: u32,
}

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

/// Data keeps slot 4 when it was there; otherwise it takes 3.
fn data_and_efi_slots(protected_slot: Option<usize>) -> (usize, usize) {
    if protected_slot == Some(4) {
        (4, 3)
    } else {
        (3, 4)
    }
}

fn write_mbr_layout(
    file: &mut File,
    sectors: u32,
    protected: Option<Protected>,
    use_efi: bool,
) -> Result<MBR, Error> {
    let root_start = if use_efi {
        BOOT_END + EFI_SECTORS
    } else {
        BOOT_END
    };
    let root_min_end = root_start + ROOT_MIN_SECTORS;
    let root_end = match protected {
        Some(p) if p.start < root_min_end => {
            return Err(Error::new(
                eyre!(
                    "{}",
                    t!(
                        "os-install.protected-partition-overlaps-os-root-sectors",
                        path = p.path.display(),
                        first_lba = p.start,
                        min_end_sector = root_min_end,
                    )
                ),
                crate::ErrorKind::DiskManagement,
            ));
        }
        Some(p) => p.start.min(root_start + ROOT_TARGET_SECTORS),
        None => root_start + ROOT_TARGET_SECTORS,
    };
    let (data_slot, efi_slot) = data_and_efi_slots(protected.map(|p| p.slot));

    let mut mbr = MBR::new_from(file, 512, rand::random())?;
    mbr[1] = entry(0x80, 0x0b, BOOT_START, BOOT_SECTORS);
    mbr[2] = entry(0, 0x83, root_start, root_end - root_start);
    mbr[data_slot] = match protected {
        Some(p) => entry(0, 0x8e, p.start, p.sectors),
        None => entry(0, 0x8e, root_end, sectors - root_end),
    };
    if use_efi {
        mbr[efi_slot] = entry(0, 0xef, BOOT_END, EFI_SECTORS);
    }
    mbr.write_into(file)?;
    Ok(mbr)
}

/// Slot, start and size of the protected partition in the disk's current MBR.
fn protected_extent(disk: &Path, protect: &Path) -> Result<(usize, u32, u32), Error> {
    let mbr = MBR::read_from(&mut File::open(disk)?, 512)?;
    (1..=4)
        .find(|&slot| same_device(&partition_for(disk, slot as u32), protect))
        .map(|slot| (slot, &mbr[slot]))
        .filter(|(_, e)| e.sectors > 0)
        .map(|(slot, e)| (slot, e.starting_lba, e.sectors))
        .ok_or_else(|| {
            Error::new(
                eyre!(
                    "Protected partition {} not found in MBR on {}",
                    protect.display(),
                    disk.display()
                ),
                crate::ErrorKind::NotFound,
            )
        })
}

/// First slot whose kernel mapping differs from the table. An empty slot must be unmapped.
fn mismatched_slot(mbr: &MBR, mapped: impl Fn(usize) -> Option<(u64, u64)>) -> Option<usize> {
    mbr.iter().map(|(slot, _)| slot).find(|&slot| {
        let e = &mbr[slot];
        let expected = (e.sectors > 0).then(|| (u64::from(e.starting_lba), u64::from(e.sectors)));
        mapped(slot) != expected
    })
}

/// Start and size in 512-byte sectors, as the kernel maps the partition.
async fn mapped_extent(part: &Path) -> Option<(u64, u64)> {
    let dev = tokio::fs::canonicalize(part).await.ok()?;
    let sys = Path::new("/sys/class/block").join(dev.file_name()?);
    let start = tokio::fs::read_to_string(sys.join("start")).await.ok()?;
    let size = tokio::fs::read_to_string(sys.join("size")).await.ok()?;
    Some((start.trim().parse().ok()?, size.trim().parse().ok()?))
}

pub async fn partition(
    disk_path: &Path,
    capacity: u64,
    protect: Option<&Path>,
    use_efi: bool,
) -> Result<OsPartitionInfo, Error> {
    if protect == Some(disk_path) {
        return Err(Error::new(
            eyre!(
                "Cannot protect the entire disk {}; must specify a partition",
                disk_path.display()
            ),
            crate::ErrorKind::InvalidRequest,
        ));
    }

    quiesce_disk(disk_path).await?;

    let disk = disk_path.to_owned();
    let protect_owned = protect.map(Path::to_owned);
    let extent = match protect_owned.clone() {
        Some(protect_path) => {
            let disk = disk.clone();
            let extent =
                tokio::task::spawn_blocking(move || protected_extent(&disk, &protect_path));
            Some(extent.await.unwrap()?)
        }
        None => None,
    };

    forget_partitions(disk_path, protect).await?;

    let sectors = (capacity / 512) as u32;
    let mbr = tokio::task::spawn_blocking(move || {
        let protected =
            protect_owned
                .as_deref()
                .zip(extent)
                .map(|(path, (slot, start, sectors))| Protected {
                    path,
                    slot,
                    start,
                    sectors,
                });
        let mut file = File::options().read(true).write(true).open(&disk)?;
        write_mbr_layout(&mut file, sectors, protected, use_efi)
    })
    .await
    .unwrap()?;

    update_partition_table(disk_path).await?;

    let mut mapped = [None; 4];
    for slot in 1..=4 {
        mapped[slot - 1] = mapped_extent(&partition_for(disk_path, slot as u32)).await;
    }
    if let Some(slot) = mismatched_slot(&mbr, |slot| mapped[slot - 1]) {
        return Err(Error::new(
            eyre!(
                "{}",
                t!(
                    "os-install.partition-not-mapped-as-written",
                    partition = partition_for(disk_path, slot as u32).display(),
                )
            ),
            crate::ErrorKind::DiskManagement,
        ));
    }

    let (data_slot, efi_slot) = data_and_efi_slots(extent.map(|(slot, _, _)| slot));
    let mut extra_boot = std::collections::BTreeMap::new();
    if use_efi {
        extra_boot.insert("efi".to_string(), partition_for(disk_path, efi_slot as u32));
    }
    Ok(OsPartitionInfo {
        bios: None,
        boot: partition_for(disk_path, 1),
        root: partition_for(disk_path, 2),
        extra_boot,
        data: Some(partition_for(disk_path, data_slot as u32)),
    })
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Seek, SeekFrom, Write};

    use super::*;

    /// 64 GiB at 512-byte sectors.
    const DISK_SECTORS: u32 = 134217728;
    /// Where a StartOS 0.3.4.1 legacy install puts its data partition, which runs to the
    /// last sector.
    const DATA_START: u32 = 33556480;
    const MARKER: [u8; 512] = [0xa5; 512];

    /// A StartOS 0.3.4.1 legacy-install MBR with the data partition at `data_start` in
    /// `data_slot`, marked at its first and last sectors.
    fn legacy_disk(data_start: u32, data_slot: usize) -> File {
        let mut file = tempfile::tempfile().unwrap();
        file.set_len(u64::from(DISK_SECTORS) * 512).unwrap();
        let mut mbr = MBR::new_from(&mut file, 512, [1, 2, 3, 4]).unwrap();
        mbr[1] = entry(0x80, 0x0b, 2048, 2097152);
        mbr[2] = entry(0, 0x83, 2099200, data_start - 2099200);
        mbr[data_slot] = entry(0, 0x8e, data_start, DISK_SECTORS - data_start);
        mbr.write_into(&mut file).unwrap();
        for sector in [data_start, DISK_SECTORS - 1] {
            file.seek(SeekFrom::Start(u64::from(sector) * 512)).unwrap();
            file.write_all(&MARKER).unwrap();
        }
        file
    }

    fn read_sector(file: &mut File, sector: u32) -> [u8; 512] {
        let mut buf = [0; 512];
        file.seek(SeekFrom::Start(u64::from(sector) * 512)).unwrap();
        file.read_exact(&mut buf).unwrap();
        buf
    }

    fn assert_markers(file: &mut File, data_start: u32) {
        for sector in [data_start, DISK_SECTORS - 1] {
            assert_eq!(read_sector(file, sector), MARKER, "sector {sector}");
        }
    }

    fn rewrite(
        file: &mut File,
        data_start: u32,
        data_slot: usize,
        use_efi: bool,
    ) -> Result<MBR, Error> {
        let protected = Protected {
            path: Path::new("/dev/sda3"),
            slot: data_slot,
            start: data_start,
            sectors: DISK_SECTORS - data_start,
        };
        write_mbr_layout(file, DISK_SECTORS, Some(protected), use_efi)
    }

    #[test]
    fn preserve_under_uefi_adds_efi_partition_and_keeps_data() {
        let mut file = legacy_disk(DATA_START, 3);
        let mbr = rewrite(&mut file, DATA_START, 3, true).unwrap();
        assert_eq!(MBR::read_from(&mut file, 512).unwrap(), mbr);
        assert_eq!(mbr[1], entry(0x80, 0x0b, 2048, 4194304));
        assert_eq!(mbr[4], entry(0, 0xef, 4196352, 204800));
        assert_eq!(mbr[2], entry(0, 0x83, 4401152, DATA_START - 4401152));
        assert_eq!(
            mbr[3],
            entry(0, 0x8e, DATA_START, DISK_SECTORS - DATA_START)
        );
        assert_markers(&mut file, DATA_START);
    }

    #[test]
    fn preserve_under_legacy_bios_keeps_three_partitions() {
        let mut file = legacy_disk(DATA_START, 3);
        let mbr = rewrite(&mut file, DATA_START, 3, false).unwrap();
        assert_eq!(mbr[1], entry(0x80, 0x0b, 2048, 4194304));
        assert_eq!(mbr[2], entry(0, 0x83, 4196352, DATA_START - 4196352));
        assert_eq!(
            mbr[3],
            entry(0, 0x8e, DATA_START, DISK_SECTORS - DATA_START)
        );
        assert_eq!(mbr[4].sectors, 0);
        assert_markers(&mut file, DATA_START);
    }

    #[test]
    fn data_in_slot_4_keeps_its_slot_under_uefi() {
        let mut file = legacy_disk(DATA_START, 4);
        let mbr = rewrite(&mut file, DATA_START, 4, true).unwrap();
        assert_eq!(
            mbr[4],
            entry(0, 0x8e, DATA_START, DISK_SECTORS - DATA_START)
        );
        assert_eq!(mbr[3], entry(0, 0xef, 4196352, 204800));
        assert_markers(&mut file, DATA_START);
    }

    #[test]
    fn efi_partition_comes_out_of_root_minimum() {
        let legacy_min_end = BOOT_END + ROOT_MIN_SECTORS;
        let uefi_min_end = legacy_min_end + EFI_SECTORS;

        let mut file = legacy_disk(legacy_min_end, 3);
        let mbr = rewrite(&mut file, legacy_min_end, 3, false).unwrap();
        assert_eq!(mbr[2], entry(0, 0x83, BOOT_END, ROOT_MIN_SECTORS));
        assert_markers(&mut file, legacy_min_end);

        for data_start in [legacy_min_end, uefi_min_end - 1] {
            let mut file = legacy_disk(data_start, 3);
            let sector0 = read_sector(&mut file, 0);
            let err = rewrite(&mut file, data_start, 3, true).unwrap_err();
            assert!(matches!(err.kind, ErrorKind::DiskManagement));
            assert_eq!(read_sector(&mut file, 0), sector0);
            assert_markers(&mut file, data_start);
        }

        let mut file = legacy_disk(uefi_min_end, 3);
        let mbr = rewrite(&mut file, uefi_min_end, 3, true).unwrap();
        assert_eq!(
            mbr[2],
            entry(0, 0x83, BOOT_END + EFI_SECTORS, ROOT_MIN_SECTORS)
        );
        assert_eq!(
            mbr[3],
            entry(0, 0x8e, uefi_min_end, DISK_SECTORS - uefi_min_end)
        );
        assert_markers(&mut file, uefi_min_end);
    }

    #[test]
    fn legacy_reinstall_after_uefi_needs_the_kernel_to_drop_the_efi_mapping() {
        let mut file = legacy_disk(DATA_START, 3);
        let uefi = rewrite(&mut file, DATA_START, 3, true).unwrap();
        let legacy = rewrite(&mut file, DATA_START, 3, false).unwrap();
        let extent = |mbr: &MBR, slot: usize| {
            let e = &mbr[slot];
            (e.sectors > 0).then(|| (u64::from(e.starting_lba), u64::from(e.sectors)))
        };

        // The kernel kept the old ESP and the busy data partition but refused the new root.
        let stale = |slot| match slot {
            2 => None,
            4 => extent(&uefi, 4),
            _ => extent(&legacy, slot),
        };
        assert_eq!(mismatched_slot(&legacy, stale), Some(2));

        let esp_left = |slot| match slot {
            4 => extent(&uefi, 4),
            _ => extent(&legacy, slot),
        };
        assert_eq!(mismatched_slot(&legacy, esp_left), Some(4));

        assert_eq!(mismatched_slot(&legacy, |slot| extent(&legacy, slot)), None);
    }
}
