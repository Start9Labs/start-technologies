use super::*;
use crate::util::squashfs::test_support::append;

fn finish(mut bytes: Vec<u8>) -> Vec<u8> {
    bytes.extend_from_slice(&[0; 1024]);
    bytes
}

async fn bytes(image: &Squashfs<DynFileSource>, path: &str) -> Vec<u8> {
    let node = resolve_hardlink(Path::new(path), |target| {
        image.contents().get_path(target).map(|entry| match entry {
            Entry::Node(node) => Ok(node),
            Entry::Hardlink(target) => Err(target.clone()),
        })
    })
    .unwrap();
    let NodeContents::File(source) = &node.contents else {
        panic!("not a file")
    };
    source.to_vec(None).await.unwrap()
}

#[tokio::test]
async fn replacing_and_whiteouting_owners_preserves_surviving_hardlink_inodes() {
    let mut lower = Vec::new();
    append(&mut lower, b"a", b'0', b"old", None);
    append(&mut lower, b"b", b'1', b"", Some(b"a"));
    let mut upper = Vec::new();
    append(&mut upper, b"c", b'1', b"", Some(b"a"));
    append(&mut upper, b"a", b'0', b"new", None);
    append(&mut upper, b"d", b'1', b"", Some(b"a"));
    let mut filesystem = LayerFilesystem::new();
    filesystem
        .apply_tar(finish(lower).as_slice())
        .await
        .unwrap();
    filesystem
        .apply_tar(finish(upper).as_slice())
        .await
        .unwrap();
    let image = filesystem.into_squashfs().unwrap();
    assert_eq!(bytes(&image, "a").await, b"new");
    assert_eq!(bytes(&image, "b").await, b"old");
    assert_eq!(bytes(&image, "c").await, b"old");
    assert_eq!(bytes(&image, "d").await, b"new");
    assert!(
        matches!(image.contents().get("c"), Some(Entry::Hardlink(path)) if path == Path::new("b"))
    );
    assert!(
        matches!(image.contents().get("d"), Some(Entry::Hardlink(path)) if path == Path::new("a"))
    );

    let mut lower = Vec::new();
    append(&mut lower, b"a", b'0', b"retained", None);
    append(&mut lower, b"b", b'1', b"", Some(b"a"));
    let mut upper = Vec::new();
    append(&mut upper, b".wh.a", b'0', b"", None);
    let mut filesystem = LayerFilesystem::new();
    filesystem
        .apply_tar(finish(lower).as_slice())
        .await
        .unwrap();
    filesystem
        .apply_tar(finish(upper).as_slice())
        .await
        .unwrap();
    let image = filesystem.into_squashfs().unwrap();
    assert!(image.contents().get("a").is_none());
    assert_eq!(bytes(&image, "b").await, b"retained");
}

#[tokio::test]
async fn whiteouts_and_opaque_directories_affect_only_lower_layer_entries() {
    let mut lower = Vec::new();
    append(&mut lower, b"dir", b'5', b"", None);
    append(&mut lower, b"dir/old/nested", b'0', b"removed", None);
    append(&mut lower, b"remove/child", b'0', b"removed", None);
    append(&mut lower, b"replace", b'0', b"old", None);
    append(&mut lower, b"outside", b'0', b"kept", None);
    append(
        &mut lower,
        b"dir-other/keep",
        b'0',
        b"adjacent sibling",
        None,
    );
    append(
        &mut lower,
        b"remove-other/keep",
        b'0',
        b"adjacent sibling",
        None,
    );
    let mut expected = None;
    for reverse in [false, true] {
        let mut entries = [
            (b"dir/new".as_slice(), b"new".as_slice()),
            (b"replace".as_slice(), b"replacement".as_slice()),
            (b"dir/.wh..wh..opq".as_slice(), b"".as_slice()),
            (b".wh.remove".as_slice(), b"".as_slice()),
            (b".wh.replace".as_slice(), b"".as_slice()),
        ];
        if reverse {
            entries.reverse();
        }
        let mut upper = Vec::new();
        for (path, data) in entries {
            append(&mut upper, path, b'0', data, None);
        }
        let mut filesystem = LayerFilesystem::new();
        filesystem
            .apply_tar(finish(lower.clone()).as_slice())
            .await
            .unwrap();
        filesystem
            .apply_tar(finish(upper).as_slice())
            .await
            .unwrap();
        let image = filesystem.into_squashfs().unwrap();
        assert!(image.contents().get_path("dir/old").is_none());
        assert!(image.contents().get("remove").is_none());
        assert_eq!(bytes(&image, "dir/new").await, b"new");
        assert_eq!(bytes(&image, "replace").await, b"replacement");
        assert_eq!(bytes(&image, "outside").await, b"kept");
        assert_eq!(bytes(&image, "dir-other/keep").await, b"adjacent sibling");
        assert_eq!(
            bytes(&image, "remove-other/keep").await,
            b"adjacent sibling"
        );
        assert_eq!(
            image
                .contents()
                .get("dir")
                .unwrap()
                .as_node()
                .unwrap()
                .metadata
                .modification_time,
            42
        );
        let mut output = std::io::Cursor::new(Vec::new());
        image.serialize(&mut output).await.unwrap();
        match &expected {
            Some(expected) => assert_eq!(&output.into_inner(), expected),
            None => expected = Some(output.into_inner()),
        }
    }
}

