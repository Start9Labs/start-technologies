use std::collections::BTreeMap;
use std::io::ErrorKind as IoErrorKind;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use futures::Future;
use lazy_static::lazy_static;
use tokio::sync::Mutex;
use tracing::instrument;

use super::filesystem::{FileSystem, MountType, ReadOnly, ReadWrite};
use super::util::{is_mountpoint, unmount};
use crate::prelude::{eyre, t};
use crate::util::sync::SyncMutex;
use crate::util::{Invoke, Never};
use crate::{Error, ErrorKind, ResultExt};

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

#[derive(Debug, Clone)]
struct DeviceIdentity {
    path: PathBuf,
    node: (u64, u64, u64),
}
impl DeviceIdentity {
    async fn capture(path: PathBuf) -> Result<Self, Error> {
        let metadata = tokio::fs::metadata(&path).await?;
        Ok(Self {
            path,
            node: (metadata.dev(), metadata.ino(), metadata.rdev()),
        })
    }

    async fn is_current(&self) -> Result<bool, Error> {
        match Self::capture(self.path.clone()).await {
            Ok(current) => Ok(self.node == current.node),
            Err(error)
                if error
                    .source
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == IoErrorKind::NotFound) =>
            {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }
}

#[derive(Debug)]
pub struct MountGuard {
    mountpoint: PathBuf,
    mounted: AtomicBool,
    device: Option<DeviceIdentity>,
    slot: Option<MountSlot>,
}
impl MountGuard {
    pub async fn mount(
        filesystem: &impl FileSystem,
        mountpoint: impl AsRef<Path>,
        mount_type: MountType,
    ) -> Result<Self, Error> {
        let mountpoint = mountpoint.as_ref().to_owned();
        let device = capture_device(filesystem).await?;
        filesystem.mount(&mountpoint, mount_type).await?;
        let guard = MountGuard {
            mountpoint,
            mounted: AtomicBool::new(true),
            device,
            slot: None,
        };
        guard.check_device().await?;
        filesystem.check_mount(guard.path()).await?;
        Ok(guard)
    }
    fn as_unmounted(&self) -> Self {
        Self {
            mountpoint: self.mountpoint.clone(),
            mounted: AtomicBool::new(false),
            device: self.device.clone(),
            slot: self.slot.clone(),
        }
    }
    pub fn take(&mut self) -> Self {
        let unmounted = self.as_unmounted();
        std::mem::replace(self, unmounted)
    }
    pub(super) fn is_mounted(&self) -> bool {
        self.mounted.load(Ordering::Acquire)
    }

    async fn device_is_current(&self) -> Result<bool, Error> {
        match &self.device {
            Some(device) => device.is_current().await,
            None => Ok(true),
        }
    }

    async fn check_device(&self) -> Result<(), Error> {
        if self.device_is_current().await? {
            Ok(())
        } else {
            Err(Error::new(
                color_eyre::eyre::eyre!(rust_i18n::t!("shared-volumes.source-unavailable")),
                crate::ErrorKind::Filesystem,
            ))
        }
    }

    async fn retire(&self) -> Result<(), Error> {
        if self.is_mounted() {
            unmount(&self.mountpoint, true).await?;
            self.mounted.store(false, Ordering::Release);
        }
        Ok(())
    }

