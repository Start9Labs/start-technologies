use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use color_eyre::eyre::eyre;
use gpt::disk::LogicalBlockSize;
use mbrman::{CHS, MBR, MBRPartitionEntry};

use crate::disk::OsPartitionInfo;
use crate::disk::util::get_logical_block_size;
use crate::os_install::partition_for;
use crate::os_install::quiesce::{
    forget_partitions, mapped_partitions, quiesce_disk, update_partition_table,
};
use crate::prelude::*;

const BOOT_START_MIB: u32 = 1;
const BOOT_MIB: u32 = 2048;
const EFI_MIB: u32 = 100;
const ROOT_TARGET_MIB: u32 = 14 * 1024;
const ROOT_MIN_MIB: u32 = 12 * 1024;

/// The target disk's logical sector size and its length in those sectors, saturating at the
/// MBR limit.
#[derive(Clone, Copy)]
struct Geometry {
    sector_size: u32,
    sectors: u32,
}

impl Geometry {
    fn new(sector_size: u64, capacity: u64) -> Result<Self, Error> {
        LogicalBlockSize::try_from(sector_size).map_err(|_| {
            Error::new(
                eyre!(
                    "{}",
                    t!("disk.format.unsupported-sector-size", size = sector_size)
                ),
                crate::ErrorKind::DiskManagement,
            )
        })?;
        Ok(Self {
            sector_size: sector_size as u32,
            sectors: u32::try_from(capacity / sector_size).unwrap_or(u32::MAX),
        })
    }

    fn mib(self) -> u32 {
        (1 << 20) / self.sector_size
    }

    /// Start and size in the kernel's 512-byte sectors; `None` for an empty entry.
    fn kernel_extent(self, e: &MBRPartitionEntry) -> Option<(u64, u64)> {
        let scale = u64::from(self.sector_size / 512);
        (e.sectors > 0).then(|| {
            (
                u64::from(e.starting_lba) * scale,
                u64::from(e.sectors) * scale,
            )
        })
    }
}

/// A partition the new table keeps at its exact start and size, in logical sectors.
struct Protected {
    path: PathBuf,
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

/// Table entry numbers. Data keeps its entry; boot, root and the EFI partition take the
/// others in order.
#[derive(Clone, Copy)]
struct Slots {
    boot: usize,
    root: usize,
    efi: usize,
    data: usize,
}

impl Slots {
    fn around(data: usize) -> Self {
        let [boot, root, efi] = match data {
            1 => [2, 3, 4],
            2 => [1, 3, 4],
            4 => [1, 2, 3],
            _ => [1, 2, 4],
        };
        Self {
            boot,
            root,
            efi,
            data,
        }
    }
}

fn protected_mismatch(path: &Path) -> Error {
    Error::new(
        eyre!(
            "{}",
            t!(
                "os-install.protected-partition-not-in-table",
                partition = path.display()
            )
        ),
        crate::ErrorKind::DiskManagement,
    )
}

/// The table's entry for a partition the kernel maps. Any difference between the two is an
/// error.
fn protected_from_table(
    mbr: &MBR,
    geometry: Geometry,
    slot: usize,
    kernel: (u64, u64),
    path: PathBuf,
) -> Result<Protected, Error> {
    let e = (1..=4).contains(&slot).then(|| &mbr[slot]);
    match e {
        Some(e) if geometry.kernel_extent(e) == Some(kernel) => Ok(Protected {
            slot,
            start: e.starting_lba,
            sectors: e.sectors,
            path,
        }),
        _ => Err(protected_mismatch(&path)),
    }
}

fn write_mbr_layout(
    file: &mut File,
    geometry: Geometry,
    protected: Option<&Protected>,
    use_efi: bool,
) -> Result<MBR, Error> {
    let mib = geometry.mib();
    let boot_start = BOOT_START_MIB * mib;
    let boot_end = boot_start + BOOT_MIB * mib;
    let root_start = if use_efi {
        boot_end + EFI_MIB * mib
    } else {
        boot_end
    };
    let root_min_end = root_start + ROOT_MIN_MIB * mib;
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
        Some(p) => p.start.min(root_start + ROOT_TARGET_MIB * mib),
        None => root_start + ROOT_TARGET_MIB * mib,
    };
    let slots = Slots::around(protected.map_or(3, |p| p.slot));

