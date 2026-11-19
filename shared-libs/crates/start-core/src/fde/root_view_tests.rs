use std::path::PathBuf;

use nix::mount::{MsFlags, mount};

use super::*;
use crate::disk::mount::filesystem::FileSystem;
use crate::disk::mount::filesystem::overlayfs::OverlayFs;
use crate::disk::mount::guard::GenericMountGuard;
use crate::util::squashfs::{NodeContents, snapshot_entries};

#[tokio::test]
async fn isolated_view_cache_identity_separates_access_and_plain_binds() {
    let root = tempfile::tempdir().unwrap();
    let isolated = IsolatedBind::new(root.path());
    assert_ne!(
        isolated.mount_hash(ReadOnly).await.unwrap(),
        isolated.mount_hash(ReadWrite).await.unwrap()
    );
    assert_ne!(
        isolated.source_hash().await.unwrap(),
        Bind::new(root.path()).source_hash().await.unwrap()
    );
}

#[tokio::test]
#[ignore = "requires exclusive managed guest, real OverlayFS/open_tree/mount_setattr; never on build host"]
async fn native_isolated_base_view_preserves_merged_references_without_source_submounts() {
    let directory = PathBuf::from(
        std::env::var_os("FDE_ROOT_VIEW_TEST_ROOT").expect("owned root-view fixture"),
    );
    let root = MountGuard::mount(&PrivateTmpfs::new(), &directory, ReadWrite)
        .await
        .unwrap();
    let scratch = MountGuard::mount(&PrivateTmpfs::new(), TmpDir::BASE, ReadWrite)
        .await
        .unwrap();
    for path in [
        "lower/etc",
        "lower/existing",
        "lower/late",
        "upper/etc",
        "work",
    ] {
        tokio::fs::create_dir_all(root.path().join(path))
            .await
            .unwrap();
    }
    let lower = PathBuf::from(crate::fde::private_config::RUNTIME)
        .join(Guid::new().to_string())
        .join("etc/key");
    let upper = PathBuf::from(crate::fde::private_config::RUNTIME)
        .join(Guid::new().to_string())
        .join("etc/key");
    for (path, target) in [
        ("lower/etc/lower-link", &lower),
        ("lower/etc/replaced", &lower),
        ("upper/etc/replaced", &upper),
    ] {
        tokio::fs::symlink(target, root.path().join(path))
            .await
            .unwrap();
    }
    tokio::fs::hard_link(
        root.path().join("lower/etc/lower-link"),
        root.path().join("lower/etc/alias"),
    )
    .await
    .unwrap();
    tokio::fs::write(
        root.path().join("lower/existing/underlay"),
        b"existing-underlay",
    )
    .await
    .unwrap();
    tokio::fs::write(root.path().join("lower/late/underlay"), b"late-underlay")
        .await
        .unwrap();
    let source = MountGuard::mount(
        &OverlayFs::new(
            vec![root.path().join("lower")],
            root.path().join("upper"),
            root.path().join("work"),
        ),
        root.path().join("source"),
        ReadWrite,
    )
    .await
    .unwrap();
    mount(
        None::<&str>,
        source.path(),
        None::<&str>,
        MsFlags::MS_SHARED,
        None::<&str>,
    )
    .unwrap();
    let existing = MountGuard::mount(
        &PrivateTmpfs::new(),
        source.path().join("existing"),
        ReadWrite,
    )
    .await
    .unwrap();
    tokio::fs::write(existing.path().join("host-mount"), b"not-image-bytes")
        .await
        .unwrap();
    let before_probe = crate::disk::mount::util::mountpoints_under(root.path())
        .await
        .unwrap();
    IsolatedBind::new(source.path()).check_readonly().unwrap();
    assert_eq!(
        crate::disk::mount::util::mountpoints_under(root.path())
            .await
            .unwrap(),
        before_probe
    );
    let base = base_image_view(source.path()).await.unwrap();
    assert!(!base.path().join("existing/host-mount").exists());
    assert_eq!(
        tokio::fs::read(base.path().join("existing/underlay"))
            .await
            .unwrap(),
        b"existing-underlay"
    );
    let late = MountGuard::mount(&PrivateTmpfs::new(), source.path().join("late"), ReadWrite)
        .await
        .unwrap();
    tokio::fs::write(late.path().join("host-mount"), b"late-not-image-bytes")
        .await
        .unwrap();
    assert!(!base.path().join("late/host-mount").exists());
    assert_eq!(
        tokio::fs::read(base.path().join("late/underlay"))
            .await
            .unwrap(),
        b"late-underlay"
    );
    let metadata = Squashfs::from_directory(base.path())
        .await
        .unwrap()
        .into_dyn();
    assert_eq!(
        tokio::fs::read_link(base.path().join("etc/lower-link"))
            .await
            .unwrap(),
        lower
    );
    assert_eq!(
        tokio::fs::read_link(base.path().join("etc/alias"))
            .await
            .unwrap(),
        lower
    );
    assert_eq!(
        tokio::fs::read_link(base.path().join("etc/replaced"))
            .await
            .unwrap(),
        upper
    );
    let links: Vec<_> = snapshot_entries(&metadata)
        .into_iter()
        .filter_map(|(_, entry)| match entry {
            crate::util::squashfs::Entry::Node(crate::util::squashfs::Node {
                contents: NodeContents::Symlink(target),
                ..
            }) => Some(target),
            _ => None,
        })
        .collect();
    assert!(links.contains(&lower));
    assert!(links.contains(&upper));
    assert_eq!(
        tokio::fs::write(base.path().join("read-only"), b"rejected")
            .await
            .unwrap_err()
            .raw_os_error(),
        Some(libc::EROFS)
    );
    tokio::fs::write(source.path().join("source-write"), b"still-writable")
        .await
        .unwrap();
    assert_eq!(
        tokio::fs::read(base.path().join("source-write"))
            .await
            .unwrap(),
        b"still-writable"
    );
    let data = data_view(source.path()).await.unwrap();
    assert_eq!(
        tokio::fs::read(data.path().join("existing/host-mount"))
            .await
            .unwrap(),
        b"not-image-bytes"
    );
    assert_eq!(
        tokio::fs::read(data.path().join("late/host-mount"))
            .await
            .unwrap(),
        b"late-not-image-bytes"
    );
    assert_eq!(
        tokio::fs::write(data.path().join("existing/read-only"), b"rejected")
            .await
            .unwrap_err()
            .raw_os_error(),
        Some(libc::EROFS)
    );
    let prospective = MountGuard::mount(
        &OverlayFs::new(
            vec![data.path().to_owned()],
            root.path().join("query-upper"),
            root.path().join("query-work"),
        ),
        root.path().join("prospective"),
        ReadWrite,
    )
    .await
    .unwrap();
    assert!(!prospective.path().join("existing/host-mount").exists());
    assert!(!prospective.path().join("late/host-mount").exists());
    assert_eq!(
        tokio::fs::read(prospective.path().join("existing/underlay"))
            .await
            .unwrap(),
        b"existing-underlay"
    );
    prospective.unmount(false).await.unwrap();
    drop(metadata);
    let base_path = base.path().to_owned();
    let data_path = data.path().to_owned();
    data.unmount(false).await.unwrap();
    base.unmount(false).await.unwrap();
    assert!(
        !crate::disk::mount::util::is_mountpoint(&base_path)
            .await
            .unwrap()
    );
    assert!(
        !crate::disk::mount::util::is_mountpoint(&data_path)
            .await
            .unwrap()
    );
    assert_eq!(
        tokio::fs::read(existing.path().join("host-mount"))
            .await
            .unwrap(),
        b"not-image-bytes"
    );
    assert_eq!(
        tokio::fs::read(late.path().join("host-mount"))
            .await
            .unwrap(),
        b"late-not-image-bytes"
    );
    late.unmount(false).await.unwrap();
    existing.unmount(false).await.unwrap();
    source.unmount(false).await.unwrap();
    scratch.unmount(false).await.unwrap();
    root.unmount(false).await.unwrap();
}
