use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use crate::VolumeId;
use crate::prelude::*;
use crate::progress::{PhaseProgressTrackerHandle, ProgressUnits};
use crate::util::clone::clone_tree;
use crate::util::io::Counter;
use crate::util::{VersionString, btrfs};
use crate::{DATA_DIR, PackageId};

pub const PKG_VOLUME_DIR: &str = "package-data/volumes";
pub const BACKUP_DIR: &str = "/media/startos/backups";

const INSTALL_BACKUP_SUFFIX: &str = ".install-backup";

pub fn data_dir<P: AsRef<Path>>(datadir: P, pkg_id: &PackageId, volume_id: &VolumeId) -> PathBuf {
    datadir
        .as_ref()
        .join(PKG_VOLUME_DIR)
        .join(pkg_id)
        .join("data")
        .join(volume_id)
}

pub fn asset_dir<P: AsRef<Path>>(
    datadir: P,
    pkg_id: &PackageId,
    version: &VersionString,
) -> PathBuf {
    datadir
        .as_ref()
        .join(PKG_VOLUME_DIR)
        .join(pkg_id)
        .join("assets")
        .join(version.as_str())
}

pub fn backup_dir(pkg_id: &PackageId) -> PathBuf {
    Path::new(BACKUP_DIR).join(pkg_id).join("data")
}

fn pkg_volume_dir(pkg_id: &PackageId) -> PathBuf {
    Path::new(DATA_DIR).join(PKG_VOLUME_DIR).join(pkg_id)
}

fn install_backup_path(pkg_id: &PackageId) -> PathBuf {
    Path::new(DATA_DIR)
        .join(PKG_VOLUME_DIR)
        .join(format!("{pkg_id}{INSTALL_BACKUP_SUFFIX}"))
}

/// Creates the package volume root if missing — as a btrfs subvolume where
/// possible, so install backups are constant-time snapshots.
pub async fn ensure_volume_root(pkg_id: &PackageId) -> Result<(), Error> {
    let path = pkg_volume_dir(pkg_id);
    if tokio::fs::metadata(&path).await.is_ok() {
        return Ok(());
    }
    let volumes_dir = Path::new(DATA_DIR).join(PKG_VOLUME_DIR);
    let volumes = volumes_dir.as_path();
    tokio::fs::create_dir_all(volumes)
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("mkdir -p {volumes:?}")))?;
    if btrfs::is_btrfs(volumes).await {
        match btrfs::create_subvolume(&path).await {
            Ok(()) => return Ok(()),
            Err(e) => tracing::warn!("Could not create subvolume for {pkg_id}: {e}"),
        }
    }
    tokio::fs::create_dir_all(&path)
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("mkdir -p {path:?}")))
}

/// Creates a CoW snapshot of the package volume directory before an
/// install/update modifies it. Volume roots are btrfs subvolumes (enforced at
/// boot and at install), so this is a constant-time, atomic `btrfs subvolume
/// snapshot`. Anything else — a volume the boot conversion could not handle,
/// or a non-btrfs data dir — degrades to no backup.
/// Returns `true` if a backup was created, `false` otherwise.
pub async fn snapshot_volumes_for_install(pkg_id: &PackageId) -> Result<bool, Error> {
    let src = pkg_volume_dir(pkg_id);
    if !btrfs::is_subvolume(&src).await {
        return Ok(false);
    }
    let dst = install_backup_path(pkg_id);
    // Remove any stale backup from a previous failed attempt
    if let Err(e) = btrfs::delete_tree(&dst).await {
        tracing::warn!("Could not remove stale install backup for {pkg_id}: {e}");
        return Ok(false);
    }
    match btrfs::snapshot_subvolume(&src, &dst).await {
        Ok(()) => {
            tracing::info!("Created install backup for {pkg_id} at {dst:?}");
            Ok(true)
        }
        Err(e) => {
            tracing::warn!("Could not create install backup for {pkg_id}: {e}");
            btrfs::delete_tree(&dst).await.log_err();
            Ok(false)
        }
    }
}

async fn with_byte_progress(
    phase: &mut PhaseProgressTrackerHandle,
    base: u64,
    ctr: &Counter,
    fut: impl std::future::Future<Output = Result<(), Error>>,
) -> Result<(), Error> {
    tokio::pin!(fut);
    loop {
        tokio::select! {
            res = &mut fut => break res,
            _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
                phase.set_done(base + ctr.load());
            }
        }
    }
}

/// Restores the package volume directory from a CoW snapshot after a failed
/// install. The current (possibly corrupted) volume dir is deleted first.
/// No-op if no backup exists.
pub async fn restore_volumes_from_install_backup(pkg_id: &PackageId) -> Result<(), Error> {
    let backup = install_backup_path(pkg_id);
    if tokio::fs::metadata(&backup).await.is_err() {
        return Ok(());
    }
    let dst = pkg_volume_dir(pkg_id);
    btrfs::delete_tree(&dst).await?;
    crate::util::io::rename(&backup, &dst).await?;
    tracing::info!("Restored volumes from install backup for {pkg_id}");
    Ok(())
}

/// Removes the install backup after a successful install.
pub async fn remove_install_backup(pkg_id: &PackageId) -> Result<(), Error> {
    btrfs::delete_tree(&install_backup_path(pkg_id)).await
}

const CONVERT_TMP_SUFFIX: &str = ".convert-tmp";
const CONVERT_OLD_SUFFIX: &str = ".convert-old";

