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
