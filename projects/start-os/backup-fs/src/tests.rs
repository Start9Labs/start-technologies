use std::future::Future;
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::{fs, io};

use fuser::MountOption;
use tempdir::TempDir;
use tokio::task::JoinSet;

use crate::error::BkfsErrorKind;
use crate::{BackupFS, BackupFSOptions};

/// Deterministic pseudorandom byte pattern keyed by absolute file offset.
/// Any misplaced byte will produce a mismatch, catching buffer ordering bugs.
fn pattern_byte(offset: u64) -> u8 {
    let mut x = offset.wrapping_mul(0x9E3779B97F4A7C15);
    x ^= x >> 27;
    x = x.wrapping_mul(0x3C79AC492BA7B653);
    x ^= x >> 33;
    (x & 0xFF) as u8
}

fn pattern_fill(offset: u64, buf: &mut [u8]) {
    for (i, b) in buf.iter_mut().enumerate() {
        *b = pattern_byte(offset + i as u64);
    }
}

fn pattern_check(offset: u64, buf: &[u8]) {
    for (i, &b) in buf.iter().enumerate() {
        let expected = pattern_byte(offset + i as u64);
        assert_eq!(
            b,
            expected,
            "byte mismatch at offset {}: got {:#04x}, want {:#04x}",
            offset + i as u64,
            b,
            expected
        );
    }
}

/// Serializes the fusermount3 invocation across parallel tests.
/// The helper talks to a per-user control socket and races with itself
/// under `cargo test -j N`, yielding sporadic EPERM from Session::new.
/// Holding the lock only through mount keeps the test bodies
/// themselves parallel.
fn mount_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

fn with_backupfs(
    data: &Path,
    password: String,
    func: impl FnOnce(&Path),
    file_size_padding: Option<f64>,
) {
    struct Unmounter(fuser::SessionUnmounter);
    impl Drop for Unmounter {
        fn drop(&mut self) {
            let _ = self.0.unmount();
        }
    }

    let mnt = tempdir::TempDir::new("backupfs_mnt").unwrap();
    let opt = vec![
        MountOption::FSName("backup-fs".to_string()),
        MountOption::AutoUnmount,
    ];
    let data_dir = data.to_owned();
    let mnt_dir = mnt.path().to_owned();
    let (ready_sender, ready_reciever) = oneshot::channel();
    let thread = std::thread::spawn(move || {
        let fs = BackupFS::new(BackupFSOptions {
            data_dir,
            setuid_support: false,
            password,
            file_size_padding,
            readonly: false,
            idmapped_root: vec![],
        })
        .unwrap();
        let mut fs = {
            let _guard = mount_lock().lock().unwrap_or_else(|e| e.into_inner());
            fuser::Session::new(fs, mnt_dir, &opt).unwrap()
        };
        ready_sender.send(Unmounter(fs.unmount_callable())).unwrap();
        fs.run().unwrap();
    });
    if let Ok(umount) = ready_reciever.recv() {
        func(mnt.path());
        drop(umount);
    }
    match thread.join() {
        Ok(()) => (),
        Err(err) => std::panic::resume_unwind(err),
    };
}

fn with_backupfs_async<F: Future<Output = ()> + Send + 'static>(
    data: &Path,
    password: String,
    func: impl FnOnce(PathBuf) -> F,
    file_size_padding: Option<f64>,
) {
    with_backupfs(
        data,
        password,
        move |path| {
            tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap()
                .block_on(func(path.to_owned()))
        },
        file_size_padding,
    )
}

fn tree(path: impl AsRef<Path>, dirs: bool) -> Result<Vec<String>, io::Error> {
    let mut children = Vec::new();
    for e in fs::read_dir(path)? {
        let e = e?;
        let name = e.file_name().to_string_lossy().into_owned();
        if e.metadata()?.is_dir() {
            if dirs {
                children.push(name.clone());
            }
            let grandchildren = tree(e.path(), dirs)?;
            children.extend(
                grandchildren
                    .into_iter()
                    .map(|child| format!("{name}/{child}")),
            )
        } else {
            children.push(name);
        }
    }
    children.sort_unstable();
    Ok(children)
}

/// Count content block files, tolerating an absent `contents/` dir — a
/// backup whose files are all inline-sized writes no content blocks at all.
fn content_files(data: &Path) -> usize {
    tree(data.join("contents"), false)
        .map(|v| v.len())
        .unwrap_or(0)
}

#[test_log::test]
fn write_file() {
    let data = TempDir::new("backupfs_data").unwrap();
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::write(mnt.join("a_file"), "foo bar").unwrap();
            assert!(fs::read_dir(mnt).unwrap().any(|e| e
                .as_ref()
                .unwrap()
                .file_name()
                .to_str()
                .unwrap()
                == "a_file"));
            assert_eq!(fs::read(mnt.join("a_file")).unwrap().as_slice(), b"foo bar");
        },
        None,
    );
    // "foo bar" is inline (no separate content block).
    assert_eq!(content_files(data.path()), 0);
}

#[test_log::test]
fn write_directory() {
    let data = TempDir::new("backupfs_data").unwrap();
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::create_dir(mnt.join("a")).unwrap();
            fs::create_dir(mnt.join("a/b")).unwrap();
            fs::create_dir(mnt.join("a/c")).unwrap();
            assert_eq!(
                tree(mnt, true).unwrap(),
                vec!["a".to_owned(), "a/b".to_owned(), "a/c".to_owned()]
            )
        },
        None,
    );
}

#[test_log::test]
fn preserves_file() {
    let data = TempDir::new("backupfs_data").unwrap();
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::write(mnt.join("a_file"), "foo bar").unwrap();
        },
        None,
    );
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            assert_eq!(fs::read(mnt.join("a_file")).unwrap().as_slice(), b"foo bar");
        },
        None,
    );
}

#[test_log::test]
fn preserves_directory() {
    let data = TempDir::new("backupfs_data").unwrap();
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::create_dir(mnt.join("a")).unwrap();
            fs::create_dir(mnt.join("a/b")).unwrap();
            fs::create_dir(mnt.join("a/c")).unwrap();
        },
        None,
    );

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            assert_eq!(
                tree(mnt, true).unwrap(),
                vec!["a".to_owned(), "a/b".to_owned(), "a/c".to_owned()]
            )
        },
        None,
    );
}

#[test_log::test]
fn checksum() {
    let data = TempDir::new("backupfs_data").unwrap();
    with_backupfs(data.path(), "ohea".to_owned(), |_mnt| {}, None);
    let res = BackupFS::new(BackupFSOptions {
        data_dir: data.path().to_owned(),
        setuid_support: false,
        password: "rtns".to_owned(),
        file_size_padding: None,
        readonly: false,
        idmapped_root: vec![],
    });
    match res {
        Ok(_) => panic!(),
        Err(err) => assert!(matches!(&err.kind, BkfsErrorKind::BadChecksum)),
    }
}

#[test_log::test]
fn change_password() {
    let data = TempDir::new("backupfs_data").unwrap();
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::write(mnt.join("a_file"), "foo bar").unwrap();
        },
        None,
    );

    {
        let mut fs = BackupFS::new(BackupFSOptions {
            data_dir: data.path().to_owned(),
            setuid_support: false,
            password: "ohea".to_owned(),
            file_size_padding: None,
            readonly: false,
            idmapped_root: vec![],
        })
        .unwrap();
        fs.change_password("rtns").unwrap();
    }

    with_backupfs(
        data.path(),
        "rtns".to_owned(),
        |mnt| {
            assert_eq!(fs::read(mnt.join("a_file")).unwrap().as_slice(), b"foo bar");
        },
        None,
    );
}

#[test_log::test]
fn write_one_file_async() {
    use tokio::fs;
    let data = TempDir::new("backupfs_data").unwrap();
    with_backupfs_async(
        data.path(),
        "ohea".to_owned(),
        |mnt| async move {
            fs::write(mnt.join("a_file"), "foo bar").await.unwrap();
            let mut contents = fs::read_dir(&mnt).await.unwrap();
            loop {
                match contents.next_entry().await.unwrap() {
                    Some(entry) => {
                        if entry.file_name() == "a_file" {
                            break;
                        }
                    }
                    None => panic!(),
                }
            }
            assert_eq!(
                fs::read(mnt.join("a_file")).await.unwrap().as_slice(),
                b"foo bar"
            );
        },
        None,
    );
    // "foo bar" is inline (no separate content block).
    assert_eq!(content_files(data.path()), 0);
}

#[test_log::test]
fn write_many_files_async() {
    use tokio::fs;
    let data = TempDir::new("backupfs_data").unwrap();
    with_backupfs_async(
        data.path(),
        "ohea".to_owned(),
        |mnt| async move {
            let mut tasks = JoinSet::new();
            for i in 0..100 {
                tasks.spawn(fs::write(mnt.join(format!("{i:02}")), format!("= {i}")));
            }
            while let Some(res) = tasks.join_next().await {
                res.unwrap().unwrap();
            }
            let mut count = 0;
            let mut contents = fs::read_dir(&mnt).await.unwrap();
            while let Some(entry) = contents.next_entry().await.unwrap() {
                if let Ok(i) = dbg!(entry.file_name()).to_str().unwrap().parse::<usize>() {
                    assert_eq!(
                        fs::read(entry.path()).await.unwrap().as_slice(),
                        format!("= {i}").as_bytes()
                    );
                    count += 1;
                }
            }
            assert_eq!(count, 100);
        },
        None,
    );
    // The 100 tiny files are inline → no content blocks.
    assert_eq!(content_files(data.path()), 0);
}

