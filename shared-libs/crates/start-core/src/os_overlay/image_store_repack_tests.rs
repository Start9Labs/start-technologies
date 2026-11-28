use super::*;

#[tokio::test]
async fn native_repack_paths_preserve_bytes_and_distinguish_empty_scaffolds() {
    let root = tempfile::tempdir().unwrap();
    let volume = root.path().join(std::ffi::OsString::from_vec(
        b"volume\nrepack-\xff".to_vec(),
    ));
    std::fs::create_dir(&volume).unwrap();
    let mut native = command();
    native.arg("startos_repack_paths").arg(&volume);
    assert_eq!(
        paths(native).await.unwrap(),
        [volume.join("next"), volume.join("upper")]
    );
    assert!(!has_repack_workspace(&volume).await.unwrap());
    for name in ["next", "upper"] {
        std::fs::create_dir(volume.join(name)).unwrap();
    }
    assert!(!has_repack_workspace(&volume).await.unwrap());
    for name in ["next", "upper"] {
        let contents = volume.join(name).join("unpublished");
        std::fs::write(&contents, b"mutable private workspace").unwrap();
        assert!(has_repack_workspace(&volume).await.unwrap());
        std::fs::remove_file(contents).unwrap();
        assert!(!has_repack_workspace(&volume).await.unwrap());
    }
}

#[tokio::test]
async fn native_repack_inspection_rejects_redirected_or_non_directory_roots() {
    let root = tempfile::tempdir().unwrap();
    let volume = root.path();
    std::fs::create_dir(volume.join("next")).unwrap();
    std::fs::write(volume.join("next/private"), b"retained workspace").unwrap();
    let upper = volume.join("upper");
    std::os::unix::fs::symlink(volume.join("absent"), &upper).unwrap();
    assert!(has_repack_workspace(volume).await.is_err());
    std::fs::remove_file(&upper).unwrap();
    std::fs::write(&upper, b"wrong type").unwrap();
    assert!(has_repack_workspace(volume).await.is_err());
    std::fs::remove_file(&upper).unwrap();
    nix::unistd::mkfifo(&upper, nix::sys::stat::Mode::S_IRUSR).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        assert!(has_repack_workspace(volume).await.is_err());
    })
    .await
    .unwrap();
}
