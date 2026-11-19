use super::*;

#[cfg(target_os = "linux")]
#[tokio::test]
#[ignore = "requires exclusive managed guest, native SquashFS gzip/Zstd tools and kernel mounts; never on build host"]
async fn native_configuration_union_retains_published_gzip_and_zstd_without_boot_authority() {
    use crate::util::squashfs::{DirectoryContents, Entry, Metadata, NodeContents};
    let directory =
        PathBuf::from(std::env::var_os("FDE_ROOT_VIEW_TEST_ROOT").expect("owned metadata fixture"));
    let root = MountGuard::mount(&PrivateTmpfs::new(), &directory, ReadWrite)
        .await
        .unwrap();
    let scratch = MountGuard::mount(&PrivateTmpfs::new(), TmpDir::BASE, ReadWrite)
        .await
        .unwrap();
    let data = root.path().join("data");
    let store = root.path().join("generations");
    let base = root.path().join("base");
    let stored = root.path().join("images");
    let upper = root.path().join("upper");
    for path in [&data, &store, &base, &stored, &upper] {
        tokio::fs::create_dir(path).await.unwrap();
    }
    let selected = root.path().join("selected.private");
    let ids: Vec<_> = (0..8).map(|_| Guid::new()).collect();
    let targets: Vec<_> = ids
        .iter()
        .map(|id| {
            Path::new(crate::fde::private_config::RUNTIME)
                .join(id.to_string())
                .join("etc/key")
        })
        .collect();
    for id in &ids {
        let parent = data
            .join(crate::fde::private_config::DIRECTORY)
            .join(id.to_string())
            .join("etc");
        tokio::fs::create_dir_all(&parent).await.unwrap();
        tokio::fs::write(parent.join("key"), b"retained")
            .await
            .unwrap();
    }
    tokio::fs::symlink(&targets[0], upper.join("current"))
        .await
        .unwrap();
    tokio::fs::symlink(&targets[4], base.join("masked"))
        .await
        .unwrap();
    tokio::fs::write(upper.join("masked"), b"running override")
        .await
        .unwrap();
    let running = MountGuard::mount(
        &crate::disk::mount::filesystem::overlayfs::OverlayFs::new(
            vec![base.clone()],
            upper,
            root.path().join("work"),
        ),
        root.path().join("running"),
        ReadWrite,
    )
    .await
    .unwrap();
    assert_eq!(
        tokio::fs::read(running.path().join("masked"))
            .await
            .unwrap(),
        b"running override"
    );
    let mut working = Squashfs::new(Metadata::new(0o755), DirectoryContents::new());
    working
        .contents_mut()
        .insert_path(
            "working",
            Entry::new(
                Metadata::new(0o777),
                NodeContents::Symlink(targets[1].clone()),
            ),
        )
        .unwrap();
    let mut published = Vec::new();
    for (index, compression) in [
        (2, "gzip"),
        (3, "zstd"),
        (5, "gzip"),
        (6, "zstd"),
        (7, "gzip"),
    ] {
        let source = root.path().join(format!("source-{index}"));
        tokio::fs::create_dir(&source).await.unwrap();
        tokio::fs::symlink(&targets[index], source.join("reference"))
            .await
            .unwrap();
        tokio::fs::hard_link(source.join("reference"), source.join("alias"))
            .await
            .unwrap();
        let image = if index < 4 {
            let generation = store.join(format!("generation-{}", Guid::new()));
            tokio::fs::create_dir(&generation).await.unwrap();
            let image = generation.join(crate::os_overlay::OVERLAY_FILE);
            published.push(image.clone());
            image
        } else if index == 7 {
            selected.clone()
        } else {
            stored.join(if index == 5 {
                "private-repack.rootfs"
            } else {
                "recovery.squashfs"
            })
        };
        Command::new("/usr/bin/mksquashfs")
            .arg(&source)
            .arg(&image)
            .args(["-noappend", "-no-progress", "-comp", compression])
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
    }
    let before = crate::disk::mount::util::mountpoints_under(Path::new(TmpDir::BASE))
        .await
        .unwrap();
    assert_eq!(
        configuration_backings(
            running.path(),
            &base,
            &data,
            &store,
            &stored,
            &selected,
            &working
        )
        .await
        .unwrap(),
        ids.iter().cloned().collect()
    );
    assert_eq!(
        crate::disk::mount::util::mountpoints_under(Path::new(TmpDir::BASE))
            .await
            .unwrap(),
        before
    );
    let unknown = store.join("unknown");
    tokio::fs::create_dir(&unknown).await.unwrap();
    assert!(
        configuration_backings(
            running.path(),
            &base,
            &data,
            &store,
            &stored,
            &selected,
            &working
        )
        .await
        .is_err()
    );
    tokio::fs::remove_dir(unknown).await.unwrap();
    for image in [
        &published[0],
        &stored.join("private-repack.rootfs"),
        &selected,
    ] {
        let original = tokio::fs::read(image).await.unwrap();
        tokio::fs::write(image, b"corrupt image").await.unwrap();
        assert!(
            configuration_backings(
                running.path(),
                &base,
                &data,
                &store,
                &stored,
                &selected,
                &working
            )
            .await
            .is_err()
        );
        tokio::fs::write(image, original).await.unwrap();
    }
    let substitute = MountGuard::mount(
        &Bind::new(stored.join("recovery.squashfs"))
            .with_type(crate::disk::mount::filesystem::bind::FileType::File),
        &selected,
        ReadOnly,
    )
    .await
    .unwrap()
    .with_strict_unmount();
    assert!(
        configuration_backings(
            running.path(),
            &base,
            &data,
            &store,
            &stored,
            &selected,
            &working
        )
        .await
        .is_err()
    );
    substitute.unmount(false).await.unwrap();
    assert_eq!(
        crate::disk::mount::util::mountpoints_under(Path::new(TmpDir::BASE))
            .await
            .unwrap(),
        before
    );
    for id in ids {
        assert_eq!(
            tokio::fs::read(
                data.join(crate::fde::private_config::DIRECTORY)
                    .join(id.to_string())
                    .join("etc/key")
            )
            .await
            .unwrap(),
            b"retained"
        );
    }
    running.unmount(false).await.unwrap();
    scratch.unmount(false).await.unwrap();
    root.unmount(false).await.unwrap();
}
