use std::os::unix::ffi::OsStrExt;
use std::process::Command;

use super::tests::{append, command, external, file_bytes, finish, pax};
use super::*;
use crate::util::io::TmpDir;

#[tokio::test]
async fn gnu_and_ustar_preserve_raw_paths_and_root_metadata() {
    for format in ["gnu", "ustar", "pax"] {
        let tmp = TmpDir::new().await.unwrap();
        let root = tmp.join("root");
        std::fs::create_dir(&root).unwrap();
        let name = OsString::from_vec(b"raw\xff".to_vec());
        std::fs::write(root.join(&name), b"raw bytes").unwrap();
        std::os::unix::fs::symlink(&name, root.join("symlink")).unwrap();
        let bytes = command(
            Command::new("tar")
                .arg(format!("--format={format}"))
                .args([
                    "--owner=12345",
                    "--group=54321",
                    "--mtime=@123",
                    "-cf",
                    "-",
                    "-C",
                ])
                .arg(&root)
                .arg("."),
        );
        let image = Squashfs::from_tar(bytes.as_slice()).await.unwrap();
        assert_eq!(
            (
                image.root_metadata().uid,
                image.root_metadata().gid,
                image.root_metadata().modification_time
            ),
            (12345, 54321, 123)
        );
        assert_eq!(file_bytes(&image, Path::new(&name)).await, b"raw bytes");
        let node = image.contents().get("symlink").unwrap().as_node().unwrap();
        let NodeContents::Symlink(target) = &node.contents else {
            panic!()
        };
        assert_eq!(target.as_os_str(), &name);
        assert_eq!(external(&image, &tmp, name.as_bytes()).await, b"raw bytes");
    }
}

#[tokio::test]
async fn gnu_long_names_and_links() {
    let tmp = TmpDir::new().await.unwrap();
    let name = "n".repeat(220);
    std::fs::write(tmp.join(&name), b"long").unwrap();
    std::os::unix::fs::symlink(&name, tmp.join("link")).unwrap();
    let bytes = command(
        Command::new("tar")
            .args(["--format=gnu", "-cf", "-", "-C"])
            .arg(&*tmp)
            .args([name.as_str(), "link"]),
    );
    let image = Squashfs::from_tar(bytes.as_slice()).await.unwrap();
    assert_eq!(file_bytes(&image, &name).await, b"long");
    let NodeContents::Symlink(target) = &image
        .contents()
        .get("link")
        .unwrap()
        .as_node()
        .unwrap()
        .contents
    else {
        panic!()
    };
    assert_eq!(target.as_os_str(), name.as_str());
    assert_eq!(external(&image, &tmp, name.as_bytes()).await, b"long");
}

#[tokio::test]
async fn devices_fifo_numeric_limits_checksum_and_xattr_encoding() {
    let mut tar = Vec::new();
    append(
        &mut tar,
        b"pax",
        b'x',
        &pax(&[
            (b"SCHILY.devmajor", b"259"),
            (b"SCHILY.devminor", b"1048575"),
            (b"LIBARCHIVE.xattr.user.%ff", b"AAo="),
        ]),
        None,
    );
    append(&mut tar, b"device", b'3', b"", None);
    append(&mut tar, b"fifo", b'6', b"", None);
    finish(&mut tar);
    let image = Squashfs::from_tar(tar.as_slice()).await.unwrap();
    let node = image.contents().get("device").unwrap().as_node().unwrap();
    assert!(matches!(
        node.contents,
        NodeContents::CharacterDevice(Device {
            major: 259,
            minor: 1048575
        })
    ));
    assert_eq!(
        node.metadata
            .xattrs
            .get(&OsString::from_vec(b"user.\xff".to_vec()))
            .unwrap(),
        b"\0\n"
    );
    assert!(matches!(
        image
            .contents()
            .get("fifo")
            .unwrap()
            .as_node()
            .unwrap()
            .contents,
        NodeContents::Fifo
    ));
    for (key, value) in [
        (b"uid".as_slice(), b"4294967296".as_slice()),
        (b"gid", b"-1"),
        (b"mtime", b"4294967296"),
        (b"mtime", b"-0.1"),
        (b"path", b"x\0y"),
    ] {
        let mut tar = Vec::new();
        append(&mut tar, b"pax", b'x', &pax(&[(key, value)]), None);
        append(&mut tar, b"file", b'0', b"", None);
        finish(&mut tar);
        assert!(Squashfs::from_tar(tar.as_slice()).await.is_err());
    }
    tar[0] ^= 1;
    assert!(Squashfs::from_tar(tar.as_slice()).await.is_err());
}

