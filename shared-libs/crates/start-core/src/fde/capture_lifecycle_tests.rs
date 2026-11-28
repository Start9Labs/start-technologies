use super::*;

#[tokio::test]
async fn unsupported_filesystem_rejects_before_any_epoch_mutation() {
    let error = ReadEpoch::new(
        ConfigurationSource::Legacy {
            path: "/unused".into(),
        },
        "/unused".into(),
        "/unused".into(),
        "/unused".into(),
        "/unused".into(),
        "/unused".into(),
        &[PathBuf::from("/proc")],
        &[],
    )
    .await
    .err()
    .expect("procfs cannot be captured as a data filesystem");
    assert_eq!(error.kind, ErrorKind::InvalidRequest);
    assert_eq!(
        error.source.to_string(),
        t!("fde.capture.unsupported-filesystem", path = "/proc").to_string()
    );
}

#[tokio::test]
#[ignore = "requires exclusive managed guest, Btrfs/loop/OverlayFS privileges and genuine source; never on build host"]
async fn native_remount_preserves_security_flags_and_raw_alias_exclusion() {
    use nix::mount::{MsFlags, mount};
    use nix::sys::statvfs::FsFlags;
    let fixture = Fixture::new().await;
    let alias = MountGuard::mount(
        &Bind::new(fixture.ram.path()),
        fixture.volume.path().join("alias"),
        ReadWrite,
    )
    .await
    .unwrap();
    let expected =
        FsFlags::ST_NOSUID | FsFlags::ST_NODEV | FsFlags::ST_NOEXEC | FsFlags::ST_NOATIME;
    mount(
        None::<&str>,
        fixture.ram.path(),
        None::<&str>,
        MsFlags::MS_REMOUNT
            | MsFlags::MS_NOSUID
            | MsFlags::MS_NODEV
            | MsFlags::MS_NOEXEC
            | MsFlags::MS_NOATIME,
        None::<&str>,
    )
    .unwrap();
    for readonly in [true, false] {
        remount(fixture.ram.path(), readonly).await.unwrap();
        let status = nix::sys::statfs::statfs(fixture.ram.path()).unwrap();
        assert!(status.flags().contains(expected));
        assert_eq!(status.flags().contains(FsFlags::ST_RDONLY), readonly);
        let write = tokio::fs::write(alias.path().join("raw-alias-writer"), b"changed").await;
        if readonly {
            assert_eq!(write.unwrap_err().raw_os_error(), Some(libc::EROFS));
        } else {
            write.unwrap();
        }
    }
    alias.unmount(true).await.unwrap();
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "requires exclusive managed guest, real PID1/Btrfs/loop/OverlayFS and current startbox; never on build host"]
async fn native_cleanup_retry_holds_mutation_admission_and_keeps_cached_reads_live() {
    let fixture = Fixture::new().await;
    let epoch = fixture.epoch().await;
    let db = patch_db::PatchDb::open(fixture.volume.path().join("cleanup.db"))
        .await
        .unwrap();
    db.put(&patch_db::json_ptr::ROOT, &1).await.unwrap();
    let pause = db.pause_mutations().await;
    tokio::fs::create_dir_all(REQUESTS).await.unwrap();
    let path = Path::new(REQUESTS).join(format!("{}.json", Guid::new()));
    write_file_private_atomic(&path, serde_json::to_vec(&epoch).unwrap())
        .await
        .unwrap();
    let unit = unit();
    Command::new("/usr/bin/systemd-run")
        .args(["--quiet", "--collect"])
        .arg(format!("--unit={unit}"))
        .arg("--property=RefuseManualStop=yes")
        .arg("--property=RuntimeMaxSec=60s")
        .arg(format!(
            "--property=ExecStopPost={} start-fde-release {}",
            executable().display(),
            path.display()
        ))
        .args(["--", "/usr/bin/sleep", "infinity"])
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    epoch.enter().await.unwrap();
    assert!(
        Command::new("/usr/bin/systemctl")
            .arg("stop")
            .arg(&unit)
            .invoke(ErrorKind::Filesystem)
            .await
            .is_err()
    );
    let mut cleanup = Box::pin(async {
        restore_after_unit(&epoch, &path, &unit).await;
        drop(pause);
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(150), &mut cleanup)
            .await
            .is_err()
    );
    assert_eq!(
        tokio::time::timeout(
            Duration::from_secs(1),
            db.get_value(&patch_db::json_ptr::ROOT)
        )
        .await
        .unwrap(),
        patch_db::value::json!(1)
    );
    let mut write = Box::pin(db.put(&patch_db::json_ptr::ROOT, &2));
    assert!(futures::poll!(&mut write).is_pending());
    Command::new("/usr/bin/systemctl")
        .args(["kill", "--kill-whom=main", "--signal=KILL"])
        .arg(&unit)
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(20), &mut cleanup)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), write)
        .await
        .unwrap()
        .unwrap();
    assert!(!path.exists());
    assert_eq!(
        db.get_value(&patch_db::json_ptr::ROOT).await,
        patch_db::value::json!(2)
    );
    drop(cleanup);
    db.close().await;
    fixture.cleanup().await;
}