/// rm -rf should fully remove a tree from disk — no orphan inode files.
///
/// Suspected zombie-inode bug in the dirty-cache coalescing: unlink goes
/// through mutate_inode, which takes the inode out of dirty, the closure
/// calls gc_inode (which removes the disk file), but mutate_inode then
/// unconditionally re-inserts the (now parent-less) attrs back into
/// dirty. On unmount, flush_all_dirty writes the zombie back to disk as
/// a resurrected orphan.
///
/// Also: after rm-rf of a subtree, re-creating a directory with the
/// same path must work — which it won't if some intermediate ancestor
/// is in a half-deleted state.
#[test_log::test]
fn rmrf_leaves_no_orphans() {
    let data = TempDir::new("backupfs_data").unwrap();

    // Phase 1: create nested tree + files
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::create_dir(mnt.join("backup")).unwrap();
            fs::create_dir(mnt.join("backup/volumes")).unwrap();
            fs::create_dir(mnt.join("backup/volumes/main")).unwrap();
            for i in 0..10 {
                // > inline threshold so each file is block-backed and the
                // block-reaping assertion below is meaningful.
                fs::write(
                    mnt.join(format!("backup/volumes/main/f{i:02}")),
                    vec![i as u8; 2 * 1024 * 1024], // > 1 MiB → block-backed
                )
                .unwrap();
            }
        },
        None,
    );

    // Snapshot content-file count after creation (inodes now live in the
    // log, counted via the index below).
    let content_files_after_create = content_files(data.path());

    // Phase 2: rm -rf the whole subtree
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::remove_dir_all(mnt.join("backup")).unwrap();
        },
        None,
    );

    // Phase 3: remount, verify state is clean
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            assert!(
                !mnt.join("backup").exists(),
                "backup/ should be gone after rm-rf"
            );
            // Re-create the exact path rsync would need.
            fs::create_dir(mnt.join("backup")).unwrap();
            fs::create_dir(mnt.join("backup/volumes")).unwrap();
            fs::create_dir(mnt.join("backup/volumes/main")).unwrap();
            assert!(
                mnt.join("backup/volumes/main").is_dir(),
                "could not recreate previously-rm-rf'd path"
            );
        },
        None,
    );

    // After rm-rf + phase-3 re-create, the only live inodes are root + the
    // 3 re-created dirs (no zombies resurrected on replay), and the 10
    // content blocks are reaped.
    let ctrl = crate::ctrl::Controller::new(opts(data.path(), "ohea")).unwrap();
    assert_eq!(
        ctrl.live_inode_count(),
        4,
        "expected root + 3 re-created dirs live, got {}",
        ctrl.live_inode_count()
    );
    drop(ctrl);
    let _ = content_files_after_create;
    let contents_now = content_files(data.path());
    assert_eq!(
        contents_now, 0,
        "orphan content blocks left after rm-rf: expected 0, got {}",
        contents_now
    );
}

/// Full backup-then-delete cycle across mount boundaries. Reproduces
/// the exact shape a user sees when they run a backup, unmount,
/// remount, and `rm -rf` the backup tree: every inode lives on disk
/// from the prior session's flush.
#[test_log::test]
fn rmrf_after_remount_updates_root() {
    let data = TempDir::new("backupfs_data").unwrap();

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::create_dir(mnt.join("backup")).unwrap();
            fs::create_dir(mnt.join("backup/volumes")).unwrap();
            fs::create_dir(mnt.join("backup/volumes/main")).unwrap();
            for i in 0..5 {
                fs::write(
                    mnt.join(format!("backup/volumes/main/f{i:02}")),
                    format!("payload {i}"),
                )
                .unwrap();
            }
        },
        None,
    );

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::remove_dir_all(mnt.join("backup")).unwrap();
            let live: Vec<String> = fs::read_dir(mnt)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            assert!(
                !live.contains(&"backup".to_owned()),
                "root listing still shows deleted subtree: {:?}",
                live
            );
        },
        None,
    );

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let after: Vec<String> = fs::read_dir(mnt)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            assert!(
                !after.contains(&"backup".to_owned()),
                "remount listing still shows deleted subtree: {:?}",
                after
            );
            assert!(
                !mnt.join("backup").exists(),
                "backup/ survived remount after rm-rf"
            );
        },
        None,
    );
}

/// A crash or earlier bug can leave a parent directory whose entries
/// reference an inode file that no longer exists. `rmdir`/`unlink` of
/// the stale entry must not keep returning ENOENT forever — the user
/// has no way to recover if cleanup is impossible.
#[test_log::test]
fn rmdir_heals_stale_parent_reference() {
    let data = TempDir::new("backupfs_data").unwrap();

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::create_dir(mnt.join("parent")).unwrap();
            fs::create_dir(mnt.join("parent/keep")).unwrap();
            fs::create_dir(mnt.join("parent/stale")).unwrap();
        },
        None,
    );

    // Mount, look up "stale" to get its inode number.
    let stale_inode = {
        let (tx, rx) = oneshot::channel::<Option<u64>>();
        let data_path = data.path().to_owned();
        std::thread::spawn(move || {
            with_backupfs(
                &data_path,
                "ohea".to_owned(),
                |mnt| {
                    let meta = fs::metadata(mnt.join("parent/stale")).unwrap();
                    let _ = tx.send(Some(meta.ino()));
                },
                None,
            );
        })
        .join()
        .unwrap();
        rx.recv().unwrap().unwrap()
    };

    // Simulate an unclean shutdown where the parent's dir entry was saved
    // but the child inode is gone: tombstone the child in the log directly,
    // leaving the parent still referencing it.
    let ctrl = crate::ctrl::Controller::new(opts(data.path(), "ohea")).unwrap();
    ctrl.log_tombstone(crate::inode::Inode(stale_inode), true)
        .unwrap();
    drop(ctrl);

    // Now remount and try to clean up the stale entry.
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            // rmdir should succeed (or at worst give ENOENT, which we
            // then want to be recoverable).
            match fs::remove_dir(mnt.join("parent/stale")) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    // Recovery path: try to look up the parent and
                    // confirm listing is clean.
                }
                Err(e) => panic!("unexpected error: {e}"),
            }
            let live: Vec<String> = fs::read_dir(mnt.join("parent"))
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            assert!(
                !live.contains(&"stale".to_owned()),
                "parent listing still shows stale entry: {:?}",
                live
            );
        },
        None,
    );
}

/// Unlink a file after remount: make sure the file disappears from
/// the parent's listing AND the content file is reaped.
#[test_log::test]
fn unlink_file_after_remount() {
    let data = TempDir::new("backupfs_data").unwrap();

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::create_dir(mnt.join("d")).unwrap();
            // > inline threshold → block-backed, so there's a content blob
            // to reap on unlink.
            fs::write(mnt.join("d/keep"), vec![1u8; 2 * 1024 * 1024]).unwrap();
            fs::write(mnt.join("d/kill"), vec![2u8; 2 * 1024 * 1024]).unwrap();
        },
        None,
    );

    let content_before = content_files(data.path());

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::remove_file(mnt.join("d/kill")).unwrap();
            let live: Vec<String> = fs::read_dir(mnt.join("d"))
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            assert_eq!(
                live,
                vec!["keep".to_owned()],
                "live listing contained unlinked file: {:?}",
                live
            );
        },
        None,
    );

    assert_eq!(
        content_files(data.path()),
        content_before - 2,
        "unlinked file's content blocks weren't reaped"
    );
}

/// Create a subtree in one mount session, then in a *fresh* mount
/// session do the removal. Matches the pattern a user hits when they
/// mount, delete, unmount — all inode files exist on disk from the
/// prior session, so the failure mode differs from the same-session
/// case: here gc_inode should find the disk file and remove it.
#[test_log::test]
fn rmdir_after_remount_updates_parent() {
    let data = TempDir::new("backupfs_data").unwrap();

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::create_dir(mnt.join("parent")).unwrap();
            fs::create_dir(mnt.join("parent/keep")).unwrap();
            fs::create_dir(mnt.join("parent/remove")).unwrap();
        },
        None,
    );

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::remove_dir(mnt.join("parent/remove")).unwrap();
            let live: Vec<String> = fs::read_dir(mnt.join("parent"))
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            assert_eq!(
                live,
                vec!["keep".to_owned()],
                "live listing contained removed child: {:?}",
                live
            );
        },
        None,
    );

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let after: Vec<String> = fs::read_dir(mnt.join("parent"))
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            assert_eq!(
                after,
                vec!["keep".to_owned()],
                "remount after removal still listed child: {:?}",
                after
            );
        },
        None,
    );
}

/// Deleting a child must remove it from the parent's live directory
/// listing AND from the persisted copy. Regression test for a batched
/// dirty-cache save racing a stale parent snapshot.
#[test_log::test]
fn rmdir_updates_parent_listing() {
    let data = TempDir::new("backupfs_data").unwrap();
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::create_dir(mnt.join("parent")).unwrap();
            fs::create_dir(mnt.join("parent/keep")).unwrap();
            fs::create_dir(mnt.join("parent/remove")).unwrap();
            fs::remove_dir(mnt.join("parent/remove")).unwrap();

            let live: Vec<String> = fs::read_dir(mnt.join("parent"))
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            assert_eq!(
                live,
                vec!["keep".to_owned()],
                "live listing contained removed child: {:?}",
                live
            );
        },
        None,
    );

    // Remount and verify the listing was persisted.
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let after: Vec<String> = fs::read_dir(mnt.join("parent"))
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            assert_eq!(
                after,
                vec!["keep".to_owned()],
                "remounted listing contained removed child: {:?}",
                after
            );
        },
        None,
    );
}

