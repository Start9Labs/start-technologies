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
const INSTALL_BACKUP_TMP_SUFFIX: &str = ".install-backup-tmp";
const RESTORE_OLD_SUFFIX: &str = ".restore-old";

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

fn volumes_dir() -> PathBuf {
    Path::new(DATA_DIR).join(PKG_VOLUME_DIR)
}

fn pkg_volume_dir(pkg_id: &PackageId) -> PathBuf {
    volumes_dir().join(pkg_id)
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    // PackageId never contains a dot, so with_extension only ever appends.
    path.with_extension(&suffix[1..])
}

/// A package's install backup and the protocol around it. `snapshot` stages a CoW copy of
/// the live root at `backup`; `restore` swaps it back in through `restore_old`, whose
/// presence marks a restore in flight — one that survives the empty volume roots the load
/// and mount paths recreate.
pub struct InstallBackup {
    pkg_id: PackageId,
    live: PathBuf,
    backup: PathBuf,
    backup_tmp: PathBuf,
    restore_old: PathBuf,
}

impl InstallBackup {
    pub fn of(pkg_id: &PackageId) -> Self {
        Self::new(&volumes_dir(), pkg_id)
    }

    fn new(volumes: &Path, pkg_id: &PackageId) -> Self {
        let live = volumes.join(pkg_id);
        Self {
            pkg_id: pkg_id.clone(),
            backup: with_suffix(&live, INSTALL_BACKUP_SUFFIX),
            backup_tmp: with_suffix(&live, INSTALL_BACKUP_TMP_SUFFIX),
            restore_old: with_suffix(&live, RESTORE_OLD_SUFFIX),
            live,
        }
    }

    pub async fn exists(&self) -> bool {
        tokio::fs::metadata(&self.backup).await.is_ok()
    }

    /// Snapshots the live root as the new rollback point, staged at `backup_tmp` so the
    /// previous backup is discarded only once its replacement exists. Returns false for a
    /// non-subvolume root, where no constant-time backup is possible.
    pub async fn snapshot(&self) -> Result<bool, Error> {
        // The backup being replaced may be the only complete copy of the package's data.
        self.resolve_pending().await?;
        if !btrfs::is_subvolume(&self.live).await {
            return Ok(false);
        }
        btrfs::delete_tree(&self.backup_tmp).await.log_err();
        let staged = async {
            btrfs::snapshot_subvolume(&self.live, &self.backup_tmp).await?;
            btrfs::delete_tree(&self.backup).await?;
            crate::util::io::rename(&self.backup_tmp, &self.backup).await
        }
        .await;
        if let Err(e) = staged {
            tracing::warn!("Could not create install backup for {}: {e}", self.pkg_id);
            btrfs::delete_tree(&self.backup_tmp).await.log_err();
            return Ok(false);
        }
        tracing::info!(
            "Created install backup for {} at {:?}",
            self.pkg_id,
            self.backup
        );
        Ok(true)
    }

    /// Swaps the backup in as the live root, moving the live root aside to `restore_old` —
    /// the marker that a restore is in flight — and dropping it once the backup has landed.
    pub async fn restore(&self) -> Result<(), Error> {
        if !self.exists().await {
            return Ok(());
        }
        btrfs::delete_tree(&self.restore_old).await?;
        if tokio::fs::metadata(&self.live).await.is_ok() {
            crate::util::io::rename(&self.live, &self.restore_old).await?;
        }
        crate::util::io::rename(&self.backup, &self.live).await?;
        // The aside tree is superseded, so failing to drop it is not a failed restore.
        btrfs::delete_tree(&self.restore_old).await.log_err();
        tracing::info!("Restored volumes from install backup for {}", self.pkg_id);
        Ok(())
    }

    /// Drives a half-applied restore — marked by the restore-old tree, which unlike the
    /// live root is never recreated by the load/mount paths — to a terminal state.
    pub async fn resolve_pending(&self) -> Result<(), Error> {
        if tokio::fs::metadata(&self.restore_old).await.is_err() {
            return Ok(());
        }
        if self.exists().await {
            // The backup never landed, so it is the only complete copy. Anything at `live`
            // should be a recreated skeleton; real files there mean we cannot pick a winner.
            if holds_files(&self.live).await? {
                return Err(Error::new(
                    eyre!(
                        "{}",
                        t!("volume.restore-conflict", id = self.pkg_id.to_string())
                    ),
                    ErrorKind::Filesystem,
                ));
            }
            btrfs::delete_tree(&self.live).await?;
            crate::util::io::rename(&self.backup, &self.live).await?;
        } else if tokio::fs::metadata(&self.live).await.is_err() {
            // The backup was consumed and the live root never came back.
            crate::util::io::rename(&self.restore_old, &self.live).await?;
            tracing::info!("Completed interrupted volume restore for {}", self.pkg_id);
            return Ok(());
        }
        btrfs::delete_tree(&self.restore_old).await?;
        tracing::info!("Completed interrupted volume restore for {}", self.pkg_id);
        Ok(())
    }

