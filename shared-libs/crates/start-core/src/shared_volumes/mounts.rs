use std::collections::BTreeMap;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

use crate::disk::mount::filesystem::syscall::{
    DetachedMount, open_beneath, open_tree_clone_fd, unmount_fd,
};
use crate::disk::mount::guard::TmpMountGuard;
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::shared_volumes::{SharedVolumeGrant, SharedVolumeId};

struct AttachedMount(OwnedFd);

impl AttachedMount {
    fn bind(source: &OwnedFd, destination: &OwnedFd, readonly: bool) -> Result<Self, Error> {
        let detached = DetachedMount::from_fd(open_tree_clone_fd(source.as_fd())?);
        detached.set_readonly(readonly)?;
        Ok(Self(detached.attach_to_fd(destination.as_fd())?))
    }

    fn detach(&self) -> Result<(), Error> {
        unmount_fd(self.0.as_fd())
    }
}

impl Drop for AttachedMount {
    fn drop(&mut self) {
        unmount_fd(self.0.as_fd()).log_err();
    }
}

struct VolumeAttachment {
    id: SharedVolumeId,
    readonly: bool,
    mount: AttachedMount,
    _source: TmpMountGuard,
}

struct AttachmentSlot {
    attachment: Option<VolumeAttachment>,
    placeholder: AttachedMount,
}

#[derive(Default)]
pub struct SharedVolumeMounts {
    blocked: bool,
    slots: BTreeMap<(Guid, PathBuf), AttachmentSlot>,
}

impl SharedVolumeMounts {
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

    pub async fn prepare(
        &mut self,
        guid: &Guid,
        root: &Path,
        location: &Path,
    ) -> Result<PathBuf, Error> {
        self.check_allowed()?;
        let location = confined_relative(location)?;
        let key = (guid.clone(), location.clone());
        if let Some(slot) = self.slots.get_mut(&key) {
            if let Some(attachment) = &slot.attachment {
                attachment.mount.detach()?;
            }
            slot.attachment = None;
        } else {
            let destination = open_beneath(root, &location)?;
            let empty = Path::new(crate::DATA_DIR).join("volume-unavailable");
            tokio::fs::create_dir_all(&empty).await?;
            tokio::fs::set_permissions(&empty, std::fs::Permissions::from_mode(0)).await?;
            let source = open_beneath(&empty, Path::new(""))?;
            let placeholder = AttachedMount::bind(&source, &destination, true)?;
            self.slots.insert(
                key,
                AttachmentSlot {
                    attachment: None,
                    placeholder,
                },
            );
        }
        Ok(location)
    }

    pub fn attach(
        &mut self,
        guid: &Guid,
        location: PathBuf,
        id: SharedVolumeId,
        source: TmpMountGuard,
        subpath: Option<&Path>,
        readonly: bool,
    ) -> Result<(), Error> {
        use crate::disk::mount::guard::GenericMountGuard;
        self.check_allowed()?;
        let source_path = confined_relative(subpath.unwrap_or(Path::new("")))?;
        let directory = open_beneath(source.path(), &source_path)?;
        let slot = self
            .slots
            .get_mut(&(guid.clone(), location))
            .or_not_found("volume mountpoint")?;
        let mount = AttachedMount::bind(&directory, &slot.placeholder.0, readonly)?;
        slot.attachment = Some(VolumeAttachment {
            id,
            readonly,
            mount,
            _source: source,
        });
        Ok(())
    }

    pub fn reconcile_grant(
        &mut self,
        id: &SharedVolumeId,
        grant: Option<&SharedVolumeGrant>,
    ) -> Result<(), Error> {
        for slot in self.slots.values_mut() {
            if slot.attachment.as_ref().is_some_and(|attachment| {
                &attachment.id == id
                    && !grant.is_some_and(|grant| attachment.readonly || !grant.readonly)
            }) {
                if let Some(attachment) = &slot.attachment {
                    attachment.mount.detach()?;
                }
                slot.attachment = None;
            }
        }
        Ok(())
    }

    pub fn remove_subcontainer(&mut self, guid: &Guid) {
        self.slots.retain(|(container, _), _| container != guid);
    }

    pub fn clear(&mut self) {
        self.blocked = true;
        self.slots.clear();
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
mod tests {
    use super::*;

    #[test]
    fn volume_paths_reject_traversal_and_normalize_container_roots() {
        assert_eq!(
            confined_relative(Path::new("/data/photos")).unwrap(),
            Path::new("data/photos")
        );
        assert_eq!(confined_relative(Path::new("/")).unwrap(), Path::new(""));
        for path in [
            "..",
            "../elsewhere",
            "/data/../elsewhere",
            "data/../../elsewhere",
        ] {
            assert!(confined_relative(Path::new(path)).is_err());
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn pinned_paths_reject_symlink_and_absolute_escapes() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("inside")).unwrap();
        symlink(outside.path(), root.path().join("escape")).unwrap();
        symlink("inside", root.path().join("safe")).unwrap();
        assert!(open_beneath(root.path(), Path::new("inside")).is_ok());
        assert!(open_beneath(root.path(), Path::new("safe")).is_ok());
        for path in ["escape", "../", "/etc", "missing"] {
            assert!(matches!(
                open_beneath(root.path(), Path::new(path)).unwrap_err().kind,
                ErrorKind::InvalidRequest
            ));
        }
    }

    #[test]
    fn backup_restore_scope_blocks_mounts_until_explicitly_reenabled() {
        let mut mounts = SharedVolumeMounts::default();
        assert!(mounts.check_allowed().is_ok());
        mounts.block();
        assert!(matches!(
            mounts.check_allowed().unwrap_err().kind,
            ErrorKind::Authorization
        ));
        mounts.allow();
        assert!(mounts.check_allowed().is_ok());
        mounts.clear();
        assert!(mounts.check_allowed().is_err());
    }
}
