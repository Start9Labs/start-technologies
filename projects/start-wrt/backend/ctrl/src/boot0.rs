//! eMMC hardware boot partition (boot0/boot1) provisioning.
//!
//! The SpaceMiT K1 BootROM boots eMMC from the *hardware boot partitions*
//! (`/dev/mmcblkXboot0`/`boot1`), not the user area: an 80-byte `bootinfo`
//! header at boot0 offset 0 tells the ROM where to find the FSBL (u-boot
//! SPL) — at `spl0_offset` in boot0, with a fallback mirror at `spl1_offset`
//! in boot1. The flash raw copy only writes the eMMC user area, so a board
//! whose factory never provisioned boot0 cannot boot from eMMC at all.
//!
//! This module converges boot0/boot1 to the bootinfo + FSBL shipped in the
//! running image (the FSBL from the SD's `fsbl` GPT partition — the exact
//! binary that just booted this board — and `bootinfo_emmc.bin` from the
//! `bootfs` partition). Convergent and idempotent: skip when the bytes
//! already match, otherwise write mirror (boot1) first, boot0 FSBL second,
//! and the bootinfo sector last as the single-sector atomic commit point,
//! each write read-back verified. The EXT_CSD boot-enable bits are *not*
//! touched: the vendor factory flasher never sets them either (it only uses
//! `mmc_set_part_conf` to switch partition access for erasing), so the K1
//! ROM evidently locates boot0 on its own.

use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use crate::flash::{device_size_sectors, run_cmd, SfdiskPartition, SECTOR_SIZE};
use crate::prelude::*;

pub(crate) const BOOTINFO_LEN: usize = 80;
const BOOTINFO_MAGIC: u32 = 0xb00714f0;
/// The stored CRC32 covers this many leading bytes; the checksum itself sits
/// right after, little-endian.
const BOOTINFO_CRC_COVERAGE: usize = 0x40;

/// Standard (zlib/IEEE 802.3) CRC32, bitwise — matches `binascii.crc32`,
/// which the vendor u-boot's `build_binary_file.py` uses to seal bootinfo.
fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                (c >> 1) ^ 0xedb8_8320
            } else {
                c >> 1
            };
        }
    }
    !c
}

/// Parsed and validated `bootinfo_emmc` blob.
pub(crate) struct Bootinfo {
    pub raw: [u8; BOOTINFO_LEN],
    /// Byte offset of the FSBL inside boot0.
    pub spl0_offset: u64,
    /// Byte offset of the FSBL mirror inside boot1.
    pub spl1_offset: u64,
}

/// Validate an `bootinfo_emmc.bin` blob (magic, flash type, CRC) and extract
/// the SPL offsets. Rejecting anything malformed here is what makes it safe
/// to write the blob to the sector the BootROM trusts.
pub(crate) fn parse_bootinfo(raw: &[u8]) -> Result<Bootinfo, Error> {
    if raw.len() != BOOTINFO_LEN {
        return Err(Error::new(
            eyre!(
                "bootinfo has wrong length {} (expected {BOOTINFO_LEN})",
                raw.len()
            ),
            ErrorKind::Deserialization,
        ));
    }
    let magic = u32::from_le_bytes(raw[0..4].try_into().unwrap());
    if magic != BOOTINFO_MAGIC {
        return Err(Error::new(
            eyre!("bootinfo has wrong magic {magic:#010x} (expected {BOOTINFO_MAGIC:#010x})"),
            ErrorKind::Deserialization,
        ));
    }
    if &raw[8..12] != b"eMMC" {
        return Err(Error::new(
            eyre!("bootinfo flash type is not eMMC"),
            ErrorKind::Deserialization,
        ));
    }
    let stored_crc = u32::from_le_bytes(
        raw[BOOTINFO_CRC_COVERAGE..BOOTINFO_CRC_COVERAGE + 4]
            .try_into()
            .unwrap(),
    );
    let computed_crc = crc32(&raw[..BOOTINFO_CRC_COVERAGE]);
    if stored_crc != computed_crc {
        return Err(Error::new(
            eyre!(
                "bootinfo CRC mismatch: stored {stored_crc:#010x}, computed {computed_crc:#010x}"
            ),
            ErrorKind::Deserialization,
        ));
    }
    let spl0_offset = u32::from_le_bytes(raw[0x20..0x24].try_into().unwrap()) as u64;
    let spl1_offset = u32::from_le_bytes(raw[0x24..0x28].try_into().unwrap()) as u64;
    if spl0_offset % SECTOR_SIZE != 0 || spl1_offset % SECTOR_SIZE != 0 {
        return Err(Error::new(
            eyre!("bootinfo SPL offsets not sector-aligned: spl0={spl0_offset:#x} spl1={spl1_offset:#x}"),
            ErrorKind::Deserialization,
        ));
    }
    Ok(Bootinfo {
        raw: raw.try_into().unwrap(),
        spl0_offset,
        spl1_offset,
    })
}

