use std::collections::BTreeMap;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::disk::mount::filesystem::syscall::{
    DetachedMount, MountNamespace, open_beneath, open_tree_clone_fd, unmount_fd,
};
use crate::disk::mount::guard::TmpMountGuard;
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::shared_volumes::{SharedVolumeGrant, SharedVolumeId};

struct AttachedMount {
    mount: OwnedFd,
    namespace: Arc<MountNamespace>,
}

impl AttachedMount {
    fn bind(
        source: &OwnedFd,
        destination: &OwnedFd,
        readonly: bool,
        namespace: Arc<MountNamespace>,
    ) -> Result<Self, Error> {
        let detached = DetachedMount::from_fd(open_tree_clone_fd(source.as_fd())?);
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

pub struct PreparedSource {
    directory: OwnedFd,
    guard: TmpMountGuard,
}

impl PreparedSource {
    pub async fn open(guard: TmpMountGuard, subpath: Option<PathBuf>) -> Result<Self, Error> {
        use crate::disk::mount::guard::GenericMountGuard;
        tokio::task::spawn_blocking(move || {
            let relative = confined_relative(subpath.as_deref().unwrap_or(Path::new("")))?;
            let directory = open_beneath(guard.path(), &relative)?;
            Ok(Self { directory, guard })
        })
        .await
        .with_kind(ErrorKind::Unknown)?
    }
}

pub(crate) struct PreparedTarget {
    location: PathBuf,
    directory: OwnedFd,
    namespace: Arc<MountNamespace>,
}

impl PreparedTarget {
    pub(crate) async fn open(
        root: PathBuf,
        location: PathBuf,
        namespace: Arc<MountNamespace>,
    ) -> Result<Self, Error> {
        tokio::task::spawn_blocking(move || {
            let location = confined_relative(&location)?;
            let directory = open_beneath(&root, &location)?;
            Ok(Self {
                location,
                directory,
                namespace,
            })
        })
        .await
        .with_kind(ErrorKind::Unknown)?
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

    pub(crate) async fn prepare(
        &mut self,
        guid: &Guid,
        target: PreparedTarget,
    ) -> Result<PathBuf, Error> {
        self.check_allowed()?;
        let PreparedTarget {
            location,
            directory,
            namespace,
        } = target;
        let key = (guid.clone(), location.clone());
        if let Some(slot) = self.slots.get_mut(&key) {
            if let Some(attachment) = &slot.attachment {
                attachment.mount.detach()?;
            }
            slot.attachment = None;
        } else {
            let empty = Path::new(crate::DATA_DIR).join("volume-unavailable");
            tokio::fs::create_dir_all(&empty).await?;
            tokio::fs::set_permissions(&empty, std::fs::Permissions::from_mode(0)).await?;
            let source = open_beneath(&empty, Path::new(""))?;
            let placeholder = AttachedMount::bind(&source, &directory, true, namespace)?;
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
        source: PreparedSource,
        readonly: bool,
    ) -> Result<(), Error> {
        self.check_allowed()?;
        let slot = self
            .slots
            .get_mut(&(guid.clone(), location))
            .or_not_found("volume mountpoint")?;
        let mount = AttachedMount::bind(
            &source.directory,
            &slot.placeholder.mount,
            readonly,
            slot.placeholder.namespace.clone(),
        )?;
        slot.attachment = Some(VolumeAttachment {
            id,
            readonly,
            mount,
            _source: source.guard,
        });
        Ok(())
    }

    pub fn reconcile_grant(
        &mut self,
        id: &SharedVolumeId,
        grant: Option<&SharedVolumeGrant>,
    ) -> Result<(), Error> {
        let mut errors = ErrorCollection::new();
        for slot in self.slots.values_mut() {
            if let Some(attachment) = slot.attachment.as_ref().filter(|attachment| {
                &attachment.id == id
                    && !grant.is_some_and(|grant| attachment.readonly || !grant.readonly)
            }) {
                errors.handle(attachment.mount.detach());
                slot.attachment = None;
            }
        }
        errors.into_result()
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
    #[tokio::test]
    async fn prepared_targets_keep_the_pinned_directory_when_renamed() {
        use std::os::unix::fs::MetadataExt;

        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("data");
        std::fs::create_dir(&directory).unwrap();
        let inode = std::fs::metadata(&directory).unwrap().ino();
        let namespace = Arc::new(MountNamespace::for_process(std::process::id()).unwrap());
        let target = PreparedTarget::open(root.path().to_owned(), "/data".into(), namespace)
            .await
            .unwrap();
        std::fs::rename(&directory, root.path().join("moved")).unwrap();
        std::fs::create_dir(&directory).unwrap();
        assert_eq!(target.location, Path::new("data"));
        assert_eq!(
            std::fs::File::from(target.directory)
                .metadata()
                .unwrap()
                .ino(),
            inode
        );
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

    #[tokio::test]
    #[ignore = "requires root in a private Linux mount namespace"]
    async fn real_mounts_enforce_readonly_and_propagate_revocation_without_fallback() {
        use std::io::{BufRead, Write};
        use std::process::Stdio;

        use crate::disk::mount::filesystem::ReadWrite;
        use crate::disk::mount::filesystem::bind::Bind;
        use crate::disk::mount::guard::{GenericMountGuard, MountGuard};
        use crate::util::Invoke;

        let fixture = tempfile::tempdir().unwrap();
        let source_path = fixture.path().join("source");
        let first_root = fixture.path().join("first");
        let second_root = fixture.path().join("second");
        for path in [&source_path, &first_root, &second_root] {
            std::fs::create_dir(path).unwrap();
        }
        for root in [&first_root, &second_root] {
            std::fs::create_dir(root.join("data")).unwrap();
        }
        std::fs::write(source_path.join("sentinel"), b"shared").unwrap();
        let first_tree = MountGuard::mount(&Bind::new(&first_root), &first_root, ReadWrite)
            .await
            .unwrap();
        let second_tree = MountGuard::mount(&Bind::new(&second_root), &second_root, ReadWrite)
            .await
            .unwrap();
        tokio::process::Command::new("mount")
            .arg("--make-rshared")
            .arg(&first_root)
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
        let source = TmpMountGuard::mount(&Bind::new(&source_path), ReadWrite)
            .await
            .unwrap();
        let id = SharedVolumeId::new();
        let first = Guid::new();
        let second = Guid::new();
        let mut mounts = SharedVolumeMounts::default();
        let mut other_service = SharedVolumeMounts::default();
        let namespace = Arc::new(MountNamespace::for_process(std::process::id()).unwrap());
        let target = PreparedTarget::open(first_root.clone(), "/data".into(), namespace.clone())
            .await
            .unwrap();
        let pending_target =
            PreparedTarget::open(first_root.clone(), "/data".into(), namespace.clone())
                .await
                .unwrap();
        for target in [target, pending_target] {
            let location = mounts.prepare(&first, target).await.unwrap();
            mounts
                .attach(
                    &first,
                    location,
                    id.clone(),
                    PreparedSource::open(source.clone(), None).await.unwrap(),
                    false,
                )
                .unwrap();
        }
        let target = PreparedTarget::open(second_root.clone(), "/data".into(), namespace)
            .await
            .unwrap();
        let location = other_service.prepare(&second, target).await.unwrap();
        other_service
            .attach(
                &second,
                location,
                id.clone(),
                PreparedSource::open(source.clone(), None).await.unwrap(),
                true,
            )
            .unwrap();
        assert_eq!(
            std::fs::read(first_root.join("data/sentinel")).unwrap(),
            b"shared"
        );
        assert!(std::fs::write(second_root.join("data/forbidden"), b"no").is_err());
        tokio::process::Command::new("unshare")
            .args(["--user", "--map-root-user", "--mount", "sh", "-c"])
            .arg("test -f \"$1/sentinel\" || exit 1; if mount -o remount,bind,rw \"$1\" 2>/dev/null; then exit 1; fi; ! touch \"$1/escalated\" 2>/dev/null")
            .arg("sh").arg(second_root.join("data"))
            .invoke(ErrorKind::Filesystem).await.unwrap();
        let mut peer = std::process::Command::new("unshare")
            .args(["--mount", "--propagation", "slave", "sh", "-c"])
            .arg("test -f \"$1/sentinel\" || exit 1; echo ready; read -r signal; test ! -e \"$1/sentinel\" && ! touch \"$1/fallback\" 2>/dev/null")
            .arg("sh").arg(first_root.join("data"))
            .stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        let mut ready = String::new();
        std::io::BufReader::new(peer.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        assert_eq!(ready, "ready\n");
        mounts.reconcile_grant(&id, None).unwrap();
        peer.stdin.take().unwrap().write_all(b"revoked\n").unwrap();
        assert!(peer.wait().unwrap().success());
        assert!(!first_root.join("data/sentinel").exists());
        assert!(std::fs::write(first_root.join("data/fallback"), b"no").is_err());
        assert_eq!(
            std::fs::read(second_root.join("data/sentinel")).unwrap(),
            b"shared"
        );
        mounts.clear();
        other_service.clear();
        source.unmount().await.unwrap();
        first_tree.unmount(false).await.unwrap();
        second_tree.unmount(false).await.unwrap();
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires root in a private Linux mount namespace"]
    async fn nested_mounts_use_the_service_namespace_and_outlive_its_process() {
        use std::io::{BufRead, Write};
        use std::process::Stdio;

        use crate::disk::mount::filesystem::ReadWrite;
        use crate::disk::mount::filesystem::bind::Bind;
        use crate::disk::mount::guard::GenericMountGuard;

        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("root");
        let own = fixture.path().join("own");
        let source_path = fixture.path().join("source");
        for path in [root.join("data"), own.clone(), source_path.clone()] {
            std::fs::create_dir_all(path).unwrap();
        }
        std::fs::write(source_path.join("sentinel"), b"shared").unwrap();
        let mut peer = std::process::Command::new("unshare")
            .args(["--mount", "--pid", "--fork", "--mount-proc", "--kill-child", "--propagation", "private", "sh", "-c"])
            .arg("mount --bind \"$1\" \"$2/data\" && mkdir \"$2/data/attached\" || exit 1; echo ready; read -r signal || exit 1; test \"$(cat \"$2/data/attached/sentinel\")\" = shared || exit 1; exec 3<\"$2/data/attached/sentinel\"; echo mounted; read -r signal || exit 1; test ! -e \"$2/data/attached/sentinel\" && ! touch \"$2/data/attached/fallback\" 2>/dev/null && test \"$(cat <&3)\" = shared")
            .arg("sh").arg(&own).arg(&root)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        let mut output = std::io::BufReader::new(peer.stdout.take().unwrap());
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        assert_eq!(line, "ready\n");
        assert!(!root.join("data/attached").exists());
        let namespace = Arc::new(MountNamespace::for_process(peer.id()).unwrap());
        let service_root = PathBuf::from(format!("/proc/{}/root", peer.id()))
            .join(root.strip_prefix("/").unwrap());
        let source = TmpMountGuard::mount(&Bind::new(&source_path), ReadWrite)
            .await
            .unwrap();
        let original_namespace = std::fs::read_link("/proc/thread-self/ns/mnt").unwrap();
        let id = SharedVolumeId::new();
        let guid = Guid::new();
        let mut mounts = SharedVolumeMounts::default();
        let target = PreparedTarget::open(service_root, "data/attached".into(), namespace)
            .await
            .unwrap();
        let location = mounts.prepare(&guid, target).await.unwrap();
        mounts
            .attach(
                &guid,
                location,
                id.clone(),
                PreparedSource::open(source.clone(), None).await.unwrap(),
                false,
            )
            .unwrap();
        peer.stdin
            .as_mut()
            .unwrap()
            .write_all(b"attached\n")
            .unwrap();
        line.clear();
        output.read_line(&mut line).unwrap();
        assert_eq!(line, "mounted\n");
        mounts.reconcile_grant(&id, None).unwrap();
        peer.stdin.take().unwrap().write_all(b"revoked\n").unwrap();
        assert!(peer.wait().unwrap().success());
        mounts.clear();
        assert_eq!(
            std::fs::read_link("/proc/thread-self/ns/mnt").unwrap(),
            original_namespace
        );
        assert_eq!(
            std::fs::read(source_path.join("sentinel")).unwrap(),
            b"shared"
        );
        source.unmount().await.unwrap();
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