#[tokio::test]
async fn metadata_limits_reject_declared_sizes_before_payload_reads() {
    let mut header = tokio_tar::Header::new_gnu();
    header.set_path("extension").unwrap();
    header.set_entry_type(tokio_tar::EntryType::new(b'x'));
    header.set_size(16 * 1024 * 1024 + 1);
    header.set_cksum();
    let error = Squashfs::from_tar(header.as_bytes().as_slice())
        .await
        .err()
        .unwrap();
    assert!(error.to_string().contains("bounded metadata limit"));

    let mut tar = Vec::new();
    append(
        &mut tar,
        b"pax",
        b'x',
        &pax(&[
            (b"GNU.sparse.major", b"1"),
            (b"GNU.sparse.minor", b"0"),
            (b"GNU.sparse.realsize", b"1"),
        ]),
        None,
    );
    append(&mut tar, b"file", b'0', b"1000001\n", None);
    finish(&mut tar);
    let error = Squashfs::from_tar(tar.as_slice()).await.err().unwrap();
    assert!(error.to_string().contains("too many sparse extents"));
}

#[tokio::test]
async fn transport_errors_during_payload_and_after_end_marker_propagate() {
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};

    struct Fault;
    impl AsyncRead for Fault {
        fn poll_read(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            _: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            Poll::Ready(Err(std::io::Error::other("injected transport failure")))
        }
    }
    let mut tar = Vec::new();
    append(&mut tar, b"file", b'0', b"data", None);
    finish(&mut tar);
    for length in [513, tar.len()] {
        let reader = std::io::Cursor::new(tar[..length].to_vec()).chain(Fault);
        let error = Squashfs::from_tar(reader).await.err().unwrap();
        assert!(error.to_string().contains("injected transport failure"));
    }
}

#[tokio::test]
async fn real_gnu_pax_preserves_binary_xattrs() {
    let tmp = TmpDir::new().await.unwrap();
    std::fs::write(tmp.join("file"), b"data").unwrap();
    xattr::set(tmp.join("file"), "user.binary", b"\0\n\xff=end\n").unwrap();
    let bytes = command(
        Command::new("tar")
            .args([
                "--format=pax",
                "--xattrs",
                "--xattrs-include=*",
                "-cf",
                "-",
                "-C",
            ])
            .arg(&*tmp)
            .arg("file"),
    );
    let image = Squashfs::from_tar(bytes.as_slice()).await.unwrap();
    let attrs = &image
        .contents()
        .get("file")
        .unwrap()
        .as_node()
        .unwrap()
        .metadata
        .xattrs;
    assert_eq!(
        attrs.get(&OsString::from("user.binary")).unwrap(),
        b"\0\n\xff=end\n"
    );
    assert_eq!(external(&image, &tmp, b"file").await, b"data");
}

#[tokio::test]
async fn global_removal_and_replacement_persist() {
    let mut tar = Vec::new();
    append(&mut tar, b"global", b'g', &pax(&[(b"uid", b"99")]), None);
    append(&mut tar, b"one", b'0', b"1", None);
    append(&mut tar, b"global", b'g', &pax(&[(b"uid", b"")]), None);
    append(&mut tar, b"two", b'0', b"2", None);
    finish(&mut tar);
    let image = Squashfs::from_tar(tar.as_slice()).await.unwrap();
    assert_eq!(
        image
            .contents()
            .get("one")
            .unwrap()
            .as_node()
            .unwrap()
            .metadata
            .uid,
        99
    );
    assert_eq!(
        image
            .contents()
            .get("two")
            .unwrap()
            .as_node()
            .unwrap()
            .metadata
            .uid,
        17
    );
}