#[tokio::test]
async fn replacements_forward_hardlinks_and_implicit_parents_preserve_metadata() {
    let mut lower = Vec::new();
    append(&mut lower, b"dir/child", b'0', b"removed", None);
    append(&mut lower, b"file", b'0', b"removed", None);
    let mut upper = Vec::new();
    append(&mut upper, b"dir", b'0', b"now a file", None);
    append(&mut upper, b"file", b'5', b"", None);
    append(&mut upper, b"file/child", b'0', b"now a directory", None);
    append(&mut upper, b"forward", b'1', b"", Some(b"future"));
    append(&mut upper, b"future", b'0', b"future data", None);
    append(&mut upper, b"empty/.wh.absent", b'0', b"", None);
    append(&mut upper, b"opaque/.wh..wh..opq", b'0', b"", None);
    let mut filesystem = LayerFilesystem::new();
    filesystem
        .apply_tar(finish(lower).as_slice())
        .await
        .unwrap();
    filesystem
        .apply_tar(finish(upper).as_slice())
        .await
        .unwrap();
    let image = filesystem.into_squashfs().unwrap();
    assert_eq!(bytes(&image, "dir").await, b"now a file");
    assert_eq!(bytes(&image, "file/child").await, b"now a directory");
    assert_eq!(bytes(&image, "forward").await, b"future data");
    assert_eq!(
        image
            .contents()
            .get("empty")
            .unwrap()
            .as_node()
            .unwrap()
            .contents
            .as_directory()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        image
            .contents()
            .get("opaque")
            .unwrap()
            .as_node()
            .unwrap()
            .contents
            .as_directory()
            .unwrap()
            .len(),
        0
    );
    let metadata = &image
        .contents()
        .get("file")
        .unwrap()
        .as_node()
        .unwrap()
        .metadata;
    assert_eq!(
        (
            metadata.mode,
            metadata.uid,
            metadata.gid,
            metadata.modification_time
        ),
        (0o640, 17, 23, 42)
    );
}

#[tokio::test]
async fn nonregular_hardlinks_keep_inode_types_when_targets_change_or_disappear() {
    let mut lower = Vec::new();
    append(&mut lower, b"link", b'2', b"", Some(b"original target"));
    append(&mut lower, b"link-alias", b'1', b"", Some(b"link"));
    append(&mut lower, b"device", b'3', b"", None);
    append(&mut lower, b"device-alias", b'1', b"", Some(b"device"));
    append(&mut lower, b"fifo", b'6', b"", None);
    append(&mut lower, b"fifo-alias", b'1', b"", Some(b"fifo"));
    let mut upper = Vec::new();
    append(&mut upper, b"link", b'2', b"", Some(b"replacement target"));
    append(&mut upper, b".wh.device", b'0', b"", None);
    let mut filesystem = LayerFilesystem::new();
    filesystem
        .apply_tar(finish(lower).as_slice())
        .await
        .unwrap();
    filesystem
        .apply_tar(finish(upper).as_slice())
        .await
        .unwrap();
    let image = filesystem.into_squashfs().unwrap();
    assert!(
        matches!(&image.contents().get("link").unwrap().as_node().unwrap().contents,
        NodeContents::Symlink(target) if target == Path::new("replacement target"))
    );
    assert!(
        matches!(&image.contents().get("link-alias").unwrap().as_node().unwrap().contents,
        NodeContents::Symlink(target) if target == Path::new("original target"))
    );
    assert!(matches!(
        image
            .contents()
            .get("device-alias")
            .unwrap()
            .as_node()
            .unwrap()
            .contents,
        NodeContents::CharacterDevice(_)
    ));
    assert!(
        matches!(image.contents().get("fifo-alias"), Some(Entry::Hardlink(target)) if target == Path::new("fifo"))
    );
    let mut output = std::io::Cursor::new(Vec::new());
    image.serialize(&mut output).await.unwrap();
    let image = Squashfs::deserialize(Arc::<[u8]>::from(output.into_inner()))
        .await
        .unwrap();
    assert!(image.contents().get("device").is_none());
    assert!(matches!(
        image
            .contents()
            .get("device-alias")
            .unwrap()
            .as_node()
            .unwrap()
            .contents,
        NodeContents::CharacterDevice(crate::util::squashfs::Device { major: 0, minor: 0 })
    ));
}

