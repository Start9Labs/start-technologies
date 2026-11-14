use std::os::unix::fs::{MetadataExt, PermissionsExt};

use super::*;

async fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let raw = root.path().join("raw");
    let data = root.path().join("data");
    for path in [raw.join("etc"), raw.join("root/hostkeys"), data.clone()] {
        tokio::fs::create_dir_all(path).await.unwrap();
    }
    tokio::fs::set_permissions(&raw, std::fs::Permissions::from_mode(0o755))
        .await
        .unwrap();
    tokio::fs::set_permissions(raw.join("root"), std::fs::Permissions::from_mode(0o700))
        .await
        .unwrap();
    tokio::fs::write(raw.join("etc/shadow"), b"private-shadow")
        .await
        .unwrap();
    tokio::fs::set_permissions(
        raw.join("etc/shadow"),
        std::fs::Permissions::from_mode(0o640),
    )
    .await
    .unwrap();
    (root, raw, data)
}

async fn project(raw: &Path, data: &Path, host_keys: &[PathBuf]) -> ProjectedConfig {
    ProjectedConfig::stage(
        Squashfs::from_directory(raw).await.unwrap().into_dyn(),
        data,
        host_keys,
    )
    .await
    .unwrap()
}

fn target<'a>(projected: &'a ProjectedConfig, path: &str) -> &'a Path {
    let (_, node) = resolved(&projected.snapshot, Path::new(path)).unwrap();
    let NodeContents::Symlink(target) = &node.contents else {
        panic!("private file remains public")
    };
    target
}

#[tokio::test]
async fn staging_redacts_credentials_and_preserves_native_metadata_and_hardlinks() {
    let (_root, raw, data) = fixture().await;
    tokio::fs::hard_link(raw.join("etc/shadow"), raw.join("etc/shadow-alias"))
        .await
        .unwrap();
    tokio::fs::write(raw.join("root/hostkeys/custom key"), b"private-host-key")
        .await
        .unwrap();
    tokio::fs::create_dir_all(raw.join("var/lib/dkms"))
        .await
        .unwrap();
    tokio::fs::write(raw.join("var/lib/dkms/mok.key"), b"private-signing-key")
        .await
        .unwrap();
    tokio::fs::write(raw.join("var/lib/dkms/mok.pub"), b"public-certificate")
        .await
        .unwrap();
    xattr::set(
        raw.join("etc/shadow"),
        "user.credential",
        b"private-attribute",
    )
    .unwrap();
    let before = tokio::fs::metadata(raw.join("etc/shadow")).await.unwrap();
    let projected = project(&raw, &data, &[PathBuf::from("/root/hostkeys/custom key")]).await;
    let shadow = backing_file(&data, target(&projected, "etc/shadow"))
        .await
        .unwrap();
    let alias = backing_file(&data, target(&projected, "etc/shadow-alias"))
        .await
        .unwrap();
    assert!(crate::util::io::same_file(&shadow, &alias).await.unwrap());
    let after = tokio::fs::metadata(&shadow).await.unwrap();
    assert_eq!(
        (
            after.uid(),
            after.gid(),
            after.mode() & 0o7777,
            after.mtime()
        ),
        (
            before.uid(),
            before.gid(),
            before.mode() & 0o7777,
            before.mtime()
        )
    );
    assert_eq!(
        xattr::get(&shadow, "user.credential").unwrap().unwrap(),
        b"private-attribute"
    );
    let host = backing_file(&data, target(&projected, "root/hostkeys/custom key"))
        .await
        .unwrap();
    assert_eq!(
        tokio::fs::metadata(host.parent().unwrap().parent().unwrap())
            .await
            .unwrap()
            .mode()
            & 0o777,
        0o700
    );
    for (path, expected) in [
        ("etc/shadow", b"private-shadow".as_slice()),
        ("root/hostkeys/custom key", b"private-host-key".as_slice()),
        ("var/lib/dkms/mok.key", b"private-signing-key".as_slice()),
    ] {
        assert_eq!(tokio::fs::read(raw.join(path)).await.unwrap(), expected);
        assert!(
            !tokio::fs::symlink_metadata(raw.join(path))
                .await
                .unwrap()
                .is_symlink()
        );
        let backing = backing_file(&data, target(&projected, path)).await.unwrap();
        assert_eq!(tokio::fs::read(backing).await.unwrap(), expected);
    }
    let mut encoded = std::io::Cursor::new(Vec::new());
    projected.snapshot.serialize(&mut encoded).await.unwrap();
    let public = Squashfs::deserialize(std::sync::Arc::<[u8]>::from(encoded.into_inner()))
        .await
        .unwrap()
        .into_dyn();
    for (_, entry) in snapshot_entries(&public) {
        if let Entry::Node(Node {
            contents: NodeContents::File(source),
            ..
        }) = entry
        {
            assert_eq!(source.to_vec(None).await.unwrap(), b"public-certificate");
        }
    }
    let (_, node) = resolved(&public, Path::new("etc/shadow")).unwrap();
    assert!(node.metadata.xattrs.is_empty());
}

