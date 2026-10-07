use std::io::Cursor;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use tokio::process::Command;

use super::*;
use crate::util::Invoke;
use crate::util::io::{TmpDir, create_file};

type Bytes = Arc<[u8]>;

fn file(mode: u16, bytes: &[u8]) -> Entry<Bytes> {
    Entry::file(Metadata::new(mode), Arc::from(bytes))
}

fn image(contents: DirectoryContents<Bytes>) -> Squashfs<Bytes> {
    Squashfs::new(Metadata::new(0o755), contents)
}

async fn extract(image: &Squashfs<Bytes>, temp: &Path) -> PathBuf {
    let path = temp.join("image.squashfs");
    image
        .serialize(&mut create_file(&path).await.unwrap())
        .await
        .unwrap();
    let dest = temp.join("extracted");
    Command::new("unsquashfs")
        .arg("-no-progress")
        .arg("-processors")
        .arg("1")
        .arg("-d")
        .arg(&dest)
        .arg(&path)
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    dest
}

#[tokio::test]
async fn symlink_targets_fit_portable_kernel_pages() {
    for size in [4095, 4096, 4097] {
        let target = "a".repeat(size);
        let mut contents = DirectoryContents::new();
        contents
            .insert(
                "link",
                Entry::new(
                    Metadata::new(0o777),
                    NodeContents::Symlink(target.clone().into()),
                ),
            )
            .unwrap();
        let image = image(contents);
        if size == 4095 {
            let temp = TmpDir::new().await.unwrap();
            let output = listing(&image, &temp).await;
            let (_, actual) = output
                .lines()
                .find_map(|line| line.split_once(" -> "))
                .unwrap();
            assert_eq!(actual, target);
        } else {
            let error = image
                .serialize(&mut Cursor::new(Vec::new()))
                .await
                .unwrap_err();
            assert!(error.to_string().contains("portable 4095-byte limit"));
        }
    }
}

async fn listing(image: &Squashfs<Bytes>, temp: &TmpDir) -> String {
    let path = temp.join("image.squashfs");
    image
        .serialize(&mut create_file(&path).await.unwrap())
        .await
        .unwrap();
    String::from_utf8(
        Command::new("unsquashfs")
            .arg("-lln")
            .arg(&path)
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap(),
    )
    .unwrap()
}