/// Outcome of a provisioning pass.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ProvisionOutcome {
    /// boot0/boot1 already hold exactly the intended bytes — nothing written.
    UpToDate,
    /// boot0/boot1 were (re)written and verified.
    Provisioned,
}

fn read_region(path: &Path, offset: u64, len: usize) -> Result<Vec<u8>, Error> {
    let f = File::open(path).map_err(|e| {
        Error::new(
            eyre!("failed to open {} for read: {e}", path.display()),
            ErrorKind::Filesystem,
        )
    })?;
    let mut buf = vec![0u8; len];
    f.read_exact_at(&mut buf, offset).map_err(|e| {
        Error::new(
            eyre!(
                "read of {len} bytes at {offset:#x} from {} failed: {e}",
                path.display()
            ),
            ErrorKind::Filesystem,
        )
    })?;
    Ok(buf)
}

/// Read a region bypassing the page cache (O_DIRECT), so read-back
/// verification sees what the device returns, not what we just wrote into
/// the cache. O_DIRECT needs the offset, length, and buffer address
/// sector-aligned; the caller guarantees the offset, we handle the rest.
fn read_region_direct(path: &Path, offset: u64, len: usize) -> Result<Vec<u8>, Error> {
    let aligned_len = len.div_ceil(SECTOR_SIZE as usize) * SECTOR_SIZE as usize;
    let mut raw = vec![0u8; aligned_len + SECTOR_SIZE as usize];
    let shift = (SECTOR_SIZE as usize - (raw.as_ptr() as usize % SECTOR_SIZE as usize))
        % SECTOR_SIZE as usize;
    let f = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_DIRECT)
        .open(path)
        .map_err(|e| {
            Error::new(
                eyre!("failed to open {} with O_DIRECT: {e}", path.display()),
                ErrorKind::Filesystem,
            )
        })?;
    f.read_exact_at(&mut raw[shift..shift + aligned_len], offset)
        .map_err(|e| {
            Error::new(
                eyre!(
                    "direct read of {aligned_len} bytes at {offset:#x} from {} failed: {e}",
                    path.display()
                ),
                ErrorKind::Filesystem,
            )
        })?;
    Ok(raw[shift..shift + len].to_vec())
}

/// Write `data` at `offset`, fsync, and read it back for comparison.
/// `direct_io` selects O_DIRECT for the read-back (real block devices);
/// tests on regular files pass `false`.
fn write_and_verify(path: &Path, offset: u64, data: &[u8], direct_io: bool) -> Result<(), Error> {
    let f = OpenOptions::new().write(true).open(path).map_err(|e| {
        Error::new(
            eyre!("failed to open {} for write: {e}", path.display()),
            ErrorKind::Filesystem,
        )
    })?;
    f.write_all_at(data, offset).map_err(|e| {
        Error::new(
            eyre!(
                "write of {} bytes at {offset:#x} to {} failed: {e}",
                data.len(),
                path.display()
            ),
            ErrorKind::Filesystem,
        )
    })?;
    f.sync_all().map_err(|e| {
        Error::new(
            eyre!("sync of {} failed: {e}", path.display()),
            ErrorKind::Filesystem,
        )
    })?;
    drop(f);

    let readback = if direct_io {
        read_region_direct(path, offset, data.len())?
    } else {
        read_region(path, offset, data.len())?
    };
    if readback != data {
        return Err(Error::new(
            eyre!(
                "read-back verification failed at {offset:#x} on {}",
                path.display()
            ),
            ErrorKind::Filesystem,
        ));
    }
    Ok(())
}

