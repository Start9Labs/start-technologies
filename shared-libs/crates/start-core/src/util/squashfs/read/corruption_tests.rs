use super::*;

fn inode_position(sb: &Superblock, reference: u64) -> usize {
    (sb.inode_table + (reference >> 16) + 2 + (reference & 0xffff)) as usize
}

#[tokio::test]
async fn malformed_inode_fields_and_huge_block_count_fail_boundedly() {
    let (bytes, _) = super::tests::fixture(false, false).await;
    let image = Image::load(bytes.clone()).await.unwrap();
    let root = image.inode(image.superblock.root_inode).await.unwrap();
    let file_ref = root
        .children
        .iter()
        .find(|c| c.name == "file")
        .unwrap()
        .reference;
    let root_start = inode_position(&image.superblock, image.superblock.root_inode);
    let file_start = inode_position(&image.superblock, file_ref);
    assert_eq!(
        u16::from_le_bytes(bytes[file_start..file_start + 2].try_into().unwrap()),
        9
    );
    let mutations = [
        (root_start, 0u16.to_le_bytes().to_vec()),
        (root_start + 2, 0xffffu16.to_le_bytes().to_vec()),
        (root_start + 4, u16::MAX.to_le_bytes().to_vec()),
        (root_start + 12, 0u32.to_le_bytes().to_vec()),
        (
            root_start + InodeHeader::SIZE + 4,
            0u32.to_le_bytes().to_vec(),
        ),
        (
            root_start + InodeHeader::SIZE + 8,
            0u16.to_le_bytes().to_vec(),
        ),
        (
            file_start + InodeHeader::SIZE + 8,
            (i64::MAX as u64).to_le_bytes().to_vec(),
        ),
        (
            file_start + InodeHeader::SIZE + 24,
            0u32.to_le_bytes().to_vec(),
        ),
        (
            file_start + InodeHeader::SIZE + 28,
            (u32::MAX - 1).to_le_bytes().to_vec(),
        ),
        (
            file_start + InodeHeader::SIZE + 40,
            (UNCOMPRESSED_DATA << 1).to_le_bytes().to_vec(),
        ),
    ];
    for (position, replacement) in mutations {
        let mut corrupt = bytes.as_ref().to_vec();
        corrupt[position..position + replacement.len()].copy_from_slice(&replacement);
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            Squashfs::deserialize(Arc::<[u8]>::from(corrupt)),
        )
        .await
        .unwrap();
        assert!(result.is_err(), "accepted mutation at {position}");
    }
}

#[tokio::test]
async fn sparse_allocation_hint_is_not_an_exact_hole_count() {
    let (bytes, _) = super::tests::fixture(true, false).await;
    let image = Image::load(bytes.clone()).await.unwrap();
    let root = image.inode(image.superblock.root_inode).await.unwrap();
    let reference = root
        .children
        .iter()
        .find(|c| c.name == "sparse")
        .unwrap()
        .reference;
    let position = inode_position(&image.superblock, reference);
    assert_eq!(
        u16::from_le_bytes(bytes[position..position + 2].try_into().unwrap()),
        9
    );
    let size = 262145u64;
    for hint in [0, size - 1, size, size + 1] {
        let mut modified = bytes.as_ref().to_vec();
        let hint_position = position + InodeHeader::SIZE + 16;
        modified[hint_position..hint_position + 8].copy_from_slice(&hint.to_le_bytes());
        let result = Squashfs::deserialize(Arc::<[u8]>::from(modified)).await;
        if hint > size {
            assert!(result.is_err());
        } else {
            let decoded = result.unwrap();
            assert_eq!(
                super::tests::file(&decoded, "sparse")
                    .to_vec(None)
                    .await
                    .unwrap(),
                vec![0; size as usize]
            );
        }
    }
}

#[tokio::test]
async fn oversized_symlink_length_fails_without_preallocating_declared_length() {
    let (bytes, _) = super::tests::fixture(false, false).await;
    let image = Image::load(bytes.clone()).await.unwrap();
    let root = image.inode(image.superblock.root_inode).await.unwrap();
    let reference = root
        .children
        .iter()
        .find(|c| c.name == "symlink")
        .unwrap()
        .reference;
    let position = inode_position(&image.superblock, reference) + InodeHeader::SIZE + 4;
    let mut modified = bytes.as_ref().to_vec();
    modified[position..position + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            Squashfs::deserialize(Arc::<[u8]>::from(modified)),
        )
        .await
        .unwrap()
        .is_err()
    );
}

#[tokio::test]
async fn directory_self_cycle_and_export_identity_corruption_are_rejected() {
    let (bytes, _) = super::tests::fixture(false, false).await;
    let image = Image::load(bytes.clone()).await.unwrap();
    let root_ref = image.superblock.root_inode;
    let root = image.inode(root_ref).await.unwrap();
    let directory = root
        .children
        .iter()
        .find(|c| c.name == "directory")
        .unwrap();
    let directory_position = inode_position(&image.superblock, directory.reference);
    let mut corrupt = bytes.as_ref().to_vec();
    // Turn the child inode into the root's identity without modifying the export table.
    corrupt[directory_position + 12..directory_position + 16]
        .copy_from_slice(&root.header.number.to_le_bytes());
    assert!(
        Squashfs::deserialize(Arc::<[u8]>::from(corrupt))
            .await
            .is_err()
    );
    let root_position = inode_position(&image.superblock, root_ref);
    let body =
        BasicDirectoryInode::deserialize(&mut &bytes[root_position + InodeHeader::SIZE..]).unwrap();
    let mut corrupt = bytes.as_ref().to_vec();
    let group_position =
        (image.superblock.directory_table + u64::from(body.block) + 2 + u64::from(body.offset))
            as usize;
    let group = DirectoryHeader::deserialize(&mut &bytes[group_position..]).unwrap();
    let entry_position = group_position + DirectoryHeader::SIZE;
    corrupt[group_position + 4..group_position + 8]
        .copy_from_slice(&((root_ref >> 16) as u32).to_le_bytes());
    corrupt[group_position + 8..group_position + 12]
        .copy_from_slice(&root.header.number.to_le_bytes());
    corrupt[entry_position..entry_position + 2].copy_from_slice(&(root_ref as u16).to_le_bytes());
    corrupt[entry_position + 2..entry_position + 4].copy_from_slice(&0i16.to_le_bytes());
    corrupt[entry_position + 4..entry_position + 6].copy_from_slice(&1u16.to_le_bytes());
    assert!(group.count < 256);
    assert!(
        Squashfs::deserialize(Arc::<[u8]>::from(corrupt))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn compressed_file_and_fragment_corruption_is_deferred_to_reads() {
    for fragment in [false, true] {
        let (bytes, _) = super::tests::fixture(false, true).await;
        let image = Squashfs::deserialize(bytes.clone()).await.unwrap();
        let source = super::tests::file(&image, "file");
        let block = if fragment {
            source.blocks.last().unwrap()
        } else {
            &source.blocks[0]
        };
        assert_eq!(block.fragment, fragment);
        assert_eq!(block.encoded & UNCOMPRESSED_DATA, 0);
        let mut corrupt = bytes.as_ref().to_vec();
        corrupt[block.position as usize..block.position as usize + 4].fill(0xff);
        let decoded = Squashfs::deserialize(Arc::<[u8]>::from(corrupt))
            .await
            .unwrap();
        assert!(
            super::tests::file(&decoded, "file")
                .to_vec(None)
                .await
                .is_err()
        );
    }
}