/// Multiple sequential writes to the same file must land in order on
/// disk, even though they're dispatched through the sharded worker
/// pool. Each write targets its own aligned region; reading back must
/// reproduce the exact pattern we wrote.
///
/// Regression guard against a pool design where different workers could
/// race on the same file's mutex and clobber each other's state.
#[test_log::test]
fn sequential_writes_preserve_order() {
    const WRITES: usize = 64;
    const BLOCK: usize = 4096;
    let data = TempDir::new("backupfs_data").unwrap();
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let path = mnt.join("file");
            let mut f = fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&path)
                .unwrap();
            for i in 0..WRITES {
                let mut chunk = vec![0u8; BLOCK];
                pattern_fill((i * BLOCK) as u64, &mut chunk);
                f.write_all(&chunk).unwrap();
            }
            drop(f);
            let read = fs::read(&path).unwrap();
            assert_eq!(read.len(), WRITES * BLOCK);
            pattern_check(0, &read);
        },
        None,
    );
}

/// `stat.st_blocks` must be in 512-byte units (POSIX). du-sh relies on this.
/// Regression test for blocks being reported in 4096-byte units.
#[test_log::test]
fn stat_blocks_units() {
    let data = TempDir::new("backupfs_data").unwrap();
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            // Write a 1 MiB file
            let size: usize = 1024 * 1024;
            let mut buf = vec![0u8; size];
            pattern_fill(0, &mut buf);
            fs::write(mnt.join("big"), &buf).unwrap();

            let meta = fs::metadata(mnt.join("big")).unwrap();
            assert_eq!(meta.size() as usize, size);
            // st_blocks is in 512-byte units. For a 1 MiB file, expect ~2048 blocks.
            // Must be at least ceil(size / 512) and within ~2x (allowing for padding).
            let expected_min = size.div_ceil(512) as u64;
            assert!(
                meta.blocks() >= expected_min,
                "blocks={} too small for {} bytes (expected >= {})",
                meta.blocks(),
                size,
                expected_min
            );
            assert!(
                meta.blocks() <= expected_min * 4,
                "blocks={} unreasonably large for {} bytes (expected <= {})",
                meta.blocks(),
                size,
                expected_min * 4
            );
        },
        None,
    );
}

/// Write a file larger than the 1 MiB BufferedDirectFile window, read back,
/// verify every byte matches the deterministic pattern. Catches reordering,
/// truncation, and partial-flush bugs.
#[test_log::test]
fn large_file_integrity() {
    let data = TempDir::new("backupfs_data").unwrap();
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            // 3.5 MiB — spans 4 windows, with a partial last window
            let size: usize = 3 * 1024 * 1024 + 512 * 1024 + 7;
            let mut buf = vec![0u8; size];
            pattern_fill(0, &mut buf);
            fs::write(mnt.join("bigdata"), &buf).unwrap();

            let readback = fs::read(mnt.join("bigdata")).unwrap();
            assert_eq!(readback.len(), size);
            pattern_check(0, &readback);
        },
        None,
    );
}

/// Write in multiple chunks at varying offsets (including crossing window
/// boundaries), verify the complete file.
#[test_log::test]
fn random_access_writes() {
    let data = TempDir::new("backupfs_data").unwrap();
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let size: u64 = 2 * 1024 * 1024 + 500_000; // ~2.5 MiB
            let path = mnt.join("scattered");

            // Create file and write chunks in a non-sequential order
            let mut f = fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&path)
                .unwrap();
            // First extend to full size
            f.set_len(size).unwrap();

            // Write in an order that forces multiple window evictions.
            // Ranges must partition [0, size) — every byte covered exactly once.
            let ranges: &[(u64, usize)] = &[
                (1_500_000, 600_000),                   // crosses window 1→2
                (0, 100_000),                           // early in window 0
                (2_100_000, size as usize - 2_100_000), // tail
                (500_000, 1_000_000),                   // spans window 0→1
                (100_000, 400_000),                     // fills gap in window 0
            ];
            for &(offset, len) in ranges {
                let mut buf = vec![0u8; len];
                pattern_fill(offset, &mut buf);
                f.seek(SeekFrom::Start(offset)).unwrap();
                f.write_all(&buf).unwrap();
            }
            drop(f);

            let readback = fs::read(&path).unwrap();
            assert_eq!(readback.len() as u64, size);
            pattern_check(0, &readback);
        },
        None,
    );
}

/// Persist, remount, and verify data integrity — catches flush/Drop bugs.
#[test_log::test]
fn large_file_persists_across_remount() {
    let data = TempDir::new("backupfs_data").unwrap();
    let size: usize = 2 * 1024 * 1024 + 3;

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let mut buf = vec![0u8; size];
            pattern_fill(0, &mut buf);
            fs::write(mnt.join("persisted"), &buf).unwrap();
        },
        None,
    );

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let readback = fs::read(mnt.join("persisted")).unwrap();
            assert_eq!(readback.len(), size);
            pattern_check(0, &readback);
        },
        None,
    );
}

/// Partial overwrite: write a file, then rewrite a middle range. Verify
/// unchanged regions are preserved and overwritten regions have new data.
#[test_log::test]
fn partial_overwrite_preserves_surrounding_data() {
    let data = TempDir::new("backupfs_data").unwrap();
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let size: u64 = 3 * 1024 * 1024;
            let path = mnt.join("overwritten");

            // Initial write: pattern keyed on offset
            let mut buf = vec![0u8; size as usize];
            pattern_fill(0, &mut buf);
            fs::write(&path, &buf).unwrap();

            // Overwrite middle region with a different pattern (offset by constant)
            let overwrite_start: u64 = 800_000;
            let overwrite_len: usize = 700_000; // spans window boundary
            let overwrite_offset = 0xDEAD_BEEF_u64;
            let mut overlay = vec![0u8; overwrite_len];
            pattern_fill(overwrite_offset, &mut overlay);

            let mut f = fs::OpenOptions::new().write(true).open(&path).unwrap();
            f.seek(SeekFrom::Start(overwrite_start)).unwrap();
            f.write_all(&overlay).unwrap();
            drop(f);

            // Read back and verify
            let readback = fs::read(&path).unwrap();
            assert_eq!(readback.len() as u64, size);

            // Prefix: original pattern
            pattern_check(0, &readback[..overwrite_start as usize]);
            // Middle: overlay pattern
            pattern_check(
                overwrite_offset,
                &readback[overwrite_start as usize..overwrite_start as usize + overwrite_len],
            );
            // Suffix: original pattern resumes at overwrite_start + overwrite_len
            pattern_check(
                overwrite_start + overwrite_len as u64,
                &readback[overwrite_start as usize + overwrite_len..],
            );
        },
        None,
    );
}

/// Regression test for the "rename-over-existing" pattern used by
/// atomic save helpers: write to `.name.tmp`, then `rename(.name.tmp,
/// name)` to atomically replace the destination. After the rename the
/// new name must resolve — `ls` showing the entry while `cat` returns
/// ENOENT was the reported symptom.
#[test_log::test]
fn rename_over_existing_resolves() {
    let data = TempDir::new("backupfs_data").unwrap();
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::write(mnt.join("os-backup.json"), b"old").unwrap();
            fs::write(mnt.join(".os-backup.json.tmp"), b"new contents").unwrap();
            fs::rename(mnt.join(".os-backup.json.tmp"), mnt.join("os-backup.json")).unwrap();

            let live: Vec<String> = fs::read_dir(mnt)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            assert!(
                live.iter().any(|n| n == "os-backup.json"),
                "listing missing os-backup.json: {live:?}"
            );
            assert!(
                !live.iter().any(|n| n == ".os-backup.json.tmp"),
                "listing still has tmp after rename: {live:?}"
            );

            assert_eq!(
                fs::read(mnt.join("os-backup.json")).unwrap(),
                b"new contents"
            );
        },
        None,
    );

    // Same check after a remount — disk state has to be consistent.
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let live: Vec<String> = fs::read_dir(mnt)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            assert!(
                live.iter().any(|n| n == "os-backup.json"),
                "post-remount listing missing os-backup.json: {live:?}"
            );
            assert_eq!(
                fs::read(mnt.join("os-backup.json")).unwrap(),
                b"new contents"
            );
        },
        None,
    );
}