#[tokio::test]
async fn rejects_duplicate_paths_nonempty_whiteouts_and_invalid_hardlinks() {
    for entries in [
        vec![
            ("duplicate", b'0', "a", None),
            ("duplicate", b'0', "b", None),
        ],
        vec![(".wh.file", b'0', "not empty", None)],
        vec![(".wh.file", b'5', "", None)],
        vec![("missing", b'1', "", Some("absent"))],
        vec![("a", b'1', "", Some("b")), ("b", b'1', "", Some("a"))],
        vec![
            ("directory", b'5', "", None),
            ("alias", b'1', "", Some("directory")),
        ],
    ] {
        let mut layer = Vec::new();
        for (path, ty, data, link) in entries {
            append(
                &mut layer,
                path.as_bytes(),
                ty,
                data.as_bytes(),
                link.map(str::as_bytes),
            );
        }
        assert!(
            LayerFilesystem::new()
                .apply_tar(finish(layer).as_slice())
                .await
                .is_err()
        );
    }
}

fn snapshot_file(data: &'static [u8]) -> Entry<Arc<[u8]>> {
    Entry::file(crate::util::squashfs::Metadata::new(0o640), Arc::from(data))
}

fn whiteout() -> Entry<Arc<[u8]>> {
    Entry::new(
        crate::util::squashfs::Metadata::new(0o600),
        NodeContents::CharacterDevice(crate::util::squashfs::Device { major: 0, minor: 0 }),
    )
}

fn directory_marker(value: &[u8]) -> Entry<Arc<[u8]>> {
    let mut metadata = crate::util::squashfs::Metadata::new(0o750);
    metadata.uid = 123;
    metadata.gid = 456;
    metadata
        .xattrs
        .insert("trusted.overlay.opaque".into(), value.to_vec());
    Entry::directory(metadata, crate::util::squashfs::DirectoryContents::new())
}