/// The full first sector of boot0: the 80-byte bootinfo zero-padded to a
/// sector, matching how the image lays out its own bootinfo at LBA 0.
fn bootinfo_sector(bootinfo: &Bootinfo) -> [u8; SECTOR_SIZE as usize] {
    let mut sector = [0u8; SECTOR_SIZE as usize];
    sector[..BOOTINFO_LEN].copy_from_slice(&bootinfo.raw);
    sector
}

/// True if boot0/boot1 already hold exactly the intended bootinfo + FSBL.
fn boot_regions_match(
    boot0: &Path,
    boot1: &Path,
    bootinfo: &Bootinfo,
    fsbl: &[u8],
) -> Result<bool, Error> {
    Ok(read_region(boot0, 0, BOOTINFO_LEN)? == bootinfo.raw
        && read_region(boot0, bootinfo.spl0_offset, fsbl.len())? == fsbl
        && read_region(boot1, bootinfo.spl1_offset, fsbl.len())? == fsbl)
}

/// Write bootinfo + FSBL to the boot partitions in commit-safe order:
/// boot1 mirror first, boot0 FSBL second, and the bootinfo sector last —
/// a power cut at any point leaves either the previous chain or the new
/// one intact, never a half-written pointer to a half-written FSBL.
fn write_boot_partitions(
    boot0: &Path,
    boot1: &Path,
    bootinfo: &Bootinfo,
    fsbl: &[u8],
    direct_io: bool,
) -> Result<(), Error> {
    write_and_verify(boot1, bootinfo.spl1_offset, fsbl, direct_io)?;
    write_and_verify(boot0, bootinfo.spl0_offset, fsbl, direct_io)?;
    write_and_verify(boot0, 0, &bootinfo_sector(bootinfo), direct_io)?;
    Ok(())
}

/// Clears `force_ro` on the given sysfs paths, restoring it (best-effort)
/// on drop. The kernel exposes eMMC boot partitions read-only by default.
struct ForceRoGuard {
    paths: Vec<PathBuf>,
}

impl ForceRoGuard {
    fn unlock(devs: &[&str]) -> Result<Self, Error> {
        let mut unlocked = Vec::new();
        for dev in devs {
            let path = PathBuf::from(format!("/sys/block/{dev}/force_ro"));
            fs::write(&path, "0").map_err(|e| {
                Error::new(
                    eyre!("failed to clear force_ro on {dev}: {e}"),
                    ErrorKind::Filesystem,
                )
            })?;
            unlocked.push(path);
        }
        Ok(ForceRoGuard { paths: unlocked })
    }
}

impl Drop for ForceRoGuard {
    fn drop(&mut self) {
        for path in &self.paths {
            let _ = fs::write(path, "1");
        }
    }
}

/// Read the release's `bootinfo_emmc.bin` from the SD's `bootfs` FAT
/// partition via a temporary read-only mount.
async fn read_bootinfo_from_bootfs(bootfs_node: &str) -> Result<Vec<u8>, Error> {
    let dir = tempfile::tempdir().map_err(|e| {
        Error::new(
            eyre!("failed to create tempdir: {e}"),
            ErrorKind::Filesystem,
        )
    })?;
    let mnt = dir
        .path()
        .to_str()
        .ok_or_else(|| Error::new(eyre!("non-UTF8 tempdir path"), ErrorKind::Filesystem))?;
    run_cmd("mount", &["-t", "vfat", "-o", "ro", bootfs_node, mnt]).await?;
    let read = tokio::fs::read(dir.path().join("bootinfo_emmc.bin")).await;
    let _ = run_cmd("umount", &[mnt]).await;
    read.map_err(|e| {
        Error::new(
            eyre!("failed to read bootinfo_emmc.bin from {bootfs_node}: {e}"),
            ErrorKind::Filesystem,
        )
    })
}

