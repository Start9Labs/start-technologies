use tokio::process::Command;

use super::*;
use crate::util::Invoke;

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
#[ignore = "requires an exclusive managed guest and real util-linux namespace privileges"]
async fn native_execution_root_excludes_parent_descriptors_but_preserves_standard_io() {
    use std::os::fd::{FromRawFd, OwnedFd};

    let file = std::fs::File::open("/dev/null").unwrap();
    let descriptor = nix::fcntl::fcntl(&file, nix::fcntl::FcntlArg::F_DUPFD(128)).unwrap();
    let _descriptor = unsafe { OwnedFd::from_raw_fd(descriptor) };
    let mut input = std::io::Cursor::new(b"standard I/O".as_slice());
    let bytes = super::super::namespace::command(Path::new("/"))
        .args(["/bin/sh", "-ec"])
        .arg(format!(
            "test ! -e /proc/self/fd/{descriptor}; /usr/bin/cat"
        ))
        .input(Some(&mut input))
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    assert_eq!(bytes, b"standard I/O");
}

#[tokio::test]
#[ignore = "requires an exclusive managed guest, current startbox and genuine pre-v2 source fixture"]
async fn native_public_source_worker_failure_releases_private_layers() {
    let root =
        PathBuf::from(std::env::var_os("FDE_SOURCE_STAGE_ROOT").expect("owned guest fixture"));
    let executable =
        PathBuf::from(std::env::var_os("FDE_SOURCE_STAGE_STARTBOX").expect("current startbox"));
    let image =
        PathBuf::from(std::env::var_os("FDE_SOURCE_IMAGE").expect("genuine public fixture"));
    let lease = crate::os_overlay::reserve_image_store(true).await.unwrap();
    let cache = SourceCache::new(&root);
    let workspace = cache.create_workspace().await.unwrap();
    tokio::fs::copy(&image, workspace.join("image.squashfs"))
        .await
        .unwrap();
    let mut input = std::io::Cursor::new(b"{}".as_slice());
    let result = source_command(&lease, &executable, &workspace)
        .unwrap()
        .input(Some(&mut input))
        .invoke(ErrorKind::Filesystem)
        .await;
    assert!(result.is_err());
    assert!(workspace.join("image.verity").is_file());
    for name in ["rootfs", "runtime", "source"] {
        assert!(
            !crate::disk::mount::util::is_mountpoint(workspace.join(name))
                .await
                .unwrap()
        );
    }
    cache.recover_workspace().await.unwrap();
    assert!(!workspace.exists());
}

#[tokio::test]
async fn public_source_command_retains_the_native_lease_in_private_namespaces() {
    let directory = tempfile::tempdir().unwrap();
    let lease = FileLock::new(directory.path().join("images.lock"), true)
        .await
        .unwrap();
    let command = source_command(&lease, Path::new("/usr/bin/startbox"), directory.path()).unwrap();
    let command = command.as_std();
    assert_eq!(command.get_program(), "/usr/bin/unshare");
    let arguments: Vec<_> = command.get_args().collect();
    for option in [
        "--mount",
        "--propagation=private",
        "--pid",
        "--fork",
        "--kill-child=SIGKILL",
        SOURCE_APPLET,
    ] {
        assert!(arguments.contains(&std::ffi::OsStr::new(option)));
    }
    assert_eq!(arguments.last().unwrap(), &directory.path().as_os_str());
    assert!(command.get_envs().any(|(key, value)| key
        == std::ffi::OsStr::new("STARTOS_IMAGE_LOCK_FD")
        && value.is_some()));
}
