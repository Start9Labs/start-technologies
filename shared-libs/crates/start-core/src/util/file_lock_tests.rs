#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};

use super::{FileLock, Invoke};
use crate::prelude::*;

fn scripts() -> PathBuf {
    std::env::var_os("FDE_IMAGE_SCRIPTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../projects/start-os/build/lib/scripts")
        })
}

fn lock_name() -> &'static str {
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../projects/start-os/build/lib/scripts/os-image-lock-name"
    ))
    .trim()
}

fn variable() -> &'static str {
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../projects/start-os/build/lib/scripts/os-image-lock-fd-name"
    ))
    .trim()
}

#[tokio::test]
async fn locking_an_existing_file_preserves_its_contents() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(lock_name());
    tokio::fs::write(&path, b"retained-lock-file")
        .await
        .unwrap();
    let identity = crate::util::io::file_identity(&path).await.unwrap();
    let lease = FileLock::new(&path, false).await.unwrap();
    assert_eq!(tokio::fs::read(&path).await.unwrap(), b"retained-lock-file");
    assert_eq!(
        crate::util::io::file_identity(&path).await.unwrap(),
        identity
    );
    lease.unlock().await.unwrap();
}

#[tokio::test]
async fn shared_descriptor_retains_the_kernel_lease_with_close_on_exec() {
    use std::os::fd::AsRawFd;

    const CHILD: &str = "STARTOS_TEST_SHARED_LEASE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        tokio::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "util::file_lock_tests::shared_descriptor_retains_the_kernel_lease_with_close_on_exec", "--nocapture"])
            .env(CHILD, "1")
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(lock_name());
    let lease = FileLock::new(&path, false).await.unwrap();
    let shared = lease.share().unwrap();
    assert!(shared.as_raw_fd() >= 3);
    let flags = nix::fcntl::fcntl(&shared, nix::fcntl::FcntlArg::F_GETFD).unwrap();
    assert_ne!(flags & libc::FD_CLOEXEC, 0);
    drop(lease);
    assert!(FileLock::new(&path, false).await.is_err());
    drop(shared);
    assert!(FileLock::new(&path, false).await.is_ok());
}

#[tokio::test]
async fn inherited_child_keeps_the_lease_after_its_guard_is_released() {
    use std::process::Stdio;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(lock_name());
    for explicit_unlock in [false, true] {
        let lease = FileLock::new(&path, true).await.unwrap();
        let mut command = tokio::process::Command::new("/bin/bash");
        command
            .args(["-euc", "printf ready; read -r release"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true);
        lease.inherit(&mut command, variable()).unwrap();
        let mut child = command.spawn().unwrap();
        drop(command);
        let mut ready = [0; 5];
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            child.stdout.as_mut().unwrap().read_exact(&mut ready),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(&ready, b"ready");
        if explicit_unlock {
            lease.unlock().await.unwrap();
        } else {
            drop(lease);
        }
        assert!(FileLock::new(&path, false).await.is_err());
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"release\n")
            .await
            .unwrap();
        assert!(child.wait().await.unwrap().success());
        FileLock::new(&path, false)
            .await
            .unwrap()
            .unlock()
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn native_shell_reservation_preserves_existing_contents_and_file_identity() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(lock_name());
    tokio::fs::write(&path, b"retained-shell-lock")
        .await
        .unwrap();
    let identity = crate::util::io::file_identity(&path).await.unwrap();
    tokio::process::Command::new("/bin/bash")
        .args([
            "-euc",
            "source \"$1/os-image-store\"; startos_reserve_image_store \"$2\"",
            "native-shell-lease",
        ])
        .arg(scripts())
        .arg(root.path())
        .env_remove(variable())
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    assert_eq!(
        tokio::fs::read(&path).await.unwrap(),
        b"retained-shell-lock"
    );
    assert_eq!(
        crate::util::io::file_identity(&path).await.unwrap(),
        identity
    );
}

#[tokio::test]
async fn image_store_native_child_reuses_the_inherited_real_file_lease() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("configuration with space");
    tokio::fs::create_dir(&directory).await.unwrap();
    let lease = FileLock::new(directory.join(lock_name()), true)
        .await
        .unwrap();
    let mut command = tokio::process::Command::new("timeout");
    command
        .args([
            "--kill-after=1s",
            "2s",
            "/bin/bash",
            "-euc",
            "source \"$1/os-image-store\"; startos_reserve_image_store \"$2\"; printf leased",
            "native-image-lease",
        ])
        .arg(scripts())
        .arg(&directory);
    lease.inherit(&mut command, variable()).unwrap();
    assert_eq!(
        command.invoke(ErrorKind::Filesystem).await.unwrap(),
        b"leased"
    );
    lease.unlock().await.unwrap();
}

#[tokio::test]
async fn image_store_native_child_rejects_a_foreign_descriptor_as_a_bypass() {
    let root = tempfile::tempdir().unwrap();
    let lease = FileLock::new(root.path().join(lock_name()), true)
        .await
        .unwrap();
    let foreign = FileLock::new(root.path().join("foreign"), true)
        .await
        .unwrap();
    let marker = root.path().join("incorrectly-entered");
    let mut command = tokio::process::Command::new("timeout");
    command
        .args([
            "--kill-after=1s",
            "0.2s",
            "/bin/bash",
            "-euc",
            "source \"$1/os-image-store\"; startos_reserve_image_store \"$2\"; touch \"$3\"",
            "foreign-image-lease",
        ])
        .arg(scripts())
        .arg(root.path())
        .arg(&marker);
    foreign.inherit(&mut command, variable()).unwrap();
    let error = command.invoke(ErrorKind::Filesystem).await.unwrap_err();
    assert!(error.to_string().contains("124"), "{error}");
    assert!(!marker.exists());
    foreign.unlock().await.unwrap();
    lease.unlock().await.unwrap();
}

#[tokio::test]
async fn image_store_native_parent_and_nested_scripts_share_one_kernel_lease() {
    let root = tempfile::tempdir().unwrap();
    let mut command = tokio::process::Command::new("timeout");
    command.args(["--kill-after=1s", "2s", "/bin/bash", "-euc", "source \"$1/os-image-store\"; startos_reserve_image_store \"$2\"; bash -euc 'source \"$1/os-image-store\"; startos_reserve_image_store \"$2\"; printf nested' nested \"$1\" \"$2\"", "parent-image-lease"])
        .arg(scripts()).arg(root.path()).env_remove(variable());
    assert_eq!(
        command.invoke(ErrorKind::Filesystem).await.unwrap(),
        b"nested"
    );
    let lease = FileLock::new(root.path().join(lock_name()), false)
        .await
        .unwrap();
    lease.unlock().await.unwrap();
}
