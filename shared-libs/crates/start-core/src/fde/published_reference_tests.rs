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
    let running = root.path().join("running");
    for path in [&data, &store, &running] {
        tokio::fs::create_dir(path).await.unwrap();
    }
    let ids: Vec<_> = (0..4).map(|_| Guid::new()).collect();
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
    tokio::fs::symlink(&targets[0], running.join("current"))
        .await
        .unwrap();
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
    let mut images = Vec::new();
    for (index, compression) in [(2, "gzip"), (3, "zstd")] {
        let source = root.path().join(compression);
        tokio::fs::create_dir(&source).await.unwrap();
        tokio::fs::symlink(&targets[index], source.join("reference"))
            .await
            .unwrap();
        tokio::fs::hard_link(source.join("reference"), source.join("alias"))
            .await
            .unwrap();
        let generation = store.join(format!("generation-{}", Guid::new()));
        tokio::fs::create_dir(&generation).await.unwrap();
        let image = generation.join(crate::os_overlay::OVERLAY_FILE);
        Command::new("/usr/bin/mksquashfs")
            .arg(&source)
            .arg(&image)
            .args(["-noappend", "-no-progress", "-comp", compression])
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
        images.push(image);
    }
    let before = crate::disk::mount::util::mountpoints_under(Path::new(TmpDir::BASE))
        .await
        .unwrap();
    assert_eq!(
        configuration_backings(&running, &data, &store, &working)
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
        configuration_backings(&running, &data, &store, &working)
            .await
            .is_err()
    );
    tokio::fs::remove_dir(unknown).await.unwrap();
    tokio::fs::write(&images[0], b"corrupt image")
        .await
        .unwrap();
    assert!(
        configuration_backings(&running, &data, &store, &working)
            .await
            .is_err()
    );
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
    scratch.unmount(false).await.unwrap();
    root.unmount(false).await.unwrap();
}
