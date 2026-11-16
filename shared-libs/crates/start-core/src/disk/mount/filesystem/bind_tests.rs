use super::*;
use crate::disk::mount::filesystem::idmapped::IdMapped;
use crate::disk::mount::filesystem::private_tmpfs::PrivateTmpfs;
use crate::disk::mount::filesystem::{ReadOnly, ReadWrite};
use crate::disk::mount::guard::{GenericMountGuard, MountGuard, TmpMountGuard};

#[tokio::test]
async fn recursive_bind_cache_preserves_tree_and_access_policy() {
    let directory = tempfile::tempdir().unwrap();
    let ordinary = Bind::new(directory.path());
    let recursive = Bind::new(directory.path()).recursive(true);
    assert_ne!(
        ordinary.source_hash().await.unwrap(),
        recursive.source_hash().await.unwrap()
    );
    assert_eq!(
        ordinary.mount_hash(ReadOnly).await.unwrap(),
        ordinary.mount_hash(ReadWrite).await.unwrap()
    );
    assert_ne!(
        recursive.mount_hash(ReadOnly).await.unwrap(),
        recursive.mount_hash(ReadWrite).await.unwrap()
    );
    let idmapped = IdMapped::new(recursive, vec![]);
    assert_ne!(
        idmapped.mount_hash(ReadOnly).await.unwrap(),
        idmapped.mount_hash(ReadWrite).await.unwrap()
    );
}

#[tokio::test]
async fn target_preparation_replaces_the_previous_leaf_kind() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("source-file");
    let folder = directory.path().join("source-directory");
    let target = directory.path().join("target");
    std::fs::write(&file, b"source").unwrap();
    std::fs::create_dir(&folder).unwrap();
    std::fs::create_dir(&target).unwrap();
    Bind::new(&file)
        .with_type(FileType::File)
        .pre_mount(&target, ReadOnly)
        .await
        .unwrap();
    assert!(target.is_file());
    Bind::new(&folder)
        .pre_mount(&target, ReadOnly)
        .await
        .unwrap();
    assert!(target.is_dir());
    Bind::new(&file)
        .with_type(FileType::Infer)
        .pre_mount(&target, ReadOnly)
        .await
        .unwrap();
    assert!(target.is_file());
    assert_eq!(std::fs::read(&file).unwrap(), b"source");
}

#[tokio::test]
#[ignore = "requires exclusive managed guest with native mount/namespace privileges"]
async fn recursive_readonly_bind_preserves_source_and_retires_nested_mounts() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    let nested = source.join("main with space");
    let target = directory.path().join("view with space");
    std::fs::create_dir_all(&source).unwrap();
    let nested_mount = MountGuard::mount(&PrivateTmpfs::new(), &nested, ReadWrite)
        .await
        .unwrap()
        .with_strict_unmount();
    std::fs::write(nested.join("marker"), b"original").unwrap();
    let filesystem = IdMapped::new(Bind::new(&source).recursive(true), vec![]);
    assert!(filesystem.is_mount_tree());
    let guard = MountGuard::mount(&filesystem, &target, ReadOnly)
        .await
        .unwrap()
        .with_strict_unmount();
    for path in [target.join("top"), target.join("main with space/marker")] {
        assert_eq!(
            std::fs::write(path, b"changed").unwrap_err().raw_os_error(),
            Some(libc::EROFS)
        );
    }
    std::fs::write(nested.join("marker"), b"source remains writable").unwrap();
    assert_eq!(
        std::fs::read(target.join("main with space/marker")).unwrap(),
        b"source remains writable"
    );
    guard.unmount(false).await.unwrap();
    for path in [&target, &target.join("main with space")] {
        assert!(!crate::disk::mount::util::is_mountpoint(path).await.unwrap());
    }
    assert!(
        crate::disk::mount::util::is_mountpoint(&nested)
            .await
            .unwrap()
    );
    let guard = MountGuard::mount(&filesystem, &target, ReadWrite)
        .await
        .unwrap()
        .with_strict_unmount();
    std::fs::write(target.join("main with space/marker"), b"writer").unwrap();
    guard.unmount(false).await.unwrap();
    assert_eq!(std::fs::read(nested.join("marker")).unwrap(), b"writer");
    let reader = TmpMountGuard::mount(&filesystem, ReadOnly).await.unwrap();
    let writer = TmpMountGuard::mount(&filesystem, ReadWrite).await.unwrap();
    assert_ne!(reader.path(), writer.path());
    assert_eq!(
        std::fs::write(reader.path().join("main with space/marker"), b"reader")
            .unwrap_err()
            .raw_os_error(),
        Some(libc::EROFS)
    );
    std::fs::write(
        writer.path().join("main with space/marker"),
        b"cached writer",
    )
    .unwrap();
    reader.unmount().await.unwrap();
    writer.unmount().await.unwrap();
    assert_eq!(
        std::fs::read(nested.join("marker")).unwrap(),
        b"cached writer"
    );
    nested_mount.unmount(false).await.unwrap();
}