    /// Discards the backup after a successful install.
    pub async fn remove(&self) -> Result<(), Error> {
        // Never discard the backup while a restore is in flight: it can be the only
        // complete copy, and every caller here believes the install succeeded.
        self.resolve_pending().await?;
        btrfs::delete_tree(&self.backup).await
    }
}

/// True if the tree holds anything other than directories.
async fn holds_files(path: &Path) -> Result<bool, Error> {
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(Error::new(e, ErrorKind::Filesystem)),
        };
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                stack.push(entry.path());
            } else {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Creates the package volume root if missing — as a btrfs subvolume where
/// possible, so install backups are constant-time snapshots.
pub async fn ensure_volume_root(pkg_id: &PackageId) -> Result<(), Error> {
    // Everything that trusts or creates the live root funnels through here.
    InstallBackup::of(pkg_id).resolve_pending().await?;
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
    let mut names = Vec::new();
    let mut entries = tokio::fs::read_dir(volumes)
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("read dir {volumes:?}")))?;
    while let Some(entry) = entries.next_entry().await? {
        if let Some(name) = entry.file_name().to_str() {
            names.push(name.to_owned());
        }
    }

    // Resolve restores first: readdir order could otherwise reap a backup that a
    // restore-old marker still needs.
    for name in &names {
        if name.ends_with(CONVERT_TMP_SUFFIX) || name.ends_with(INSTALL_BACKUP_TMP_SUFFIX) {
            // incomplete conversion or snapshot from an interrupted boot
            btrfs::delete_tree(volumes.join(name)).await.log_err();
        } else if let Some(owner) = name.strip_suffix(RESTORE_OLD_SUFFIX) {
            if let Ok(owner) = owner.parse::<PackageId>() {
                InstallBackup::new(volumes, &owner)
                    .resolve_pending()
                    .await
                    .log_err();
            }
        }
    }

    for name in &names {
        let entry_path = volumes.join(name);
        let name = name.as_str();
        if let Some(owner) = name.strip_suffix(CONVERT_OLD_SUFFIX) {
            let live = volumes.join(owner);
            if tokio::fs::metadata(&live).await.is_ok() {
                // interrupted after the swap completed
                btrfs::delete_tree(&entry_path).await.log_err();
            } else {
                // interrupted mid-swap: put the original back
                crate::util::io::rename(&entry_path, &live).await.log_err();
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
                // No live root: the backup is the only copy, so finish the rename.
                tracing::info!("Completing interrupted volume restore for {owner}");
                crate::util::io::rename(&entry_path, &live).await.log_err();
            } else if !all.contains(&owner) {
                tracing::info!("Removing orphaned install backup {:?}", entry_path);
                btrfs::delete_tree(&entry_path).await.log_err();
            }
            // No restore-old marker: the live tree is authoritative, so leave the backup
            // as a rollback point.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::io::TmpDir;

    async fn write(path: &Path, contents: &str) -> Result<(), Error> {
        tokio::fs::create_dir_all(path.parent().unwrap()).await?;
        tokio::fs::write(path, contents).await?;
        Ok(())
    }

    // data/db/PG_VERSION stands in for "this tree holds the user's database".
    async fn seed_tree(root: &Path, marker: &str) -> Result<(), Error> {
        write(&root.join("data/db/PG_VERSION"), marker).await
    }

    async fn read_marker(root: &Path) -> Option<String> {
        tokio::fs::read_to_string(root.join("data/db/PG_VERSION"))
            .await
            .ok()
    }

    fn pkg() -> PackageId {
        "testpkg".parse().unwrap()
    }

    struct Case {
        _tmp: TmpDir,
        volumes: PathBuf,
        ib: InstallBackup,
    }

    async fn case() -> Result<Case, Error> {
        let tmp = TmpDir::new().await?;
        let volumes = tmp.join("volumes");
        tokio::fs::create_dir_all(&volumes).await?;
        Ok(Case {
            ib: InstallBackup::new(&volumes, &pkg()),
            volumes,
            _tmp: tmp,
        })
    }

    #[tokio::test]
    async fn restore_puts_the_backup_in_place_and_leaves_no_debris() -> Result<(), Error> {
        let c = case().await?;
        seed_tree(&c.ib.live, "new").await?;
        seed_tree(&c.ib.backup, "pre-update").await?;

        c.ib.restore().await?;

        assert_eq!(read_marker(&c.ib.live).await.as_deref(), Some("pre-update"));
        assert!(!c.ib.exists().await);
        assert!(tokio::fs::metadata(&c.ib.restore_old).await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn restore_is_a_noop_without_a_backup() -> Result<(), Error> {
        let c = case().await?;
        seed_tree(&c.ib.live, "live").await?;

        c.ib.restore().await?;

        assert_eq!(read_marker(&c.ib.live).await.as_deref(), Some("live"));
        Ok(())
    }

    /// Interrupted between the renames, with the live root since recreated as a skeleton.
    #[tokio::test]
    async fn resolve_finishes_a_restore_interrupted_between_the_renames() -> Result<(), Error> {
        let c = case().await?;
        seed_tree(&c.ib.restore_old, "new").await?;
        seed_tree(&c.ib.backup, "pre-update").await?;
        tokio::fs::create_dir_all(c.ib.live.join("data/db")).await?; // recreated skeleton

        c.ib.resolve_pending().await?;

        assert_eq!(read_marker(&c.ib.live).await.as_deref(), Some("pre-update"));
        assert!(!c.ib.exists().await);
        assert!(tokio::fs::metadata(&c.ib.restore_old).await.is_err());
        Ok(())
    }

    /// Interrupted after the backup landed but before the moved-aside tree was dropped.
    #[tokio::test]
    async fn resolve_drops_the_aside_tree_once_the_backup_landed() -> Result<(), Error> {
        let c = case().await?;
        seed_tree(&c.ib.live, "pre-update").await?;
        seed_tree(&c.ib.restore_old, "new").await?;

        c.ib.resolve_pending().await?;

        assert_eq!(read_marker(&c.ib.live).await.as_deref(), Some("pre-update"));
        assert!(tokio::fs::metadata(&c.ib.restore_old).await.is_err());
        Ok(())
    }

    /// Nothing at the live path and no backup: the moved-aside tree is the only copy left.
    #[tokio::test]
    async fn resolve_recovers_the_aside_tree_when_nothing_else_survives() -> Result<(), Error> {
        let c = case().await?;
        seed_tree(&c.ib.restore_old, "pre-update").await?;

        c.ib.resolve_pending().await?;

        assert_eq!(read_marker(&c.ib.live).await.as_deref(), Some("pre-update"));
        assert!(tokio::fs::metadata(&c.ib.restore_old).await.is_err());
        Ok(())
    }

    /// Two populated trees: fail loudly rather than pick a winner.
    #[tokio::test]
    async fn resolve_refuses_to_choose_between_two_populated_trees() -> Result<(), Error> {
        let c = case().await?;
        seed_tree(&c.ib.live, "written-since").await?;
        seed_tree(&c.ib.restore_old, "new").await?;
        seed_tree(&c.ib.backup, "pre-update").await?;

        assert!(c.ib.resolve_pending().await.is_err());
        assert_eq!(
            read_marker(&c.ib.live).await.as_deref(),
            Some("written-since")
        );
        assert_eq!(
            read_marker(&c.ib.backup).await.as_deref(),
            Some("pre-update")
        );
        Ok(())
    }

    #[tokio::test]
    async fn resolve_is_a_noop_with_no_pending_restore() -> Result<(), Error> {
        let c = case().await?;
        seed_tree(&c.ib.live, "live").await?;
        seed_tree(&c.ib.backup, "rollback-point").await?;

        c.ib.resolve_pending().await?;

        assert_eq!(read_marker(&c.ib.live).await.as_deref(), Some("live"));
        assert_eq!(
            read_marker(&c.ib.backup).await.as_deref(),
            Some("rollback-point")
        );
        Ok(())
    }

    /// A rollback point stranded beside a gutted live tree must not be discarded.
    #[tokio::test]
    async fn sweep_completes_a_stranded_restore_rather_than_stranding_it() -> Result<(), Error> {
        let c = case().await?;
        seed_tree(&c.ib.restore_old, "new").await?;
        seed_tree(&c.ib.backup, "pre-update").await?;
        tokio::fs::create_dir_all(c.ib.live.join("data/db")).await?;

        let installed = [pkg()].into_iter().collect();
        recover_and_sweep(&c.volumes, &installed, &installed).await?;

        assert_eq!(read_marker(&c.ib.live).await.as_deref(), Some("pre-update"));
        assert!(tokio::fs::metadata(&c.ib.restore_old).await.is_err());
        Ok(())
    }

    /// The orphan-reaping arm must not delete a backup a restore-old marker still needs.
    #[tokio::test]
    async fn sweep_resolves_before_reaping_an_orphaned_backup() -> Result<(), Error> {
        let c = case().await?;
        seed_tree(&c.ib.restore_old, "new").await?;
        seed_tree(&c.ib.backup, "pre-update").await?;
        tokio::fs::create_dir_all(c.ib.live.join("data/db")).await?;

        // owner present on disk but absent from the db, i.e. the orphan-reaping branch
        recover_and_sweep(&c.volumes, &BTreeSet::new(), &BTreeSet::new()).await?;

        assert_eq!(read_marker(&c.ib.live).await.as_deref(), Some("pre-update"));
        assert!(tokio::fs::metadata(&c.ib.restore_old).await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn sweep_clears_a_half_written_snapshot() -> Result<(), Error> {
        let c = case().await?;
        seed_tree(&c.ib.live, "live").await?;
        seed_tree(&c.ib.backup_tmp, "partial").await?;

        let installed = [pkg()].into_iter().collect();
        recover_and_sweep(&c.volumes, &installed, &installed).await?;

        assert!(tokio::fs::metadata(&c.ib.backup_tmp).await.is_err());
        assert_eq!(read_marker(&c.ib.live).await.as_deref(), Some("live"));
        Ok(())
    }
}
