use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

use futures::Future;
use lazy_static::lazy_static;
use tokio::sync::Mutex;
use tracing::instrument;

use super::filesystem::{FileSystem, MountType, ReadOnly, ReadWrite};
use super::util::{is_mountpoint, unmount};
use crate::util::sync::SyncMutex;
use crate::util::{Invoke, Never};
use crate::{Error, ResultExt};

pub const TMP_MOUNTPOINT: &'static str = "/media/startos/tmp";

pub trait GenericMountGuard: std::fmt::Debug + Send + Sync + 'static {
    fn path(&self) -> &Path;
    fn unmount(self) -> impl Future<Output = Result<(), Error>> + Send;
}

impl GenericMountGuard for Never {
    fn path(&self) -> &Path {
        match *self {}
    }
    async fn unmount(self) -> Result<(), Error> {
        match self {}
    }
}

impl<T> GenericMountGuard for Arc<T>
where
    T: GenericMountGuard,
{
    fn path(&self) -> &Path {
        (&**self).path()
    }
    async fn unmount(self) -> Result<(), Error> {
        if let Ok(guard) = Arc::try_unwrap(self) {
            guard.unmount().await?;
        }
        Ok(())
    }
}

type MountSlot = Arc<Mutex<(MountType, Weak<MountGuard>)>>;

#[derive(Debug)]
pub struct MountGuard {
    mountpoint: PathBuf,
    pub(super) mounted: bool,
    // `TmpMountGuard`-managed mounts carry their shared slot so teardown can
    // skip the umount once the slot's been re-occupied (see `unmount_slot`).
    slot: Option<MountSlot>,
}
impl MountGuard {
    pub async fn mount(
        filesystem: &impl FileSystem,
        mountpoint: impl AsRef<Path>,
        mount_type: MountType,
    ) -> Result<Self, Error> {
        let mountpoint = mountpoint.as_ref().to_owned();
        filesystem.mount(&mountpoint, mount_type).await?;
        Ok(MountGuard {
            mountpoint,
            mounted: true,
            slot: None,
        })
    }
    fn as_unmounted(&self) -> Self {
        Self {
            mountpoint: self.mountpoint.clone(),
            mounted: false,
            slot: self.slot.clone(),
        }
    }
    pub fn take(&mut self) -> Self {
        let unmounted = self.as_unmounted();
        std::mem::replace(self, unmounted)
    }
    pub async fn unmount(mut self, delete_mountpoint: bool) -> Result<(), Error> {
        if self.mounted {
            unmount_slot(
                &self.slot,
                &self.mountpoint,
                !cfg!(feature = "unstable"),
                delete_mountpoint,
            )
            .await?;
            self.mounted = false;
        }
        Ok(())
    }
}
/// Unmount `mountpoint`, but for a slot-managed mount only while the slot has
/// no live guard. A remount records itself as the slot's `Weak`, so a nonzero
/// `strong_count` means another mount now owns this path and will tear itself
/// down — the slot lock is held across the umount so no remount slips in
/// between the check and the umount.
async fn unmount_slot(
    slot: &Option<MountSlot>,
    mountpoint: &Path,
    lazy: bool,
    delete_mountpoint: bool,
) -> Result<(), Error> {
    let occupancy = match slot {
        Some(slot) => {
            let slot = slot.lock().await;
            if slot.1.strong_count() != 0 {
                return Ok(());
            }
            Some(slot)
        }
        None => None,
    };
    unmount(mountpoint, lazy).await?;
    if delete_mountpoint {
        match tokio::fs::remove_dir(mountpoint).await {
            Err(e) if e.raw_os_error() == Some(39) => Ok(()), // directory not empty
            a => a,
        }
        .with_ctx(|_| {
            (
                crate::ErrorKind::Filesystem,
                format!("rm {}", mountpoint.display()),
            )
        })?;
    }
    drop(occupancy);
    Ok(())
}
impl Drop for MountGuard {
    fn drop(&mut self) {
        if self.mounted {
            let mountpoint = std::mem::take(&mut self.mountpoint);
            let slot = self.slot.take();
            tokio::spawn(async move {
                unmount_slot(&slot, &mountpoint, true, false)
                    .await
                    .log_err()
            });
        }
    }
}
impl GenericMountGuard for MountGuard {
    fn path(&self) -> &Path {
        &self.mountpoint
    }
    async fn unmount(self) -> Result<(), Error> {
        MountGuard::unmount(self, false).await
    }
}

