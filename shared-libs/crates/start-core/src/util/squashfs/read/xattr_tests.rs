use super::*;

#[tokio::test]
async fn independent_security_trusted_and_user_namespaces() {
    use crate::prelude::ErrorKind;
    use crate::util::Invoke;

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("file"), b"contents").unwrap();
    std::os::unix::fs::symlink("file", root.join("symlink")).unwrap();
    let path = temp.path().join("image.sqfs");
    tokio::process::Command::new("mksquashfs")
        .arg(&root)
        .arg(&path)
        .args([
            "-noappend",
            "-comp",
            "zstd",
            "-processors",
            "1",
            "-no-progress",
            "-xattrs-add",
            "user.binary=0x000aff",
            "-xattrs-add",
            "trusted.binary=0x000aff",
            "-xattrs-add",
            "security.binary=0x000aff",
        ])
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    let decoded = Squashfs::deserialize(Arc::<[u8]>::from(std::fs::read(path).unwrap()))
        .await
        .unwrap();
    for name in ["file", "symlink"] {
        let attrs = &decoded
            .contents()
            .get(name)
            .unwrap()
            .as_node()
            .unwrap()
            .metadata
            .xattrs;
        for namespace in ["trusted", "security"] {
            assert_eq!(
                attrs
                    .get(&OsString::from(format!("{namespace}.binary")))
                    .unwrap(),
                b"\0\n\xff"
            );
        }
        if name == "file" {
            assert_eq!(
                attrs.get(&OsString::from("user.binary")).unwrap(),
                b"\0\n\xff"
            );
        }
    }
}

#[tokio::test]
async fn independent_inline_and_out_of_line_xattrs_and_bad_references() {
    let (bytes, _) = super::tests::fixture(true, false).await;
    let image = Image::load(bytes.clone()).await.unwrap();
    let mut inline = false;
    let mut out_of_line = false;
    let mut reference_position = None;
    for id in &image.xattrs {
        let reader = image
            .metadata(
                image.xattr_start,
                image.xattr_end,
                &image.xattr_blocks,
                id.reference,
            )
            .await
            .unwrap();
        let mut reader = TrackingIO::new(0, reader);
        for _ in 0..id.count {
            let entry = disk::<XattrEntry>(&mut reader).await.unwrap();
            let mut name = vec![0; entry.name_size as usize];
            reader.read_exact(&mut name).await.unwrap();
            let size = u32_value(&mut reader).await.unwrap();
            if entry.kind & 0x100 == 0 {
                inline = true;
            } else {
                out_of_line = true;
                let logical = image.xattr_blocks[&(image.xattr_start + (id.reference >> 16))]
                    + (id.reference & 0xffff)
                    + reader.position();
                let (&physical, &base) = image
                    .xattr_blocks
                    .iter()
                    .filter(|(_, base)| **base <= logical)
                    .next_back()
                    .unwrap();
                let offset = (logical - base) as usize;
                if offset + 8 <= METADATA_SIZE {
                    reference_position = Some(physical as usize + 2 + offset);
                }
            }
            let mut skip = vec![0; size as usize];
            reader.read_exact(&mut skip).await.unwrap();
        }
    }
    assert!(inline && out_of_line);
    let position = reference_position.unwrap();
    let mut corrupt = bytes.as_ref().to_vec();
    corrupt[position..position + 8].fill(0xff);
    assert!(
        Squashfs::deserialize(Arc::<[u8]>::from(corrupt))
            .await
            .is_err()
    );
    let mut corrupt = bytes.as_ref().to_vec();
    // The first ID-table pointer is followed by uncompressed XattrId records.
    let pointer_position = image.superblock.xattr_id_table as usize + XattrTable::SIZE;
    let pointer = u64::from_le_bytes(
        bytes[pointer_position..pointer_position + 8]
            .try_into()
            .unwrap(),
    ) as usize;
    assert_ne!(
        u16::from_le_bytes(bytes[pointer..pointer + 2].try_into().unwrap()) & 0x8000,
        0
    );
    corrupt[pointer + 2 + 8..pointer + 2 + 12].copy_from_slice(&u32::MAX.to_le_bytes());
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        Squashfs::deserialize(Arc::<[u8]>::from(corrupt)),
    )
    .await
    .unwrap();
    assert!(result.is_err());
}