fn random_bytes(size: usize) -> Vec<u8> {
    let mut state = 123456789u64;
    (0..size)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

#[tokio::test]
async fn external_reader_extracts_every_data_shape_and_hardlinks() {
    let temp = TmpDir::new().await.unwrap();
    let mut contents = DirectoryContents::new();
    let mut expected = BTreeMap::new();
    for size in [0, 1, 8192, 131071, 131072, 131073, 262144, 300001] {
        let bytes = random_bytes(size);
        let name = format!("file-{size}");
        contents.insert(&name, file(0o640, &bytes)).unwrap();
        expected.insert(name, bytes);
    }
    contents
        .insert("alias", Entry::Hardlink("forward".into()))
        .unwrap();
    contents
        .insert("forward", Entry::Hardlink("file-131073".into()))
        .unwrap();
    contents
        .insert(
            "symlink",
            Entry::new(
                Metadata::new(0o777),
                NodeContents::Symlink("file-131073".into()),
            ),
        )
        .unwrap();
    let raw = std::ffi::OsString::from_vec(b"non-utf8\xff".to_vec());
    contents
        .insert(raw.clone(), file(0o755, b"raw filename"))
        .unwrap();
    let root = extract(&image(contents), &temp).await;
    for (name, bytes) in expected {
        assert_eq!(tokio::fs::read(root.join(&name)).await.unwrap(), bytes);
        assert_eq!(
            tokio::fs::metadata(root.join(&name)).await.unwrap().mode() & 0o7777,
            0o640
        );
    }
    assert_eq!(
        tokio::fs::read(root.join(raw)).await.unwrap(),
        b"raw filename"
    );
    let target = tokio::fs::metadata(root.join("file-131073")).await.unwrap();
    for name in ["alias", "forward"] {
        let alias = tokio::fs::metadata(root.join(name)).await.unwrap();
        assert_eq!(alias.ino(), target.ino());
        assert_eq!(alias.nlink(), 3);
    }
    assert_eq!(
        tokio::fs::read_link(root.join("symlink")).await.unwrap(),
        Path::new("file-131073")
    );
    temp.delete().await.unwrap();
}

#[tokio::test]
async fn external_reader_extracts_large_directories() {
    let temp = TmpDir::new().await.unwrap();
    let mut contents = DirectoryContents::new();
    for i in 0u32..1200 {
        let name = format!("subdir/{i:04}-{}", "x".repeat(240));
        contents
            .insert_path(name, file(0o644, &i.to_le_bytes()))
            .unwrap();
    }
    contents
        .insert_path(
            "empty",
            Entry::directory(Metadata::new(0o700), DirectoryContents::new()),
        )
        .unwrap();
    let root = extract(&image(contents), &temp).await;
    for i in [0u32, 255, 256, 1199] {
        let name = format!("subdir/{i:04}-{}", "x".repeat(240));
        assert_eq!(
            tokio::fs::read(root.join(name)).await.unwrap(),
            i.to_le_bytes()
        );
    }
    assert_eq!(
        tokio::fs::metadata(root.join("empty"))
            .await
            .unwrap()
            .mode()
            & 0o7777,
        0o700
    );
    temp.delete().await.unwrap();
}

#[tokio::test]
async fn external_reader_preserves_special_files_and_inode_attributes() {
    let temp = TmpDir::new().await.unwrap();
    let mut contents = DirectoryContents::<Bytes>::new();
    let mut metadata = Metadata::new(0o4751);
    metadata.uid = 123;
    metadata.gid = 456;
    metadata.modification_time = 946684800;
    contents
        .insert(
            "binary",
            Entry::file(metadata.clone(), Arc::from(&b"binary"[..])),
        )
        .unwrap();
    contents
        .insert(
            "char",
            Entry::new(
                metadata.clone(),
                NodeContents::CharacterDevice(Device { major: 1, minor: 3 }),
            ),
        )
        .unwrap();
    contents
        .insert(
            "block",
            Entry::new(
                metadata.clone(),
                NodeContents::BlockDevice(Device {
                    major: 314,
                    minor: 3,
                }),
            ),
        )
        .unwrap();
    contents
        .insert("fifo", Entry::new(metadata.clone(), NodeContents::Fifo))
        .unwrap();
    contents
        .insert("socket", Entry::new(metadata, NodeContents::Socket))
        .unwrap();
    let output = listing(&image(contents), &temp).await;
    assert!(output.contains("rwsr-x--x"), "{output}");
    assert!(output.contains("123/456"), "{output}");
    assert!(output.contains("2000-01-01"), "{output}");
    assert!(output.contains("1,  3"), "{output}");
    assert!(output.contains("314,  3"), "{output}");
    assert!(
        output
            .lines()
            .any(|line| line.starts_with('p') && line.ends_with("fifo")),
        "{output}"
    );
    assert!(
        output
            .lines()
            .any(|line| line.starts_with('s') && line.ends_with("socket")),
        "{output}"
    );
    temp.delete().await.unwrap();
}

#[tokio::test]
async fn sparse_blocks_and_deduplicated_files_are_independently_readable() {
    let temp = TmpDir::new().await.unwrap();
    let mut contents = DirectoryContents::new();
    contents
        .insert("zeros", file(0o644, &vec![0; 393216]))
        .unwrap();
    let bytes = random_bytes(262144);
    contents.insert("first", file(0o644, &bytes)).unwrap();
    contents.insert("second", file(0o755, &bytes)).unwrap();
    let mut image = image(contents);
    let mut dedup = Cursor::new(Vec::new());
    image.serialize(&mut dedup).await.unwrap();
    image.options.deduplicate = false;
    let mut repeated = Cursor::new(Vec::new());
    image.serialize(&mut repeated).await.unwrap();
    assert!(repeated.get_ref().len() >= dedup.get_ref().len() + bytes.len());
    let root = extract(&image, &temp).await;
    assert_eq!(
        tokio::fs::read(root.join("zeros")).await.unwrap(),
        vec![0; 393216]
    );
    assert!(
        tokio::fs::metadata(root.join("zeros"))
            .await
            .unwrap()
            .blocks()
            < 393216 / 512
    );
    for name in ["first", "second"] {
        assert_eq!(tokio::fs::read(root.join(name)).await.unwrap(), bytes);
    }
    assert_eq!(
        tokio::fs::metadata(root.join("second"))
            .await
            .unwrap()
            .mode()
            & 0o7777,
        0o755
    );
    temp.delete().await.unwrap();
}

#[tokio::test]
async fn rejects_unresolved_cyclic_and_directory_hardlinks() {
    for entries in [
        vec![("a", Entry::Hardlink("missing".into()))],
        vec![("a", Entry::Hardlink("a".into()))],
        vec![
            ("a", Entry::Hardlink("b".into())),
            ("b", Entry::Hardlink("a".into())),
        ],
        vec![
            ("a", Entry::Hardlink("dir".into())),
            (
                "dir",
                Entry::directory(Metadata::new(0o755), DirectoryContents::new()),
            ),
        ],
    ] {
        let mut contents = DirectoryContents::<Bytes>::new();
        for (name, entry) in entries {
            contents.insert(name, entry).unwrap();
        }
        assert!(
            image(contents)
                .serialize(&mut Cursor::new(Vec::new()))
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn embedded_image_offsets_and_padding_are_relative_to_image_start() {
    let image = image(DirectoryContents::<Bytes>::new());
    let mut output = Cursor::new(vec![0x53; 127]);
    output.set_position(127);
    image.serialize(&mut output).await.unwrap();
    let end = output.position();
    assert_eq!((end - 127) % 4096, 0);
    assert_eq!(&output.get_ref()[..127], &[0x53; 127]);
    let mut bytes = &output.get_ref()[127..];
    let header = Superblock::deserialize(&mut bytes).unwrap();
    header.validate().unwrap();
    assert!(header.bytes_used <= end - 127);
}

#[tokio::test]
async fn external_reader_preserves_binary_and_block_spanning_xattrs() {
    #[cfg(target_os = "linux")]
    let temp = tempfile::tempdir_in("/dev/shm").unwrap();
    #[cfg(target_os = "macos")]
    let temp = tempfile::tempdir().unwrap();
    let mut root_metadata = Metadata::new(0o755);
    root_metadata
        .xattrs
        .insert("user.root".into(), b"root\0value".to_vec());
    let large = random_bytes(40000);
    let mut contents = DirectoryContents::new();
    let mut metadata = Metadata::new(0o644);
    metadata
        .xattrs
        .insert("user.binary".into(), b"before\0after".to_vec());
    metadata.xattrs.insert("user.large".into(), large.clone());
    metadata.xattrs.insert("user.empty".into(), Vec::new());
    contents
        .insert("attributes", Entry::file(metadata, Arc::from(&b"data"[..])))
        .unwrap();
    for index in 0u32..600 {
        let mut metadata = Metadata::new(0o644);
        metadata
            .xattrs
            .insert("user.index".into(), index.to_le_bytes().to_vec());
        contents
            .insert(
                format!("entry-{index}"),
                Entry::file(metadata, Arc::from(&b"same data"[..])),
            )
            .unwrap();
    }
    let root = extract(&Squashfs::new(root_metadata, contents), temp.path()).await;
    assert_eq!(
        xattr::get(&root, "user.root").unwrap().unwrap(),
        b"root\0value"
    );
    let path = root.join("attributes");
    assert_eq!(
        xattr::get(&path, "user.binary").unwrap().unwrap(),
        b"before\0after"
    );
    assert_eq!(xattr::get(&path, "user.large").unwrap().unwrap(), large);
    assert_eq!(
        xattr::get(&path, "user.empty").unwrap().unwrap(),
        Vec::<u8>::new()
    );
    for index in [0u32, 511, 512, 599] {
        assert_eq!(
            xattr::get(root.join(format!("entry-{index}")), "user.index")
                .unwrap()
                .unwrap(),
            index.to_le_bytes()
        );
    }
    temp.close().unwrap();
}

struct FixtureSource {
    size: u64,
    bytes: [&'static [u8]; 2],
    opens: std::sync::atomic::AtomicUsize,
}
impl FileSource for FixtureSource {
    type Reader = Cursor<&'static [u8]>;
    type SliceReader = tokio::io::Take<Self::Reader>;
    async fn size(&self) -> Result<u64, Error> {
        Ok(self.size)
    }
    async fn reader(&self) -> Result<Self::Reader, Error> {
        let index = self
            .opens
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            .min(1);
        Ok(Cursor::new(self.bytes[index]))
    }
    async fn slice(&self, position: u64, size: u64) -> Result<Self::SliceReader, Error> {
        let mut reader = self.reader().await?;
        reader.set_position(position);
        Ok(reader.take(size))
    }
}

#[tokio::test]
async fn rejects_short_growing_and_mutating_sources() {
    for bytes in [
        [&b"tiny"[..], &b"tiny"[..]],
        [&b"oversized"[..], &b"oversized"[..]],
        [&b"first"[..], &b"other"[..]],
    ] {
        let source = FixtureSource {
            size: 5,
            bytes,
            opens: std::sync::atomic::AtomicUsize::new(0),
        };
        let mut contents = DirectoryContents::new();
        contents
            .insert("file", Entry::file(Metadata::new(0o644), source))
            .unwrap();
        let image = Squashfs::new(Metadata::new(0o755), contents);
        assert!(image.serialize(&mut Cursor::new(Vec::new())).await.is_err());
    }
}
