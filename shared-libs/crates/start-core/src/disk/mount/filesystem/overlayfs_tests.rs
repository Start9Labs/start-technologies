use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use crate::disk::mount::filesystem::bind::Bind;
use crate::disk::mount::util::is_mountpoint;
use crate::util::Invoke;
use crate::util::directory::open_directory_nofollow;

#[derive(Debug)]
struct DirectoryLower(PathBuf);
impl GenericMountGuard for DirectoryLower {
    fn path(&self) -> &Path {
        &self.0
    }
    async fn unmount(self) -> Result<(), Error> {
        Ok(())
    }
}

#[derive(Debug)]
struct ReplacingLower {
    path: PathBuf,
    parent: PathBuf,
    outside: PathBuf,
    replaced: AtomicBool,
}
impl GenericMountGuard for ReplacingLower {
    fn path(&self) -> &Path {
        if !self.replaced.swap(true, Ordering::SeqCst) {
            std::fs::rename(self.parent.join("view"), self.parent.join("moved")).unwrap();
            std::os::unix::fs::symlink(&self.outside, self.parent.join("view")).unwrap();
        }
        &self.path
    }
    async fn unmount(self) -> Result<(), Error> {
        Ok(())
    }
}

#[tokio::test]
async fn overlay_staging_rejects_a_symlink_destination_before_mounting() {
    let parent = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let lower = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), parent.path().join("view")).unwrap();
    let parent_fd = open_directory_nofollow(parent.path()).unwrap();
    assert!(
        OverlayGuard::mount_layers_at::<&Path>(
            &[],
            DirectoryLower(lower.path().into()),
            &[],
            &parent_fd,
            c"view"
        )
        .await
        .is_err()
    );
    assert!(std::fs::read_dir(outside.path()).unwrap().next().is_none());
}

#[tokio::test]
#[ignore = "requires root in a private mount namespace with overlayfs"]
async fn overlay_attachment_and_cleanup_ignore_replaced_targets() {
    let parent = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let protected = tempfile::tempdir().unwrap();
    let lower = tempfile::tempdir().unwrap();
    std::fs::write(protected.path().join("host"), "protected").unwrap();
    std::fs::write(lower.path().join("value"), "lower").unwrap();
    let host = MountGuard::mount(&Bind::new(protected.path()), outside.path(), ReadWrite)
        .await
        .unwrap();
    let parent_fd = open_directory_nofollow(parent.path()).unwrap();
    let overlay = OverlayGuard::mount_layers_at::<&Path>(
        &[],
        ReplacingLower {
            path: lower.path().into(),
            parent: parent.path().into(),
            outside: outside.path().into(),
            replaced: AtomicBool::new(false),
        },
        &[],
        &parent_fd,
        c"view",
    )
    .await
    .unwrap();
    let backing = overlay.backing.upper.as_ref().unwrap().as_ref().to_owned();
    assert!(!backing.starts_with(parent.path()));
    assert_eq!(
        std::fs::read_to_string(parent.path().join("moved/value")).unwrap(),
        "lower"
    );
    std::fs::write(overlay.path().join("value"), "changed").unwrap();
    assert_eq!(
        std::fs::read_to_string(lower.path().join("value")).unwrap(),
        "lower"
    );
    assert_eq!(
        std::fs::read_to_string(parent.path().join("moved/value")).unwrap(),
        "changed"
    );
    overlay.unmount().await.unwrap();
    assert!(!parent.path().join("moved/value").exists());
    assert!(!backing.exists());
    assert!(is_mountpoint(outside.path()).await.unwrap());
    assert_eq!(
        std::fs::read_to_string(outside.path().join("host")).unwrap(),
        "protected"
    );
    host.unmount(false).await.unwrap();
}

#[tokio::test]
#[ignore = "requires root in a private mount namespace with overlayfs"]
async fn overlay_drop_detaches_the_view_before_private_backing_cleanup() {
    let parent = tempfile::tempdir().unwrap();
    let lower = tempfile::tempdir().unwrap();
    let parent_fd = open_directory_nofollow(parent.path()).unwrap();
    let overlay = OverlayGuard::mount_layers_at::<&Path>(
        &[],
        DirectoryLower(lower.path().into()),
        &[],
        &parent_fd,
        c"view",
    )
    .await
    .unwrap();
    let backing = overlay.backing.upper.as_ref().unwrap().as_ref().to_owned();
    assert!(is_mountpoint(&parent.path().join("view")).await.unwrap());
    drop(overlay);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while backing.exists() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(!is_mountpoint(&parent.path().join("view")).await.unwrap());
}

#[tokio::test]
#[ignore = "requires root in a private mount namespace with overlayfs"]
async fn ordinary_overlays_preserve_layer_order_take_and_path_cleanup() {
    let pre = tempfile::tempdir().unwrap();
    let main = tempfile::tempdir().unwrap();
    let post = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    std::fs::write(pre.path().join("value"), "pre").unwrap();
    std::fs::write(main.path().join("value"), "main").unwrap();
    std::fs::write(post.path().join("only-post"), "post").unwrap();
    let mut overlay = OverlayGuard::mount_layers(
        &[pre.path()],
        DirectoryLower(main.path().into()),
        &[post.path()],
        target.path(),
    )
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(overlay.path().join("value")).unwrap(),
        "pre"
    );
    assert_eq!(
        std::fs::read_to_string(overlay.path().join("only-post")).unwrap(),
        "post"
    );
    let path = overlay.path().to_owned();
    let taken = overlay.take();
    assert_eq!(overlay.path(), path);
    drop(overlay);
    assert!(is_mountpoint(&path).await.unwrap());
    taken.unmount(true).await.unwrap();
    assert!(!path.exists());
}

#[tokio::test]
#[ignore = "requires root in a private mount namespace with overlayfs"]
async fn overlay_view_preserves_shared_submount_visibility() {
    let parent = tempfile::tempdir().unwrap();
    let lower = tempfile::tempdir().unwrap();
    std::fs::create_dir(lower.path().join("sub")).unwrap();
    nix::mount::mount(
        Some(parent.path()),
        parent.path(),
        None::<&str>,
        nix::mount::MsFlags::MS_BIND,
        None::<&str>,
    )
    .unwrap();
    nix::mount::mount(
        None::<&str>,
        parent.path(),
        None::<&str>,
        nix::mount::MsFlags::MS_SHARED | nix::mount::MsFlags::MS_REC,
        None::<&str>,
    )
    .unwrap();
    let parent_fd = open_directory_nofollow(parent.path()).unwrap();
    let overlay = OverlayGuard::mount_layers_at::<&Path>(
        &[],
        DirectoryLower(lower.path().into()),
        &[],
        &parent_fd,
        c"view",
    )
    .await
    .unwrap();
    tokio::process::Command::new("unshare")
        .args([
            "--mount",
            "--propagation",
            "unchanged",
            "--",
            "sh",
            "-c",
            "mount -t tmpfs tmpfs \"$1/sub\" && printf propagated > \"$1/sub/value\"",
            "_",
        ])
        .arg(parent.path().join("view"))
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(overlay.path().join("sub/value")).unwrap(),
        "propagated"
    );
    assert!(
        !overlay
            .backing
            .upper
            .as_ref()
            .unwrap()
            .join("upper/sub/value")
            .exists()
    );
    overlay.unmount().await.unwrap();
    nix::mount::umount2(parent.path(), nix::mount::MntFlags::MNT_DETACH).unwrap();
}