/// Real-world pattern: a backup root that also holds several sibling
/// directories (service subdirs) alongside `os-backup.json`. We write
/// the atomic-save over it, then cleanly unmount and remount, then
/// repeat the atomic save — cumulative cross-session state is what
/// users actually run into.
#[test_log::test]
fn rename_over_existing_with_siblings_across_remounts() {
    let data = TempDir::new("backupfs_data").unwrap();

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            for d in [
                "actual-budget",
                "bitcoind",
                "btcpayserver",
                "luks",
                "vaultwarden",
            ] {
                fs::create_dir(mnt.join(d)).unwrap();
            }
            fs::write(mnt.join("os-backup.json"), b"first").unwrap();
        },
        None,
    );

    for round in 0..5 {
        with_backupfs(
            data.path(),
            "ohea".to_owned(),
            |mnt| {
                let tmp = mnt.join(".os-backup.json.tmp");
                {
                    let mut f = fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(&tmp)
                        .unwrap();
                    f.write_all(format!("round-{round}").as_bytes()).unwrap();
                    f.sync_all().unwrap();
                }
                fs::rename(&tmp, mnt.join("os-backup.json")).unwrap();

                let got = fs::read(mnt.join("os-backup.json")).unwrap();
                assert_eq!(got, format!("round-{round}").as_bytes());
            },
            None,
        );

        // Fresh mount: must still see os-backup.json and be able to
        // read it back. This is the exact pattern the user reported
        // failing ("ls shows it, cat says ENOENT").
        with_backupfs(
            data.path(),
            "ohea".to_owned(),
            |mnt| {
                let live: Vec<String> = fs::read_dir(mnt)
                    .unwrap()
                    .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                    .collect();
                assert!(
                    live.iter().any(|n| n == "os-backup.json"),
                    "round {round}: post-remount ls missing os-backup.json: {live:?}"
                );
                let got = fs::read(mnt.join("os-backup.json"))
                    .unwrap_or_else(|e| panic!("round {round}: post-remount cat failed: {e}"));
                assert_eq!(got, format!("round-{round}").as_bytes());
            },
            None,
        );
    }
}

/// Repeated rename-over-existing (the "atomic save" pattern applied
/// many times). Each round overwrites the previous version and must
/// leave exactly one resolvable entry.
#[test_log::test]
fn rename_over_existing_many_rounds() {
    let data = TempDir::new("backupfs_data").unwrap();
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            for i in 0..20 {
                let payload = format!("round-{i}");
                let tmp = mnt.join(".os-backup.json.tmp");
                {
                    let mut f = fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(&tmp)
                        .unwrap();
                    f.write_all(payload.as_bytes()).unwrap();
                    f.sync_all().unwrap();
                }
                fs::rename(&tmp, mnt.join("os-backup.json")).unwrap();

                let got = fs::read(mnt.join("os-backup.json")).unwrap();
                assert_eq!(got, payload.as_bytes(), "round {i}: read back wrong bytes");
                let live: Vec<String> = fs::read_dir(mnt)
                    .unwrap()
                    .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                    .collect();
                assert!(
                    !live.iter().any(|n| n == ".os-backup.json.tmp"),
                    "round {i}: tmp still visible: {live:?}"
                );
            }
        },
        None,
    );

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            assert_eq!(
                fs::read(mnt.join("os-backup.json")).unwrap(),
                b"round-19",
                "final round not persisted"
            );
        },
        None,
    );
}

/// Recovery path: a rename over an entry whose target inode file is
/// already gone from disk (legacy damage from the pre-fix crash
/// window) must succeed — otherwise the bad name is unrecoverable and
/// no future backup can write to it.
#[test_log::test]
fn rename_over_stale_parent_reference_recovers() {
    let data = TempDir::new("backupfs_data").unwrap();

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::write(mnt.join("os-backup.json"), b"old").unwrap();
        },
        None,
    );

    // Capture the inode number for os-backup.json.
    let stale_inode = {
        let (tx, rx) = oneshot::channel::<u64>();
        let data_path = data.path().to_owned();
        std::thread::spawn(move || {
            with_backupfs(
                &data_path,
                "ohea".to_owned(),
                |mnt| {
                    let meta = fs::metadata(mnt.join("os-backup.json")).unwrap();
                    let _ = tx.send(meta.ino());
                },
                None,
            );
        })
        .join()
        .unwrap();
        rx.recv().unwrap()
    };

    // Simulate the post-crash state: the parent's dir entry still
    // references os-backup.json → stale_inode, but stale_inode is gone from
    // the log (as if gc_inode ran but the parent's new state was lost to a
    // lazy unmount). Tombstone it directly.
    let ctrl = crate::ctrl::Controller::new(opts(data.path(), "ohea")).unwrap();
    ctrl.log_tombstone(crate::inode::Inode(stale_inode), true)
        .unwrap();
    drop(ctrl);

    // Now do the backup pattern: write .tmp, rename over the ghost.
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let tmp = mnt.join(".os-backup.json.tmp");
            let dst = mnt.join("os-backup.json");
            fs::write(&tmp, b"new contents").unwrap();
            fs::rename(&tmp, &dst).expect("rename over stale entry must succeed");
            assert_eq!(fs::read(&dst).unwrap(), b"new contents");
        },
        None,
    );

    // And it should persist across a remount.
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            assert_eq!(
                fs::read(mnt.join("os-backup.json")).unwrap(),
                b"new contents"
            );
        },
        None,
    );
}

/// `fsync(dirfd)` on the mount is the only durability checkpoint that
/// actually reaches a non-bdev FUSE daemon. Verify both that it hits
/// our handler and that it drains the dirty cache — so callers can
/// force a flush before `umount -l` (which would otherwise tear down
/// the backing FS before the backup-fs daemon's destroy gets to run).
#[test_log::test]
fn fsync_on_dirfd_reaches_handler_and_flushes() {
    use std::os::fd::AsRawFd;
    use std::sync::atomic::Ordering;
    let data = TempDir::new("backupfs_data").unwrap();
    let before = crate::FSYNCDIR_CALL_COUNT.load(Ordering::Relaxed);
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::write(mnt.join("a"), b"hello").unwrap();
            let dir = std::fs::File::open(mnt).unwrap();
            // Call fsync via libc since std's File::sync_all on a
            // directory may error on some platforms.
            let ret = unsafe { libc::fsync(dir.as_raw_fd()) };
            assert_eq!(
                ret,
                0,
                "fsync(dirfd) failed: {:?}",
                io::Error::last_os_error()
            );
        },
        None,
    );
    let after = crate::FSYNCDIR_CALL_COUNT.load(Ordering::Relaxed);
    assert!(after > before, "FUSE_FSYNCDIR never reached the handler");
}

/// Mirrors start-os's AtomicFile pattern exactly: create tmp, write,
/// fsync, drop fd, rename, then immediately unmount. The user reports
/// this sequence loses data on clean unmount.
#[test_log::test]
fn rename_immediately_before_unmount() {
    let data = TempDir::new("backupfs_data").unwrap();

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::write(mnt.join("os-backup.json"), b"old").unwrap();
        },
        None,
    );

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let tmp = mnt.join(".os-backup.json.tmp");
            let dst = mnt.join("os-backup.json");
            {
                let mut f = fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&tmp)
                    .unwrap();
                f.write_all(b"new contents").unwrap();
                f.sync_all().unwrap();
            }
            fs::rename(&tmp, &dst).unwrap();
            // Do nothing else — let Drop trigger unmount right here.
        },
        None,
    );

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let live: Vec<String> = fs::read_dir(mnt)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            assert!(
                live.iter().any(|n| n == "os-backup.json"),
                "post-remount listing missing os-backup.json: {live:?}"
            );
            let got = fs::read(mnt.join("os-backup.json"))
                .unwrap_or_else(|e| panic!("post-remount cat failed: {e} — listing was {live:?}"));
            assert_eq!(got, b"new contents");
        },
        None,
    );
}

/// As above but across a remount between the create+write+tmp and the
/// rename — models a backup process that is killed part-way through an
/// atomic save and resumes in a fresh mount.
#[test_log::test]
fn rename_over_existing_across_remount() {
    let data = TempDir::new("backupfs_data").unwrap();
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::write(mnt.join("os-backup.json"), b"old").unwrap();
            fs::write(mnt.join(".os-backup.json.tmp"), b"new contents").unwrap();
        },
        None,
    );

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::rename(mnt.join(".os-backup.json.tmp"), mnt.join("os-backup.json")).unwrap();
            assert_eq!(
                fs::read(mnt.join("os-backup.json")).unwrap(),
                b"new contents"
            );
        },
        None,
    );

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let live: Vec<String> = fs::read_dir(mnt)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            assert!(
                live.iter().any(|n| n == "os-backup.json"),
                "post-remount listing missing os-backup.json: {live:?}"
            );
            assert!(
                !live.iter().any(|n| n == ".os-backup.json.tmp"),
                "tmp name persisted after rename: {live:?}"
            );
            assert_eq!(
                fs::read(mnt.join("os-backup.json")).unwrap(),
                b"new contents"
            );
        },
        None,
    );
}

// ── Redesign coverage: block store, ECC, device nodes ──────────────

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::FileTypeExt;

use crate::ctrl::Controller;
use crate::inode::{ContentId, Inode};

fn opts(data: &Path, password: &str) -> BackupFSOptions {
    BackupFSOptions {
        data_dir: data.to_owned(),
        setuid_support: false,
        password: password.to_owned(),
        file_size_padding: None,
        readonly: false,
        idmapped_root: vec![],
    }
}

/// Mount, run `f` to obtain a value, unmount, return it. Used to capture
/// an inode number from inside a session (the FS is only readable while
/// mounted).
fn capture<T: Send + 'static>(
    data: &Path,
    password: &str,
    f: impl FnOnce(&Path) -> T + Send + 'static,
) -> T {
    let (tx, rx) = oneshot::channel::<T>();
    let data_path = data.to_owned();
    let password = password.to_owned();
    std::thread::spawn(move || {
        with_backupfs(
            &data_path,
            password,
            move |mnt| {
                let _ = tx.send(f(mnt));
            },
            None,
        );
    })
    .join()
    .unwrap();
    rx.recv().unwrap()
}

