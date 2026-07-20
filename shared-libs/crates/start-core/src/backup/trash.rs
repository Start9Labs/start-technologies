use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::FutureExt;
use futures::future::{BoxFuture, Shared};

use crate::disk::mount::guard::{GenericMountGuard, TmpMountGuard};
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::util::sync::SyncMutex;

/// Deleting a backup is an unlink per encrypted file — hours on a large backup,
/// depending on the target's filesystem — so deletion instead atomically renames
/// the backup into this hidden dir at the target root and a background sweep
/// unlinks the contents. A sweep interrupted by shutdown or an unplugged drive
/// leaves the trash in place; the next sweep of the same target (one runs before
/// any backup to it) picks it up.
pub const TRASH_DIR_NAME: &str = ".startos-trash";

pub fn trash_dir(backup_root: impl AsRef<Path>) -> PathBuf {
    backup_root.as_ref().join(TRASH_DIR_NAME)
}

pub async fn has_trash(backup_root: impl AsRef<Path>) -> bool {
    tokio::fs::metadata(trash_dir(backup_root)).await.is_ok()
}

/// Atomically move `path` (which must be on the filesystem mounted at
/// `backup_root`) into the target's trash for background deletion.
pub async fn move_to_trash(
    backup_root: impl AsRef<Path>,
    path: impl AsRef<Path>,
) -> Result<(), Error> {
    crate::util::io::rename(
        path.as_ref(),
        trash_dir(backup_root).join(Guid::new().as_ref()),
    )
    .await
}

type SweepFuture = Shared<BoxFuture<'static, Result<(), Arc<Error>>>>;

lazy_static::lazy_static! {
    /// At most one sweep per mounted target, keyed by its tmp mountpoint (which
    /// is derived from the source hash), so every flow touching the same target
    /// joins the same sweep instead of racing it.
    static ref SWEEPS: SyncMutex<BTreeMap<PathBuf, SweepFuture>> =
        SyncMutex::new(BTreeMap::new());
}

/// Unlink everything in the target's trash, then return. A joined sweep that
/// started before the caller's dir landed in the trash may finish without having
/// seen it, so loop until the trash is actually gone. Each sweep holds its own
/// clone of the guard, keeping the target mounted for as long as it runs.
pub async fn sweep_until_clear(guard: &TmpMountGuard) -> Result<(), Error> {
    while has_trash(guard.path()).await {
        ensure_sweep(guard.clone())
            .await
            .map_err(|e| Error::new(eyre!("{e}"), e.kind))?;
    }
    Ok(())
}

fn ensure_sweep(guard: TmpMountGuard) -> SweepFuture {
    SWEEPS.mutate(|sweeps| {
        let key = guard.path().to_owned();
        if let Some(sweep) = sweeps.get(&key) {
            if sweep.peek().is_none() {
                return sweep.clone();
            }
        }
        let sweep = tokio::spawn(sweep(guard))
            .map(|res| {
                res.map_err(|e| Error::new(eyre!("sweep task panicked: {e}"), ErrorKind::Unknown))
                    .and_then(|res| res)
                    .map_err(Arc::new)
            })
            .boxed()
            .shared();
        sweeps.insert(key, sweep.clone());
        sweep
    })
}

async fn sweep(guard: TmpMountGuard) -> Result<(), Error> {
    let trash = trash_dir(guard.path());
    match tokio::fs::remove_dir_all(&trash).await {
        // 39 = directory not empty: something was trashed mid-sweep; the sweep
        // that follows it (see `sweep_until_clear`) takes care of it
        Err(e) if e.kind() == std::io::ErrorKind::NotFound || e.raw_os_error() == Some(39) => {
            Ok(())
        }
        res => res,
    }
    .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("rm -rf {trash:?}")))?;
    // v1 backups are trashed out of `StartOSBackups`; once the last server's is
    // gone, remove the then-empty dir too (fails harmlessly if non-empty)
    let _ = tokio::fs::remove_dir(guard.path().join(crate::disk::LEGACY_BACKUP_DIR_NAME)).await;
    Ok(())
}
