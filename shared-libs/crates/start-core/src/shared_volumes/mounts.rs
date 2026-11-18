use std::collections::BTreeMap;
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::disk::mount::filesystem::syscall::{
    DetachedMount, MountNamespace, open_beneath, open_tree_clone_fd, unmount_fd,
};
use crate::disk::mount::guard::{GenericMountGuard, TmpMountGuard};
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::shared_volumes::SharedVolumeId;
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
    ) -> Result<Self, Error> {
        let detached = DetachedMount::from_fd(open_tree_clone_fd(source.as_fd())?);
        detached.set_private()?;
        detached.set_readonly(readonly)?;
        let mount = namespace.run(|| detached.attach_to_fd(destination.as_fd()))?;
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
}

impl PreparedSource {
    pub(crate) fn mount(&self) -> &TmpMountGuard {
        &self.guard
    }

    pub async fn open(guard: TmpMountGuard, subpath: Option<PathBuf>) -> Result<Self, Error> {
        tokio::task::spawn_blocking(move || {
            let directory =
                PinnedDirectory::open(guard.path(), subpath.as_deref().unwrap_or(Path::new("")))?;
            Ok(Self { directory, guard })
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

struct StagedMount {
    mount: AttachedMount,
    source: PreparedSource,
    directory: PathBuf,
}

impl StagedMount {
    async fn retire(self) -> Result<(), Error> {
        self.mount.detach()?;
        drop(self.mount);
        self.source.unmount().await?;
        tokio::fs::remove_dir(self.directory).await?;
        Ok(())
    }
}

/// Host-owned stages propagate into the service user namespace before SDK binds.
/// Grant revocation belongs to the whole LXC payload, not these source mounts.
#[derive(Default)]
pub struct SharedVolumeMounts {
    blocked: bool,
    slots: BTreeMap<(Guid, PathBuf), StagedMount>,
    root: Option<StagingRoot>,
}

impl SharedVolumeMounts {
    /// The caller must establish this root before executing package JavaScript.
    pub(crate) async fn new(rootfs: &Path) -> Result<Self, Error> {
        let backing = TmpDir::new().await?;
        tokio::fs::set_permissions(&*backing, std::fs::Permissions::from_mode(0o755)).await?;
        std::os::unix::fs::chown(&*backing, Some(0), Some(0))?;
        let source = PinnedDirectory::open(&backing, Path::new(""))?;
        let target = rootfs.join(SERVICE_ROOT);
        tokio::fs::create_dir_all(&target).await?;
        let destination = open_beneath(rootfs, Path::new(SERVICE_ROOT))?;
        let namespace = Arc::new(MountNamespace::for_process(std::process::id())?);
        let mount = AttachedMount::bind(&source, &destination, false, namespace.clone())?;
        let directory = PinnedDirectory::open(&target, Path::new(""))?;
        let empty = directory.path().join("unavailable");
        tokio::fs::create_dir(&empty).await?;
        tokio::fs::set_permissions(&empty, std::fs::Permissions::from_mode(0)).await?;
        let empty = open_beneath(directory.path(), Path::new("unavailable"))?;
        let unavailable = AttachedMount::bind(&empty, &empty, true, namespace)?;
        Ok(Self {
            blocked: false,
            root: Some(StagingRoot {
                _unavailable: unavailable,
                directory,
                _mount: mount,
                _backing: backing,
            }),
            slots: BTreeMap::new(),
        })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.slots.is_empty()
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

    pub(crate) fn unavailable_path(&self) -> PathBuf {
        Path::new("/").join(SERVICE_ROOT).join("unavailable")
    }

    pub(crate) async fn stage(
        &mut self,
        guid: &Guid,
        location: PathBuf,
        id: &SharedVolumeId,
        source: PreparedSource,
        readonly: bool,
    ) -> Result<PathBuf, Error> {
        self.check_allowed()?;
        let root = self
            .root
            .as_ref()
            .or_not_found("shared-volume staging root")?;
        let relative = Path::new(id.as_ref()).join(Guid::new().to_string());
        let path = root.directory.path().join(&relative);
        tokio::fs::create_dir_all(&path).await?;
        let destination = open_beneath(root.directory.path(), &relative)?;
        let mount = AttachedMount::bind(
            &source.directory,
            &destination,
            readonly,
            root._mount.namespace.clone(),
        )?;
        let staged = StagedMount {
            mount,
            source,
            directory: path,
        };
        if let Some(previous) = self
            .slots
            .insert((guid.clone(), confined_relative(&location)?), staged)
        {
            previous.retire().await?;
        }
        Ok(Path::new("/").join(SERVICE_ROOT).join(relative))
    }

    pub async fn remove_subcontainer(&mut self, guid: &Guid) -> Result<(), Error> {
        let keys = self
            .slots
            .keys()
            .filter(|(container, _)| container == guid)
            .cloned()
            .collect::<Vec<_>>();
        let mut errors = ErrorCollection::new();
        for key in keys {
            errors.handle(self.slots.remove(&key).unwrap().retire().await);
        }
        errors.into_result()
    }

    pub async fn clear(&mut self) -> Result<(), Error> {
        self.blocked = true;
        let mut errors = ErrorCollection::new();
        for (_, stage) in std::mem::take(&mut self.slots) {
            errors.handle(stage.retire().await);
        }
        errors.into_result()
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