fn sha256_file(path: &Path) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    Sha256::digest(fs::read(path).unwrap()).to_vec()
}

/// A FIFO created through the mount must round-trip its file type across a
/// remount — exercises mknod(S_IFIFO) end to end (no privilege needed,
/// unlike char/block devices).
#[test_log::test]
fn mkfifo_persists_across_remount() {
    let data = TempDir::new("backupfs_data").unwrap();
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let p = mnt.join("pipe");
            let c = CString::new(p.as_os_str().as_bytes()).unwrap();
            let r = unsafe { libc::mkfifo(c.as_ptr(), 0o644) };
            assert_eq!(r, 0, "mkfifo failed: {}", io::Error::last_os_error());
            assert!(
                fs::symlink_metadata(&p).unwrap().file_type().is_fifo(),
                "created node is not a FIFO"
            );
        },
        None,
    );
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            assert!(
                fs::symlink_metadata(mnt.join("pipe"))
                    .unwrap()
                    .file_type()
                    .is_fifo(),
                "FIFO type was not preserved across remount"
            );
        },
        None,
    );
}

/// Unit-level check that the device-number attribute round-trips through
/// the inode model and surfaces in `FileAttr.rdev` with the right type.
#[test_log::test]
fn char_device_rdev_roundtrip() {
    use crate::inode::{FileData, InodeAttributes};
    let rdev = 0x0103; // e.g. /dev/null-ish major:minor
    let attrs = InodeAttributes::new(Inode(42), None, FileData::CharDevice(rdev));
    let fa: fuser::FileAttr = (&attrs).into();
    assert_eq!(fa.rdev, rdev);
    assert_eq!(fa.kind, fuser::FileType::CharDevice);
    assert_eq!(fa.rdev, attrs.attrs.contents.rdev());
}

/// Corrupting bytes inside one block file on disk (simulating bit rot)
/// must be transparently repaired by the per-block Reed-Solomon parity:
/// the file still reads back byte-for-byte.
#[test_log::test]
fn ecc_recovers_corrupted_block() {
    let data = TempDir::new("backupfs_data").unwrap();
    let size = 2 * 1024 * 1024usize; // > 1 MiB → block-backed (has block files)
    let mut payload = vec![0u8; size];
    pattern_fill(0, &mut payload);
    let payload2 = payload.clone();

    let ino = capture(data.path(), "ohea", move |mnt| {
        fs::write(mnt.join("data"), &payload2).unwrap();
        fs::metadata(mnt.join("data")).unwrap().ino()
    });

    // Locate the block file and flip a small contiguous run of bytes at
    // the very start of the shard region — damages a single shard, well
    // within the parity budget.
    let ctrl = Controller::new(opts(data.path(), "ohea")).unwrap();
    let block = ctrl.resolve_block_path(ContentId(ino), 0);
    drop(ctrl);
    assert!(block.exists(), "expected block file at {block:?}");
    let mut bytes = fs::read(&block).unwrap();
    let start = 28 + 4; // skip header + first shard's CRC → corrupt shard data
    for b in &mut bytes[start..start + 16] {
        *b ^= 0xFF;
    }
    fs::write(&block, &bytes).unwrap();

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let got = fs::read(mnt.join("data")).unwrap();
            assert_eq!(got.len(), size);
            pattern_check(0, &got);
        },
        None,
    );
}

/// Overwriting a small region of a multi-block file must rewrite only the
/// affected block file and leave the others byte-for-byte identical on
/// disk — the property that makes rsync/rclone incremental copies cheap.
#[test_log::test]
fn incremental_overwrite_touches_one_block() {
    let data = TempDir::new("backupfs_data").unwrap();
    let size = 3 * 1024 * 1024usize; // exactly 3 blocks
    let mut payload = vec![0u8; size];
    pattern_fill(0, &mut payload);
    let payload2 = payload.clone();

    let ino = capture(data.path(), "ohea", move |mnt| {
        fs::write(mnt.join("big"), &payload2).unwrap();
        fs::metadata(mnt.join("big")).unwrap().ino()
    });

    let ctrl = Controller::new(opts(data.path(), "ohea")).unwrap();
    let paths: Vec<_> = (0..3)
        .map(|i| ctrl.resolve_block_path(ContentId(ino), i))
        .collect();
    drop(ctrl);
    for p in &paths {
        assert!(p.exists(), "missing block file {p:?}");
    }
    let before: Vec<_> = paths.iter().map(|p| sha256_file(p)).collect();

    // Overwrite 100 bytes inside block 1 only.
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .open(mnt.join("big"))
                .unwrap();
            f.seek(SeekFrom::Start(CHUNK_OFFSET + 4096)).unwrap();
            f.write_all(&[0xABu8; 100]).unwrap();
        },
        None,
    );

    let after: Vec<_> = paths.iter().map(|p| sha256_file(p)).collect();
    assert_eq!(before[0], after[0], "block 0 was rewritten unnecessarily");
    assert_ne!(before[1], after[1], "block 1 should have changed");
    assert_eq!(before[2], after[2], "block 2 was rewritten unnecessarily");

    // And the edit is correct on read-back.
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let got = fs::read(mnt.join("big")).unwrap();
            pattern_check(0, &got[..(CHUNK_OFFSET as usize + 4096)]);
            assert_eq!(&got[CHUNK_OFFSET as usize + 4096..][..100], &[0xABu8; 100]);
            pattern_check(
                CHUNK_OFFSET + 4096 + 100,
                &got[CHUNK_OFFSET as usize + 4096 + 100..],
            );
        },
        None,
    );
}

const CHUNK_OFFSET: u64 = 1024 * 1024; // one CHUNK_SIZE

/// A sparse file (size set, middle written) stores only the touched
/// blocks: holes occupy no block files on disk and read back as zeros.
#[test_log::test]
fn sparse_file_omits_hole_blocks() {
    let data = TempDir::new("backupfs_data").unwrap();
    let total = 3 * 1024 * 1024u64;

    let ino = capture(data.path(), "ohea", move |mnt| {
        let f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(mnt.join("sparse"))
            .unwrap();
        f.set_len(total).unwrap();
        // Write only into block 2.
        let mut f = f;
        f.seek(SeekFrom::Start(2 * CHUNK_OFFSET + 123)).unwrap();
        f.write_all(b"hello sparse world").unwrap();
        fs::metadata(mnt.join("sparse")).unwrap().ino()
    });

    let ctrl = Controller::new(opts(data.path(), "ohea")).unwrap();
    assert!(
        !ctrl.resolve_block_path(ContentId(ino), 0).exists(),
        "hole block 0 should not exist on disk"
    );
    assert!(
        !ctrl.resolve_block_path(ContentId(ino), 1).exists(),
        "hole block 1 should not exist on disk"
    );
    assert!(
        ctrl.resolve_block_path(ContentId(ino), 2).exists(),
        "written block 2 should exist on disk"
    );
    drop(ctrl);

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let got = fs::read(mnt.join("sparse")).unwrap();
            assert_eq!(got.len() as u64, total);
            assert!(got[..2 * CHUNK_OFFSET as usize].iter().all(|&b| b == 0));
            assert_eq!(
                &got[2 * CHUNK_OFFSET as usize + 123..][..18],
                b"hello sparse world"
            );
        },
        None,
    );
}

/// The superblock is the one unrecoverable single point of failure (lose it
/// → the master key is gone → no data is readable). It is written as
/// redundant copies, so losing or corrupting the primary must not brick the
/// backup, and the damaged copy should self-heal on the next mount.
#[test_log::test]
fn superblock_survives_primary_loss_and_self_heals() {
    let data = TempDir::new("backupfs_data").unwrap();
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::write(mnt.join("file"), b"important backup data").unwrap();
        },
        None,
    );

    let primary = data.path().join("superblock");
    let backup = data.path().join("superblock.bak1");
    assert!(primary.exists(), "primary superblock missing");
    assert!(backup.exists(), "redundant superblock copy was not written");

    // Simulate total loss of the primary (deleted / lost dir entry).
    fs::remove_file(&primary).unwrap();

    // Remount: must recover from the backup, read data back, and re-heal
    // the primary copy.
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            assert_eq!(
                fs::read(mnt.join("file")).unwrap(),
                b"important backup data"
            );
        },
        None,
    );
    assert!(primary.exists(), "primary superblock was not self-healed");

    // Now corrupt the backup instead (zero it out) and confirm recovery
    // from the (healed) primary.
    fs::write(&backup, b"\x00\x00\x00\x00").unwrap();
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            assert_eq!(
                fs::read(mnt.join("file")).unwrap(),
                b"important backup data"
            );
        },
        None,
    );
}