#[tokio::test]
async fn configuration_container_mode_does_not_block_the_runtime_namespace() {
    let (_root, raw, data) = fixture().await;
    tokio::fs::set_permissions(&raw, std::fs::Permissions::from_mode(0o700))
        .await
        .unwrap();
    let projected = project(&raw, &data, &[]).await;
    let backing = backing_file(&data, target(&projected, "etc/shadow"))
        .await
        .unwrap();
    let namespace = backing.parent().unwrap().parent().unwrap();
    assert_eq!(
        tokio::fs::metadata(namespace).await.unwrap().mode() & 0o777,
        0o755
    );
    assert_eq!(projected.snapshot.root_metadata().mode, 0o700);
}

#[tokio::test]
async fn unused_installer_host_keys_stay_private_without_a_current_hostkey_directive() {
    let (_root, raw, data) = fixture().await;
    tokio::fs::create_dir(raw.join("etc/ssh")).await.unwrap();
    for name in ["ssh_host_key", "ssh_host_dsa_key"] {
        tokio::fs::write(raw.join("etc/ssh").join(name), b"unused-private-host-key")
            .await
            .unwrap();
    }
    tokio::fs::write(raw.join("etc/ssh/ssh_host_dsa_key.pub"), b"public-key")
        .await
        .unwrap();
    tokio::fs::write(raw.join("etc/ssh/sshd_config"), b"public-config")
        .await
        .unwrap();
    let projected = project(&raw, &data, &[]).await;
    for name in ["etc/ssh/ssh_host_key", "etc/ssh/ssh_host_dsa_key"] {
        let backing = backing_file(&data, target(&projected, name)).await.unwrap();
        assert_eq!(
            tokio::fs::read(backing).await.unwrap(),
            b"unused-private-host-key"
        );
    }
    for path in ["etc/ssh/ssh_host_dsa_key.pub", "etc/ssh/sshd_config"] {
        assert!(matches!(
            resolved(&projected.snapshot, Path::new(path))
                .unwrap()
                .1
                .contents,
            NodeContents::File(_)
        ));
    }
}

#[tokio::test]
async fn private_parent_symlinks_are_not_misclassified_as_missing_files() {
    let (_root, raw, data) = fixture().await;
    tokio::fs::create_dir_all(raw.join("var/lib"))
        .await
        .unwrap();
    tokio::fs::create_dir(raw.join("elsewhere")).await.unwrap();
    tokio::fs::write(raw.join("elsewhere/mok.key"), b"private-signing-key")
        .await
        .unwrap();
    tokio::fs::symlink("/elsewhere", raw.join("var/lib/dkms"))
        .await
        .unwrap();
    let result = ProjectedConfig::stage(
        Squashfs::from_directory(&raw).await.unwrap().into_dyn(),
        &data,
        &[],
    )
    .await;
    assert!(result.is_err());
    assert!(!data.join(DIRECTORY).exists());
    assert_eq!(
        tokio::fs::read(raw.join("elsewhere/mok.key"))
            .await
            .unwrap(),
        b"private-signing-key"
    );
}

