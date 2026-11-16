use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use color_eyre::eyre::eyre;
use tokio::process::Command;

use crate::disk::util::pvscan;
use crate::prelude::*;
use crate::util::Invoke;

/// `/dev/<part>` paths for every partition currently exposed on `disk_path`.
pub async fn list_partitions(disk_path: &Path) -> Result<Vec<PathBuf>, Error> {
    let canonical = tokio::fs::canonicalize(disk_path)
        .await
        .with_ctx(|_| (ErrorKind::BlockDevice, disk_path.display().to_string()))?;
    let disk_name = canonical.file_name().ok_or_else(|| {
        Error::new(
            eyre!("invalid disk path: {}", disk_path.display()),
            ErrorKind::BlockDevice,
        )
    })?;
    let sys_block = Path::new("/sys/class/block").join(disk_name);
    let mut out = Vec::new();
    let mut entries = tokio::fs::read_dir(&sys_block).await?;
    while let Some(entry) = entries.next_entry().await.map_err(|e| {
        Error::new(
            eyre!("reading {}: {e}", sys_block.display()),
            ErrorKind::Filesystem,
        )
    })? {
        match tokio::fs::metadata(entry.path().join("partition")).await {
            Ok(_) => out.push(Path::new("/dev").join(entry.file_name())),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(out)
}

/// Canonical `/dev` node for `path`, falling back to the input on error so it
/// stays usable as a set key even for an unresolvable device.
async fn canonical(path: &Path) -> PathBuf {
    crate::util::io::canonicalize(path, false)
        .await
        .unwrap_or_else(|_| path.to_path_buf())
}

async fn holders_of(dev: &Path) -> Result<Vec<PathBuf>, Error> {
    let dev = tokio::fs::canonicalize(dev).await?;
    let name = dev
        .file_name()
        .ok_or_else(|| Error::new(eyre!(t!("error.invalid-request")), ErrorKind::BlockDevice))?;
    let dir = Path::new("/sys/class/block").join(name).join("holders");
    let mut out = Vec::new();
    let mut entries = tokio::fs::read_dir(&dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        out.push(Path::new("/dev").join(entry.file_name()));
    }
    Ok(out)
}

/// None after removal or for a non-device-mapper block device.
async fn dm_name(dev: &Path) -> Result<Option<String>, Error> {
    let Some(name) = dev.file_name() else {
        return Ok(None);
    };
    let value = crate::util::io::maybe_read_file_to_string(
        Path::new("/sys/class/block").join(name).join("dm/name"),
    )
    .await?;
    Ok(value
        .map(|value| value.trim().to_owned())
        .filter(|name| !name.is_empty()))
}

/// Holders of selected devices and their partitions, deepest first.
async fn stacked_holders(devices: &[PathBuf]) -> Result<Vec<PathBuf>, Error> {
    let mut depth: BTreeMap<PathBuf, usize> = BTreeMap::new();
    let mut frontier = Vec::new();
    for device in devices {
        frontier.push(device.clone());
        frontier.extend(list_partitions(device).await?);
    }
    let mut d = 0usize;
    while !frontier.is_empty() && d < 64 {
        let mut next = Vec::new();
        for dev in &frontier {
            for holder in holders_of(dev).await? {
                let ch = tokio::fs::canonicalize(&holder).await?;
                let slot = depth.entry(ch.clone()).or_insert(0);
                *slot = (*slot).max(d + 1);
                next.push(ch);
            }
        }
        frontier = next;
        d += 1;
    }
    let mut devs: Vec<(PathBuf, usize)> = depth.into_iter().collect();
    devs.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(devs.into_iter().map(|(dev, _)| dev).collect())
}

/// Rejects busy device-mapper holders; leaves unrelated targets untouched.
pub(crate) async fn release_block_holders(devices: &[PathBuf]) -> Result<(), Error> {
    for device in stacked_holders(devices).await? {
        if let Some(name) = dm_name(&device).await? {
            Command::new("dmsetup")
                .arg("remove")
                .arg(name)
                .invoke(ErrorKind::DiskManagement)
                .await?;
        }
    }
    Ok(())
}

/// VG names whose PVs sit on one of `target_devs`.
async fn target_vgs(target_devs: &BTreeSet<PathBuf>) -> Vec<String> {
    let pvs = match pvscan().await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("pvscan during quiesce failed: {e}");
            return Vec::new();
        }
    };
    let mut vgs = BTreeSet::new();
    for (pv, vg) in pvs {
        if let Some(vg) = vg {
            if target_devs.contains(&canonical(&pv).await) {
                vgs.insert(vg.to_string());
            }
        }
    }
    vgs.into_iter().collect()
}

