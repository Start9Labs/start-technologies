use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::prelude::ErrorKind;
use crate::util::Invoke;

pub(super) async fn fixture(extended: bool, fragments: bool) -> (Arc<[u8]>, Vec<u8>) {
    #[cfg(target_os = "linux")]
    let temp = tempfile::tempdir_in("/dev/shm").unwrap();
    #[cfg(not(target_os = "linux"))]
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(root.join("directory")).unwrap();
    let bytes: Vec<_> = (0..200003).map(|i| (i * 17 % 251) as u8).collect();
    std::fs::write(root.join("file"), &bytes).unwrap();
    std::fs::write(root.join("empty"), []).unwrap();
    std::fs::write(root.join("small"), b"fragment data").unwrap();
    std::fs::write(root.join("sparse"), vec![0; 262145]).unwrap();
    let raw = OsString::from_vec(b"raw\xff".to_vec());
    std::fs::write(root.join(raw), b"raw data").unwrap();
    std::os::unix::fs::symlink(
        OsString::from_vec(b"file\xff".to_vec()),
        root.join("symlink"),
    )
    .unwrap();
    std::fs::hard_link(root.join("file"), root.join("hardlink")).unwrap();
    std::fs::hard_link(root.join("symlink"), root.join("symlink-hardlink")).unwrap();
    let fifo = root.join("fifo");
    tokio::process::Command::new("mkfifo")
        .arg(&fifo)
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    let socket = std::os::unix::net::UnixListener::bind(root.join("socket")).unwrap();
    if extended {
        for name in ["file", "small", "directory", "sparse", "empty"] {
            xattr::set(
                root.join(name),
                "user.test",
                if name == "file" {
                    &b"value"[..]
                } else {
                    name.as_bytes()
                },
            )
            .unwrap();
            xattr::set(root.join(name), "user.large", &vec![42; 10000]).unwrap();
        }
        xattr::set(&root, "user.root", b"root").unwrap();
        xattr::set(
            root.join("file"),
            OsString::from_vec(b"user.raw\xff".to_vec()),
            b"binary\0\xff",
        )
        .unwrap();
    }
    let path = temp.path().join("image.sqfs");
    let mut command = tokio::process::Command::new("mksquashfs");
    command.arg(&root).arg(&path).args([
        "-noappend",
        "-comp",
        "zstd",
        "-b",
        "131072",
        "-processors",
        "1",
        "-no-progress",
    ]);
    command.args([
        "-p",
        "block b 600 123 456 8 1",
        "-p",
        "character c 600 321 654 1 3",
    ]);
    if !extended {
        command.arg("-no-xattrs");
    }
    if !fragments {
        command.args(["-no-fragments", "-noI", "-noD", "-noF", "-noX"]);
    } else {
        command.arg("-always-use-fragments");
    }
    command.invoke(ErrorKind::Filesystem).await.unwrap();
    drop(socket);
    let image = Arc::from(std::fs::read(path).unwrap());
    drop(temp);
    (image, bytes)
}

pub(super) fn file<'a, S>(image: &'a Squashfs<S>, name: &str) -> &'a S {
    match &image
        .contents()
        .get(name)
        .unwrap()
        .as_node()
        .unwrap()
        .contents
    {
        NodeContents::File(file) => file,
        _ => panic!("not a file"),
    }
}

