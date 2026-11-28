use super::*;
use crate::fde::private_config::{DIRECTORY, reserve_backing, reserve_visible_backings};
use crate::util::FileLockAccess;

#[derive(Debug)]
struct BackingUseProbe {
    path: PathBuf,
    backing: PathBuf,
    expect_busy: bool,
}

impl GenericMountGuard for BackingUseProbe {
    fn path(&self) -> &Path {
        &self.path
    }

    async fn unmount(self) -> Result<(), Error> {
        let exclusive = reserve_backing(&self.backing, FileLockAccess::Exclusive).await;
        assert_eq!(exclusive.is_err(), self.expect_busy);
        Ok(())
    }
}

#[tokio::test]
async fn capture_releases_directory_uses_after_base_and_before_data_view_retirement() {
    if crate::util::file_lock_tests::isolated_test("fde::generation::tests::lifetimes::capture_releases_directory_uses_after_base_and_before_data_view_retirement").await {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let backing = data
        .join(crate::MAIN_FILESYSTEM)
        .join(DIRECTORY)
        .join(Guid::new().to_string());
    tokio::fs::create_dir_all(&backing).await.unwrap();
    let workspace = root.path().join("workspace");
    tokio::fs::create_dir(&workspace).await.unwrap();
    let roots = ConfigurationRoots {
        base: BackingUseProbe {
            path: root.path().join("base"),
            backing: backing.clone(),
            expect_busy: true,
        },
        data: BackingUseProbe {
            path: data.clone(),
            backing,
            expect_busy: false,
        },
        workspace: workspace.clone(),
        private_backing_uses: reserve_visible_backings(&data.join(crate::MAIN_FILESYSTEM))
            .await
            .unwrap(),
    };
    roots.unmount().await.unwrap();
    assert!(!workspace.exists());
}
