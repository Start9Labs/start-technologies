use super::*;

#[derive(Debug, Clone)]
struct Directory(PathBuf);
impl GenericMountGuard for Directory {
    fn path(&self) -> &Path {
        &self.0
    }
    async fn unmount(self) -> Result<(), Error> {
        Ok(())
    }
}

#[tokio::test]
async fn aliases_of_the_same_target_join_one_sweep() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("target");
    let alias = root.path().join("alias");
    tokio::fs::create_dir_all(trash_dir(&directory))
        .await
        .unwrap();
    tokio::fs::write(trash_dir(&directory).join("old-backup"), b"old")
        .await
        .unwrap();
    tokio::fs::symlink(&directory, &alias).await.unwrap();
    let first = ensure_sweep(Directory(directory.clone())).await.unwrap();
    let second = ensure_sweep(Directory(alias.clone())).await.unwrap();
    assert!(first.ptr_eq(&second));
    first.await.unwrap();
    second.await.unwrap();
    assert!(!has_trash(&directory).await);
    tokio::fs::create_dir_all(trash_dir(&directory))
        .await
        .unwrap();
    tokio::fs::write(trash_dir(&directory).join("later-backup"), b"later")
        .await
        .unwrap();
    sweep_until_clear(&Directory(alias)).await.unwrap();
    assert!(!has_trash(&directory).await);
}
