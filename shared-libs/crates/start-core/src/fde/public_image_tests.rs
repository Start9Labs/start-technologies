use tokio::process::Command;

use super::*;
use crate::util::Invoke;

fn release() -> Release {
    Release {
        version: "0.4.3".parse().unwrap(),
        commit: "build-a".into(),
        platform: "x86_64".into(),
    }
}

#[tokio::test]
async fn source_release_metadata_is_confined_and_includes_commit_identity() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    let metadata = root.join("usr/lib/startos");
    tokio::fs::create_dir_all(&metadata).await.unwrap();
    for (name, bytes) in [
        ("VERSION.txt", "0.4.3\n"),
        ("GIT_HASH.txt", "build-a\n"),
        ("PLATFORM.txt", "x86_64\n"),
    ] {
        tokio::fs::write(metadata.join(name), bytes).await.unwrap();
    }
    assert_eq!(Release::read(&root).await.unwrap(), release());
    tokio::fs::write(metadata.join("GIT_HASH.txt"), "build-b")
        .await
        .unwrap();
    assert_ne!(Release::read(&root).await.unwrap(), release());
    tokio::fs::remove_file(metadata.join("GIT_HASH.txt"))
        .await
        .unwrap();
    tokio::fs::write(dir.path().join("outside"), "build-a")
        .await
        .unwrap();
    std::os::unix::fs::symlink(dir.path().join("outside"), metadata.join("GIT_HASH.txt")).unwrap();
    assert!(Release::read(&root).await.is_err());
}

#[tokio::test]
async fn native_public_bundle_contains_only_the_three_nonempty_roles() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let destination = dir.path().join("output");
    tokio::fs::create_dir_all(&source).await.unwrap();
    tokio::fs::create_dir_all(&destination).await.unwrap();
    for (name, bytes) in [
        ("linux", "kernel bytes"),
        ("initrd", "archive bytes"),
        ("os-release", "ID=startos\n"),
    ] {
        tokio::fs::write(source.join(name), bytes).await.unwrap();
    }
    let bundle = Command::new("tar")
        .args(["--format=posix", "-C"])
        .arg(&source)
        .args(["-cf", "-", "--", "linux", "initrd", "os-release"])
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    stage_bundle(&bundle, &destination).await.unwrap();
    assert_eq!(
        tokio::fs::read(destination.join("linux")).await.unwrap(),
        b"kernel bytes"
    );
    assert_eq!(
        tokio::fs::read(destination.join("initrd")).await.unwrap(),
        b"archive bytes"
    );
    assert_eq!(
        tokio::fs::read(destination.join("os-release"))
            .await
            .unwrap(),
        b"ID=startos\n"
    );
    assert!(stage_bundle(&bundle, &destination).await.is_err());
}

async fn archive(entries: &[(&str, &[u8], tokio_tar::EntryType)]) -> Vec<u8> {
    let mut builder = tokio_tar::Builder::new(Vec::new());
    for (path, bytes, kind) in entries {
        let mut header = tokio_tar::Header::new_gnu();
        header.set_entry_type(*kind);
        header.set_size(bytes.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        builder
            .append_data(&mut header, path, *bytes)
            .await
            .unwrap();
    }
    builder.into_inner().await.unwrap()
}

#[tokio::test]
async fn public_bundle_rejects_extra_duplicate_missing_empty_and_nonregular_members() {
    let regular = tokio_tar::EntryType::Regular;
    for entries in [
        vec![("foreign", b"bytes".as_slice(), regular)],
        vec![
            ("linux", b"first".as_slice(), regular),
            ("linux", b"second".as_slice(), regular),
        ],
        vec![("linux", b"only".as_slice(), regular)],
        vec![("linux", b"".as_slice(), regular)],
        vec![("linux", b"".as_slice(), tokio_tar::EntryType::Directory)],
    ] {
        let destination = tempfile::tempdir().unwrap();
        assert!(
            stage_bundle(&archive(&entries).await, destination.path())
                .await
                .is_err()
        );
        assert!(!destination.path().join("foreign").exists());
    }
}

#[tokio::test]
async fn public_bundle_checks_past_the_first_archive_terminator() {
    let regular = tokio_tar::EntryType::Regular;
    let complete = archive(&[
        ("linux", b"kernel", regular),
        ("initrd", b"archive", regular),
        ("os-release", b"release", regular),
    ])
    .await;
    let extra = archive(&[("foreign", b"not an input", regular)]).await;
    for suffix in [extra, b"trailing non-archive bytes".to_vec()] {
        let mut bytes = complete.clone();
        bytes.extend(suffix);
        let destination = tempfile::tempdir().unwrap();
        assert!(stage_bundle(&bytes, destination.path()).await.is_err());
        assert!(!destination.path().join("foreign").exists());
    }
}

#[tokio::test]
async fn public_workspace_cleanup_never_recurses_into_remaining_mount_targets() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("source");
    tokio::fs::create_dir(&target).await.unwrap();
    tokio::fs::write(target.join("retained"), b"keep")
        .await
        .unwrap();
    assert!(clean_workspace(directory.path()).await.is_err());
    assert_eq!(
        tokio::fs::read(target.join("retained")).await.unwrap(),
        b"keep"
    );
}