fn find_partition<'a>(
    partitions: &'a [SfdiskPartition],
    name: &str,
) -> Result<&'a SfdiskPartition, Error> {
    partitions
        .iter()
        .find(|p| p.name.as_deref() == Some(name))
        .ok_or_else(|| {
            Error::new(
                eyre!("no partition named '{name}' found on source device"),
                ErrorKind::NotFound,
            )
        })
}

/// Converge the eMMC hardware boot partitions to the running image's
/// bootinfo + FSBL. `sd_partitions` is the source (SD) partition table
/// already read by the flash; `emmc_dev` is the target base device
/// (e.g. "mmcblk2").
pub(crate) async fn provision_emmc_boot(
    sd_partitions: &[SfdiskPartition],
    emmc_dev: &str,
    report: &(dyn Fn(&str) + Sync),
) -> Result<ProvisionOutcome, Error> {
    // Sources: the FSBL that just booted this very board, and the eMMC
    // bootinfo blob generated by the same u-boot build.
    let fsbl_part = find_partition(sd_partitions, "fsbl")?;
    let bootfs_part = find_partition(sd_partitions, "bootfs")?;

    let fsbl = read_region(
        Path::new(&fsbl_part.node),
        0,
        (fsbl_part.size * SECTOR_SIZE) as usize,
    )?;
    if fsbl.iter().all(|&b| b == 0) {
        return Err(Error::new(
            eyre!("fsbl partition {} is empty", fsbl_part.node),
            ErrorKind::InvalidRequest,
        ));
    }

    let bootinfo_raw = read_bootinfo_from_bootfs(&bootfs_part.node).await?;
    let bootinfo = parse_bootinfo(&bootinfo_raw)?;

    let boot0_dev = format!("{emmc_dev}boot0");
    let boot1_dev = format!("{emmc_dev}boot1");
    let boot0 = PathBuf::from(format!("/dev/{boot0_dev}"));
    let boot1 = PathBuf::from(format!("/dev/{boot1_dev}"));
    if !boot0.exists() || !boot1.exists() {
        return Err(Error::new(
            eyre!("eMMC boot partitions {boot0_dev}/{boot1_dev} not found"),
            ErrorKind::NotFound,
        ));
    }

    // The FSBL (+ its offset) must fit inside each boot partition.
    for (dev, offset) in [
        (&boot0_dev, bootinfo.spl0_offset),
        (&boot1_dev, bootinfo.spl1_offset),
    ] {
        let size = device_size_sectors(dev)? * SECTOR_SIZE;
        if offset + fsbl.len() as u64 > size {
            return Err(Error::new(
                eyre!(
                    "FSBL ({} bytes at {offset:#x}) does not fit in {dev} ({size} bytes)",
                    fsbl.len()
                ),
                ErrorKind::InvalidRequest,
            ));
        }
    }

    if boot_regions_match(&boot0, &boot1, &bootinfo, &fsbl)? {
        report("eMMC boot firmware already up to date.");
        return Ok(ProvisionOutcome::UpToDate);
    }

    report("Writing eMMC boot firmware (boot0/boot1)...");
    let guard = ForceRoGuard::unlock(&[&boot0_dev, &boot1_dev])?;
    write_boot_partitions(&boot0, &boot1, &bootinfo, &fsbl, true)?;
    drop(guard);
    report("eMMC boot firmware written and verified.");
    Ok(ProvisionOutcome::Provisioned)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `bootinfo_emmc.bin` emitted by the pinned u-boot build
    /// (`spacemit-com/uboot-2022.10@ad097c0e`) — golden reference so the
    /// parser and the artifact can't drift apart silently.
    const BOOTINFO_EMMC: [u8; BOOTINFO_LEN] =
        *b"\xf0\x14\x07\xb0\x01\x00\x01\x00\x65\x4d\x4d\x43\x00\x00\x00\x00\
        \x00\x02\x00\x00\x00\x00\x01\x00\x00\x00\x00\x10\x00\x00\x00\x00\
        \x00\x02\x00\x00\x00\x00\x00\x00\x00\x60\x03\x00\x00\x00\x00\x00\
        \x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\
        \x67\xdc\xac\x43\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";

    #[test]
    fn parse_golden_bootinfo() {
        let bi = parse_bootinfo(&BOOTINFO_EMMC).expect("golden bootinfo must parse");
        assert_eq!(bi.spl0_offset, 0x200);
        assert_eq!(bi.spl1_offset, 0);
        assert_eq!(bi.raw, BOOTINFO_EMMC);
    }

    #[test]
    fn reject_corrupt_bootinfo() {
        // wrong length
        assert!(parse_bootinfo(&BOOTINFO_EMMC[..79]).is_err());
        // wrong magic
        let mut bad = BOOTINFO_EMMC;
        bad[0] = 0;
        assert!(parse_bootinfo(&bad).is_err());
        // flipped payload byte → CRC mismatch
        let mut bad = BOOTINFO_EMMC;
        bad[0x20] ^= 0xff;
        assert!(parse_bootinfo(&bad).is_err());
        // flipped CRC byte
        let mut bad = BOOTINFO_EMMC;
        bad[BOOTINFO_CRC_COVERAGE] ^= 0xff;
        assert!(parse_bootinfo(&bad).is_err());
        // SD-flavored blob must be rejected (flash type "SDC")
        let mut sd = BOOTINFO_EMMC;
        sd[8..12].copy_from_slice(b"SDC\0");
        let crc = crc32(&sd[..BOOTINFO_CRC_COVERAGE]);
        sd[BOOTINFO_CRC_COVERAGE..BOOTINFO_CRC_COVERAGE + 4].copy_from_slice(&crc.to_le_bytes());
        assert!(parse_bootinfo(&sd).is_err());
    }

    #[test]
    fn crc32_matches_zlib() {
        // "123456789" → 0xcbf43926, the standard CRC32 check value.
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    fn fake_boot_partition(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let f = File::create(&path).unwrap();
        f.set_len(4 * 1024 * 1024).unwrap(); // 4 MiB, like the real boot0/boot1
        path
    }

    #[test]
    fn provision_writes_gates_and_heals() {
        let dir = tempfile::tempdir().unwrap();
        let boot0 = fake_boot_partition(dir.path(), "boot0");
        let boot1 = fake_boot_partition(dir.path(), "boot1");
        let bootinfo = parse_bootinfo(&BOOTINFO_EMMC).unwrap();
        let fsbl: Vec<u8> = (0..8192u32).map(|i| (i % 251) as u8 + 1).collect();

        // Fresh (all-zero) partitions differ from the target state.
        assert!(!boot_regions_match(&boot0, &boot1, &bootinfo, &fsbl).unwrap());

        write_boot_partitions(&boot0, &boot1, &bootinfo, &fsbl, false).unwrap();

        // Layout: bootinfo at boot0[0] (zero-padded sector), FSBL at
        // boot0[spl0_offset] and boot1[spl1_offset].
        assert_eq!(read_region(&boot0, 0, BOOTINFO_LEN).unwrap(), BOOTINFO_EMMC);
        assert_eq!(
            read_region(
                &boot0,
                BOOTINFO_LEN as u64,
                SECTOR_SIZE as usize - BOOTINFO_LEN
            )
            .unwrap(),
            vec![0u8; SECTOR_SIZE as usize - BOOTINFO_LEN]
        );
        assert_eq!(read_region(&boot0, 0x200, fsbl.len()).unwrap(), fsbl);
        assert_eq!(read_region(&boot1, 0, fsbl.len()).unwrap(), fsbl);

        // Idempotent: converged partitions now match.
        assert!(boot_regions_match(&boot0, &boot1, &bootinfo, &fsbl).unwrap());

        // Self-healing: corrupt one FSBL byte in the boot1 mirror.
        {
            let f = OpenOptions::new().write(true).open(&boot1).unwrap();
            f.write_all_at(&[0xff], 100).unwrap();
        }
        assert!(!boot_regions_match(&boot0, &boot1, &bootinfo, &fsbl).unwrap());
        write_boot_partitions(&boot0, &boot1, &bootinfo, &fsbl, false).unwrap();
        assert!(boot_regions_match(&boot0, &boot1, &bootinfo, &fsbl).unwrap());
    }
}
