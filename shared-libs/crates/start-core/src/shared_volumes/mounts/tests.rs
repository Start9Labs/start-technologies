use super::*;

#[cfg(target_os = "linux")]
#[tokio::test]
async fn pinned_directory_keeps_identity_after_name_replacement() {
    let root = tempfile::tempdir().unwrap();
    let selected = root.path().join("selected");
    std::fs::create_dir(&selected).unwrap();
    std::fs::write(selected.join("marker"), b"original").unwrap();
    let directory = PinnedDirectory::open(root.path(), Path::new("selected")).unwrap();
    std::fs::rename(&selected, root.path().join("old")).unwrap();
    std::fs::create_dir(&selected).unwrap();
    std::fs::write(selected.join("marker"), b"replacement").unwrap();
    assert_eq!(
        tokio::fs::read(directory.path().join("marker"))
            .await
            .unwrap(),
        b"original"
    );
    assert_eq!(
        std::fs::read(selected.join("marker")).unwrap(),
        b"replacement"
    );
}

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

#[tokio::test]
async fn backup_restore_scope_blocks_staging_until_explicitly_reenabled() {
    let mut mounts = SharedVolumeMounts::default();
    assert!(mounts.check_allowed().is_ok());
    mounts.block();
    assert!(mounts.check_allowed().is_err());
    mounts.allow();
    assert!(mounts.check_allowed().is_ok());
    mounts.clear().await.unwrap();
    assert!(mounts.check_allowed().is_err());
}

async fn grows(path: &Path, previous: u64) -> u64 {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if let Ok(metadata) = tokio::fs::metadata(path).await {
                if metadata.len() > previous {
                    return metadata.len();
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("container heartbeat did not advance")
}

#[tokio::test]
#[ignore = "requires a StartOS VM with LXC, root and BTRFS_TEST_VOLUMES_DIR"]
async fn real_lxc_revocation_kills_independent_aliases_and_locks_readonly_stages() {
    use crate::disk::mount::filesystem::ReadWrite;
    use crate::disk::mount::filesystem::bind::Bind;
    use crate::lxc::{LxcConfig, LxcManager, service_filesystem};

    let fixture = crate::util::btrfs::tests::directory();
    std::fs::write(fixture.path().join("sentinel"), b"private data").unwrap();
    std::fs::set_permissions(
        fixture.path().join("sentinel"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let source = TmpMountGuard::mount(
        &service_filesystem(Bind::new(fixture.path()), &[]),
        ReadWrite,
    )
    .await
    .unwrap();
    let manager = Arc::new(LxcManager::new());
    let first = manager.create(None, LxcConfig::default()).await.unwrap();
    let second = manager.create(None, LxcConfig::default()).await.unwrap();
    let first_control = first.control().clone();
    let second_control = second.control().clone();
    let mut first_mounts = SharedVolumeMounts::new(first.rootfs_dir()).await.unwrap();
    let mut second_mounts = SharedVolumeMounts::new(second.rootfs_dir()).await.unwrap();
    let id = SharedVolumeId::new();
    let first_guid = Guid::new();
    let second_guid = Guid::new();
    let first_path = first_mounts
        .stage(
            &first_guid,
            "/alias".into(),
            &id,
            PreparedSource::open(source.clone(), None).await.unwrap(),
            false,
        )
        .await
        .unwrap();
    let second_path = second_mounts
        .stage(
            &second_guid,
            "/alias".into(),
            &id,
            PreparedSource::open(source.clone(), None).await.unwrap(),
            false,
        )
        .await
        .unwrap();
    let worker = |path: &Path, name: &str| {
        format!(
            "mkdir /alias; start-container bind-mount --source '{}' --target /alias --recursive; unshare --mount --pid --cgroup --fork sh -c 'exec 3>>/alias/{name}; echo ready >&3; while :; do printf x >&3; sleep 0.02; done' >/dev/null 2>&1 </dev/null &",
            path.display(),
        )
    };
    first
        .command(&["sh", "-c", &worker(&first_path, "first")])
        .await
        .unwrap();
    second
        .command(&["sh", "-c", &worker(&second_path, "second")])
        .await
        .unwrap();
    grows(&fixture.path().join("first"), 0).await;
    grows(&fixture.path().join("second"), 0).await;
    let before = std::fs::metadata(fixture.path().join("first"))
        .unwrap()
        .len();
    first_mounts.remove_subcontainer(&first_guid).await.unwrap();
    grows(&fixture.path().join("first"), before).await;
    first.kill_and_wait().await.unwrap();
    assert!(first_control.is_stopped().unwrap());
    first_control.kill_and_wait().await.unwrap();
    let stopped = std::fs::metadata(fixture.path().join("first"))
        .unwrap()
        .len();
    let other = std::fs::metadata(fixture.path().join("second"))
        .unwrap()
        .len();
    grows(&fixture.path().join("second"), other).await;
    assert_eq!(
        std::fs::metadata(fixture.path().join("first"))
            .unwrap()
            .len(),
        stopped
    );
    assert!(!second_control.is_stopped().unwrap());

    let readonly = second_mounts
        .stage(
            &second_guid,
            "/readonly".into(),
            &id,
            PreparedSource::open(source.clone(), None).await.unwrap(),
            true,
        )
        .await
        .unwrap();
    let probe = format!(
        "set -e; mkdir /readonly; start-container bind-mount --source '{}' --target /readonly --recursive --readonly; test \"$(cat /readonly/sentinel)\" = 'private data'; if mount -o remount,bind,rw /readonly 2>/dev/null; then exit 101; fi; if mount -o remount,bind,rw '{}' 2>/dev/null; then exit 102; fi; if touch /readonly/forbidden 2>/dev/null; then exit 103; fi; if touch /media/startos/shared-volumes/forbidden 2>/dev/null; then exit 104; fi",
        readonly.display(),
        readonly.display(),
    );
    second.command(&["sh", "-c", &probe]).await.unwrap();
    let placeholder = second_mounts.unavailable_path();
    let probe = format!(
        "set -e; mkdir /optional; start-container bind-mount --source '{}' --target /optional --recursive --readonly; if touch /optional/forbidden 2>/dev/null; then exit 105; fi; start-container bind-mount --source '{}' --target /optional --recursive --readonly; test \"$(cat /optional/sentinel)\" = 'private data'; umount /optional; if test -e /optional/sentinel; then exit 106; fi; if touch /optional/forbidden 2>/dev/null; then exit 107; fi",
        placeholder.display(),
        readonly.display(),
    );
    second.command(&["sh", "-c", &probe]).await.unwrap();
    second.kill_and_wait().await.unwrap();
    assert!(second_control.is_stopped().unwrap());
    first_mounts.clear().await.unwrap();
    second_mounts.clear().await.unwrap();
    drop(first_mounts);
    drop(second_mounts);
    first.exit().await.unwrap();
    second.exit().await.unwrap();
    first_control.kill_and_wait().await.unwrap();
    second_control.kill_and_wait().await.unwrap();
    source.unmount().await.unwrap();
}