async fn tmp_mountpoint(source: &impl FileSystem) -> Result<PathBuf, Error> {
    Ok(Path::new(TMP_MOUNTPOINT).join(base32::encode(
        base32::Alphabet::Rfc4648 { padding: false },
        &source.source_hash().await?[0..20],
    )))
}

lazy_static! {
    // Maps each tmp mountpoint to its own lock. The outer map lock is held only
    // while fetching/creating a slot — never across the mount itself.
    static ref TMP_MOUNTS: SyncMutex<BTreeMap<PathBuf, MountSlot>> =
        SyncMutex::new(BTreeMap::new());
}

#[derive(Debug, Clone)]
pub struct TmpMountGuard {
    guard: Arc<MountGuard>,
}
impl TmpMountGuard {
    /// DRAGONS: if you try to mount something as ro and rw at the same time, the ro mount will be upgraded to rw.
    #[instrument(skip_all)]
    pub async fn mount(filesystem: &impl FileSystem, mount_type: MountType) -> Result<Self, Error> {
        let mountpoint = tmp_mountpoint(filesystem).await?;
        let slot_handle = TMP_MOUNTS.mutate(|m| {
            m.entry(mountpoint.clone())
                .or_insert_with(|| Arc::new(Mutex::new((mount_type, Weak::new()))))
                .clone()
        });
        let mut slot = slot_handle.lock().await;
        let (prev_mt, weak_slot) = &mut *slot;
        if let Some(guard) = weak_slot.upgrade() {
            // upgrade to rw
            if *prev_mt == ReadOnly && mount_type != ReadOnly {
                tokio::process::Command::new("mount")
                    .arg("-o")
                    .arg("remount,rw")
                    .arg(&mountpoint)
                    .invoke(crate::ErrorKind::Filesystem)
                    .await?;
                *prev_mt = ReadWrite;
            }
            Ok(TmpMountGuard { guard })
        } else {
            // No live guard, yet a hard-killed prior process (Drop never ran)
            // can leave a stale kernel mount here. The mountpoint is derived
            // from the source hash, so any mount present is the same content —
            // safe to lazily unmount and remount.
            if is_mountpoint(&mountpoint).await? {
                unmount(&mountpoint, true).await?;
            }
            let mut mount_guard = MountGuard::mount(filesystem, &mountpoint, mount_type).await?;
            mount_guard.slot = Some(slot_handle.clone());
            let guard = Arc::new(mount_guard);
            *weak_slot = Arc::downgrade(&guard);
            *prev_mt = mount_type;
            Ok(TmpMountGuard { guard })
        }
    }

    pub fn take(&mut self) -> Self {
        let unmounted = Self {
            guard: Arc::new(self.guard.as_unmounted()),
        };
        std::mem::replace(self, unmounted)
    }
}
impl GenericMountGuard for TmpMountGuard {
    fn path(&self) -> &Path {
        self.guard.path()
    }
    async fn unmount(self) -> Result<(), Error> {
        self.guard.unmount().await
    }
}

#[derive(Debug)]
pub struct SubPath<G: GenericMountGuard> {
    guard: G,
    path: PathBuf,
}
impl<G: GenericMountGuard> SubPath<G> {
    pub fn new(guard: G, path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        let path = guard.path().join(path.strip_prefix("/").unwrap_or(path));
        Self { guard, path }
    }
}
impl<G: GenericMountGuard> GenericMountGuard for SubPath<G> {
    fn path(&self) -> &Path {
        self.path.as_path()
    }
    async fn unmount(self) -> Result<(), Error> {
        self.guard.unmount().await
    }
}