/// A file several times larger than the in-memory write buffer must spill
/// completed blocks to disk mid-write (bounding memory) and still read back
/// byte-for-byte, both live and across a remount. Guards the eager-spill
/// path added to bound dirty memory under FOPEN_DIRECT_IO.
#[test_log::test]
fn large_file_spills_and_stays_intact() {
    let data = TempDir::new("backupfs_data").unwrap();
    // 40 MiB > the 16 MiB default write buffer → forces several spills.
    let size = 40 * 1024 * 1024usize;
    let mut buf = vec![0u8; size];
    pattern_fill(0, &mut buf);

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            // Stream it in 256 KiB writes, like a real copy would.
            let mut f = fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(mnt.join("big"))
                .unwrap();
            for chunk in buf.chunks(256 * 1024) {
                f.write_all(chunk).unwrap();
            }
            f.sync_all().unwrap();
            let readback = fs::read(mnt.join("big")).unwrap();
            assert_eq!(readback.len(), size);
            pattern_check(0, &readback);
        },
        None,
    );

    // 40 MiB / 1 MiB chunk = 40 block files.
    assert_eq!(
        tree(data.path().join("contents"), false).unwrap().len(),
        40,
        "unexpected block file count"
    );

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let readback = fs::read(mnt.join("big")).unwrap();
            assert_eq!(readback.len(), size);
            pattern_check(0, &readback);
        },
        None,
    );
}

/// A directory larger than the spill threshold stores its entries in bucket
/// files (not inline in the inode) yet behaves identically: every entry is
/// listable, lookup-able, and readable; it survives a remount; unlink
/// removes exactly one entry; and rm -rf reaps every bucket file.
#[test_log::test]
fn large_directory_spills_and_stays_consistent() {
    let data = TempDir::new("backupfs_data").unwrap();
    let n = 2500usize; // > the 1024 spill threshold → spills

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let d = mnt.join("big");
            fs::create_dir(&d).unwrap();
            for i in 0..n {
                fs::write(d.join(format!("f{i:05}")), format!("data {i}")).unwrap();
            }
            let listed = fs::read_dir(&d).unwrap().count();
            assert_eq!(listed, n, "readdir of spilled dir missing entries");
            for i in (0..n).step_by(137) {
                assert_eq!(
                    fs::read(d.join(format!("f{i:05}"))).unwrap(),
                    format!("data {i}").as_bytes()
                );
            }
        },
        None,
    );

    // The listing must actually have spilled to bucket files on disk.
    assert!(
        data.path().join("dirents").exists()
            && tree(data.path().join("dirents"), false).unwrap().len() > 1,
        "expected multiple directory bucket files for a spilled directory"
    );

    // Remount: everything is still there; unlink half; remainder intact.
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let d = mnt.join("big");
            assert_eq!(fs::read_dir(&d).unwrap().count(), n);
            for i in (0..n).step_by(2) {
                fs::remove_file(d.join(format!("f{i:05}"))).unwrap();
            }
            assert_eq!(fs::read_dir(&d).unwrap().count(), n / 2);
            for i in (1..n).step_by(2) {
                assert_eq!(
                    fs::read(d.join(format!("f{i:05}"))).unwrap(),
                    format!("data {i}").as_bytes()
                );
            }
        },
        None,
    );

    // rm -rf the spilled directory; remount shows it gone and no buckets leak.
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| fs::remove_dir_all(mnt.join("big")).unwrap(),
        None,
    );
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| assert!(!mnt.join("big").exists(), "spilled dir survived rm -rf"),
        None,
    );
    let leftover = tree(data.path().join("dirents"), false).unwrap_or_default();
    assert!(
        leftover.is_empty(),
        "leftover dir bucket files after rm -rf: {leftover:?}"
    );
}

/// rename within a spilled directory, across into another directory, and
/// rename-over-existing — all must keep the entries resolvable and persist.
#[test_log::test]
fn spilled_directory_rename_paths() {
    let data = TempDir::new("backupfs_data").unwrap();
    let n = 1500usize; // spills

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::create_dir(mnt.join("a")).unwrap();
            fs::create_dir(mnt.join("b")).unwrap();
            for i in 0..n {
                fs::write(mnt.join(format!("a/f{i:05}")), format!("{i}")).unwrap();
            }
            // rename within the spilled directory
            fs::rename(mnt.join("a/f00000"), mnt.join("a/renamed")).unwrap();
            assert!(!mnt.join("a/f00000").exists());
            assert_eq!(fs::read(mnt.join("a/renamed")).unwrap(), b"0");
            // rename across to a small (inline) directory
            fs::rename(mnt.join("a/f00001"), mnt.join("b/moved")).unwrap();
            assert!(!mnt.join("a/f00001").exists());
            assert_eq!(fs::read(mnt.join("b/moved")).unwrap(), b"1");
            // rename-over-existing within the spilled directory
            fs::write(mnt.join("a/target"), b"old").unwrap();
            fs::rename(mnt.join("a/f00002"), mnt.join("a/target")).unwrap();
            assert!(!mnt.join("a/f00002").exists());
            assert_eq!(fs::read(mnt.join("a/target")).unwrap(), b"2");
        },
        None,
    );

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            assert_eq!(fs::read(mnt.join("a/renamed")).unwrap(), b"0");
            assert_eq!(fs::read(mnt.join("b/moved")).unwrap(), b"1");
            assert_eq!(fs::read(mnt.join("a/target")).unwrap(), b"2");
            // count: started 1500, moved 2 out (renamed in place keeps count),
            // overwrote target (net: -2 from a, +target). renamed: f00000->renamed,
            // f00001 moved out, f00002 -> target (overwrote a new 'target').
            // a has: 1500 - 1(f00001 moved) = 1499 names still (renamed counts as 1, target counts as 1, f00002 gone but target existed).
            // Simpler: just assert the three probes above; exact count is fiddly.
            assert!(!mnt.join("a/f00001").exists());
        },
        None,
    );
}

/// A small file's content is stored inline in its inode record (no content
/// block file), and survives fsync, repeated flushes, and a remount.
/// Regression: an earlier inline flush drained the in-memory buffer, so a
/// second flush (fsync-then-close, as in the atomic-save pattern) persisted
/// empty bytes — the file read back as zeros.
#[test_log::test]
fn inline_small_file_survives_fsync_and_remount() {
    let data = TempDir::new("backupfs_data").unwrap();
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let p = mnt.join("tiny");
            let mut f = fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&p)
                .unwrap();
            f.write_all(b"hello inline").unwrap();
            f.sync_all().unwrap(); // first flush — used to drain the buffer
            assert_eq!(fs::read(&p).unwrap(), b"hello inline", "lost after fsync");
            f.write_all(b" more").unwrap();
            f.sync_all().unwrap();
            drop(f);
            assert_eq!(fs::read(&p).unwrap(), b"hello inline more");
        },
        None,
    );
    // Inline → no content block on disk.
    assert_eq!(content_files(data.path()), 0, "tiny file should be inline");
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            assert_eq!(fs::read(mnt.join("tiny")).unwrap(), b"hello inline more");
        },
        None,
    );
}

/// A file that grows straight past one chunk migrates inline→blocks (the
/// jump skips the packed tier), preserving the bytes already written inline,
/// and reads back correctly live and across a remount.
#[test_log::test]
fn file_grows_from_inline_to_blocks() {
    let data = TempDir::new("backupfs_data").unwrap();
    let total = 3 * 1024 * 1024usize; // > 1 MiB → block-backed
    let mut buf = vec![0u8; total];
    pattern_fill(0, &mut buf);

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let p = mnt.join("grower");
            let mut f = fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&p)
                .unwrap();
            // First a sub-threshold write (stored inline), then extend past
            // one chunk (forces the inline→blocks migration).
            f.write_all(&buf[..3000]).unwrap();
            f.write_all(&buf[3000..]).unwrap();
            f.sync_all().unwrap();
            drop(f);
            let got = fs::read(&p).unwrap();
            assert_eq!(got.len(), total);
            pattern_check(0, &got);
        },
        None,
    );
    // Now block-backed: at least one content block exists.
    assert!(
        content_files(data.path()) >= 1,
        "expected a content block after growth"
    );
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let got = fs::read(mnt.join("grower")).unwrap();
            assert_eq!(got.len(), total);
            pattern_check(0, &got);
        },
        None,
    );
}

/// A medium file (between the inline threshold and one chunk) is stored as a
/// single packed extent in the shared content log — no per-file content
/// block — yet reads back correctly, survives a remount, and its extent is
/// reaped on delete.
#[test_log::test]
fn medium_file_is_packed_not_blocked() {
    let data = TempDir::new("backupfs_data").unwrap();
    let size = 64 * 1024usize; // 64 KiB ∈ (4 KiB, 1 MiB] → packed
    let mut buf = vec![0u8; size];
    pattern_fill(0, &mut buf);

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::write(mnt.join("m"), &buf).unwrap();
            let got = fs::read(mnt.join("m")).unwrap();
            assert_eq!(got.len(), size);
            pattern_check(0, &got);
        },
        None,
    );
    // Packed → no content block files; the extent lives in the log.
    assert_eq!(
        content_files(data.path()),
        0,
        "packed file must not create a content block"
    );
    {
        let ctrl = crate::ctrl::Controller::new(opts(data.path(), "ohea")).unwrap();
        assert_eq!(ctrl.live_content_count(), 1, "expected one packed extent");
    }

    // Survives remount, then delete reaps the extent.
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let got = fs::read(mnt.join("m")).unwrap();
            assert_eq!(got.len(), size);
            pattern_check(0, &got);
            fs::remove_file(mnt.join("m")).unwrap();
        },
        None,
    );
    {
        let ctrl = crate::ctrl::Controller::new(opts(data.path(), "ohea")).unwrap();
        assert_eq!(
            ctrl.live_content_count(),
            0,
            "packed extent not reaped on delete"
        );
        assert_eq!(ctrl.live_inode_count(), 1, "only root should remain");
    }
}

