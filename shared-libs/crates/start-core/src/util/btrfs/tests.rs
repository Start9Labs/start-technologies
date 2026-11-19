use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::*;
use crate::util::io::Counter;

pub(crate) fn directory() -> tempfile::TempDir {
    tempfile::tempdir_in(
        std::env::var_os("BTRFS_TEST_VOLUMES_DIR")
            .expect("BTRFS_TEST_VOLUMES_DIR must name a writable Btrfs fixture"),
    )
    .unwrap()
}

async fn set_readonly(path: &Path, readonly: bool) {
    Command::new("btrfs")
        .args(["property", "set", "-ts"])
        .arg(path)
        .args(["ro", if readonly { "true" } else { "false" }])
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires BTRFS_TEST_VOLUMES_DIR and Btrfs subvolume permissions"]
async fn failed_subvolume_deletion_never_falls_back_to_walking_files() {
    let fixture = directory();
    let parent = fixture.path().join("parent");
    create_subvolume(&parent).await.unwrap();
    let root = parent.join("volume");
    create_subvolume(&root).await.unwrap();
    tokio::fs::write(root.join("sentinel"), b"preserved on failure")
        .await
        .unwrap();
    set_readonly(&parent, true).await;
    assert!(delete_tree(&root).await.is_err());
    assert_eq!(
        tokio::fs::read(root.join("sentinel")).await.unwrap(),
        b"preserved on failure"
    );
    set_readonly(&parent, false).await;
    delete_volume_root(&parent).await.unwrap();
    assert!(!parent.exists());
}

#[tokio::test]
#[ignore = "requires BTRFS_TEST_VOLUMES_DIR and Btrfs subvolume permissions"]
async fn deletion_removes_readonly_nested_subvolumes_without_walking_their_files() {
    let fixture = directory();
    let root = fixture.path().join("volume");
    create_subvolume(&root).await.unwrap();
    let nested = root.join("nested");
    create_subvolume(&nested).await.unwrap();
    tokio::fs::write(nested.join("sentinel"), b"readonly contents")
        .await
        .unwrap();
    set_readonly(&nested, true).await;
    delete_volume_root(&root).await.unwrap();
    assert!(!root.exists());
    delete_volume_root(&root).await.unwrap();
}

#[tokio::test]
#[ignore = "requires BTRFS_TEST_VOLUMES_DIR and Btrfs subvolume permissions"]
async fn conversion_preserves_contents_links_and_root_metadata() {
    let fixture = directory();
    let root = fixture.path().join("volume");
    tokio::fs::create_dir(&root).await.unwrap();
    tokio::fs::write(root.join("data"), b"existing local contents")
        .await
        .unwrap();
    tokio::fs::hard_link(root.join("data"), root.join("hardlink"))
        .await
        .unwrap();
    std::os::unix::fs::symlink("data", root.join("symlink")).unwrap();
    tokio::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o751))
        .await
        .unwrap();
    let original = std::fs::metadata(&root).unwrap();
    convert_directory(&root, Arc::new(Counter::new(0, Ordering::Relaxed)))
        .await
        .unwrap();
    assert!(is_subvolume(&root).await);
    assert_eq!(
        tokio::fs::read(root.join("data")).await.unwrap(),
        b"existing local contents"
    );
    assert_eq!(
        tokio::fs::read_link(root.join("symlink")).await.unwrap(),
        Path::new("data")
    );
    assert_eq!(
        std::fs::metadata(root.join("data")).unwrap().ino(),
        std::fs::metadata(root.join("hardlink")).unwrap().ino()
    );
    let converted = std::fs::metadata(&root).unwrap();
    assert_eq!(converted.permissions().mode() & 0o777, 0o751);
    assert_eq!(
        (converted.uid(), converted.gid()),
        (original.uid(), original.gid())
    );
    let (temporary, old) = conversion_paths(&root);
    assert!(!temporary.exists());
    assert!(!old.exists());
    delete_volume_root(&root).await.unwrap();
    assert!(!root.exists());
}

#[tokio::test]
#[ignore = "requires BTRFS_TEST_VOLUMES_DIR and Btrfs subvolume permissions"]
async fn interrupted_conversion_recovers_before_during_and_after_the_swap() {
    let fixture = directory();
    for stage in 0..3 {
        let root = fixture.path().join(format!("volume-{stage}"));
        let (temporary, old) = conversion_paths(&root);
        let original = if stage == 0 { &root } else { &old };
        tokio::fs::create_dir(original).await.unwrap();
        tokio::fs::write(original.join("sentinel"), b"original contents")
            .await
            .unwrap();
        let copied = if stage == 2 { &root } else { &temporary };
        create_subvolume(copied).await.unwrap();
        tokio::fs::write(copied.join("sentinel"), b"converted contents")
            .await
            .unwrap();
        recover_conversion(&root).await.unwrap();
        assert_eq!(
            tokio::fs::read(root.join("sentinel")).await.unwrap(),
            if stage == 2 {
                b"converted contents".as_slice()
            } else {
                b"original contents".as_slice()
            }
        );
        assert!(!temporary.exists());
        assert!(!old.exists());
        delete_volume_root(&root).await.unwrap();
        assert!(!root.exists());
    }
}
