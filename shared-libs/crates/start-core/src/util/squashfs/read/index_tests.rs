use super::tests::file;
use super::*;
use crate::prelude::ErrorKind;
use crate::util::Invoke;
use crate::util::io::TmpDir;

#[tokio::test]
async fn independent_large_directory_indexes_and_no_export_table() {
    let temp = Arc::new(TmpDir::new().await.unwrap());
    let root = temp.join("root");
    std::fs::create_dir(&root).unwrap();
    for i in 0..1800 {
        std::fs::write(root.join(format!("entry-{i:04}-{}", "n".repeat(90))), []).unwrap();
    }
    let path = temp.join("large.sqfs");
    tokio::process::Command::new("mksquashfs")
        .arg(&root)
        .arg(&path)
        .args([
            "-noappend",
            "-comp",
            "zstd",
            "-no-exports",
            "-no-xattrs",
            "-processors",
            "1",
            "-no-progress",
        ])
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    let bytes: Arc<[u8]> = Arc::from(std::fs::read(&path).unwrap());
    let image = Image::load(bytes.clone()).await.unwrap();
    assert!(image.exports.is_none());
    let parsed = image.inode(image.superblock.root_inode).await.unwrap();
    assert_eq!(parsed.header.kind, InodeKind::Directory.extended());
    let decoded = Squashfs::deserialize(bytes).await.unwrap();
    assert_eq!(decoded.contents().len(), 1800);
    assert_eq!(
        file(&decoded, &format!("entry-1799-{}", "n".repeat(90)))
            .size()
            .await
            .unwrap(),
        0
    );
    temp.gc().await.unwrap();
}

#[tokio::test]
async fn native_empty_root_and_nested_directories() {
    let mut image = Squashfs::<Arc<[u8]>>::new(Metadata::new(0o755), DirectoryContents::new());
    for nested in [false, true] {
        if nested {
            image
                .contents_mut()
                .insert_path(
                    "one/two/three",
                    Entry::directory(Metadata::new(0o700), DirectoryContents::new()),
                )
                .unwrap();
        }
        let mut output = std::io::Cursor::new(Vec::new());
        image.serialize(&mut output).await.unwrap();
        let decoded = Squashfs::deserialize(Arc::<[u8]>::from(output.into_inner()))
            .await
            .unwrap();
        if nested {
            assert!(
                decoded
                    .contents()
                    .get_path("one/two/three")
                    .unwrap()
                    .as_node()
                    .unwrap()
                    .contents
                    .as_directory()
                    .unwrap()
                    .is_empty()
            );
        } else {
            assert!(decoded.contents().is_empty());
        }
    }
}
