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

#[cfg(target_os = "linux")]
#[test]
fn mount_identity_distinguishes_mounts_not_directories() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("child")).unwrap();
    let parent = open_beneath(root.path(), Path::new("")).unwrap();
    let child = open_beneath(root.path(), Path::new("child")).unwrap();
    let procfs = open_beneath(Path::new("/proc"), Path::new("")).unwrap();
    assert!(same_mount(parent.as_fd(), child.as_fd()).unwrap());
    assert!(!same_mount(parent.as_fd(), procfs.as_fd()).unwrap());
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

async fn prepare_target(
    container: &crate::lxc::LxcContainer,
    mounts: &mut SharedVolumeMounts,
    guid: &Guid,
    location: &str,
) -> PathBuf {
    container.command(&["mkdir", "-p", location]).await.unwrap();
    let pid = container.pid().await.unwrap();
    let root = PathBuf::from(format!("/proc/{pid}/root"));
    let namespace = Arc::new(MountNamespace::for_process(pid).unwrap());
    mounts
        .prepare_target(guid, Path::new(location), &root, Path::new(""), namespace)
        .await
        .unwrap();
    root
}

async fn mount_managed(
    container: &crate::lxc::LxcContainer,
    mounts: &Arc<tokio::sync::Mutex<SharedVolumeMounts>>,
    guid: &Guid,
    location: &str,
    id: &SharedVolumeId,
    source: &TmpMountGuard,
    readonly: bool,
) -> PathBuf {
    let mut mounts = mounts.lock().await;
    let root = prepare_target(container, &mut mounts, guid, location).await;
    mounts
        .mount_source(
            guid,
            Path::new(location),
            id,
            PreparedSource::open(source.clone(), None)
                .await
                .unwrap()
                .idmapped(
                    crate::disk::mount::filesystem::syscall::userns_fd_from_idmap(
                        &[container.slot().idmap()],
                        None,
                    )
                    .await
                    .unwrap(),
                ),
            readonly,
            &root,
        )
        .await
        .unwrap();
    PathBuf::from(location)
}

