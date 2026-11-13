use std::os::unix::fs::symlink;

use super::*;

#[tokio::test]
async fn persistent_identity_survives_port_reuse_and_aliases_share_mounts() {
    let fixture = tempfile::tempdir().unwrap();
    let disk = fixture.path().join("disk-a");
    let other = fixture.path().join("disk-b");
    let port = fixture.path().join("port");
    let uuids = fixture.path().join("by-uuid");
    std::fs::write(&disk, "original contents").unwrap();
    std::fs::write(&other, "other drive contents").unwrap();
    std::fs::create_dir(&uuids).unwrap();
    let identity = uuids.join("filesystem-identity");
    symlink(&disk, &identity).unwrap();
    symlink(&disk, &port).unwrap();
    let persistent = uuid_path(&port, &uuids).await.unwrap();
    assert_eq!(persistent, identity);
    assert_eq!(
        BlockDev::new(&persistent).source_hash().await.unwrap(),
        BlockDev::new(&port).source_hash().await.unwrap(),
    );

    std::fs::remove_file(&port).unwrap();
    symlink(&other, &port).unwrap();
    assert_eq!(
        std::fs::read_to_string(&persistent).unwrap(),
        "original contents"
    );
    assert_ne!(
        BlockDev::new(&persistent).source_hash().await.unwrap(),
        BlockDev::new(&port).source_hash().await.unwrap(),
    );
    assert!(uuid_path(&other, &uuids).await.is_err());

    std::fs::remove_file(&disk).unwrap();
    assert!(BlockDev::new(&persistent).source_hash().await.is_err());
}