/// Best-effort target-only claim teardown; leaves on-disk bytes unchanged.
/// Failures are logged; `update_partition_table` establishes kernel partition availability.
pub async fn quiesce_disk(disk_path: &Path) -> Result<(), Error> {
    let partitions = list_partitions(disk_path).await.unwrap_or_else(|error| {
        tracing::warn!(?error, "could not inspect target partitions");
        Vec::new()
    });

    let dm_devices = stacked_holders(&[disk_path.to_owned()])
        .await
        .unwrap_or_else(|error| {
            tracing::warn!(?error, "could not inspect target block holders");
            Vec::new()
        });

    let mut target_devs = BTreeSet::from([canonical(disk_path).await]);
    for part in &partitions {
        target_devs.insert(canonical(part).await);
    }
    target_devs.extend(dm_devices.iter().cloned());

    for dev in &target_devs {
        if let Err(e) = Command::new("swapoff")
            .arg(dev)
            .invoke(ErrorKind::DiskManagement)
            .await
        {
            tracing::debug!("swapoff {} skipped: {e}", dev.display());
        }
    }

    for dev in &target_devs {
        if Command::new("umount")
            .arg("-A")
            .arg(dev)
            .invoke(ErrorKind::Filesystem)
            .await
            .is_err()
        {
            if let Err(e) = Command::new("umount")
                .arg("-A")
                .arg("-l")
                .arg(dev)
                .invoke(ErrorKind::Filesystem)
                .await
            {
                tracing::warn!("lazy umount of {} failed: {e}", dev.display());
            }
        }
    }

    for vg in target_vgs(&target_devs).await {
        if let Err(e) = Command::new("vgchange")
            .arg("-an")
            .arg(&vg)
            .invoke(ErrorKind::DiskManagement)
            .await
        {
            tracing::warn!("vgchange -an {vg} during quiesce failed: {e}");
        }
    }

    for dev in &dm_devices {
        let Some(name) = dm_name(dev).await.log_err().flatten() else {
            continue;
        };
        if let Err(e) = Command::new("dmsetup")
            .arg("remove")
            .arg("--force")
            .arg(&name)
            .invoke(ErrorKind::DiskManagement)
            .await
        {
            tracing::warn!("dmsetup remove {name} during quiesce failed: {e}");
        }
    }

    for part in &partitions {
        if let Err(e) = forget_btrfs_scan(part).await {
            tracing::debug!("btrfs forget {} skipped: {e}", part.display());
        }
    }

    Ok(())
}

async fn forget_btrfs_scan(partition: &Path) -> Result<(), Error> {
    Command::new("btrfs")
        .args(["device", "scan", "--forget"])
        .arg(partition)
        .invoke(ErrorKind::DiskManagement)
        .await?;
    Ok(())
}

/// Per-entry BLKPG update via `partx`. Tolerates a still-busy partition on the
/// same disk (unlike BLKRRPART, which would fail with EBUSY).
pub async fn update_partition_table(disk_path: &Path) -> Result<(), Error> {
    Command::new("partx")
        .arg("--update")
        .arg(disk_path)
        .invoke(ErrorKind::DiskManagement)
        .await?;
    crate::disk::util::settle_udev().await?;
    // Udev may reinstall Btrfs scan references during partition updates.
    for part in list_partitions(disk_path).await.unwrap_or_default() {
        if let Err(e) = forget_btrfs_scan(&part).await {
            tracing::warn!(
                "btrfs device scan --forget {} after partx failed: {e}",
                part.display()
            );
        }
    }
    Ok(())
}