#[tokio::test]
async fn independent_basic_and_extended_images() {
    for extended in [false, true] {
        for fragments in [false, true] {
            let (bytes, expected) = fixture(extended, fragments).await;
            let image = Squashfs::deserialize(bytes).await.unwrap();
            // The lexicographically first hardlink owns the inode in the decoded tree.
            assert!(
                matches!(image.contents().get("hardlink").unwrap(), Entry::Hardlink(target) if target == &PathBuf::from("file"))
            );
            assert_eq!(file(&image, "file").to_vec(None).await.unwrap(), expected);
            assert_eq!(
                file(&image, "small").to_vec(None).await.unwrap(),
                b"fragment data"
            );
            assert!(file(&image, "empty").to_vec(None).await.unwrap().is_empty());
            assert_eq!(
                file(&image, "sparse").to_vec(None).await.unwrap(),
                vec![0; 262145]
            );
            assert!(
                image
                    .contents()
                    .get(OsString::from_vec(b"raw\xff".to_vec()))
                    .is_some()
            );
            assert!(
                matches!(&image.contents().get("symlink").unwrap().as_node().unwrap().contents, NodeContents::Symlink(target) if target.as_os_str().as_encoded_bytes() == b"file\xff")
            );
            assert!(matches!(
                &image
                    .contents()
                    .get("block")
                    .unwrap()
                    .as_node()
                    .unwrap()
                    .contents,
                NodeContents::BlockDevice(Device { major: 8, minor: 1 })
            ));
            assert!(matches!(
                &image
                    .contents()
                    .get("character")
                    .unwrap()
                    .as_node()
                    .unwrap()
                    .contents,
                NodeContents::CharacterDevice(Device { major: 1, minor: 3 })
            ));
            assert!(matches!(
                &image
                    .contents()
                    .get("fifo")
                    .unwrap()
                    .as_node()
                    .unwrap()
                    .contents,
                NodeContents::Fifo
            ));
            assert!(matches!(
                &image
                    .contents()
                    .get("socket")
                    .unwrap()
                    .as_node()
                    .unwrap()
                    .contents,
                NodeContents::Socket
            ));
            let attrs = &image
                .contents()
                .get("file")
                .unwrap()
                .as_node()
                .unwrap()
                .metadata
                .xattrs;
            if extended {
                assert_eq!(
                    attrs.get(&OsString::from("user.large")).unwrap(),
                    &vec![42; 10000]
                );
                assert_eq!(attrs.get(&OsString::from("user.test")).unwrap(), b"value");
                assert_eq!(
                    attrs
                        .get(&OsString::from_vec(b"user.raw\xff".to_vec()))
                        .unwrap(),
                    b"binary\0\xff"
                );
            } else {
                assert!(attrs.is_empty());
            }
        }
    }
}

#[tokio::test]
async fn lazy_slices_at_block_and_fragment_boundaries() {
    let (bytes, expected) = fixture(false, true).await;
    let image = Squashfs::deserialize(bytes).await.unwrap();
    let source = file(&image, "file");
    for (start, size) in [
        (0, 0),
        (0, 1),
        (131071, 3),
        (131072, 400),
        (199999, 100),
        (200003, 1),
        (u64::MAX, u64::MAX),
        (5, u64::MAX),
    ] {
        let mut reader = source.slice(start, size).await.unwrap();
        let mut actual = Vec::new();
        reader.read_to_end(&mut actual).await.unwrap();
        let start = (start.min(expected.len() as u64)) as usize;
        let end = start + size.min((expected.len() - start) as u64) as usize;
        assert_eq!(actual, expected[start..end]);
    }
}

struct CountingSource {
    bytes: Arc<[u8]>,
    data_reads: Arc<AtomicUsize>,
    data_end: u64,
    fail_data: bool,
}
impl ArchiveSource for CountingSource {
    type FetchReader = tokio::io::Take<Cursor<Arc<[u8]>>>;
    type FetchAllReader = Cursor<Arc<[u8]>>;
    async fn size(&self) -> Option<u64> {
        Some(self.bytes.len() as u64)
    }
    async fn fetch_all(&self) -> Result<Self::FetchAllReader, Error> {
        panic!("reader must not fetch the whole image")
    }
    async fn fetch(&self, position: u64, size: u64) -> Result<Self::FetchReader, Error> {
        if position >= 96 && position < self.data_end {
            self.data_reads.fetch_add(1, Ordering::SeqCst);
            if self.fail_data {
                return Err(invalid("injected data fetch failure").into());
            }
        }
        self.bytes.fetch(position, size).await
    }
}