    let mut mbr = MBR::new_from(file, geometry.sector_size, rand::random())?;
    mbr[slots.boot] = entry(0x80, 0x0b, boot_start, boot_end - boot_start);
    mbr[slots.root] = entry(0, 0x83, root_start, root_end - root_start);
    mbr[slots.data] = match protected {
        Some(p) => entry(0, 0x8e, p.start, p.sectors),
        None => entry(0, 0x8e, root_end, geometry.sectors - root_end),
    };
    if use_efi {
        mbr[slots.efi] = entry(0, 0xef, boot_end, EFI_MIB * mib);
    }
    mbr.write_into(file)?;
    Ok(mbr)
}

/// First partition number whose kernel mapping differs from the table. An empty slot must be
/// unmapped.
fn mismatched_slot(
    mbr: &MBR,
    geometry: Geometry,
    mapped: &BTreeMap<usize, (u64, u64)>,
) -> Option<usize> {
    let expected: BTreeMap<usize, (u64, u64)> = (1..=4)
        .filter_map(|slot| Some((slot, geometry.kernel_extent(&mbr[slot])?)))
        .collect();
    (1..=4)
        .chain(mapped.keys().copied())
        .find(|slot| expected.get(slot) != mapped.get(slot))
}

/// The protected partition as both the kernel and the disk's current table place it.
async fn selected_extent(
    disk_path: &Path,
    protect: &Path,
    geometry: Geometry,
) -> Result<Protected, Error> {
    let dev = tokio::fs::canonicalize(protect)
        .await
        .with_ctx(|_| (ErrorKind::BlockDevice, protect.display().to_string()))?;
    let (slot, kernel) = mapped_partitions(disk_path)
        .await?
        .into_iter()
        .find(|(_, p)| p.dev == dev)
        .map(|(slot, p)| (slot, p.extent))
        .ok_or_else(|| protected_mismatch(protect))?;
    let disk = disk_path.to_owned();
    let mbr = tokio::task::spawn_blocking(move || {
        MBR::read_from(&mut File::open(&disk)?, geometry.sector_size)
    })
    .await
    .unwrap()?;
    protected_from_table(&mbr, geometry, slot, kernel, protect.to_owned())
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

    let geometry = Geometry::new(get_logical_block_size(disk_path).await?, capacity)?;

    quiesce_disk(disk_path).await?;

    let protected = match protect {
        Some(protect) => Some(selected_extent(disk_path, protect, geometry).await?),
        None => None,
    };
    let slots = Slots::around(protected.as_ref().map_or(3, |p| p.slot));

    forget_partitions(disk_path, protect).await?;

    let disk = disk_path.to_owned();
    let mbr = tokio::task::spawn_blocking(move || {
        let mut file = File::options().read(true).write(true).open(&disk)?;
        write_mbr_layout(&mut file, geometry, protected.as_ref(), use_efi)
    })
    .await
    .unwrap()?;

    update_partition_table(disk_path).await?;

    let mapped: BTreeMap<usize, (u64, u64)> = mapped_partitions(disk_path)
        .await?
        .into_iter()
        .map(|(slot, p)| (slot, p.extent))
        .collect();
    if let Some(slot) = mismatched_slot(&mbr, geometry, &mapped) {
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

    let mut extra_boot = BTreeMap::new();
    if use_efi {
        extra_boot.insert(
            "efi".to_string(),
            partition_for(disk_path, slots.efi as u32),
        );
    }
    Ok(OsPartitionInfo {
        bios: None,
        boot: partition_for(disk_path, slots.boot as u32),
        root: partition_for(disk_path, slots.root as u32),
        extra_boot,
        data: Some(partition_for(disk_path, slots.data as u32)),
    })
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Seek, SeekFrom, Write};

    use super::*;

    /// 64 GiB.
    const DISK_BYTES: u64 = 64 << 30;
    /// Where a StartOS 0.3.4.1 legacy install puts its data partition, in 512-byte sectors.
    const DATA_START: u32 = 33556480;
    const MARKER: [u8; 512] = [0xa5; 512];

    fn geometry(sector_size: u32) -> Geometry {
        Geometry::new(u64::from(sector_size), DISK_BYTES).unwrap()
    }

    /// A StartOS 0.3.4.1 legacy-install MBR with the data partition at `data_start` in
    /// `data_slot`, running to the last sector and marked at its first and last bytes. Boot
    /// and root take the first two other entries.
    fn legacy_disk(g: Geometry, data_start: u32, data_slot: usize) -> File {
        let mut file = tempfile::tempfile().unwrap();
        file.set_len(DISK_BYTES).unwrap();
        let mut mbr = MBR::new_from(&mut file, g.sector_size, [1, 2, 3, 4]).unwrap();
        let mib = g.mib();
        let mut os = (1..=4).filter(|&slot| slot != data_slot);
        mbr[os.next().unwrap()] = entry(0x80, 0x0b, mib, 1024 * mib);
        mbr[os.next().unwrap()] = entry(0, 0x83, 1025 * mib, data_start - 1025 * mib);
        mbr[data_slot] = entry(0, 0x8e, data_start, g.sectors - data_start);
        mbr.write_into(&mut file).unwrap();
        for at in marker_offsets(g, data_start) {
            file.seek(SeekFrom::Start(at)).unwrap();
            file.write_all(&MARKER).unwrap();
        }
        file
    }

    fn marker_offsets(g: Geometry, data_start: u32) -> [u64; 2] {
        [
            u64::from(data_start) * u64::from(g.sector_size),
            DISK_BYTES - 512,
        ]
    }

    fn read_at(file: &mut File, at: u64) -> [u8; 512] {
        let mut buf = [0; 512];
        file.seek(SeekFrom::Start(at)).unwrap();
        file.read_exact(&mut buf).unwrap();
        buf
    }

    fn assert_markers(file: &mut File, g: Geometry, data_start: u32) {
        for at in marker_offsets(g, data_start) {
            assert_eq!(read_at(file, at), MARKER, "byte {at}");
        }
    }

    /// Validates the slot against the kernel's view, then rewrites the table.
    fn rewrite_checked(
        file: &mut File,
        g: Geometry,
        slot: usize,
        kernel: (u64, u64),
        use_efi: bool,
    ) -> Result<MBR, Error> {
        let current = MBR::read_from(file, g.sector_size).unwrap();
        let protected =
            protected_from_table(&current, g, slot, kernel, PathBuf::from("/dev/sda3"))?;
        write_mbr_layout(file, g, Some(&protected), use_efi)
    }

    fn rewrite(
        file: &mut File,
        g: Geometry,
        data_start: u32,
        data_slot: usize,
        use_efi: bool,
    ) -> Result<MBR, Error> {
        let scale = u64::from(g.sector_size / 512);
        let kernel = (
            u64::from(data_start) * scale,
            u64::from(g.sectors - data_start) * scale,
        );
        rewrite_checked(file, g, data_slot, kernel, use_efi)
    }

    fn mapped(mbr: &MBR, g: Geometry) -> BTreeMap<usize, (u64, u64)> {
        (1..=4)
            .filter_map(|slot| Some((slot, g.kernel_extent(&mbr[slot])?)))
            .collect()
    }

    #[test]
    fn preserve_under_uefi_adds_efi_partition_and_keeps_data() {
        let g = geometry(512);
        let mut file = legacy_disk(g, DATA_START, 3);
        let mbr = rewrite(&mut file, g, DATA_START, 3, true).unwrap();
        assert_eq!(MBR::read_from(&mut file, 512).unwrap(), mbr);
        assert_eq!(mbr[1], entry(0x80, 0x0b, 2048, 4194304));
        assert_eq!(mbr[4], entry(0, 0xef, 4196352, 204800));
        assert_eq!(mbr[2], entry(0, 0x83, 4401152, DATA_START - 4401152));
        assert_eq!(mbr[3], entry(0, 0x8e, DATA_START, g.sectors - DATA_START));
        assert_markers(&mut file, g, DATA_START);
    }

    #[test]
    fn preserve_under_legacy_bios_keeps_three_partitions() {
        let g = geometry(512);
        let mut file = legacy_disk(g, DATA_START, 3);
        let mbr = rewrite(&mut file, g, DATA_START, 3, false).unwrap();
        assert_eq!(mbr[1], entry(0x80, 0x0b, 2048, 4194304));
        assert_eq!(mbr[2], entry(0, 0x83, 4196352, DATA_START - 4196352));
        assert_eq!(mbr[3], entry(0, 0x8e, DATA_START, g.sectors - DATA_START));
        assert_eq!(mbr[4].sectors, 0);
        assert_markers(&mut file, g, DATA_START);
    }

    /// Data entry, then the boot, root and EFI entries.
    const SLOTS: [(usize, [usize; 3]); 4] = [
        (1, [2, 3, 4]),
        (2, [1, 3, 4]),
        (3, [1, 2, 4]),
        (4, [1, 2, 3]),
    ];

    #[test]
    fn data_keeps_its_entry_number_in_every_slot() {
        let g = geometry(512);
        for (data, [boot, root, efi]) in SLOTS {
            for use_efi in [true, false] {
                let mut file = legacy_disk(g, DATA_START, data);
                let mbr = rewrite(&mut file, g, DATA_START, data, use_efi).unwrap();
                let root_start = if use_efi { 4401152 } else { 4196352 };
                assert_eq!(mbr[boot], entry(0x80, 0x0b, 2048, 4194304), "data {data}");
                assert_eq!(
                    mbr[root],
                    entry(0, 0x83, root_start, DATA_START - root_start),
                    "data {data}"
                );
                assert_eq!(
                    mbr[data],
                    entry(0, 0x8e, DATA_START, g.sectors - DATA_START),
                    "data {data}"
                );
                if use_efi {
                    assert_eq!(mbr[efi], entry(0, 0xef, 4196352, 204800), "data {data}");
                } else {
                    assert_eq!(mbr[efi].sectors, 0, "data {data}");
                }
                assert_markers(&mut file, g, DATA_START);
            }
        }
    }

    #[test]
    fn a_busy_data_mapping_already_matches_the_new_table() {
        let g = geometry(512);
        for (data, _) in SLOTS {
            let mut file = legacy_disk(g, DATA_START, data);
            let held = g
                .kernel_extent(&MBR::read_from(&mut file, 512).unwrap()[data])
                .unwrap();
            let mbr = rewrite(&mut file, g, DATA_START, data, true).unwrap();

            // `partx --update` cannot move or delete the open data partition.
            let mut after = mapped(&mbr, g);
            after.insert(data, held);
            assert_eq!(mismatched_slot(&mbr, g, &after), None, "data {data}");
        }
    }

    #[test]
    fn every_data_slot_is_rejected_before_sector_0_changes() {
        let g = geometry(512);
        let too_close = 2048 + 4194304 + 204800 + 12 * 1024 * 2048 - 1;
        for (data, _) in SLOTS {
            let mut file = legacy_disk(g, too_close, data);
            let sector0 = read_at(&mut file, 0);
            assert!(rewrite(&mut file, g, too_close, data, true).is_err());
            assert_eq!(read_at(&mut file, 0), sector0, "data {data}");
            assert_markers(&mut file, g, too_close);

            let mut file = legacy_disk(g, DATA_START, data);
            let sector0 = read_at(&mut file, 0);
            let stale = (4401152, 29155328);
            assert!(rewrite_checked(&mut file, g, data, stale, true).is_err());
            assert_eq!(read_at(&mut file, 0), sector0, "data {data}");
            assert_markers(&mut file, g, DATA_START);
        }
    }

    #[test]
    fn efi_partition_comes_out_of_root_minimum() {
        let g = geometry(512);
        let boot_end = 2048 + 4194304;
        let root_min = 12 * 1024 * 2048;
        let legacy_min_end = boot_end + root_min;
        let uefi_min_end = legacy_min_end + 204800;

        let mut file = legacy_disk(g, legacy_min_end, 3);
        let mbr = rewrite(&mut file, g, legacy_min_end, 3, false).unwrap();
        assert_eq!(mbr[2], entry(0, 0x83, boot_end, root_min));
        assert_markers(&mut file, g, legacy_min_end);

        for data_start in [legacy_min_end, uefi_min_end - 1] {
            let mut file = legacy_disk(g, data_start, 3);
            let sector0 = read_at(&mut file, 0);
            let err = rewrite(&mut file, g, data_start, 3, true).unwrap_err();
            assert!(matches!(err.kind, ErrorKind::DiskManagement));
            assert_eq!(read_at(&mut file, 0), sector0);
            assert_markers(&mut file, g, data_start);
        }

        let mut file = legacy_disk(g, uefi_min_end, 3);
        let mbr = rewrite(&mut file, g, uefi_min_end, 3, true).unwrap();
        assert_eq!(mbr[2], entry(0, 0x83, boot_end + 204800, root_min));
        assert_eq!(
            mbr[3],
            entry(0, 0x8e, uefi_min_end, g.sectors - uefi_min_end)
        );
        assert_markers(&mut file, g, uefi_min_end);
    }

    #[test]
    fn stale_kernel_mapping_of_the_data_stops_before_any_write() {
        let g = geometry(512);
        let mut file = legacy_disk(g, DATA_START, 2);
        let sector0 = read_at(&mut file, 0);

        // The kernel still maps slot 2 where the data sat before the table changed.
        let stale = (4401152, 29155328);
        let err = rewrite_checked(&mut file, g, 2, stale, true).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::DiskManagement));

        let shifted = (
            u64::from(DATA_START) + 2048,
            u64::from(g.sectors - DATA_START),
        );
        assert!(rewrite_checked(&mut file, g, 2, shifted, true).is_err());

        assert_eq!(read_at(&mut file, 0), sector0);
        assert_markers(&mut file, g, DATA_START);
    }

    #[test]
    fn native_4k_sectors_scale_to_the_kernel_view_and_keep_the_data_bytes() {
        let g = geometry(4096);
        let data_start = DATA_START / 8;
        let mut file = legacy_disk(g, data_start, 3);
        let sector0 = read_at(&mut file, 0);

        let raw = (u64::from(data_start), u64::from(g.sectors - data_start));
        assert!(rewrite_checked(&mut file, g, 3, raw, true).is_err());
        assert_eq!(read_at(&mut file, 0), sector0);

        let mbr = rewrite(&mut file, g, data_start, 3, true).unwrap();
        assert_eq!(MBR::read_from(&mut file, 4096).unwrap().header, mbr.header);
        assert_eq!(mbr[1], entry(0x80, 0x0b, 256, 524288));
        assert_eq!(mbr[4], entry(0, 0xef, 524544, 25600));
        assert_eq!(mbr[2], entry(0, 0x83, 550144, data_start - 550144));
        assert_eq!(mbr[3], entry(0, 0x8e, data_start, g.sectors - data_start));
        assert_eq!(
            u64::from(mbr[3].starting_lba) * 4096,
            u64::from(DATA_START) * 512
        );
        assert_markers(&mut file, g, data_start);

        assert_eq!(mismatched_slot(&mbr, g, &mapped(&mbr, g)), None);
        let unscaled = (1..=4)
            .filter(|&slot| mbr[slot].sectors > 0)
            .map(|slot| {
                let e = &mbr[slot];
                (slot, (u64::from(e.starting_lba), u64::from(e.sectors)))
            })
            .collect();
        assert_eq!(mismatched_slot(&mbr, g, &unscaled), Some(1));
    }

    #[test]
    fn unsupported_sector_size_is_rejected() {
        assert!(Geometry::new(2048, DISK_BYTES).is_err());
        assert!(Geometry::new(4096, DISK_BYTES).is_ok());
    }

    #[test]
    fn legacy_reinstall_after_uefi_needs_the_kernel_to_drop_the_efi_mapping() {
        let g = geometry(512);
        let mut file = legacy_disk(g, DATA_START, 3);
        let uefi = rewrite(&mut file, g, DATA_START, 3, true).unwrap();
        let legacy = rewrite(&mut file, g, DATA_START, 3, false).unwrap();
        let uefi_view = mapped(&uefi, g);

        // The kernel kept the old ESP and the busy data partition but refused the new root.
        let mut stale = mapped(&legacy, g);
        stale.remove(&2);
        stale.insert(4, uefi_view[&4]);
        assert_eq!(mismatched_slot(&legacy, g, &stale), Some(2));

        let mut esp_left = mapped(&legacy, g);
        esp_left.insert(4, uefi_view[&4]);
        assert_eq!(mismatched_slot(&legacy, g, &esp_left), Some(4));

        let mut extra = mapped(&legacy, g);
        extra.insert(5, (1, 1));
        assert_eq!(mismatched_slot(&legacy, g, &extra), Some(5));

        assert_eq!(mismatched_slot(&legacy, g, &mapped(&legacy, g)), None);
    }
}