#[tokio::test]
async fn pending_retry_never_overwrites_an_earlier_candidate() {
    let (_root, raw, data) = fixture().await;
    let first = project(&raw, &data, &[]).await;
    let old = backing_file(&data, target(&first, "etc/shadow"))
        .await
        .unwrap();
    drop(first);
    tokio::fs::write(raw.join("etc/shadow"), b"changed-while-pending")
        .await
        .unwrap();
    let second = project(&raw, &data, &[]).await;
    let new = backing_file(&data, target(&second, "etc/shadow"))
        .await
        .unwrap();
    assert_ne!(old, new);
    assert_eq!(tokio::fs::read(old).await.unwrap(), b"private-shadow");
    assert_eq!(
        tokio::fs::read(new).await.unwrap(),
        b"changed-while-pending"
    );
    assert_eq!(
        tokio::fs::read(raw.join("etc/shadow")).await.unwrap(),
        b"changed-while-pending"
    );
}

#[tokio::test]
async fn legacy_replacement_is_retryable_and_existing_projection_keeps_live_data() {
    let (_root, raw, data) = fixture().await;
    let first = project(&raw, &data, &[]).await;
    let backing = backing_file(&data, target(&first, "etc/shadow"))
        .await
        .unwrap();
    first.replace_legacy(&raw).await.unwrap();
    first.replace_legacy(&raw).await.unwrap();
    assert_eq!(
        tokio::fs::read_link(raw.join("etc/shadow")).await.unwrap(),
        target(&first, "etc/shadow")
    );
    drop(first);
    tokio::fs::write(&backing, b"committed-new-password")
        .await
        .unwrap();
    let second = project(&raw, &data, &[]).await;
    assert_eq!(
        backing_file(&data, target(&second, "etc/shadow"))
            .await
            .unwrap(),
        backing
    );
    second.replace_legacy(&raw).await.unwrap();
    assert_eq!(
        tokio::fs::read(backing).await.unwrap(),
        b"committed-new-password"
    );
    assert_eq!(std::fs::read_dir(data.join(DIRECTORY)).unwrap().count(), 1);
}

#[tokio::test]
async fn resumed_projection_retires_stale_legacy_without_overwriting_live_credentials() {
    let (_root, raw, data) = fixture().await;
    let first = project(&raw, &data, &[]).await;
    let backing = backing_file(&data, target(&first, "etc/shadow"))
        .await
        .unwrap();
    let snapshot = first.snapshot.clone();
    drop(first);
    tokio::fs::write(&backing, b"live-committed-password")
        .await
        .unwrap();
    let resumed = ProjectedConfig::stage(snapshot, &data, &[]).await.unwrap();
    assert_eq!(
        tokio::fs::read(raw.join("etc/shadow")).await.unwrap(),
        b"private-shadow"
    );
    resumed.replace_legacy(&raw).await.unwrap();
    assert!(
        tokio::fs::symlink_metadata(raw.join("etc/shadow"))
            .await
            .unwrap()
            .is_symlink()
    );
    assert_eq!(
        tokio::fs::read(backing).await.unwrap(),
        b"live-committed-password"
    );
}

#[tokio::test]
async fn unsupported_private_symlink_rejects_before_staging_or_legacy_changes() {
    let (_root, raw, data) = fixture().await;
    tokio::fs::remove_file(raw.join("etc/shadow"))
        .await
        .unwrap();
    tokio::fs::symlink("/elsewhere", raw.join("etc/shadow"))
        .await
        .unwrap();
    let result = ProjectedConfig::stage(
        Squashfs::from_directory(&raw).await.unwrap().into_dyn(),
        &data,
        &[],
    )
    .await;
    assert!(result.is_err());
    assert!(!data.join(DIRECTORY).exists());
    assert_eq!(
        tokio::fs::read_link(raw.join("etc/shadow")).await.unwrap(),
        Path::new("/elsewhere")
    );
}