    pub async fn unmount(self, delete_mountpoint: bool) -> Result<(), Error> {
        if self.is_mounted() {
            unmount_slot(
                &self.slot,
                &self.mountpoint,
                !cfg!(feature = "unstable"),
                delete_mountpoint,
            )
            .await?;
            self.mounted.store(false, Ordering::Release);
        }
        Ok(())
    }
}
async fn capture_device(filesystem: &impl FileSystem) -> Result<Option<DeviceIdentity>, Error> {
    match filesystem.device().await? {
        Some(path) => DeviceIdentity::capture(path).await.map(Some),
        None => Ok(None),
    }
}

// The slot lock excludes replacement mounts during teardown.
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
            Err(e)
                if matches!(
                    e.kind(),
                    IoErrorKind::NotFound | IoErrorKind::DirectoryNotEmpty
                ) =>
            {
                Ok(())
            }
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
        if self.is_mounted() {
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

async fn tmp_mountpoint(source: &impl FileSystem, mount_type: MountType) -> Result<PathBuf, Error> {
    Ok(Path::new(TMP_MOUNTPOINT).join(base32::encode(
        base32::Alphabet::Rfc4648 { padding: false },
        &source.mount_hash(mount_type).await?[0..20],
    )))
}

lazy_static! {
    static ref TMP_MOUNTS: SyncMutex<BTreeMap<PathBuf, MountSlot>> =
        SyncMutex::new(BTreeMap::new());
}

fn mount_slot(mountpoint: PathBuf, mount_type: MountType) -> MountSlot {
    TMP_MOUNTS.mutate(|mounts| {
        // In-flight mounts and teardown retain their slots.
        mounts.retain(|_, slot| Arc::strong_count(slot) > 1);
        mounts
            .entry(mountpoint)
            .or_insert_with(|| Arc::new(Mutex::new((mount_type, Weak::new()))))
            .clone()
    })
}

pub(crate) struct UnusedMountSource {
    _slot: tokio::sync::OwnedMutexGuard<(MountType, Weak<MountGuard>)>,
}

#[derive(Debug, Clone)]
pub struct TmpMountGuard {
    guard: Arc<MountGuard>,
}
impl TmpMountGuard {
    /// Read-write requests upgrade a shared read-only mount.
    #[instrument(skip_all)]
    pub async fn mount(filesystem: &impl FileSystem, mount_type: MountType) -> Result<Self, Error> {
        let device = capture_device(filesystem).await?;
        let mountpoint = tmp_mountpoint(filesystem, mount_type).await?;
        let slot_handle = mount_slot(mountpoint.clone(), mount_type);
        let mut slot = slot_handle.lock().await;
        let (prev_mt, weak_slot) = &mut *slot;
        if let Some(guard) = weak_slot.upgrade() {
            if guard.is_mounted() && guard.device_is_current().await? {
                filesystem.check_mount(&mountpoint).await?;
                if *prev_mt == ReadOnly && mount_type != ReadOnly {
                    tokio::process::Command::new("mount")
                        .arg("-o")
                        .arg("remount,rw")
                        .arg(&mountpoint)
                        .invoke(crate::ErrorKind::Filesystem)
                        .await?;
                    *prev_mt = ReadWrite;
                }
                return Ok(TmpMountGuard { guard });
            }
            guard.retire().await?;
            *weak_slot = Weak::new();
        }
        // Mounts can survive a hard-killed owner.
        if is_mountpoint(&mountpoint).await? {
            unmount(&mountpoint, true).await?;
        }
        let mount_guard = MountGuard {
            mountpoint: mountpoint.clone(),
            mounted: AtomicBool::new(true),
            device,
            slot: Some(slot_handle.clone()),
        };
        filesystem.mount(&mountpoint, mount_type).await?;
        mount_guard.check_device().await?;
        filesystem.check_mount(&mountpoint).await?;
        let guard = Arc::new(mount_guard);
        *weak_slot = Arc::downgrade(&guard);
        *prev_mt = mount_type;
        Ok(TmpMountGuard { guard })
    }

    pub(crate) async fn reserve_unmounted(
        filesystem: &impl FileSystem,
        mount_type: MountType,
    ) -> Result<UnusedMountSource, Error> {
        let slot = mount_slot(tmp_mountpoint(filesystem, mount_type).await?, mount_type)
            .lock_owned()
            .await;
        if slot.1.upgrade().is_some() {
            return Err(Error::new(
                eyre!(t!("backup.target.in-use")),
                ErrorKind::InvalidRequest,
            ));
        }
        Ok(UnusedMountSource { _slot: slot })
    }

    pub(crate) async fn existing(
        filesystem: &impl FileSystem,
        mount_type: MountType,
    ) -> Result<Option<WeakMountGuard>, Error> {
        let path = tmp_mountpoint(filesystem, mount_type).await?;
        let slot = TMP_MOUNTS.mutate(|mounts| mounts.get(&path).cloned());
        let Some(slot) = slot else { return Ok(None) };
        let Ok(slot) = slot.try_lock() else {
            return Ok(None);
        };
        Ok(slot
            .1
            .upgrade()
            .filter(|guard| guard.is_mounted())
            .map(|guard| WeakMountGuard(Arc::downgrade(&guard))))
    }

    pub(crate) async fn existing_all(
        filesystem: &impl FileSystem,
    ) -> Result<Vec<WeakMountGuard>, Error> {
        let mut mounts = BTreeMap::new();
        for mode in [ReadOnly, ReadWrite, super::filesystem::BackupWrite] {
            if let Some(mount) = Self::existing(filesystem, mode).await? {
                mounts.insert(mount.key(), mount);
            }
        }
        Ok(mounts.into_values().collect())
    }

    pub(crate) async fn retire_changed_devices() -> (Vec<PathBuf>, bool) {
        let slots = TMP_MOUNTS.mutate(|mounts| mounts.values().cloned().collect::<Vec<_>>());
        let mut retired = Vec::new();
        let mut pending = false;
        for slot in slots {
            let Ok(mut slot) = slot.try_lock() else {
                pending = true;
                continue;
            };
            let Some(guard) = slot.1.upgrade() else {
                continue;
            };
            if !guard.is_mounted() {
                continue;
            }
            let result = async {
                if !guard.device_is_current().await? {
                    guard.retire().await?;
                    Ok(Some(guard.device.as_ref().unwrap().path.clone()))
                } else {
                    Ok(None)
                }
            }
            .await;
            match result {
                Ok(Some(path)) => {
                    retired.push(path);
                    slot.1 = Weak::new();
                }
                Ok(None) => {}
                Err(error) => {
                    pending = true;
                    Result::<(), Error>::Err(error).log_err();
                }
            }
        }
        (retired, pending)
    }

    pub(crate) fn downgrade(&self) -> WeakMountGuard {
        WeakMountGuard(Arc::downgrade(&self.guard))
    }

    pub(crate) fn is_active(&self) -> bool {
        self.guard.is_mounted()
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

#[derive(Clone, Debug)]
pub(crate) struct WeakMountGuard(Weak<MountGuard>);
// Observations never mutate the mount or its slot.
impl std::panic::RefUnwindSafe for WeakMountGuard {}
impl WeakMountGuard {
    pub(crate) fn key(&self) -> usize {
        self.0.as_ptr() as usize
    }

    pub(crate) fn active_path(&self) -> Option<PathBuf> {
        self.0
            .upgrade()
            .filter(|guard| guard.is_mounted())
            .map(|guard| guard.mountpoint.clone())
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

#[cfg(test)]
mod tests {
    use super::*;

    struct UnmountedFilesystem {
        id: String,
        attempted: AtomicBool,
    }
    impl FileSystem for UnmountedFilesystem {
        async fn mount<P: AsRef<Path> + Send>(&self, _: P, _: MountType) -> Result<(), Error> {
            self.attempted.store(true, Ordering::Release);
            Err(Error::new(eyre!("unavailable"), ErrorKind::Filesystem))
        }
        async fn source_hash(&self) -> Result<digest::Output<sha2::Sha256>, Error> {
            use digest::Digest;
            Ok(sha2::Sha256::digest(self.id.as_bytes()))
        }
    }

    #[tokio::test]
    async fn unavailable_cached_mount_rejects_reuse_without_retirement_and_recovers() {
        struct CheckedFilesystem {
            inner: UnmountedFilesystem,
            available: AtomicBool,
        }
        impl FileSystem for CheckedFilesystem {
            async fn source_hash(&self) -> Result<digest::Output<sha2::Sha256>, Error> {
                self.inner.source_hash().await
            }
            async fn mount<P: AsRef<Path> + Send>(
                &self,
                path: P,
                mode: MountType,
            ) -> Result<(), Error> {
                self.inner.mount(path, mode).await
            }
            async fn check_mount(&self, _: &Path) -> Result<(), Error> {
                if self.available.load(Ordering::Acquire) {
                    Ok(())
                } else {
                    Err(Error::new(eyre!("disconnected"), ErrorKind::Filesystem))
                }
            }
        }
        let filesystem = CheckedFilesystem {
            inner: UnmountedFilesystem {
                id: crate::shared_volumes::SharedVolumeId::new().to_string(),
                attempted: AtomicBool::new(false),
            },
            available: AtomicBool::new(false),
        };
        let mountpoint = tmp_mountpoint(&filesystem, ReadOnly).await.unwrap();
        let slot = mount_slot(mountpoint.clone(), ReadOnly);
        let guard = Arc::new(MountGuard {
            mountpoint,
            mounted: AtomicBool::new(true),
            device: None,
            slot: Some(slot.clone()),
        });
        slot.lock().await.1 = Arc::downgrade(&guard);
        assert!(TmpMountGuard::mount(&filesystem, ReadWrite).await.is_err());
        assert!(guard.is_mounted());
        assert_eq!(slot.lock().await.0, ReadOnly);
        assert!(
            TmpMountGuard::existing(&filesystem, ReadOnly)
                .await
                .unwrap()
                .is_some()
        );
        assert!(!filesystem.inner.attempted.load(Ordering::Acquire));
        filesystem.available.store(true, Ordering::Release);
        let recovered = TmpMountGuard::mount(&filesystem, ReadOnly).await.unwrap();
        assert!(Arc::ptr_eq(&guard, &recovered.guard));
        assert!(!filesystem.inner.attempted.load(Ordering::Acquire));
        guard.mounted.store(false, Ordering::Release);
    }

    #[tokio::test]
    async fn existing_lookup_does_not_mount_an_unseen_filesystem() {
        let filesystem = UnmountedFilesystem {
            id: crate::shared_volumes::SharedVolumeId::new().to_string(),
            attempted: AtomicBool::new(false),
        };
        assert!(
            TmpMountGuard::existing(&filesystem, ReadOnly)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            TmpMountGuard::existing_all(&filesystem)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(!filesystem.attempted.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn existing_lookup_finds_both_cifs_policies_without_duplicating_access_upgrades() {
        use crate::disk::mount::filesystem::cifs::Cifs;
        let filesystem = Cifs {
            hostname: crate::shared_volumes::SharedVolumeId::new().to_string(),
            path: "/share".into(),
            username: "user".into(),
            password: Some("private".into()),
        };
        let mut guards = Vec::new();
        for mode in [ReadOnly, super::super::filesystem::BackupWrite] {
            let mountpoint = tmp_mountpoint(&filesystem, mode).await.unwrap();
            let slot = mount_slot(mountpoint.clone(), mode);
            let guard = Arc::new(MountGuard {
                mountpoint,
                mounted: AtomicBool::new(true),
                device: None,
                slot: Some(slot.clone()),
            });
            slot.lock().await.1 = Arc::downgrade(&guard);
            guards.push(guard);
        }
        let mounts = TmpMountGuard::existing_all(&filesystem).await.unwrap();
        assert_eq!(mounts.len(), 2);
        for guard in guards {
            assert!(
                mounts
                    .iter()
                    .any(|mount| mount.key() == Arc::as_ptr(&guard) as usize)
            );
            guard.mounted.store(false, Ordering::Release);
        }
    }

    #[tokio::test]
    async fn removing_an_unused_source_excludes_mounts_until_reservation_release() {
        let filesystem = UnmountedFilesystem {
            id: crate::shared_volumes::SharedVolumeId::new().to_string(),
            attempted: AtomicBool::new(false),
        };
        let reservation = TmpMountGuard::reserve_unmounted(&filesystem, ReadOnly)
            .await
            .unwrap();
        {
            let mount = TmpMountGuard::mount(&filesystem, ReadOnly);
            tokio::pin!(mount);
            assert!(futures::poll!(&mut mount).is_pending());
        }
        let next = TmpMountGuard::reserve_unmounted(&filesystem, ReadOnly);
        tokio::pin!(next);
        assert!(futures::poll!(&mut next).is_pending());
        drop(reservation);
        let _reservation = next.await.unwrap();
        assert!(!filesystem.attempted.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn ordinary_mounts_share_a_slot_across_access_upgrades() {
        let filesystem = UnmountedFilesystem {
            id: crate::shared_volumes::SharedVolumeId::new().to_string(),
            attempted: AtomicBool::new(false),
        };
        assert_eq!(
            tmp_mountpoint(&filesystem, ReadOnly).await.unwrap(),
            tmp_mountpoint(&filesystem, ReadWrite).await.unwrap()
        );
        assert_eq!(
            tmp_mountpoint(&filesystem, ReadWrite).await.unwrap(),
            tmp_mountpoint(&filesystem, super::super::filesystem::BackupWrite)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn source_identity_detects_removal_and_replacement_at_the_same_path() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("node");
        tokio::fs::write(&path, b"original").await.unwrap();
        let identity = DeviceIdentity::capture(path.clone()).await.unwrap();
        assert!(identity.is_current().await.unwrap());
        tokio::fs::write(&path, b"changed contents").await.unwrap();
        assert!(identity.is_current().await.unwrap());
        tokio::fs::rename(&path, directory.path().join("old-node"))
            .await
            .unwrap();
        assert!(!identity.is_current().await.unwrap());
        tokio::fs::write(&path, b"replacement").await.unwrap();
        assert!(!identity.is_current().await.unwrap());
    }

    #[tokio::test]
    async fn source_identity_follows_the_device_behind_an_alias() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first");
        let second = directory.path().join("second");
        let alias = directory.path().join("uuid");
        tokio::fs::write(&first, b"first").await.unwrap();
        tokio::fs::write(&second, b"second").await.unwrap();
        tokio::fs::symlink(&first, &alias).await.unwrap();
        let identity = DeviceIdentity::capture(alias.clone()).await.unwrap();
        tokio::fs::remove_file(&alias).await.unwrap();
        tokio::fs::symlink(&first, &alias).await.unwrap();
        assert!(identity.is_current().await.unwrap());
        tokio::fs::remove_file(&alias).await.unwrap();
        tokio::fs::symlink(&second, &alias).await.unwrap();
        assert!(!identity.is_current().await.unwrap());
    }

    #[tokio::test]
    async fn unmount_slot_succeeds_when_mountpoint_is_missing() {
        let directory = tempfile::tempdir().unwrap();
        let mountpoint = directory.path().join("missing");

        unmount_slot(&None, &mountpoint, true, true).await.unwrap();
    }
}
