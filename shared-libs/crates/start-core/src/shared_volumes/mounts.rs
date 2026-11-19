use std::collections::BTreeMap;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::disk::mount::filesystem::syscall::{
    DetachedMount, MountNamespace, open_beneath, open_tree_attr_idmap_fd, open_tree_clone_fd,
    same_mount, unmount_fd, userns_fd_from_idmap,
};
use crate::disk::mount::filesystem::idmapped::IdMap;
use crate::disk::mount::guard::{GenericMountGuard, TmpMountGuard};
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::shared_volumes::{SharedVolumeGrant, SharedVolumeId};
use crate::util::io::TmpDir;

const SERVICE_ROOT: &str = "media/startos/shared-volumes";

struct AttachedMount {
    mount: OwnedFd,
    namespace: Arc<MountNamespace>,
}

impl AttachedMount {
    fn bind(
        source: &impl AsFd,
        destination: &OwnedFd,
        readonly: bool,
        namespace: Arc<MountNamespace>,
        userns: Option<&OwnedFd>,
    ) -> Result<Self, Error> {
        let detached = DetachedMount::from_fd(match userns {
            Some(userns) => open_tree_attr_idmap_fd(source.as_fd(), userns.as_fd())?,
            None => open_tree_clone_fd(source.as_fd(), false)?,
        });
        detached.set_private()?;
        detached.set_readonly(readonly)?;
        let mount = namespace.run(|| detached.attach_to_fd(destination.as_fd()))?;
        Ok(Self { mount, namespace })
    }

    fn bind_in_namespace(
        source: BorrowedFd,
        destination: BorrowedFd,
        readonly: bool,
        recursive: bool,
        namespace: Arc<MountNamespace>,
    ) -> Result<Self, Error> {
        let mount = namespace.run(|| {
            let detached = DetachedMount::from_fd(open_tree_clone_fd(source, recursive)?);
            detached.configure_bind(readonly)?;
            detached.attach_to_fd(destination)
        })?;
        Ok(Self { mount, namespace })
    }

    fn detach(&self) -> Result<(), Error> {
        self.namespace.run(|| unmount_fd(self.mount.as_fd()))
    }
}

impl Drop for AttachedMount {
    fn drop(&mut self) {
        self.detach().log_err();
    }
}

/// Pins directory identity without retaining mount-cache ownership.
#[derive(Debug)]
pub(crate) struct PinnedDirectory {
    directory: OwnedFd,
    path: PathBuf,
}

impl PinnedDirectory {
    pub(crate) fn open(root: &Path, relative: &Path) -> Result<Self, Error> {
        let directory = open_beneath(root, &confined_relative(relative)?)?;
        let path = PathBuf::from(format!(
            "/proc/{}/fd/{}",
            std::process::id(),
            directory.as_raw_fd()
        ));
        Ok(Self { directory, path })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl AsFd for PinnedDirectory {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.directory.as_fd()
    }
}

#[derive(Debug)]
pub struct PreparedSource {
    directory: PinnedDirectory,
    guard: TmpMountGuard,
    /// The host view holds container ids; attaching applies the container's map.
    container_ids: bool,
}

impl PreparedSource {
    pub(crate) fn mount(&self) -> &TmpMountGuard {
        &self.guard
    }

    pub(crate) fn storing_container_ids(mut self) -> Self {
        self.container_ids = true;
        self
    }

    pub(crate) fn stores_container_ids(&self) -> bool {
        self.container_ids
    }

    pub async fn open(guard: TmpMountGuard, subpath: Option<PathBuf>) -> Result<Self, Error> {
        tokio::task::spawn_blocking(move || {
            let directory =
                PinnedDirectory::open(guard.path(), subpath.as_deref().unwrap_or(Path::new("")))?;
            Ok(Self {
                directory,
                guard,
                container_ids: false,
            })
        })
        .await
        .with_kind(ErrorKind::Unknown)?
    }

    pub async fn into_subpath(self, subpath: Option<PathBuf>) -> Result<Self, Error> {
        let Some(subpath) = subpath else {
            return Ok(self);
        };
        tokio::task::spawn_blocking(move || {
            let directory = PinnedDirectory::open(self.path(), &subpath)?;
            Ok(Self {
                directory,
                guard: self.guard,
                container_ids: self.container_ids,
            })
        })
        .await
        .with_kind(ErrorKind::Unknown)?
    }
}

impl GenericMountGuard for PreparedSource {
    fn path(&self) -> &Path {
        self.directory.path()
    }

