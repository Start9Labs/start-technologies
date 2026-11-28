use super::*;
use crate::disk::mount::filesystem::ReadOnly;

#[tokio::test]
async fn metadata_image_mounts_require_squashfs_and_exclude_external_helpers() {
    let directory = tempfile::tempdir().unwrap();
    let image = directory.path().join("image");
    tokio::fs::write(&image, b"fixture").await.unwrap();
    let filesystem = SquashfsImage::new(&image);
    let command =
        super::super::default_mount_command(&filesystem, directory.path().join("mount"), ReadOnly)
            .await
            .unwrap();
    let args: Vec<_> = command.get_args().map(|s| s.to_owned()).collect();
    assert!(args.windows(2).any(|pair| pair
        == [
            std::ffi::OsString::from("-t"),
            std::ffi::OsString::from("squashfs")
        ]));
    assert!(args.contains(&std::ffi::OsString::from("--internal-only")));
    assert_ne!(
        filesystem.source_hash().await.unwrap(),
        LoopDev::new(&image, 0, 0).source_hash().await.unwrap()
    );
}