#[tokio::test]
async fn contents_are_lazy_and_data_faults_propagate() {
    for fail_data in [false, true] {
        let (bytes, expected) = fixture(false, true).await;
        let sb = Superblock::deserialize(&mut bytes.as_ref()).unwrap();
        let reads = Arc::new(AtomicUsize::new(0));
        let image = Squashfs::deserialize(CountingSource {
            bytes,
            data_reads: reads.clone(),
            data_end: sb.inode_table,
            fail_data,
        })
        .await
        .unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), 0);
        let mut reader = file(&image, "file").reader().await.unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), 0);
        let mut data = vec![0; 10];
        let result = reader.read_exact(&mut data).await;
        if fail_data {
            assert!(result.is_err());
            assert!(reader.read_exact(&mut data).await.is_err());
        } else {
            result.unwrap();
            assert_eq!(data, expected[..10]);
        }
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn malformed_headers_tables_and_compressed_metadata_are_rejected() {
    let (bytes, _) = fixture(false, true).await;
    for (offset, replacement) in [
        (0, vec![0; 4]),
        (20, vec![1, 0]),
        (40, vec![0xff; 8]),
        (48, vec![0xff; 8]),
        (80, vec![0xff; 8]),
    ] {
        let mut corrupted = bytes.as_ref().to_vec();
        corrupted[offset..offset + replacement.len()].copy_from_slice(&replacement);
        assert!(
            Squashfs::deserialize(Arc::<[u8]>::from(corrupted))
                .await
                .is_err()
        );
    }
    let sb = Superblock::deserialize(&mut bytes.as_ref()).unwrap();
    for length in [0, 95, sb.bytes_used as usize - 1] {
        assert!(
            Squashfs::deserialize(CountingSource {
                bytes: Arc::from(&bytes[..length]),
                data_reads: Arc::new(AtomicUsize::new(0)),
                data_end: 96,
                fail_data: false
            })
            .await
            .is_err()
        );
    }
    let mut corrupted = bytes.as_ref().to_vec();
    corrupted[sb.inode_table as usize + 2..sb.inode_table as usize + 6].fill(0xff);
    assert!(
        Squashfs::deserialize(Arc::<[u8]>::from(corrupted))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn native_writer_roundtrip_extended_devices_and_xattrs() {
    let mut directory = DirectoryContents::new();
    directory
        .insert(
            "file",
            Entry::file(Metadata::new(0o640), Arc::<[u8]>::from(vec![7; 300001])),
        )
        .unwrap();
    for (name, contents) in [
        (
            "block",
            NodeContents::BlockDevice(Device {
                major: 4095,
                minor: 1048575,
            }),
        ),
        (
            "character",
            NodeContents::CharacterDevice(Device { major: 1, minor: 3 }),
        ),
        (
            "symlink",
            NodeContents::Symlink(PathBuf::from(OsString::from_vec(b"raw\xff".to_vec()))),
        ),
        ("fifo", NodeContents::Fifo),
        ("socket", NodeContents::Socket),
    ] {
        let mut metadata = Metadata::new(0o600);
        metadata.uid = 10001;
        metadata.gid = 10002;
        metadata
            .xattrs
            .insert(OsString::from("user.key"), b"data".to_vec());
        directory
            .insert(name, Entry::new(metadata, contents))
            .unwrap();
    }
    let source = Squashfs::new(Metadata::new(0o755), directory);
    let mut output = Cursor::new(Vec::new());
    source.serialize(&mut output).await.unwrap();
    let decoded = Squashfs::deserialize(Arc::<[u8]>::from(output.into_inner()))
        .await
        .unwrap();
    assert_eq!(
        file(&decoded, "file").to_vec(None).await.unwrap(),
        vec![7; 300001]
    );
    for name in ["block", "character", "symlink", "fifo", "socket"] {
        let node = decoded.contents().get(name).unwrap().as_node().unwrap();
        assert_eq!(node.metadata.uid, 10001);
        assert_eq!(node.metadata.gid, 10002);
        assert_eq!(
            node.metadata
                .xattrs
                .get(&OsString::from("user.key"))
                .unwrap(),
            b"data"
        );
    }
}