    async fn unmount(self) -> Result<(), Error> {
        drop(self.directory);
        self.guard.unmount().await
    }
}

struct StagingRoot {
    _unavailable: AttachedMount,
    directory: PinnedDirectory,
    _mount: AttachedMount,
    _backing: TmpDir,
}

struct MountSlot {
    stage: Option<StagedMount>,
    placeholder: AttachedMount,
}

struct StagedMount {
    id: SharedVolumeId,
    readonly: bool,
    target: Option<AttachedMount>,
    mount: AttachedMount,
    source: PreparedSource,
    directory: PathBuf,
    path: PathBuf,
}

impl StagedMount {
    async fn retire(self) -> Result<(), Error> {
        if let Some(target) = self.target {
            target.detach()?;
            drop(target);
        }
        self.mount.detach()?;
        drop(self.mount);
        self.source.unmount().await?;
        tokio::fs::remove_dir(self.directory).await?;
        Ok(())
    }
}

/// Revocation detaches SDK-managed targets and stages, including manually managed subcontainers.
/// Independent aliases and open handles remain outside this best-effort boundary.
#[derive(Default)]
pub struct SharedVolumeMounts {
    blocked: bool,
    /// The container's map, applied to sources that hold container ids.
    idmap: Option<IdMap>,
    slots: BTreeMap<(Guid, PathBuf), MountSlot>,
    root: Option<StagingRoot>,
}

impl SharedVolumeMounts {
    /// The caller must establish this root before executing package JavaScript.
    pub(crate) async fn new(rootfs: &Path, idmap: IdMap) -> Result<Self, Error> {
        let backing = TmpDir::new().await?;
        tokio::fs::set_permissions(&*backing, std::fs::Permissions::from_mode(0o755)).await?;
        std::os::unix::fs::chown(&*backing, Some(0), Some(0))?;
        let source = PinnedDirectory::open(&backing, Path::new(""))?;
        let target = rootfs.join(SERVICE_ROOT);
        tokio::fs::create_dir_all(&target).await?;
        let destination = open_beneath(rootfs, Path::new(SERVICE_ROOT))?;
        let namespace = Arc::new(MountNamespace::for_process(std::process::id())?);
        let mount = AttachedMount::bind(&source, &destination, false, namespace.clone(), None)?;
        let directory = PinnedDirectory::open(&target, Path::new(""))?;
        let empty = directory.path().join("unavailable");
        tokio::fs::create_dir(&empty).await?;
        tokio::fs::set_permissions(&empty, std::fs::Permissions::from_mode(0)).await?;
        let empty = open_beneath(directory.path(), Path::new("unavailable"))?;
        let unavailable = AttachedMount::bind(&empty, &empty, true, namespace, None)?;
        Ok(Self {
            blocked: false,
            idmap: Some(idmap),
            root: Some(StagingRoot {
                _unavailable: unavailable,
                directory,
                _mount: mount,
                _backing: backing,
            }),
            slots: BTreeMap::new(),
        })
    }

    pub fn block(&mut self) {
        self.blocked = true;
    }
    pub fn allow(&mut self) {
        self.blocked = false;
    }

    pub fn check_allowed(&self) -> Result<(), Error> {
        if self.blocked {
            return Err(Error::new(
                eyre!(t!("shared-volumes.backup-isolation")),
                ErrorKind::Authorization,
            ));
        }
        Ok(())
    }

    /// The caller holds the management gate and subcontainer lifetime through mounting.
    /// Remounts follow the requested path after parent-directory moves.
    pub(crate) async fn prepare_target(
        &mut self,
        guid: &Guid,
        location: &Path,
        service_root: &Path,
        container_root: &Path,
        namespace: Arc<MountNamespace>,
    ) -> Result<(), Error> {
        self.check_allowed()?;
        let relative = confined_relative(location)?;
        let key = (guid.clone(), relative.clone());
        if let Some(slot) = self.slots.get_mut(&key) {
            if let Some(stage) = slot.stage.take() {
                stage.retire().await?;
            }
        }
        let destination = open_beneath(&service_root.join(container_root), &relative)?;
        if let Some(slot) = self.slots.get(&key) {
            if same_mount(slot.placeholder.mount.as_fd(), destination.as_fd())? {
                return Ok(());
            }
        }
        self.slots.remove(&key);
        let source = open_beneath(service_root, &Path::new(SERVICE_ROOT).join("unavailable"))?;
        let placeholder = AttachedMount::bind_in_namespace(
            source.as_fd(),
            destination.as_fd(),
            true,
            false,
            namespace,
        )?;
        self.slots.insert(
            key,
            MountSlot {
                placeholder,
                stage: None,
            },
        );
        Ok(())
    }