/// A packed file that grows past one chunk migrates to block storage and its
/// stale packed extent is dropped.
#[test_log::test]
fn packed_grows_to_blocks() {
    let data = TempDir::new("backupfs_data").unwrap();
    let total = 3 * 1024 * 1024usize; // > 1 MiB → must end up block-backed
    let mut buf = vec![0u8; total];
    pattern_fill(0, &mut buf);
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let p = mnt.join("g");
            let mut f = fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&p)
                .unwrap();
            f.write_all(&buf[..64 * 1024]).unwrap(); // packed
            f.write_all(&buf[64 * 1024..]).unwrap(); // grows → blocks
            f.sync_all().unwrap();
            drop(f);
            let got = fs::read(&p).unwrap();
            assert_eq!(got.len(), total);
            pattern_check(0, &got);
        },
        None,
    );
    assert!(
        content_files(data.path()) >= 1,
        "expected block files after growth"
    );
    {
        let ctrl = crate::ctrl::Controller::new(opts(data.path(), "ohea")).unwrap();
        assert_eq!(
            ctrl.live_content_count(),
            0,
            "stale packed extent should be tombstoned"
        );
    }
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let got = fs::read(mnt.join("g")).unwrap();
            assert_eq!(got.len(), total);
            pattern_check(0, &got);
        },
        None,
    );
}

/// Total bytes across the log segment files.
fn segment_bytes(data: &Path) -> u64 {
    let dir = data.join("segments");
    let Ok(rd) = fs::read_dir(&dir) else { return 0 };
    rd.flatten()
        .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
        .sum()
}

/// Deleting most files leaves dead extents in the log; the unmount
/// compaction pass reclaims the heavily-dead sealed segments while the
/// surviving files relocate intact (verbatim) and still read correctly.
#[test_log::test]
fn compaction_reclaims_dead_space() {
    let data = TempDir::new("backupfs_data").unwrap();
    let n = 220usize; // ~14 MiB of 64 KiB files → spans multiple 8 MiB segments
    let payload = |i: usize| {
        let mut b = vec![0u8; 64 * 1024];
        pattern_fill(i as u64, &mut b);
        b
    };

    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            for i in 0..n {
                fs::write(mnt.join(format!("f{i:04}")), payload(i)).unwrap();
            }
        },
        None,
    );
    let before = segment_bytes(data.path());
    assert!(
        before > 8 * 1024 * 1024,
        "expected multiple segments, got {before} bytes"
    );

    // Delete all but 10 files, then unmount → compaction runs.
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            for i in 0..n {
                if i % 22 != 0 {
                    fs::remove_file(mnt.join(format!("f{i:04}"))).unwrap();
                }
            }
        },
        None,
    );
    let after = segment_bytes(data.path());
    assert!(
        after * 2 < before,
        "compaction did not reclaim space: before={before} after={after}"
    );

    // Survivors relocated by compaction must still read back correctly.
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            let mut survivors = 0;
            for i in (0..n).step_by(22) {
                let got = fs::read(mnt.join(format!("f{i:04}"))).unwrap();
                assert_eq!(got.len(), 64 * 1024);
                pattern_check(i as u64, &got);
                survivors += 1;
            }
            assert_eq!(survivors, 10);
            // Deleted files are gone.
            assert!(!mnt.join("f0001").exists());
        },
        None,
    );
}

/// Sorted list of segment file names.
fn segment_names(data: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(data.join("segments"))
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// Regression for the recompute_live bug: a fully-live set of packed files
/// must not be recompacted on an idle mount/unmount cycle (which would
/// needlessly re-transfer all packed content and churn extent locations).
/// With the content index counted as live, the unmount compaction finds
/// nothing dead and leaves every segment untouched.
#[test_log::test]
fn idle_remount_does_not_recompact_live_packed() {
    let data = TempDir::new("backupfs_data").unwrap();
    let n = 220usize; // > one 8 MiB segment of 64 KiB packed files
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            for i in 0..n {
                let mut b = vec![0u8; 64 * 1024];
                pattern_fill(i as u64, &mut b);
                fs::write(mnt.join(format!("f{i:04}")), b).unwrap();
            }
        },
        None,
    );
    let segs_before = segment_names(data.path());
    assert!(segs_before.len() > 1, "expected multiple segments");

    // Idle cycle: mount, touch nothing, unmount → compaction runs.
    with_backupfs(data.path(), "ohea".to_owned(), |_mnt| {}, None);

    let segs_after = segment_names(data.path());
    assert_eq!(
        segs_before, segs_after,
        "idle remount recompacted live packed segments (recompute_live regression)"
    );

    // Content still intact.
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            for i in (0..n).step_by(37) {
                let mut want = vec![0u8; 64 * 1024];
                pattern_fill(i as u64, &mut want);
                assert_eq!(fs::read(mnt.join(format!("f{i:04}"))).unwrap(), want);
            }
        },
        None,
    );
}

/// Total bytes across content block files.
fn content_bytes(data: &Path) -> u64 {
    let dir = data.join("contents");
    let mut total = 0u64;
    let mut stack = vec![dir];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let m = e.metadata().unwrap();
            if m.is_dir() {
                stack.push(e.path());
            } else {
                total += m.len();
            }
        }
    }
    total
}

/// A compressible file (by extension policy) is stored much smaller than its
/// logical size; an incompressible-extension file is stored ~raw. Both read
/// back byte-for-byte.
#[test_log::test]
fn compression_shrinks_compressible_content() {
    // 2 MiB of highly compressible text → block-backed (>1 MiB), .log → zstd.
    let text: Vec<u8> = b"the quick brown fox jumps over the lazy dog\n"
        .iter()
        .copied()
        .cycle()
        .take(2 * 1024 * 1024)
        .collect();
    let data_log = TempDir::new("backupfs_log").unwrap();
    with_backupfs(
        data_log.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::write(mnt.join("big.log"), &text).unwrap();
            assert_eq!(fs::read(mnt.join("big.log")).unwrap(), text);
        },
        None,
    );
    let log_on_disk = content_bytes(data_log.path());
    assert!(
        log_on_disk < text.len() as u64 / 4,
        "compressible .log not compressed: {log_on_disk} bytes for {} logical",
        text.len()
    );

    // 2 MiB of random with an incompressible extension → stored raw.
    let mut rnd = vec![0u8; 2 * 1024 * 1024];
    {
        use rand::Rng;
        rand::rand_core::UnwrapErr(rand::rng()).fill_bytes(&mut rnd);
    }
    let data_jpg = TempDir::new("backupfs_jpg").unwrap();
    with_backupfs(
        data_jpg.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::write(mnt.join("photo.jpg"), &rnd).unwrap();
            assert_eq!(fs::read(mnt.join("photo.jpg")).unwrap(), rnd);
        },
        None,
    );
    let jpg_on_disk = content_bytes(data_jpg.path());
    assert!(
        jpg_on_disk >= rnd.len() as u64,
        "incompressible .jpg unexpectedly smaller than raw: {jpg_on_disk}"
    );
}

/// Regression for the close()-path packed→blocks crash-consistency blocker.
///
/// When the last fd closes a file that just grew packed→blocks, `close()`
/// must get the replacement `File` inode record into the log *before* the
/// superseded packed extent's tombstone — relying on the batched syncfs,
/// which flushes the log prefix and the block files together. The original
/// code parked the `File` record only in the in-RAM dirty cache while
/// appending the tombstone to the log, so a crash before the clean-unmount
/// flush left the durable inode still `Packed` pointing at a tombstoned (and
/// compactable) extent — the file read back as zeros.
///
/// We reproduce the crash faithfully by driving the migration at the Handler
/// level and dropping the Handler WITHOUT flush_all_dirty: log appends and
/// block renames are real syscalls (survive in the page cache), but the dirty
/// cache is pure Handler RAM and is lost — exactly what a crash discards.
#[test_log::test]
fn packed_to_blocks_on_close_survives_crash_before_flush() {
    let data = TempDir::new("backupfs_data").unwrap();

    // Session 1: create a packed file (> inline_threshold, <= pack_max) and
    // clean-unmount, so a durable `Packed` inode + extent exist. Grab its ino.
    let small = vec![0xABu8; 200 * 1024];
    let ino = {
        let small = small.clone();
        capture(data.path(), "ohea", move |mnt| {
            fs::write(mnt.join("m.bin"), &small).unwrap();
            fs::metadata(mnt.join("m.bin")).unwrap().ino()
        })
    };

    // Session 2 (unclean): open, overwrite with 2 MiB so it migrates
    // packed→blocks, close the fd (commits the migration + tombstones the old
    // extent), then drop the Handler without flush_all_dirty — the "crash".
    let big: Vec<u8> = (0..2 * 1024 * 1024u64).map(pattern_byte).collect();
    {
        let ctrl = Controller::new(opts(data.path(), "ohea")).unwrap();
        let mut handler = crate::handle::Handler::new(ctrl);
        let fh = handler
            .fopen(Inode(ino), true, true, |_, _| Ok(()))
            .unwrap();
        {
            // Scoped clone so it is dropped before fclose — otherwise
            // Arc::try_unwrap in close() would fail and take the fsync path.
            let c = handler.handle(fh).unwrap().contents.clone();
            c.lock().unwrap().write_all_at(&big, 0).unwrap();
        }
        handler.fclose(fh).unwrap();
        drop(handler); // crash: discards the in-RAM dirty cache
    }

    // Session 3: remount and read back. With the fix the file is the full
    // 2 MiB; the bug surfaced as a zero-filled / truncated read.
    let got = capture(data.path(), "ohea", move |mnt| {
        fs::read(mnt.join("m.bin")).unwrap()
    });
    assert_eq!(got.len(), big.len(), "file truncated/lost after crash");
    pattern_check(0, &got);
}