#[tokio::test]
async fn failed_staging_keeps_legacy_boot_files_and_fences_partial_private_candidate() {
    let (root, raw, _) = fixture().await;
    let data = root
        .path()
        .join("extended-data-parent".repeat(10))
        .join("another-data-parent".repeat(10))
        .join("data");
    tokio::fs::create_dir_all(&data).await.unwrap();
    let desired = 3800 - raw.as_os_str().len() - 1;
    let mut relative = PathBuf::from("z");
    while relative.as_os_str().len() + 201 < desired {
        relative.push("d".repeat(200));
    }
    relative.push("k".repeat(desired - relative.as_os_str().len() - 1));
    tokio::fs::create_dir_all(raw.join(relative.parent().unwrap()))
        .await
        .unwrap();
    tokio::fs::write(raw.join(&relative), b"private-long-host-key")
        .await
        .unwrap();
    let result = ProjectedConfig::stage(
        Squashfs::from_directory(&raw).await.unwrap().into_dyn(),
        &data,
        &[Path::new("/").join(relative)],
    )
    .await;
    assert!(result.is_err());
    assert_eq!(
        tokio::fs::read(raw.join("etc/shadow")).await.unwrap(),
        b"private-shadow"
    );
    assert!(
        tokio::fs::symlink_metadata(raw.join("etc/shadow"))
            .await
            .unwrap()
            .is_file()
    );
    let entries = std::fs::read_dir(data.join(DIRECTORY))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].metadata().unwrap().mode() & 0o777, 0o700);
}

#[tokio::test]
async fn private_deletion_markers_keep_their_semantics_without_publishing_payloads() {
    for nonempty in [false, true] {
        let (_root, raw, data) = fixture().await;
        if !nonempty {
            tokio::fs::write(raw.join("etc/shadow"), b"").await.unwrap();
        }
        let mut snapshot = Squashfs::from_directory(&raw).await.unwrap().into_dyn();
        let (_, node) = resolved(&snapshot, Path::new("etc/shadow")).unwrap();
        let mut marker = node.clone();
        marker
            .metadata
            .xattrs
            .insert("user.overlay.whiteout".into(), b"y".to_vec());
        snapshot
            .contents_mut()
            .insert_path("etc/shadow", Entry::Node(marker))
            .unwrap();
        let result = ProjectedConfig::stage(snapshot, &data, &[]).await;
        assert!(!data.join(DIRECTORY).exists());
        if nonempty {
            assert!(result.is_err());
        } else {
            let projected = result.unwrap();
            let (_, node) = resolved(&projected.snapshot, Path::new("etc/shadow")).unwrap();
            assert!(overlay_whiteout(node));
            assert!(projected.replacements.is_empty());
        }
    }
}

#[tokio::test]
async fn no_private_files_needs_no_backing_directory() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let data = root.path().join("data");
    tokio::fs::create_dir(&source).await.unwrap();
    let projected = project(&source, &data, &[]).await;
    assert!(projected.replacements.is_empty());
    assert!(!data.exists());
}

