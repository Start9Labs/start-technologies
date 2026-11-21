use super::*;

#[tokio::test]
async fn reference_discovery_unifies_aliases_without_reading_file_contents() {
    let (_root, raw, data) = fixture().await;
    let first = Guid::new();
    let second = Guid::new();
    for (id, name) in [(&first, "first"), (&first, "second"), (&second, "third")] {
        let relative = PathBuf::from(id.to_string()).join(name);
        let backing = data.join(DIRECTORY).join(&relative);
        tokio::fs::create_dir_all(backing.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&backing, b"backing bytes").await.unwrap();
        tokio::fs::symlink(
            Path::new(RUNTIME).join(relative),
            raw.join("etc").join(name),
        )
        .await
        .unwrap();
    }
    tokio::fs::hard_link(raw.join("etc/first"), raw.join("etc/alias"))
        .await
        .unwrap();
    use std::os::unix::ffi::OsStringExt;
    let native = PathBuf::from(first.to_string())
        .join(std::ffi::OsString::from_vec(b"native-\xff".to_vec()));
    tokio::fs::write(data.join(DIRECTORY).join(&native), b"native filename")
        .await
        .unwrap();
    tokio::fs::symlink(
        Path::new(RUNTIME).join(native),
        raw.join("etc/native-reference"),
    )
    .await
    .unwrap();
    let mut snapshot = Squashfs::from_directory(&raw).await.unwrap().into_dyn();
    snapshot
        .contents_mut()
        .insert_path(
            "unreadable",
            Entry::new(
                Metadata::new(0o600),
                NodeContents::File(DynFileSource::new(raw.join("unopened-missing-file"))),
            ),
        )
        .unwrap();
    assert_eq!(
        referenced_backings(&snapshot, &data).await.unwrap(),
        BTreeSet::from([first.clone(), second])
    );
    assert_eq!(reference_nodes(&snapshot).unwrap().len(), 4);
    assert_eq!(
        tokio::fs::read(data.join(DIRECTORY).join(first.to_string()).join("first"))
            .await
            .unwrap(),
        b"backing bytes"
    );
}

#[tokio::test]
async fn reference_discovery_rejects_missing_redirected_and_malformed_backings() {
    let (_root, raw, data) = fixture().await;
    let id = Guid::new();
    let parent = data.join(DIRECTORY).join(id.to_string());
    tokio::fs::create_dir_all(parent.join("etc")).await.unwrap();
    tokio::fs::write(parent.join("etc/key"), b"retained")
        .await
        .unwrap();
    let target = Path::new(RUNTIME).join(id.to_string()).join("etc/key");
    let mut snapshot = Squashfs::from_directory(&raw).await.unwrap().into_dyn();
    for invalid in [
        PathBuf::from(RUNTIME),
        Path::new(RUNTIME).join("not-a-guid/etc/key"),
        Path::new(RUNTIME).join(id.to_string()),
        Path::new(RUNTIME).join(id.to_string()).join("../etc/key"),
        Path::new(RUNTIME).join(id.to_string()).join("etc/missing"),
        Path::new(RUNTIME).join(id.to_string()).join("etc"),
    ] {
        snapshot
            .contents_mut()
            .insert_path("reference", private_reference(invalid.clone()))
            .unwrap();
        assert!(
            referenced_backings(&snapshot, &data).await.is_err(),
            "{invalid:?}"
        );
    }
    snapshot
        .contents_mut()
        .insert_path("reference", private_reference(target.clone()))
        .unwrap();
    assert_eq!(
        referenced_backings(&snapshot, &data).await.unwrap(),
        BTreeSet::from([id])
    );
    tokio::fs::rename(parent.join("etc"), parent.join("saved"))
        .await
        .unwrap();
    tokio::fs::symlink("saved", parent.join("etc"))
        .await
        .unwrap();
    assert!(referenced_backings(&snapshot, &data).await.is_err());
    tokio::fs::remove_file(parent.join("etc")).await.unwrap();
    let other = Guid::new();
    let other_parent = data.join(DIRECTORY).join(other.to_string()).join("etc");
    tokio::fs::create_dir_all(&other_parent).await.unwrap();
    tokio::fs::write(other_parent.join("key"), b"other retained")
        .await
        .unwrap();
    tokio::fs::symlink(
        Path::new("..").join(other.to_string()).join("etc"),
        parent.join("etc"),
    )
    .await
    .unwrap();
    assert!(referenced_backings(&snapshot, &data).await.is_err());
    tokio::fs::remove_file(parent.join("etc")).await.unwrap();
    tokio::fs::rename(parent.join("saved"), parent.join("etc"))
        .await
        .unwrap();
    tokio::fs::rename(parent.join("etc/key"), parent.join("etc/value"))
        .await
        .unwrap();
    tokio::fs::symlink("value", parent.join("etc/key"))
        .await
        .unwrap();
    assert!(referenced_backings(&snapshot, &data).await.is_err());
    assert_eq!(
        tokio::fs::read(parent.join("etc/value")).await.unwrap(),
        b"retained"
    );
}

#[tokio::test]
async fn reference_discovery_rejects_unresolved_or_cyclic_hardlinks() {
    let (_root, raw, data) = fixture().await;
    let mut snapshot = Squashfs::from_directory(&raw).await.unwrap().into_dyn();
    snapshot
        .contents_mut()
        .insert_path("unresolved", Entry::Hardlink("absent".into()))
        .unwrap();
    assert!(referenced_backings(&snapshot, &data).await.is_err());
    snapshot
        .contents_mut()
        .insert_path("unresolved", Entry::Hardlink("cycle".into()))
        .unwrap();
    snapshot
        .contents_mut()
        .insert_path("cycle", Entry::Hardlink("unresolved".into()))
        .unwrap();
    assert!(referenced_backings(&snapshot, &data).await.is_err());
}