/// Not a correctness test — a measurement. Writes a representative mixed
/// backup corpus (compressible logs/JSON/source + incompressible blobs)
/// through the mount and reports logical vs on-disk content size, so the PR
/// can quote a real end-to-end compression ratio. Run with:
///   cargo test --release measure_compression_ratio -- --ignored --nocapture
#[test_log::test]
#[ignore]
fn measure_compression_ratio() {
    fn log_corpus(bytes: usize) -> Vec<u8> {
        // Realistic structured log lines — repetitive, highly compressible.
        let mut out = Vec::with_capacity(bytes);
        let mut n = 0u64;
        while out.len() < bytes {
            let line = format!(
                "2026-05-28T12:{:02}:{:02}.{:03}Z INFO  start_core::backup::worker[{}]: \
                 transferred chunk seq={} size=1048576 retries=0 host=backup-target.local ok\n",
                (n / 60) % 60,
                n % 60,
                n % 1000,
                n % 8,
                n
            );
            out.extend_from_slice(line.as_bytes());
            n += 1;
        }
        out.truncate(bytes);
        out
    }
    fn json_corpus(bytes: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(bytes);
        let mut n = 0u64;
        while out.len() < bytes {
            let obj = format!(
                "{{\"id\":{},\"service\":\"registry\",\"status\":\"running\",\"healthy\":true,\
                 \"uptime_s\":{},\"version\":\"0.3.6\",\"tags\":[\"net\",\"db\",\"sync\"]}}\n",
                n,
                n * 37
            );
            out.extend_from_slice(obj.as_bytes());
            n += 1;
        }
        out.truncate(bytes);
        out
    }
    fn random_corpus(bytes: usize) -> Vec<u8> {
        use rand::Rng;
        let mut v = vec![0u8; bytes];
        rand::rand_core::UnwrapErr(rand::rng()).fill_bytes(&mut v);
        v
    }

    let mib = 1024 * 1024;
    let files: Vec<(&str, Vec<u8>)> = vec![
        ("services.log", log_corpus(24 * mib)),
        ("journal.log", log_corpus(16 * mib)),
        ("status.json", json_corpus(12 * mib)),
        ("db-dump.sql", log_corpus(8 * mib)), // sql ext → zstd 9, log-like text
        ("media.jpg", random_corpus(12 * mib)), // incompressible ext → raw
        ("blob.bin", random_corpus(8 * mib)), // unknown ext, random → adaptive raw
    ];
    let logical: usize = files.iter().map(|(_, d)| d.len()).sum();

    let data = TempDir::new("backupfs_ratio").unwrap();
    let files_for_write = files.clone();
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        move |mnt| {
            for (name, bytes) in &files_for_write {
                fs::write(mnt.join(name), bytes).unwrap();
            }
            // read back a couple to confirm round-trip under compression
            for (name, bytes) in &files_for_write {
                assert_eq!(&fs::read(mnt.join(name)).unwrap(), bytes, "{name} mismatch");
            }
        },
        None,
    );

    let on_disk = content_bytes(data.path()) as usize;
    let pct = 100.0 * on_disk as f64 / logical as f64;
    eprintln!("──────────── compression ratio (mixed backup corpus) ────────────");
    eprintln!(
        "  logical content : {:>10} bytes ({} MiB)",
        logical,
        logical / mib
    );
    eprintln!(
        "  on-disk content : {:>10} bytes ({} MiB)",
        on_disk,
        on_disk / mib
    );
    eprintln!(
        "  stored          : {:.1}% of logical  ({:.2}× reduction)",
        pct,
        logical as f64 / on_disk as f64
    );
    eprintln!("──────────────────────────────────────────────────────────────────");
}

/// The version gate refuses a store written by a newer format than this build
/// supports — and does so with an actionable `UnsupportedFormat`, read from the
/// plaintext envelope without needing to decode the (newer-shaped) body.
#[test_log::test]
fn superblock_rejects_newer_format_version() {
    use crate::error::BkfsErrorKind;
    let data = TempDir::new("backupfs_data").unwrap();
    // Create a fresh, valid store.
    crate::ctrl::Controller::new(opts(data.path(), "ohea")).unwrap();

    // Bump the plaintext envelope's format_version (offset 5..9, u32 LE) far
    // past SUPPORTED in BOTH replicas, simulating a store written by a newer
    // build. The sealed body is untouched, so this exercises the pre-decode
    // version gate specifically.
    for name in ["superblock", "superblock.bak1"] {
        let path = data.path().join(name);
        let mut bytes = fs::read(&path).unwrap();
        bytes[5..9].copy_from_slice(&9999u32.to_le_bytes());
        fs::write(&path, &bytes).unwrap();
    }

    let err = open_ctrl_err(data.path(), "ohea");
    assert!(
        matches!(err.kind, BkfsErrorKind::UnsupportedFormat(_)),
        "newer format_version must be refused with UnsupportedFormat, got {err:?}"
    );
}

/// Open a controller expecting failure, returning the error (Controller isn't
/// Debug, so `unwrap_err` can't be used directly).
fn open_ctrl_err(data: &Path, password: &str) -> crate::error::BkfsError {
    match crate::ctrl::Controller::new(opts(data, password)) {
        Ok(_) => panic!("expected Controller::new to fail"),
        Err(e) => e,
    }
}

/// A wrong password surfaces as `BadChecksum` (the vault integrity tag fails
/// before any decode runs), not a confusing decode error.
#[test_log::test]
fn superblock_wrong_password_is_bad_checksum() {
    use crate::error::BkfsErrorKind;
    let data = TempDir::new("backupfs_data").unwrap();
    crate::ctrl::Controller::new(opts(data.path(), "correct horse")).unwrap();

    let err = open_ctrl_err(data.path(), "wrong");
    assert!(
        matches!(err.kind, BkfsErrorKind::BadChecksum),
        "wrong password must be BadChecksum, got {err:?}"
    );
}

/// A pre-versioning store (a `cryptinfo` file, no superblock) is refused with a
/// clear error rather than silently overwritten with a fresh, empty superblock.
#[test_log::test]
fn legacy_cryptinfo_store_is_refused() {
    use crate::error::BkfsErrorKind;
    let data = TempDir::new("backupfs_data").unwrap();
    fs::write(data.path().join("cryptinfo"), b"legacy unversioned header").unwrap();

    let err = open_ctrl_err(data.path(), "ohea");
    assert!(
        matches!(err.kind, BkfsErrorKind::UnsupportedFormat(_)),
        "legacy cryptinfo store must be refused, got {err:?}"
    );
    // And the fresh superblock must NOT have been created over it.
    assert!(!data.path().join("superblock").exists());
}

/// A password change is durable across a remount: the new password opens the
/// store and the old one no longer does (generation-based replica selection
/// converges to the rewritten superblock).
#[test_log::test]
fn superblock_change_password_persists() {
    use crate::error::BkfsErrorKind;
    let data = TempDir::new("backupfs_data").unwrap();
    {
        let ctrl = crate::ctrl::Controller::new(opts(data.path(), "old-pass")).unwrap();
        ctrl.change_password("new-pass").unwrap();
    }
    // New password opens.
    crate::ctrl::Controller::new(opts(data.path(), "new-pass")).unwrap();
    // Old password is rejected.
    let err = open_ctrl_err(data.path(), "old-pass");
    assert!(
        matches!(err.kind, BkfsErrorKind::BadChecksum),
        "old password must no longer open after change, got {err:?}"
    );
}

/// If both superblock replicas are lost but the data store survives (e.g. an
/// unreliable backing store dropped the two small superblock files), mounting
/// must REFUSE rather than mint a fresh superblock with a new key — which would
/// present an empty filesystem and orphan/overwrite the surviving data.
#[test_log::test]
fn missing_superblock_over_existing_data_is_refused() {
    use crate::error::BkfsErrorKind;
    let data = TempDir::new("backupfs_data").unwrap();
    // Create a populated store (writes superblock + segments/contents).
    with_backupfs(
        data.path(),
        "ohea".to_owned(),
        |mnt| {
            fs::write(mnt.join("important.txt"), b"survivor").unwrap();
        },
        None,
    );
    // Lose BOTH superblock replicas; leave segments/contents/dirents intact.
    for name in ["superblock", "superblock.bak1"] {
        let p = data.path().join(name);
        if p.exists() {
            fs::remove_file(&p).unwrap();
        }
    }
    assert!(
        data.path().join("segments").exists(),
        "precondition: data store should still be present"
    );

    let err = open_ctrl_err(data.path(), "ohea");
    assert!(
        matches!(err.kind, BkfsErrorKind::UnsupportedFormat(_)),
        "missing superblock over existing data must be refused, got {err:?}"
    );
    // And no fresh superblock was created over the data.
    assert!(!data.path().join("superblock").exists());
}
