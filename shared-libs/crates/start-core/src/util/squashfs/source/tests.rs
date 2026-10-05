use std::io::{Seek, SeekFrom, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use tokio::io::AsyncWriteExt;
use tokio_tar::{EntryType, Header};

use super::*;
use crate::s9pk::merkle_archive::source::FileSource;
use crate::util::io::{TmpDir, create_file};

pub(super) fn command(command: &mut Command) -> Vec<u8> {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{:?}: {}",
        command,
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

pub(super) fn append(tar: &mut Vec<u8>, path: &[u8], ty: u8, payload: &[u8], link: Option<&[u8]>) {
    let mut header = Header::new_gnu();
    header
        .set_path(Path::new(std::ffi::OsStr::from_bytes(path)))
        .unwrap();
    header.set_mode(0o640);
    header.set_uid(17);
    header.set_gid(23);
    header.set_mtime(42);
    header.set_size(payload.len() as u64);
    header.set_entry_type(EntryType::new(ty));
    if let Some(link) = link {
        header
            .set_link_name(Path::new(std::ffi::OsStr::from_bytes(link)))
            .unwrap();
    }
    header.set_cksum();
    tar.extend_from_slice(header.as_bytes());
    tar.extend_from_slice(payload);
    tar.resize(tar.len().next_multiple_of(512), 0);
}

pub(super) fn pax(records: &[(&[u8], &[u8])]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (key, value) in records {
        let mut len = key.len() + value.len() + 4;
        loop {
            let real = key.len() + value.len() + 3 + len.to_string().len();
            if real == len {
                break;
            }
            len = real;
        }
        bytes.extend_from_slice(format!("{len} ").as_bytes());
        bytes.extend_from_slice(key);
        bytes.push(b'=');
        bytes.extend_from_slice(value);
        bytes.push(b'\n');
    }
    bytes
}

pub(super) fn finish(tar: &mut Vec<u8>) {
    tar.resize(tar.len() + 1024, 0);
}

pub(super) async fn file_bytes<S: FileSource>(
    image: &Squashfs<S>,
    path: impl AsRef<Path>,
) -> Vec<u8> {
    let NodeContents::File(source) = &image
        .contents()
        .get_path(path)
        .unwrap()
        .as_node()
        .unwrap()
        .contents
    else {
        panic!("not a file")
    };
    source.to_vec(None).await.unwrap()
}

pub(super) async fn external<S: FileSource>(
    image: &Squashfs<S>,
    tmp: &TmpDir,
    name: &[u8],
) -> Vec<u8> {
    let path = tmp.join("image.sqfs");
    let mut file = create_file(&path).await.unwrap();
    image.serialize(&mut file).await.unwrap();
    file.flush().await.unwrap();
    command(
        Command::new("unsquashfs")
            .arg("-cat")
            .arg(path)
            .arg(std::ffi::OsStr::from_bytes(name)),
    )
}

#[tokio::test]
async fn directory_preserves_inodes_without_mutating_source() {
    let tmp = TmpDir::new().await.unwrap();
    let root = tmp.join("root");
    std::fs::create_dir(&root).unwrap();
    let raw = OsString::from_vec(b"file\xff".to_vec());
    std::fs::write(root.join(&raw), b"contents").unwrap();
    xattr::set(root.join(&raw), "user.binary", b"\0\n\xff").unwrap();
    std::fs::set_permissions(root.join(&raw), std::fs::Permissions::from_mode(0o4510)).unwrap();
    std::fs::hard_link(root.join(&raw), root.join("hard")).unwrap();
    std::os::unix::fs::symlink("/outside", root.join("link")).unwrap();
    let socket = std::os::unix::net::UnixListener::bind(root.join("socket")).unwrap();
    command(Command::new("mkfifo").arg(root.join("fifo")));
    let image = Squashfs::from_directory(&root).await.unwrap();
    assert_eq!(
        image.root_metadata().mode,
        std::fs::metadata(&root).unwrap().mode() as u16 & 0o7777
    );
    assert_eq!(image.contents().len(), 5);
    let node = image
        .contents()
        .iter()
        .find_map(|(_, e)| {
            e.as_node()
                .filter(|n| matches!(n.contents, NodeContents::File(_)))
        })
        .unwrap();
    assert_eq!(node.metadata.mode, 0o4510);
    assert_eq!(
        node.metadata
            .xattrs
            .get(std::ffi::OsStr::new("user.binary"))
            .unwrap(),
        b"\0\n\xff"
    );
    assert!(matches!(
        image
            .contents()
            .get("link")
            .unwrap()
            .as_node()
            .unwrap()
            .contents,
        NodeContents::Symlink(_)
    ));
    assert_eq!(external(&image, &tmp, raw.as_bytes()).await, b"contents");
    assert_eq!(
        std::fs::metadata(root.join(raw)).unwrap().mode() & 0o7777,
        0o4510
    );
    drop(socket);
}

#[tokio::test]
async fn gnu_sparse_versions_and_old_extensions() {
    for version in ["0.0", "0.1", "1.0", "old"] {
        let tmp = TmpDir::new().await.unwrap();
        let mut sparse = std::fs::File::create(tmp.join("sparse")).unwrap();
        for i in 0..32 {
            sparse.seek(SeekFrom::Start(i * 65536)).unwrap();
            sparse.write_all(b"payload").unwrap();
        }
        sparse.set_len(3 * 1024 * 1024).unwrap();
        let original = std::fs::read(tmp.join("sparse")).unwrap();
        let mut tar = Command::new("tar");
        tar.arg("--sparse");
        if version == "old" {
            tar.arg("--format=gnu");
        } else {
            tar.arg("--format=pax")
                .arg(format!("--sparse-version={version}"));
        }
        let bytes = command(tar.args(["-cf", "-", "-C"]).arg(&*tmp).arg("sparse"));
        let image = Squashfs::from_tar(bytes.as_slice()).await.unwrap();
        assert_eq!(file_bytes(&image, "sparse").await, original, "{version}");
        assert_eq!(
            external(&image, &tmp, b"sparse").await,
            original,
            "{version}"
        );
        let NodeContents::File(source) = &image
            .contents()
            .get("sparse")
            .unwrap()
            .as_node()
            .unwrap()
            .contents
        else {
            panic!()
        };
        let stat = std::fs::metadata(source.source().path().unwrap()).unwrap();
        assert!(
            stat.blocks() * 512 < stat.len() / 2,
            "spool lost holes for {version}"
        );
    }
}

#[tokio::test]
async fn pax_binary_names_globals_size_and_late_directories() {
    let mut tar = Vec::new();
    append(
        &mut tar,
        b"global",
        b'g',
        &pax(&[(b"uid", b"123"), (b"gid", b"456")]),
        None,
    );
    append(
        &mut tar,
        b"local",
        b'x',
        &pax(&[
            (b"path", b"dir/new\nname\xff"),
            (b"size", b"513"),
            (b"SCHILY.xattr.security.capability", b"\x01\x0a\0\xff"),
            (b"uid", b""),
        ]),
        None,
    );
    let start = tar.len();
    append(&mut tar, b"ignored", b'0', &[b'a'; 513], None);
    let mut header = Header::new_gnu();
    header
        .as_mut_bytes()
        .copy_from_slice(&tar[start..start + 512]);
    header.set_size(1);
    header.set_cksum();
    tar[start..start + 512].copy_from_slice(header.as_bytes());
    append(&mut tar, b"dir", b'5', b"", None);
    append(&mut tar, b".", b'5', b"", None);
    finish(&mut tar);
    let image = Squashfs::from_tar(tar.as_slice()).await.unwrap();
    let path = PathBuf::from(OsString::from_vec(b"dir/new\nname\xff".to_vec()));
    assert_eq!(file_bytes(&image, &path).await, vec![b'a'; 513]);
    let metadata = &image
        .contents()
        .get_path(path)
        .unwrap()
        .as_node()
        .unwrap()
        .metadata;
    assert_eq!((metadata.uid, metadata.gid), (17, 456));
    assert_eq!(
        metadata
            .xattrs
            .get(std::ffi::OsStr::new("security.capability"))
            .unwrap(),
        b"\x01\x0a\0\xff"
    );
    assert_eq!(image.root_metadata().uid, 123);
    assert_eq!(
        image
            .contents()
            .get("dir")
            .unwrap()
            .as_node()
            .unwrap()
            .metadata
            .mode,
        0o640
    );
}

#[tokio::test]
async fn forward_chained_and_cyclic_hardlinks() {
    let mut tar = Vec::new();
    append(&mut tar, b"first", b'1', b"", Some(b"second"));
    append(&mut tar, b"second", b'1', b"", Some(b"last"));
    append(&mut tar, b"last", b'0', b"data", None);
    finish(&mut tar);
    let tmp = TmpDir::new().await.unwrap();
    let image = Squashfs::from_tar(tar.as_slice()).await.unwrap();
    assert_eq!(external(&image, &tmp, b"first").await, b"data");
    for target in [b"first".as_slice(), b"absent"] {
        let mut tar = Vec::new();
        append(&mut tar, b"first", b'1', b"", Some(target));
        finish(&mut tar);
        let image = Squashfs::from_tar(tar.as_slice()).await.unwrap();
        assert!(
            image
                .serialize(&mut std::io::Cursor::new(Vec::new()))
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn unsafe_paths_truncation_dangling_and_sparse_maps() {
    for path in [b"../escape".as_slice(), b"/absolute", b"a/../../escape"] {
        let mut tar = Vec::new();
        append(&mut tar, b"pax", b'x', &pax(&[(b"path", path)]), None);
        append(&mut tar, b"file", b'0', b"", None);
        finish(&mut tar);
        assert!(Squashfs::from_tar(tar.as_slice()).await.is_err());
    }
    let mut tar = Vec::new();
    append(&mut tar, b"link", b'2', b"", Some(b"target"));
    append(&mut tar, b"link/child", b'0', b"data", None);
    finish(&mut tar);
    assert!(Squashfs::from_tar(tar.as_slice()).await.is_err());
    let mut tar = Vec::new();
    append(&mut tar, b"file", b'0', b"data", None);
    finish(&mut tar);
    for len in [1, 511, 514, 1023, 1024, 1535, 2047] {
        assert!(Squashfs::from_tar(&tar[..len]).await.is_err(), "{len}");
    }
    let mut tar = Vec::new();
    append(&mut tar, b"pax", b'x', b"", None);
    finish(&mut tar);
    assert!(Squashfs::from_tar(tar.as_slice()).await.is_err());
    for map in [b"0,2,1,2".as_slice(), b"0,5", b"0,1", b"0,2,3"] {
        let mut tar = Vec::new();
        append(
            &mut tar,
            b"pax",
            b'x',
            &pax(&[(b"GNU.sparse.size", b"4"), (b"GNU.sparse.map", map)]),
            None,
        );
        append(&mut tar, b"file", b'0', b"data", None);
        finish(&mut tar);
        assert!(Squashfs::from_tar(tar.as_slice()).await.is_err());
    }
}

#[tokio::test]
async fn imported_symlink_targets_fit_portable_kernel_pages() {
    for extension in [b'x', b'K'] {
        for size in [4095, 4096, 4097] {
            let target = vec![b'a'; size];
            let mut tar = Vec::new();
            let payload = if extension == b'x' {
                pax(&[(b"linkpath", &target)])
            } else {
                let mut payload = target.clone();
                payload.push(0);
                payload
            };
            append(&mut tar, b"extension", extension, &payload, None);
            append(&mut tar, b"link", b'2', b"", Some(b"fallback"));
            finish(&mut tar);
            let image = Squashfs::from_tar(tar.as_slice()).await.unwrap();
            let NodeContents::Symlink(imported) = &image
                .contents()
                .get("link")
                .unwrap()
                .as_node()
                .unwrap()
                .contents
            else {
                panic!("not a symlink")
            };
            assert_eq!(imported.as_os_str().as_bytes(), target);
            let mut output = std::io::Cursor::new(Vec::new());
            let result = image.serialize(&mut output).await;
            if size == 4095 {
                result.unwrap();
                let decoded =
                    Squashfs::deserialize(std::sync::Arc::<[u8]>::from(output.into_inner()))
                        .await
                        .unwrap();
                let NodeContents::Symlink(actual) = &decoded
                    .contents()
                    .get("link")
                    .unwrap()
                    .as_node()
                    .unwrap()
                    .contents
                else {
                    panic!("not a symlink")
                };
                assert_eq!(actual.as_os_str().as_bytes(), target);
            } else {
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("portable 4095-byte limit")
                );
            }
        }
    }
}

#[tokio::test]
async fn tar_builder_preserves_permissions_and_payload() {
    use futures::StreamExt;

    let tmp = TmpDir::new().await.unwrap();
    let root = tmp.join("root");
    std::fs::create_dir_all(root.join("dir")).unwrap();
    let name = OsString::from_vec(b"file\xff".to_vec());
    std::fs::write(root.join("dir").join(&name), b"contents").unwrap();
    std::fs::set_permissions(root.join("dir"), std::fs::Permissions::from_mode(0o3751)).unwrap();
    std::fs::set_permissions(
        root.join("dir").join(&name),
        std::fs::Permissions::from_mode(0o6751),
    )
    .unwrap();
    let mut builder = tokio_tar::Builder::new(Vec::new());
    builder.append_dir_all("assets", &root).await.unwrap();
    let tar = builder.into_inner().await.unwrap();
    let mut archive = tokio_tar::Archive::new(tar.as_slice());
    let mut entries = archive.entries().unwrap();
    let mut checked = 0;
    while let Some(entry) = entries.next().await {
        let entry = entry.unwrap();
        let path = entry.path().unwrap();
        let expected = if path == Path::new("assets/dir") {
            Some((0o040000, 0o3751))
        } else if path == Path::new("assets/dir").join(&name) {
            Some((0o100000, 0o6751))
        } else {
            None
        };
        if let Some((kind, mode)) = expected {
            assert_eq!(entry.header().mode().unwrap(), kind | mode);
            checked += 1;
        }
    }
    assert_eq!(checked, 2);
    let image = Squashfs::from_tar(tar.as_slice()).await.unwrap();
    let mut output = std::io::Cursor::new(Vec::new());
    image.serialize(&mut output).await.unwrap();
    let decoded = Squashfs::deserialize(std::sync::Arc::<[u8]>::from(output.into_inner()))
        .await
        .unwrap();
    for (path, mode) in [
        (PathBuf::from("assets/dir"), 0o3751),
        (Path::new("assets/dir").join(&name), 0o6751),
    ] {
        assert_eq!(
            decoded
                .contents()
                .get_path(path)
                .unwrap()
                .as_node()
                .unwrap()
                .metadata
                .mode,
            mode
        );
    }
    assert_eq!(
        file_bytes(&decoded, Path::new("assets/dir").join(name)).await,
        b"contents"
    );
}

#[tokio::test]
async fn programmatic_xattrs_remain_strict() {
    for name in [
        b"system.posix_acl_access".as_slice(),
        b"com.apple.quarantine",
        b"user.",
        b"user.bad\0name",
    ] {
        let mut image = Squashfs::<PathBuf>::new(Metadata::new(0o755), DirectoryContents::new());
        image
            .root_metadata_mut()
            .xattrs
            .insert(OsString::from_vec(name.to_vec()), b"value".to_vec());
        assert!(
            image
                .serialize(&mut std::io::Cursor::new(Vec::new()))
                .await
                .is_err()
        );
    }
}

#[test]
fn directory_xattr_errors_other_than_enotsup_propagate() {
    let tmp = tempfile::tempdir().unwrap();
    let stat = std::fs::metadata(tmp.path()).unwrap();
    let error = unix_metadata(&tmp.path().join("absent"), &stat).unwrap_err();
    assert_eq!(error.raw_os_error(), Some(libc::ENOENT));
}

#[tokio::test]
async fn pax_skips_unstorable_xattr_namespaces() {
    for unsupported in [
        b"SCHILY.xattr.system.posix_acl_access".as_slice(),
        b"LIBARCHIVE.xattr.system.posix_acl_access",
        b"SCHILY.xattr.com.apple.quarantine",
        b"LIBARCHIVE.xattr.com.apple.quarantine",
    ] {
        let mut tar = Vec::new();
        append(
            &mut tar,
            b"local",
            b'x',
            &pax(&[
                (unsupported, b"AgAAAA=="),
                (b"SCHILY.xattr.user.keep", b"value"),
                (b"LIBARCHIVE.xattr.user.%ff", b"AAo="),
            ]),
            None,
        );
        append(&mut tar, b"file", b'0', b"contents", None);
        finish(&mut tar);
        let image = Squashfs::from_tar(tar.as_slice()).await.unwrap();
        let mut output = std::io::Cursor::new(Vec::new());
        image.serialize(&mut output).await.unwrap();
        let decoded = Squashfs::deserialize(std::sync::Arc::<[u8]>::from(output.into_inner()))
            .await
            .unwrap();
        let node = decoded.contents().get("file").unwrap().as_node().unwrap();
        assert_eq!(node.metadata.mode, 0o640);
        assert_eq!(
            node.metadata.xattrs,
            BTreeMap::from([
                (OsString::from("user.keep"), b"value".to_vec()),
                (OsString::from_vec(b"user.\xff".to_vec()), b"\0\n".to_vec()),
            ])
        );
        assert_eq!(file_bytes(&decoded, "file").await, b"contents");
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn directory_skips_posix_acl_and_retains_user_xattrs() {
    let tmp = TmpDir::new().await.unwrap();
    let root = tmp.join("root");
    std::fs::create_dir(&root).unwrap();
    let path = root.join("file");
    std::fs::write(&path, b"contents").unwrap();
    xattr::set(&path, "user.keep", b"value").unwrap();
    let mut acl = 2u32.to_le_bytes().to_vec();
    for (tag, perm, id) in [
        (0x01u16, 7u16, u32::MAX),
        (0x02, 4, 65534),
        (0x04, 5, u32::MAX),
        (0x10, 5, u32::MAX),
        (0x20, 1, u32::MAX),
    ] {
        acl.extend_from_slice(&tag.to_le_bytes());
        acl.extend_from_slice(&perm.to_le_bytes());
        acl.extend_from_slice(&id.to_le_bytes());
    }
    xattr::set(&path, "system.posix_acl_access", &acl).unwrap();
    assert_eq!(
        xattr::get(&path, "system.posix_acl_access").unwrap(),
        Some(acl)
    );
    assert!(
        xattr::list(&path)
            .unwrap()
            .any(|name| name == "system.posix_acl_access")
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o6751)).unwrap();
    let image = Squashfs::from_directory(&root).await.unwrap();
    let mut output = std::io::Cursor::new(Vec::new());
    image.serialize(&mut output).await.unwrap();
    let decoded = Squashfs::deserialize(std::sync::Arc::<[u8]>::from(output.into_inner()))
        .await
        .unwrap();
    let node = decoded.contents().get("file").unwrap().as_node().unwrap();
    assert_eq!(node.metadata.mode, 0o6751);
    assert_eq!(
        node.metadata.xattrs,
        BTreeMap::from([(OsString::from("user.keep"), b"value".to_vec())])
    );
    assert_eq!(file_bytes(&decoded, "file").await, b"contents");
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn directory_skips_macos_quarantine_and_retains_user_xattrs() {
    let tmp = TmpDir::new().await.unwrap();
    let root = tmp.join("root");
    std::fs::create_dir(&root).unwrap();
    let path = root.join("file");
    std::fs::write(&path, b"contents").unwrap();
    xattr::set(&path, "user.keep", b"value").unwrap();
    xattr::set(&path, "com.apple.quarantine", b"0081;00000000;test;").unwrap();
    assert!(
        xattr::list(&path)
            .unwrap()
            .any(|name| name == "com.apple.quarantine")
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o751)).unwrap();
    let image = Squashfs::from_directory(&root).await.unwrap();
    let mut output = std::io::Cursor::new(Vec::new());
    image.serialize(&mut output).await.unwrap();
    let decoded = Squashfs::deserialize(std::sync::Arc::<[u8]>::from(output.into_inner()))
        .await
        .unwrap();
    let node = decoded.contents().get("file").unwrap().as_node().unwrap();
    assert_eq!(node.metadata.mode, 0o751);
    assert_eq!(
        node.metadata.xattrs,
        BTreeMap::from([(OsString::from("user.keep"), b"value".to_vec())])
    );
    assert_eq!(file_bytes(&decoded, "file").await, b"contents");
}

#[tokio::test]
async fn drains_large_transport_padding() {
    let (mut writer, reader) = tokio::io::duplex(1024);
    let mut tar = Vec::new();
    append(&mut tar, b"file", b'0', b"data", None);
    finish(&mut tar);
    tar.resize(tar.len() + 1024 * 1024, 0);
    let (image, _) = tokio::join!(Squashfs::from_tar(reader), async {
        writer.write_all(&tar).await.unwrap();
        writer.shutdown().await.unwrap();
    });
    assert_eq!(file_bytes(&image.unwrap(), "file").await, b"data");
}