/// Boot-time volume maintenance, run after the db loads and before any service
/// container exists (so nothing can hold a mount on a volume): recovers any
/// interrupted conversion, sweeps orphaned install backups, and converts
/// plain-directory volume roots of installed packages into btrfs subvolumes so
/// install backups become constant-time snapshots. The conversion is a reflink
/// clone — O(extents), minutes for a large fragmented volume — hence the byte
/// progress on `phase`; it runs once per package, and re-checking on later
/// boots is a stat per package. Per-package failures degrade that package to
/// the legacy clone path rather than failing boot.
pub async fn boot_volume_maintenance(
    installed: &BTreeSet<PackageId>,
    all: &BTreeSet<PackageId>,
    phase: &mut PhaseProgressTrackerHandle,
) -> Result<(), Error> {
    let volumes = Path::new(DATA_DIR).join(PKG_VOLUME_DIR);
    if tokio::fs::metadata(&volumes).await.is_err() {
        return Ok(());
    }
    recover_and_sweep(&volumes, installed, all).await?;
    convert_to_subvolumes(&volumes, installed, phase).await
}

async fn recover_and_sweep(
    volumes: &Path,
    installed: &BTreeSet<PackageId>,
    all: &BTreeSet<PackageId>,
) -> Result<(), Error> {
    let mut entries = tokio::fs::read_dir(volumes)
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("read dir {volumes:?}")))?;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.strip_suffix(CONVERT_TMP_SUFFIX).is_some() {
            // incomplete conversion from an interrupted boot
            btrfs::delete_tree(entry.path()).await.log_err();
        } else if let Some(owner) = name.strip_suffix(CONVERT_OLD_SUFFIX) {
            let live = volumes.join(owner);
            if tokio::fs::metadata(&live).await.is_ok() {
                // interrupted after the swap completed
                btrfs::delete_tree(entry.path()).await.log_err();
            } else {
                // interrupted mid-swap: put the original back
                crate::util::io::rename(&entry.path(), &live)
                    .await
                    .log_err();
            }
        } else if let Some(owner) = name.strip_suffix(INSTALL_BACKUP_SUFFIX) {
            let Ok(owner) = owner.parse::<PackageId>() else {
                continue;
            };
            // Backups beside a mid-install/update package belong to service
            // load recovery — never touch those.
            if !installed.contains(&owner) && all.contains(&owner) {
                continue;
            }
            let live = volumes.join(&owner);
            if tokio::fs::metadata(&live).await.is_err() {
                // A backup with no live volume is an interrupted restore
                // (delete-then-rename) — the backup is the only copy, so
                // finish the rename.
                tracing::info!("Completing interrupted volume restore for {owner}");
                crate::util::io::rename(&entry.path(), &live)
                    .await
                    .log_err();
            } else if !all.contains(&owner) {
                tracing::info!("Removing orphaned install backup {:?}", entry.path());
                btrfs::delete_tree(entry.path()).await.log_err();
            }
            // A backup beside an Installed package with a live volume may be a
            // crash orphan or a stranded rollback point — indistinguishable,
            // so leave it; the next update of that package removes it as
            // stale.
        }
    }
    Ok(())
}

async fn convert_to_subvolumes(
    volumes: &Path,
    installed: &BTreeSet<PackageId>,
    phase: &mut PhaseProgressTrackerHandle,
) -> Result<(), Error> {
    let mut pending = Vec::new();
    for id in installed {
        let src = volumes.join(id);
        if tokio::fs::metadata(&src)
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false)
            && !btrfs::is_subvolume(&src).await
            && btrfs::is_btrfs(&src).await
        {
            pending.push((id, src));
        }
    }
    if pending.is_empty() {
        return Ok(());
    }
    let mut sized = Vec::new();
    let mut total = 0u64;
    for (id, src) in pending {
        match crate::util::io::dir_size(&src, None).await {
            Ok(size) => {
                total += size;
                sized.push((id, src, size));
            }
            Err(e) => tracing::warn!("Could not size volumes of {id} for conversion: {e}"),
        }
    }
    phase.set_units(Some(ProgressUnits::Bytes));
    phase.set_total(total);
    let mut done = 0;
    for (id, src, size) in sized {
        if let Err(e) = convert_one(id, &src, phase, done).await {
            tracing::warn!("Could not convert volumes of {id} to a subvolume: {e}");
        }
        done += size;
        phase.set_done(done);
    }
    Ok(())
}

async fn convert_one(
    id: &PackageId,
    src: &Path,
    phase: &mut PhaseProgressTrackerHandle,
    base: u64,
) -> Result<(), Error> {
    let start = std::time::Instant::now();
    let tmp = src.with_file_name(format!("{id}{CONVERT_TMP_SUFFIX}"));
    let old = src.with_file_name(format!("{id}{CONVERT_OLD_SUFFIX}"));
    btrfs::delete_tree(&tmp).await?;
    btrfs::create_subvolume(&tmp).await?;
    let ctr = Arc::new(Counter::new(0, std::sync::atomic::Ordering::Relaxed));
    if let Err(e) = with_byte_progress(phase, base, &ctr, clone_tree(src, &tmp, ctr.clone())).await
    {
        btrfs::delete_tree(&tmp).await.log_err();
        return Err(e);
    }
    if let Err(e) = crate::util::io::rename(src, &old).await {
        btrfs::delete_tree(&tmp).await.log_err();
        return Err(e);
    }
    if let Err(e) = crate::util::io::rename(&tmp, src).await {
        crate::util::io::rename(&old, src).await.log_err();
        btrfs::delete_tree(&tmp).await.log_err();
        return Err(e);
    }
    crate::util::io::delete_dir(&old).await.log_err();
    tracing::info!(
        "Converted volumes of {id} to a subvolume in {:?}",
        start.elapsed()
    );
    Ok(())
}