#[tokio::test]
async fn overlayfs_snapshot_preserves_root_policy_and_applies_working_changes() {
    let mut lower = Squashfs::<Arc<[u8]>>::default();
    for (path, entry) in [
        ("root-hidden", whiteout()),
        ("opaque", directory_marker(b"y")),
        ("opaque/retained", snapshot_file(b"retained")),
        ("override", snapshot_file(b"override")),
        ("owner", snapshot_file(b"old inode")),
        ("alias", Entry::Hardlink("owner".into())),
        ("modified", snapshot_file(b"old")),
        ("recreated/old", snapshot_file(b"old child")),
    ] {
        lower.contents_mut().insert_path(path, entry).unwrap();
    }
    let mut upper = Squashfs::<Arc<[u8]>>::default();
    let mut copied = directory_marker(b"x");
    let metadata = &mut copied.as_node_mut().unwrap().metadata;
    metadata.mode = 0o700;
    metadata.xattrs.insert(
        "trusted.overlay.origin".into(),
        b"copyup internals".to_vec(),
    );
    metadata
        .xattrs
        .insert("user.config".into(), b"retained xattr".to_vec());
    for (path, entry) in [
        ("override", whiteout()),
        ("owner", whiteout()),
        ("opaque", copied),
        ("opaque/added", snapshot_file(b"added")),
        ("modified", snapshot_file(b"new")),
        ("recreated", directory_marker(b"y")),
        ("recreated/new", snapshot_file(b"new child")),
        ("z-new", snapshot_file(b"new inode")),
        ("a-new", Entry::Hardlink("z-new".into())),
    ] {
        upper.contents_mut().insert_path(path, entry).unwrap();
    }
    let mut lower_before = std::io::Cursor::new(Vec::new());
    lower.serialize(&mut lower_before).await.unwrap();
    let mut upper_before = std::io::Cursor::new(Vec::new());
    upper.serialize(&mut upper_before).await.unwrap();
    let image = lower.clone().with_overlayfs_upper(upper.clone()).unwrap();
    assert!(matches!(
        image
            .contents()
            .get("root-hidden")
            .unwrap()
            .as_node()
            .unwrap()
            .contents,
        NodeContents::CharacterDevice(_)
    ));
    assert!(image.contents().get("override").is_none());
    assert!(image.contents().get("owner").is_none());
    assert_eq!(bytes(&image, "alias").await, b"old inode");
    assert_eq!(bytes(&image, "opaque/retained").await, b"retained");
    assert_eq!(bytes(&image, "opaque/added").await, b"added");
    assert_eq!(bytes(&image, "modified").await, b"new");
    assert!(image.contents().get_path("recreated/old").is_none());
    assert_eq!(bytes(&image, "recreated/new").await, b"new child");
    assert_eq!(bytes(&image, "a-new").await, b"new inode");
    assert_eq!(bytes(&image, "z-new").await, b"new inode");
    let metadata = &image
        .contents()
        .get("opaque")
        .unwrap()
        .as_node()
        .unwrap()
        .metadata;
    assert_eq!(
        (metadata.mode, metadata.uid, metadata.gid),
        (0o700, 123, 456)
    );
    assert_eq!(
        metadata
            .xattrs
            .get(std::ffi::OsStr::new("trusted.overlay.opaque"))
            .unwrap(),
        b"y"
    );
    assert_eq!(
        metadata
            .xattrs
            .get(std::ffi::OsStr::new("user.config"))
            .unwrap(),
        b"retained xattr"
    );
    assert!(
        !metadata
            .xattrs
            .contains_key(std::ffi::OsStr::new("trusted.overlay.origin"))
    );
    assert!(
        image
            .contents()
            .get("recreated")
            .unwrap()
            .as_node()
            .unwrap()
            .metadata
            .xattrs
            .is_empty()
    );
    let mut output = std::io::Cursor::new(Vec::new());
    image.serialize(&mut output).await.unwrap();
    let decoded = Squashfs::deserialize(Arc::<[u8]>::from(output.into_inner()))
        .await
        .unwrap()
        .into_dyn();
    assert_eq!(bytes(&decoded, "alias").await, b"old inode");
    assert_eq!(bytes(&decoded, "a-new").await, b"new inode");
    let mut after = std::io::Cursor::new(Vec::new());
    lower.serialize(&mut after).await.unwrap();
    assert_eq!(after.into_inner(), lower_before.into_inner());
    let mut after = std::io::Cursor::new(Vec::new());
    upper.serialize(&mut after).await.unwrap();
    assert_eq!(after.into_inner(), upper_before.into_inner());
}

#[tokio::test]
async fn overlayfs_snapshot_decodes_shared_and_user_whiteouts_and_rejects_redirects() {
    let mut lower = Squashfs::<Arc<[u8]>>::default();
    for path in ["a", "b", "c"] {
        lower
            .contents_mut()
            .insert(path, snapshot_file(b"removed"))
            .unwrap();
    }
    let mut upper = Squashfs::<Arc<[u8]>>::default();
    upper.contents_mut().insert("a", whiteout()).unwrap();
    upper
        .contents_mut()
        .insert("b", Entry::Hardlink("a".into()))
        .unwrap();
    let mut regular = snapshot_file(b"");
    regular
        .as_node_mut()
        .unwrap()
        .metadata
        .xattrs
        .insert("user.overlay.whiteout".into(), Vec::new());
    upper.contents_mut().insert("c", regular).unwrap();
    assert!(
        lower
            .with_overlayfs_upper(upper)
            .unwrap()
            .contents()
            .is_empty()
    );
    for name in ["trusted.overlay.redirect", "user.overlay.metacopy"] {
        let mut upper = Squashfs::<Arc<[u8]>>::default();
        let mut file = snapshot_file(b"");
        file.as_node_mut()
            .unwrap()
            .metadata
            .xattrs
            .insert(name.into(), Vec::new());
        upper.contents_mut().insert("file", file).unwrap();
        assert!(
            Squashfs::<Arc<[u8]>>::default()
                .with_overlayfs_upper(upper)
                .is_err()
        );
    }
}