#[tokio::test]
#[ignore = "requires a StartOS VM, BTRFS_TEST_VOLUMES_DIR and STARTOS_TEST_CONTAINER_HELPER"]
async fn real_lxc_best_effort_revocation_preserves_aliases_and_locks_readonly_stages() {
    use crate::disk::mount::filesystem::ReadWrite;
    use crate::disk::mount::filesystem::bind::{Bind, FileType};
    use crate::disk::mount::guard::MountGuard;
    use crate::lxc::slot::ContainerSlot;
    use crate::lxc::{LxcConfig, LxcManager};

    let fixture = crate::util::btrfs::tests::directory();
    std::fs::write(fixture.path().join("sentinel"), b"private data").unwrap();
    std::fs::set_permissions(
        fixture.path().join("sentinel"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let source = TmpMountGuard::mount(&Bind::new(fixture.path()), ReadWrite)
        .await
        .unwrap();
    let manager = Arc::new(LxcManager::new());
    let first = manager
        .create(
            None,
            ContainerSlot::new(0).unwrap(),
            None,
            LxcConfig::default(),
        )
        .await
        .unwrap();
    let second = manager
        .create(
            None,
            ContainerSlot::new(1).unwrap(),
            None,
            LxcConfig::default(),
        )
        .await
        .unwrap();
    let helper = std::env::var_os("STARTOS_TEST_CONTAINER_HELPER")
        .expect("STARTOS_TEST_CONTAINER_HELPER must name this build's start-container");
    let mut helper_mounts = Vec::new();
    for container in [&first, &second] {
        helper_mounts.push(
            MountGuard::mount(
                &Bind::new(Path::new(&helper)).with_type(FileType::File),
                container.rootfs_dir().join("usr/bin/start-container"),
                crate::disk::mount::filesystem::ReadOnly,
            )
            .await
            .unwrap(),
        );
    }
    let first_mounts = Arc::new(tokio::sync::Mutex::new(
        SharedVolumeMounts::new(first.rootfs_dir()).await.unwrap(),
    ));
    let second_mounts = Arc::new(tokio::sync::Mutex::new(
        SharedVolumeMounts::new(second.rootfs_dir()).await.unwrap(),
    ));
    let registry = crate::shared_volumes::access::SharedVolumeAccess::default();
    let first_package = "first".parse().unwrap();
    registry.register(&first_package, &first_mounts);
    registry.register(&"second".parse().unwrap(), &second_mounts);
    let id = SharedVolumeId::new();
    let first_guid = Guid::new();
    let second_guid = Guid::new();
    let first_path = mount_managed(
        &first,
        &first_mounts,
        &first_guid,
        "/alias",
        &id,
        &source,
        false,
    )
    .await;
    let second_path = mount_managed(
        &second,
        &second_mounts,
        &second_guid,
        "/alias",
        &id,
        &source,
        false,
    )
    .await;
    let worker = |path: &Path, name: &str| {
        format!(
            "mkdir /independent; start-container bind-mount --source '{}' --target /independent --recursive; unshare --mount --pid --cgroup --fork sh -c 'exec 3>>/independent/{name}; echo ready >&3; while :; do printf x >&3; sleep 0.02; done' >/dev/null 2>&1 </dev/null &",
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
    first.command(&["sh", "-ec", &format!(
        "mkdir '{0}/nested'; mount -t tmpfs tmpfs '{0}/nested'; echo nested > '{0}/nested/marker'",
        first_path.display(),
    )]).await.unwrap();
    first
        .command(&["sh", "-ec", &format!(
            "test \"$(cat /alias/nested/marker)\" = nested; mkdir /readonly-tree; start-container bind-mount --source '{}' --target /readonly-tree --recursive --readonly; test \"$(cat /readonly-tree/nested/marker)\" = nested; if touch /readonly-tree/nested/forbidden 2>/dev/null; then exit 110; fi; mkdir '{0}/late-child'; mount -t tmpfs tmpfs '{0}/late-child'; echo late > '{0}/late-child/marker'; test \"$(cat /alias/late-child/marker)\" = late; test ! -e /readonly-tree/late-child/marker",
            first_path.display(),
        )])
        .await
        .unwrap();
    grows(&fixture.path().join("first"), 0).await;
    grows(&fixture.path().join("second"), 0).await;
    let before = std::fs::metadata(fixture.path().join("first"))
        .unwrap()
        .len();
    let read_stage = mount_managed(
        &first,
        &first_mounts,
        &first_guid,
        "/read",
        &id,
        &source,
        true,
    )
    .await;
    let unrelated = SharedVolumeId::new();
    let other_stage = mount_managed(
        &first,
        &first_mounts,
        &first_guid,
        "/other",
        &unrelated,
        &source,
        false,
    )
    .await;
    first.command(&["sh", "-ec", "unshare --mount --propagation slave sh -c 'touch /execution-ready; exec sleep 300' >/dev/null 2>&1 & echo $! > /execution-pid; timeout 5 sh -c 'until test -e /execution-ready; do sleep 0.01; done'"]).await.unwrap();
    registry
        .reconcile_grant(
            &first_package,
            &id,
            Some(&crate::shared_volumes::SharedVolumeGrant { readonly: true }),
        )
        .await
        .unwrap();
    first
        .command(&[
            "sh",
            "-ec",
            &format!(
                "test ! -e '{}/sentinel'; test ! -e /alias/sentinel; test -r /read/sentinel; test -r /other/sentinel; if mount -o remount,bind,rw /read 2>/dev/null; then exit 109; fi; if touch /alias/forbidden 2>/dev/null; then exit 108; fi; test -r '{}/sentinel'; test -r '{}/sentinel'",
                first_path.display(),
                read_stage.display(),
                other_stage.display(),
            ),
        ])
        .await.unwrap();
    first.command(&["sh", "-ec", "nsenter --mount=/proc/$(cat /execution-pid)/ns/mnt -- sh -ec 'test ! -e /alias/sentinel; test -r /read/sentinel; test -r /other/sentinel'; kill $(cat /execution-pid)"]).await.unwrap();
    grows(&fixture.path().join("first"), before).await;
    registry
        .reconcile_grant(&first_package, &id, None)
        .await
        .unwrap();
    first
        .command(&[
            "sh",
            "-ec",
            &format!(
                "test ! -e '{}/sentinel'; test ! -e /read/sentinel; test -r /other/sentinel; test -r '{}/sentinel'",
                read_stage.display(),
                other_stage.display(),
            ),
        ])
        .await
        .unwrap();
    second
        .command(&[
            "sh",
            "-ec",
            &format!(
                "test -r /alias/sentinel; test -r '{}/sentinel'",
                second_path.display()
            ),
        ])
        .await
        .unwrap();
    registry.remove_volume(&id).await.unwrap();
    second
        .command(&[
            "sh",
            "-ec",
            &format!(
                "test ! -e /alias/sentinel; test ! -e '{}/sentinel'",
                second_path.display()
            ),
        ])
        .await
        .unwrap();
    let other = std::fs::metadata(fixture.path().join("second"))
        .unwrap()
        .len();
    grows(&fixture.path().join("second"), other).await;

    let readonly = mount_managed(
        &second,
        &second_mounts,
        &second_guid,
        "/readonly",
        &id,
        &source,
        true,
    )
    .await;
    let probe = format!(
        "set -e; mkdir /readonly-alias; start-container bind-mount --source '{}' --target /readonly-alias --recursive --readonly; test \"$(cat /readonly/sentinel)\" = 'private data'; if mount -o remount,bind,rw /readonly-alias 2>/dev/null; then exit 111; fi; if mount -o remount,bind,rw /readonly 2>/dev/null; then exit 101; fi; if touch /readonly/forbidden 2>/dev/null; then exit 103; fi",
        readonly.display(),
    );
    second.command(&["sh", "-c", &probe]).await.unwrap();
    mount_managed(
        &second,
        &second_mounts,
        &second_guid,
        "/optional",
        &id,
        &source,
        true,
    )
    .await;
    second
        .command(&["sh", "-ec", "test -r /optional/sentinel"])
        .await
        .unwrap();
    {
        let mut mounts = second_mounts.lock().await;
        prepare_target(&second, &mut mounts, &second_guid, "/optional").await;
    }
    second.command(&["sh", "-ec", "test ! -e /optional/sentinel; if touch /optional/forbidden 2>/dev/null; then exit 107; fi; if mount -o remount,bind,rw /optional 2>/dev/null; then exit 112; fi"]).await.unwrap();
    mount_managed(
        &second,
        &second_mounts,
        &second_guid,
        "/optional",
        &id,
        &source,
        true,
    )
    .await;
    second
        .command(&["sh", "-ec", "test -r /optional/sentinel"])
        .await
        .unwrap();
    mount_managed(
        &first,
        &first_mounts,
        &first_guid,
        "/moving/data",
        &id,
        &source,
        true,
    )
    .await;
    first
        .command(&["sh", "-ec", "mv /moving /moved; mkdir -p /moving/data"])
        .await
        .unwrap();
    mount_managed(
        &first,
        &first_mounts,
        &first_guid,
        "/moving/data",
        &id,
        &source,
        true,
    )
    .await;
    first.command(&["sh", "-ec", "test -r /moving/data/sentinel; test ! -e /moved/data/sentinel; if touch /moving/data/forbidden 2>/dev/null; then exit 113; fi"]).await.unwrap();
    assert!(first_mounts.lock().await.check_allowed().is_ok());
    first_mounts.lock().await.clear().await.unwrap();
    second_mounts.lock().await.clear().await.unwrap();
    drop(first_mounts);
    drop(second_mounts);
    for helper in helper_mounts {
        helper.unmount(false).await.unwrap();
    }
    first.exit().await.unwrap();
    second.exit().await.unwrap();
    source.unmount().await.unwrap();
}