#[tokio::test]
#[ignore = "requires managed root guest, mount, setpriv, setfacl, sshd and veritysetup"]
async fn real_private_view_preserves_group_access_restart_and_strict_scoped_teardown() {
    use crate::util::Invoke;
    let (root, raw, data) = fixture().await;
    let defaults = crate::ssh::host_key_files(Path::new("/dev/null"))
        .await
        .unwrap();
    assert!(!defaults.is_empty());
    assert!(defaults.iter().all(|path| path.is_absolute()));
    let configuration = root.path().join("sshd_config");
    let included = root.path().join("extra config");
    tokio::fs::write(&included, "HostKey /custom/included-key\n")
        .await
        .unwrap();
    tokio::fs::write(
        &configuration,
        format!(
            "Include \"{}\"\nHostKey \"/custom/key with space\"\n",
            included.display()
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        crate::ssh::host_key_files(&configuration).await.unwrap(),
        vec![
            PathBuf::from("/custom/included-key"),
            PathBuf::from("/custom/key with space")
        ]
    );
    tokio::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755))
        .await
        .unwrap();
    tokio::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700))
        .await
        .unwrap();
    nix::unistd::chown(&raw.join("etc/shadow"), Some(0.into()), Some(65534.into())).unwrap();
    tokio::process::Command::new("setfacl")
        .args(["-m", "d:u:1234:r-x"])
        .arg(&data)
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    let projected = project(&raw, &data, &[]).await;
    let backing = backing_file(&data, target(&projected, "etc/shadow"))
        .await
        .unwrap();
    let rootfs = tokio::fs::canonicalize("/media/startos/config/current.rootfs")
        .await
        .unwrap();
    let store = root.path().join("generations");
    tokio::fs::create_dir(&store).await.unwrap();
    let key = crate::os_overlay::generate_signing_key().unwrap();
    let prepared = projected
        .prepare_generation(&rootfs, &store, &key)
        .await
        .unwrap();
    let published = prepared.publish().await.unwrap();
    assert!(published.join(crate::os_overlay::OVERLAY_FILE).is_file());
    assert!(
        tokio::fs::symlink_metadata(raw.join("etc/shadow"))
            .await
            .unwrap()
            .is_file()
    );
    projected.replace_legacy(&raw).await.unwrap();
    assert!(
        !crate::disk::mount::util::is_mountpoint(RUNTIME)
            .await
            .unwrap()
    );
    mount_runtime(&data).await.unwrap();
    let _cleanup = crate::util::GeneralBoxedGuard::new(|| {
        let _ = std::process::Command::new("umount").arg(RUNTIME).output();
    });
    let mount = || {
        std::fs::read_to_string("/proc/self/mountinfo")
            .unwrap()
            .lines()
            .find(|line| line.split(' ').nth(4) == Some(RUNTIME))
            .unwrap()
            .to_owned()
    };
    let first_mount = mount();
    mount_runtime(&data).await.unwrap();
    assert_eq!(mount(), first_mount);
    let read_as_group = |path: &Path| {
        let mut command = tokio::process::Command::new("setpriv");
        command
            .args(["--reuid=65534", "--regid=65534", "--clear-groups", "cat"])
            .arg(path);
        command
    };
    assert!(
        read_as_group(&backing)
            .invoke(ErrorKind::Filesystem)
            .await
            .is_err()
    );
    assert_eq!(
        read_as_group(&raw.join("etc/shadow"))
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap(),
        b"private-shadow"
    );
    assert_eq!(
        xattr::get(&backing, "system.posix_acl_access").unwrap(),
        None
    );
    assert!(
        tokio::process::Command::new("setpriv")
            .args(["--reuid=1234", "--regid=1234", "--clear-groups", "cat"])
            .arg(raw.join("etc/shadow"))
            .invoke(ErrorKind::Filesystem)
            .await
            .is_err()
    );
    let held = open_file(raw.join("etc/shadow")).await.unwrap();
    unmount_runtime(&root.path().join("unrelated-data"))
        .await
        .unwrap();
    assert_eq!(mount(), first_mount);
    assert!(unmount_runtime(&data).await.is_err());
    assert_eq!(mount(), first_mount);
    drop(held);
    unmount_runtime(&data).await.unwrap();
    assert!(
        !crate::disk::mount::util::is_mountpoint(RUNTIME)
            .await
            .unwrap()
    );
}
