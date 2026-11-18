use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::Arc;

use super::LocalSubvolume;
use crate::disk::mount::filesystem::ReadOnly;
use crate::disk::mount::guard::{GenericMountGuard, TmpMountGuard};
use crate::lxc::{BASE_IDMAP, unshifted_filesystem};
use crate::prelude::*;
use crate::util::btrfs;
use crate::util::io::Counter;

/// Converts legacy shifted owners before services or observers can mount the root.
/// The caller must exclude mounts and writers throughout recovery and replacement.
pub(crate) async fn migrate_local_ownership(path: &Path) -> Result<(), Error> {
    btrfs::recover_conversion(path).await?;
    let metadata = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let shifted = BASE_IDMAP.to_id..BASE_IDMAP.to_id + BASE_IDMAP.range;
    if !metadata.is_dir()
        || !(shifted.contains(&metadata.uid()) || shifted.contains(&metadata.gid()))
    {
        return Ok(());
    }

    let source = TmpMountGuard::mount(
        &unshifted_filesystem(LocalSubvolume(path.to_owned())),
        ReadOnly,
    )
    .await?;
    let prepared = btrfs::prepare_conversion(
        path,
        source.path(),
        Arc::new(Counter::new(0, std::sync::atomic::Ordering::Relaxed)),
    )
    .await;
    let unmounted = source.unmount().await;
    prepared?;
    unmounted?;
    btrfs::commit_conversion(path).await
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

    use super::*;
    use crate::disk::mount::filesystem::ReadWrite;
    use crate::shared_volumes::source::SourceFilesystem;

    #[tokio::test]
    async fn current_and_missing_local_roots_need_no_conversion() {
        let fixture = tempfile::tempdir().unwrap();
        let path = fixture.path().join("volume");
        migrate_local_ownership(&path).await.unwrap();
        assert!(!path.exists());
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("sentinel"), b"current").unwrap();
        let inode = std::fs::metadata(&path).unwrap().ino();
        migrate_local_ownership(&path).await.unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().ino(), inode);
        assert_eq!(std::fs::read(path.join("sentinel")).unwrap(), b"current");
    }

    fn acl(user: u32) -> Vec<u8> {
        let mut acl = 2u32.to_le_bytes().to_vec();
        for (tag, permission, id) in [
            (0x01u16, 7u16, u32::MAX),
            (0x02, 4, user),
            (0x04, 5, u32::MAX),
            (0x10, 5, u32::MAX),
            (0x20, 0, u32::MAX),
        ] {
            acl.extend_from_slice(&tag.to_le_bytes());
            acl.extend_from_slice(&permission.to_le_bytes());
            acl.extend_from_slice(&id.to_le_bytes());
        }
        acl
    }

    async fn legacy(path: &Path) {
        btrfs::create_subvolume(path).await.unwrap();
        std::fs::write(path.join("sentinel"), b"existing data").unwrap();
        std::os::unix::fs::chown(path, Some(100000), Some(100000)).unwrap();
        std::os::unix::fs::chown(path.join("sentinel"), Some(101000), Some(102000)).unwrap();
        std::fs::set_permissions(
            path.join("sentinel"),
            std::fs::Permissions::from_mode(0o6750),
        )
        .unwrap();
        std::fs::hard_link(path.join("sentinel"), path.join("hardlink")).unwrap();
        symlink("sentinel", path.join("symlink")).unwrap();
        std::os::unix::fs::lchown(path.join("symlink"), Some(100123), Some(100456)).unwrap();
        xattr::set(path.join("sentinel"), "user.migration", b"retained").unwrap();
        xattr::set(
            path.join("sentinel"),
            "system.posix_acl_access",
            &acl(100123),
        )
        .unwrap();
        xattr::set(path, "system.posix_acl_default", &acl(100123)).unwrap();
    }

    #[tokio::test]
    #[ignore = "requires root, BTRFS_TEST_VOLUMES_DIR and a private Linux mount namespace"]
    async fn legacy_local_ownership_is_replaced_once_without_losing_metadata() {
        let fixture = btrfs::tests::directory();
        let path = fixture.path().join("volume");
        legacy(&path).await;
        migrate_local_ownership(&path).await.unwrap();
        let root = std::fs::metadata(&path).unwrap();
        assert_eq!((root.uid(), root.gid()), (0, 0));
        let file = std::fs::metadata(path.join("sentinel")).unwrap();
        assert_eq!(
            (file.uid(), file.gid(), file.mode() & 0o7777),
            (1000, 2000, 0o6750)
        );
        assert_eq!(
            std::fs::read(path.join("sentinel")).unwrap(),
            b"existing data"
        );
        assert_eq!(
            file.ino(),
            std::fs::metadata(path.join("hardlink")).unwrap().ino()
        );
        assert_eq!(
            std::fs::read_link(path.join("symlink")).unwrap(),
            Path::new("sentinel")
        );
        let link = std::fs::symlink_metadata(path.join("symlink")).unwrap();
        assert_eq!((link.uid(), link.gid()), (123, 456));
        assert_eq!(
            xattr::get(path.join("sentinel"), "user.migration")
                .unwrap()
                .unwrap(),
            b"retained"
        );
        assert_eq!(
            xattr::get(path.join("sentinel"), "system.posix_acl_access").unwrap(),
            Some(acl(123))
        );
        assert_eq!(
            xattr::get(&path, "system.posix_acl_default").unwrap(),
            Some(acl(123))
        );
        let view = TmpMountGuard::mount(&SourceFilesystem::local(path.clone()), ReadWrite)
            .await
            .unwrap();
        let visible = std::fs::metadata(view.path().join("sentinel")).unwrap();
        assert_eq!((visible.uid(), visible.gid()), (101000, 102000));
        view.unmount().await.unwrap();
        let retired = fixture.path().join("volume.convert-old");
        legacy(&retired).await;
        migrate_local_ownership(&path).await.unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().ino(), root.ino());
        assert!(!retired.exists());
    }

    #[tokio::test]
    #[ignore = "requires root, BTRFS_TEST_VOLUMES_DIR and a private Linux mount namespace"]
    async fn interrupted_ownership_conversion_recovers_before_mounting() {
        for moved in [false, true] {
            let fixture = btrfs::tests::directory();
            let path = fixture.path().join("volume");
            legacy(&path).await;
            let temporary = fixture.path().join("volume.convert-tmp");
            btrfs::create_subvolume(&temporary).await.unwrap();
            std::fs::write(temporary.join("incomplete"), b"not published").unwrap();
            if moved {
                std::fs::rename(&path, fixture.path().join("volume.convert-old")).unwrap();
            }
            migrate_local_ownership(&path).await.unwrap();
            assert_eq!(
                std::fs::read(path.join("sentinel")).unwrap(),
                b"existing data"
            );
            assert_eq!(
                std::fs::metadata(path.join("sentinel")).unwrap().uid(),
                1000
            );
            assert!(!path.join("incomplete").exists());
            assert!(!temporary.exists());
            assert!(!fixture.path().join("volume.convert-old").exists());
        }
    }
}