    /// Cross-namespace staging supplies the read-only lock; service-owned targets remain detachable.
    pub(crate) async fn mount_source(
        &mut self,
        guid: &Guid,
        location: &Path,
        id: &SharedVolumeId,
        source: PreparedSource,
        readonly: bool,
        service_root: &Path,
    ) -> Result<(), Error> {
        self.check_allowed()?;
        let root = self
            .root
            .as_ref()
            .or_not_found("shared-volume staging root")?;
        let key = (guid.clone(), confined_relative(location)?);
        let slot = self.slots.get_mut(&key).or_not_found("volume mountpoint")?;
        let relative = Path::new(id.as_ref()).join(Guid::new().to_string());
        let path = root.directory.path().join(&relative);
        tokio::fs::create_dir_all(&path).await?;
        let destination = open_beneath(root.directory.path(), &relative)?;
        let userns = if source.container_ids {
            let idmap = self.idmap.or_not_found("container idmap")?;
            Some(userns_fd_from_idmap(&[idmap], None).await?)
        } else {
            None
        };
        let mount = AttachedMount::bind(
            &source.directory,
            &destination,
            readonly,
            root._mount.namespace.clone(),
            userns.as_ref(),
        )?;
        slot.stage = Some(StagedMount {
            id: id.clone(),
            readonly,
            mount,
            source,
            directory: path,
            path: Path::new("/").join(SERVICE_ROOT).join(relative),
            target: None,
        });
        let stage = slot.stage.as_mut().unwrap();
        let source = open_beneath(service_root, &confined_relative(&stage.path)?)?;
        let namespace = slot.placeholder.namespace.clone();
        stage.target = Some(AttachedMount::bind_in_namespace(
            source.as_fd(),
            slot.placeholder.mount.as_fd(),
            readonly,
            true,
            namespace,
        )?);
        Ok(())
    }

    pub async fn remove_subcontainer(&mut self, guid: &Guid) -> Result<(), Error> {
        let keys = self
            .slots
            .keys()
            .filter(|(container, _)| container == guid)
            .cloned()
            .collect::<Vec<_>>();
        let result = self.retire_stages(keys).await;
        self.slots.retain(|(container, _), _| container != guid);
        result
    }

    pub(crate) async fn reconcile_grant(
        &mut self,
        volume: &SharedVolumeId,
        grant: Option<&SharedVolumeGrant>,
    ) -> Result<(), Error> {
        let keys = self
            .slots
            .iter()
            .filter(|(_, slot)| {
                slot.stage.as_ref().is_some_and(|stage| {
                    stage.id == *volume && !grant.is_some_and(|grant| grant.allows(stage.readonly))
                })
            })
            .map(|(key, _)| key.clone())
            .collect();
        self.retire_stages(keys).await
    }

    async fn retire_stages(&mut self, keys: Vec<(Guid, PathBuf)>) -> Result<(), Error> {
        let mut errors = ErrorCollection::new();
        for key in keys {
            if let Some(stage) = self.slots.get_mut(&key).and_then(|slot| slot.stage.take()) {
                errors.handle(stage.retire().await);
            }
        }
        errors.into_result()
    }

    pub async fn clear(&mut self) -> Result<(), Error> {
        self.blocked = true;
        let result = self
            .retire_stages(self.slots.keys().cloned().collect())
            .await;
        self.slots.clear();
        result
    }
}

pub fn confined_relative(path: &Path) -> Result<PathBuf, Error> {
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(name) => relative.push(name),
            _ => {
                return Err(Error::new(
                    eyre!(t!("shared-volumes.invalid-path")),
                    ErrorKind::InvalidRequest,
                ));
            }
        }
    }
    Ok(relative)
}

#[cfg(test)]
mod tests;
