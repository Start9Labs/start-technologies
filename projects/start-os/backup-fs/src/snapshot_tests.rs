use std::collections::{BTreeMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::blockstore::{self, CHUNK_SIZE};
use crate::contents::Contents;
use crate::ctrl::Controller;
use crate::directory::{DirectoryContents, DirectoryEntry};
use crate::inode::{ContentId, FileData, Inode, InodeAttributes};
use crate::{BackupFS, BackupFSOptions, FUSE_ROOT_ID};

fn config(path: &Path) -> BackupFSOptions {
    BackupFSOptions {
        data_dir: path.to_owned(),
        password: "password".to_owned(),
        readonly: false,
        setuid_support: false,
        file_size_padding: None,
        idmapped: false,
    }
}

#[test]
fn versioned_storage_persists_without_retained_snapshots() {
    let data = tempfile::tempdir().unwrap();
    let mut fs = BackupFS::new(config(data.path())).unwrap();
    fs.create_snapshot("first").unwrap();
    fs.delete_snapshot("first").unwrap();
    assert!(fs.list_snapshots().unwrap().is_empty());
    let primary = data.path().join("superblock");
    drop(fs);
    let mut fs = BackupFS::new(config(data.path())).unwrap();
    assert!(fs.list_snapshots().unwrap().is_empty());
    fs.create_snapshot("second").unwrap();
    assert_eq!(fs.list_snapshots().unwrap().len(), 1);
    assert_eq!(
        crate::superblock::Superblock::open_or_create(&primary, "password", true)
            .unwrap()
            .format_version,
        crate::superblock::FORMAT_VERSION
    );
}

fn ctrl(fs: &BackupFS) -> Controller {
    fs.handler.lock().unwrap().ctrl().clone()
}

fn create_file(ctrl: &Controller, bytes: &[u8]) -> Inode {
    let inode = ctrl.next_inode().unwrap();
    let attrs = InodeAttributes::new(
        inode,
        Some((Inode(FUSE_ROOT_ID), OsString::from("file"))),
        FileData::Inline(Vec::new()),
    );
    ctrl.save(&attrs).unwrap();
    let mut contents = Contents::open(ctrl.clone(), inode).unwrap();
    contents.write_at(bytes, 0).unwrap();
    contents.flush().unwrap();
    inode
}

fn read_file(ctrl: &Controller, inode: Inode) -> Vec<u8> {
    let mut contents = Contents::open(ctrl.clone(), inode).unwrap();
    let mut bytes = vec![0; contents.inode.attrs.size as usize];
    contents.read_exact_at(&mut bytes, 0).unwrap();
    bytes
}

fn create_legacy_block_file(data: &Path) -> Inode {
    crate::legacy::fixture(data);
    Inode(4)
}

fn objects(data: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(path: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
        if !path.exists() {
            return;
        }
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                walk(&entry.path(), result);
            } else {
                result.insert(entry.path(), std::fs::read(entry.path()).unwrap());
            }
        }
    }
    let mut result = BTreeMap::new();
    walk(&data.join("contents"), &mut result);
    walk(&data.join("dirents"), &mut result);
    result
}

#[test]
fn snapshots_share_objects_and_partial_writes_change_one_chunk() {
    let data = tempfile::tempdir().unwrap();
    let mut fs = BackupFS::new(config(data.path())).unwrap();
    let c = ctrl(&fs);
    let original = vec![0x31; 3 * CHUNK_SIZE as usize];
    let inode = create_file(&c, &original);
    let before = objects(data.path());
    assert_eq!(before.len(), 3);
    fs.create_snapshot("first").unwrap();
    fs.create_snapshot("unchanged").unwrap();
    assert_eq!(objects(data.path()), before);
    let mut contents = Contents::open(c.clone(), inode).unwrap();
    contents.write_at(b"changed", CHUNK_SIZE + 123).unwrap();
    contents.flush().unwrap();
    let after = objects(data.path());
    assert_eq!(after.len(), before.len() + 1);
    assert_eq!(
        after.values().map(Vec::len).sum::<usize>() - before.values().map(Vec::len).sum::<usize>(),
        after
            .iter()
            .find(|(p, _)| !before.contains_key(*p))
            .unwrap()
            .1
            .len()
    );
    for (path, bytes) in &before {
        assert_eq!(after.get(path), Some(bytes));
    }
    fs.create_snapshot("edited").unwrap();
    drop(contents);
    drop(c);
    drop(fs);
    for name in ["first", "unchanged"] {
        let fs = BackupFS::open_snapshot(config(data.path()), name).unwrap();
        assert_eq!(read_file(&ctrl(&fs), inode), original);
    }
    let fs = BackupFS::open_snapshot(config(data.path()), "edited").unwrap();
    let mut expected = original;
    expected[CHUNK_SIZE as usize + 123..CHUNK_SIZE as usize + 130].copy_from_slice(b"changed");
    assert_eq!(read_file(&ctrl(&fs), inode), expected);
}

#[test]
fn snapshots_preserve_inline_packed_metadata_links_and_unlinked_inodes() {
    let data = tempfile::tempdir().unwrap();
    let mut fs = BackupFS::new(config(data.path())).unwrap();
    let c = ctrl(&fs);
    let inline = create_file(&c, b"tiny");
    let packed_bytes = vec![0x42; 12000];
    let packed = create_file(&c, &packed_bytes);
    let mut attrs: InodeAttributes = c.load(packed).unwrap();
    attrs
        .attrs
        .parents
        .insert((Inode(FUSE_ROOT_ID), OsString::from("second-link")));
    attrs
        .attrs
        .xattrs
        .insert(b"user.test".to_vec(), b"original".to_vec());
    attrs.attrs.uid = 123;
    attrs.attrs.gid = 456;
    attrs.attrs.mode = 0o6751;
    c.save(&attrs).unwrap();
    fs.create_snapshot("before").unwrap();
    attrs
        .attrs
        .parents
        .remove(&(Inode(FUSE_ROOT_ID), OsString::from("file")));
    attrs
        .attrs
        .parents
        .insert((Inode(FUSE_ROOT_ID), OsString::from("renamed")));
    attrs
        .attrs
        .xattrs
        .insert(b"user.test".to_vec(), b"changed".to_vec());
    c.save(&attrs).unwrap();
    let mut content = Contents::open(c.clone(), packed).unwrap();
    content.write_at(b"edited", 0).unwrap();
    content.flush().unwrap();
    c.log_tombstone(inline, true).unwrap();
    c.log_tombstone(packed, true).unwrap();
    c.cpack_tombstone(ContentId(packed.0), true).unwrap();
    c.compact().unwrap();
    drop(content);
    drop(c);
    drop(fs);
    let fs = BackupFS::open_snapshot(config(data.path()), "before").unwrap();
    let c = ctrl(&fs);
    assert_eq!(read_file(&c, inline), b"tiny");
    assert_eq!(read_file(&c, packed), packed_bytes);
    let restored: InodeAttributes = c.load(packed).unwrap();
    assert_eq!(restored.attrs.parents.len(), 2);
    assert!(restored
        .attrs
        .parents
        .contains(&(Inode(FUSE_ROOT_ID), OsString::from("file"))));
    assert!(!restored
        .attrs
        .parents
        .contains(&(Inode(FUSE_ROOT_ID), OsString::from("renamed"))));
    assert_eq!(
        restored.attrs.xattrs.get(b"user.test".as_slice()).unwrap(),
        b"original"
    );
    assert_eq!(
        (restored.attrs.uid, restored.attrs.gid, restored.attrs.mode),
        (123, 456, 0o6751)
    );
}

#[test]
fn snapshots_keep_sparse_truncated_and_regrown_views_separate() {
    let data = tempfile::tempdir().unwrap();
    let mut fs = BackupFS::new(config(data.path())).unwrap();
    let c = ctrl(&fs);
    let inode = create_file(&c, &vec![7; 2 * CHUNK_SIZE as usize]);
    fs.create_snapshot("original").unwrap();
    let mut contents = Contents::open(c.clone(), inode).unwrap();
    contents.resize(17).unwrap();
    fs.create_snapshot("short").unwrap();
    contents.resize(3 * CHUNK_SIZE).unwrap();
    contents.write_at(b"tail", 3 * CHUNK_SIZE - 4).unwrap();
    contents.flush().unwrap();
    let mut expected = vec![0; 3 * CHUNK_SIZE as usize];
    expected[..17].fill(7);
    expected[3 * CHUNK_SIZE as usize - 4..].copy_from_slice(b"tail");
    assert_eq!(read_file(&c, inode), expected);
    fs.create_snapshot("grown").unwrap();
    c.compact().unwrap();
    drop(contents);
    drop(c);
    drop(fs);
    for (name, expected) in [
        ("original", vec![7; 2 * CHUNK_SIZE as usize]),
        ("short", vec![7; 17]),
        ("grown", expected),
    ] {
        let fs = BackupFS::open_snapshot(config(data.path()), name).unwrap();
        assert_eq!(read_file(&ctrl(&fs), inode), expected);
    }
}

#[test]
fn spilled_directory_snapshots_survive_removal_and_generation_reaping() {
    let data = tempfile::tempdir().unwrap();
    let mut fs = BackupFS::new(config(data.path())).unwrap();
    let c = ctrl(&fs);
    let inode = c.next_inode().unwrap();
    let mut dir = DirectoryContents::new();
    for i in 0..1100 {
        dir.insert(
            &c,
            inode,
            OsString::from(format!("entry-{i}")),
            DirectoryEntry {
                inode: Inode(10000 + i),
                ty: fuser::FileType::RegularFile,
            },
            false,
        )
        .unwrap();
    }
    let mut attrs = InodeAttributes::new(inode, None, FileData::Directory(dir.clone()));
    c.save(&attrs).unwrap();
    let before = objects(data.path());
    assert!(!before.is_empty());
    fs.create_snapshot("directory").unwrap();
    assert_eq!(objects(data.path()), before);
    dir.remove(&c, inode, std::ffi::OsStr::new("entry-12"), true)
        .unwrap();
    dir.insert(
        &c,
        inode,
        OsString::from("replacement"),
        DirectoryEntry {
            inode: Inode(42),
            ty: fuser::FileType::RegularFile,
        },
        true,
    )
    .unwrap();
    attrs.attrs.contents = FileData::Directory(dir.clone());
    c.save(&attrs).unwrap();
    dir.gc(&c, inode).unwrap();
    c.log_tombstone(inode, true).unwrap();
    c.compact().unwrap();
    drop(c);
    drop(fs);
    let fs = BackupFS::open_snapshot(config(data.path()), "directory").unwrap();
    let c = ctrl(&fs);
    let attrs: InodeAttributes = c.load(inode).unwrap();
    let FileData::Directory(dir) = attrs.attrs.contents else {
        panic!()
    };
    assert_eq!(dir.snapshot(&c, inode).unwrap().len(), 1100);
    assert_eq!(
        dir.get(&c, inode, std::ffi::OsStr::new("entry-12"))
            .unwrap()
            .unwrap()
            .inode,
        Inode(10012)
    );
    assert!(dir
        .get(&c, inode, std::ffi::OsStr::new("replacement"))
        .unwrap()
        .is_none());
}

#[test]
fn snapshot_deletion_reclaims_only_unreferenced_versions_and_keeps_tombstones_effective() {
    let data = tempfile::tempdir().unwrap();
    let mut fs = BackupFS::new(config(data.path())).unwrap();
    let c = ctrl(&fs);
    let inode = create_file(&c, &vec![1; 2 * CHUNK_SIZE as usize]);
    fs.create_snapshot("one").unwrap();
    fs.create_snapshot("also-one").unwrap();
    let mut contents = Contents::open(c.clone(), inode).unwrap();
    contents.write_at(&[2; 20], 0).unwrap();
    contents.flush().unwrap();
    fs.create_snapshot("two").unwrap();
    assert_eq!(objects(data.path()).len(), 3);
    fs.delete_snapshot("one").unwrap();
    assert_eq!(objects(data.path()).len(), 3);
    fs.delete_snapshot("also-one").unwrap();
    assert_eq!(objects(data.path()).len(), 2);
    c.log_tombstone(inode, true).unwrap();
    blockstore::remove_all_blocks(&c, ContentId(inode.0), 2 * CHUNK_SIZE).unwrap();
    fs.delete_snapshot("two").unwrap();
    assert!(objects(data.path()).is_empty());
    c.compact().unwrap();
    drop(contents);
    drop(c);
    drop(fs);
    let fs = BackupFS::new(config(data.path())).unwrap();
    assert!(ctrl(&fs).log_load(inode).unwrap().is_none());
    assert!(fs.list_snapshots().unwrap().is_empty());
}

#[test]
fn snapshot_names_catalog_durability_readonly_and_exclusive_lock() {
    let data = tempfile::tempdir().unwrap();
    let mut fs = BackupFS::new(config(data.path())).unwrap();
    for name in ["", ".", "..", "a/b", "has space"] {
        assert_eq!(
            fs.create_snapshot(name).unwrap_err().to_errno(),
            libc::EINVAL
        );
    }
    let c = ctrl(&fs);
    let inode = create_file(&c, b"unchanged");
    fs.create_snapshot("named-1").unwrap();
    assert_eq!(
        fs.create_snapshot("named-1").unwrap_err().to_errno(),
        libc::EEXIST
    );
    assert_eq!(
        fs.delete_snapshot("absent").unwrap_err().to_errno(),
        libc::ENOENT
    );
    assert!(BackupFS::open_snapshot(config(data.path()), "named-1").is_err());
    drop(c);
    drop(fs);
    assert!(BackupFS::open_snapshot(config(data.path()), "absent").is_err());
    let mut fs = BackupFS::open_snapshot(config(data.path()), "named-1").unwrap();
    assert_eq!(fs.list_snapshots().unwrap()[0].name, "named-1");
    assert_eq!(
        fs.create_snapshot("denied").unwrap_err().to_errno(),
        libc::EROFS
    );
    assert_eq!(
        fs.delete_snapshot("named-1").unwrap_err().to_errno(),
        libc::EROFS
    );
    let c = ctrl(&fs);
    assert_eq!(c.next_inode().unwrap_err().to_errno(), libc::EROFS);
    assert_eq!(
        c.log_tombstone(inode, true).unwrap_err().to_errno(),
        libc::EROFS
    );
    let mut contents = Contents::open(c, inode).unwrap();
    assert_eq!(
        contents.write_at(b"denied", 0).unwrap_err().to_errno(),
        libc::EROFS
    );
    assert_eq!(read_file(&ctrl(&fs), inode), b"unchanged");
    contents.flush().unwrap();
}

#[test]
fn snapshot_failure_before_publication_preserves_data_and_blocks_further_writes() {
    let data = tempfile::tempdir().unwrap();
    let mut fs = BackupFS::new(config(data.path())).unwrap();
    let c = ctrl(&fs);
    let inode = create_file(&c, b"safe");
    c.syncfs().unwrap();
    let anchor = data.path().join("segments/state");
    let saved = data.path().join("saved-state");
    std::fs::rename(&anchor, &saved).unwrap();
    std::fs::create_dir(&anchor).unwrap();
    assert!(fs.create_snapshot("not-published").is_err());
    assert!(fs.list_snapshots().unwrap().is_empty());
    assert_eq!(c.next_inode().unwrap_err().to_errno(), libc::EIO);
    drop(c);
    drop(fs);
    std::fs::remove_dir(&anchor).unwrap();
    std::fs::rename(saved, anchor).unwrap();
    let fs = BackupFS::new(config(data.path())).unwrap();
    assert_eq!(read_file(&ctrl(&fs), inode), b"safe");
    assert!(fs.list_snapshots().unwrap().is_empty());
}

#[test]
fn legacy_snapshot_truncate_remount_regrow_keeps_holes_zeroed() {
    let data = tempfile::tempdir().unwrap();
    let inode = create_legacy_block_file(data.path());
    {
        let mut fs = BackupFS::new(config(data.path())).unwrap();
        let c = ctrl(&fs);
        fs.create_snapshot("original").unwrap();
        let mut contents = Contents::open(c.clone(), inode).unwrap();
        contents.resize(CHUNK_SIZE).unwrap();
        contents.flush().unwrap();
        drop(contents);
        c.compact().unwrap();
    }
    let mut expected = vec![0; 4 * CHUNK_SIZE as usize];
    expected[..CHUNK_SIZE as usize].fill(0x31);
    {
        let fs = BackupFS::new(config(data.path())).unwrap();
        let c = ctrl(&fs);
        let mut contents = Contents::open(c.clone(), inode).unwrap();
        contents.resize(4 * CHUNK_SIZE).unwrap();
        contents.flush().unwrap();
        assert!(read_file(&c, inode) == expected);
        contents.write_at(b"edit", 2 * CHUNK_SIZE + 9).unwrap();
        contents.flush().unwrap();
        expected[2 * CHUNK_SIZE as usize + 9..2 * CHUNK_SIZE as usize + 13]
            .copy_from_slice(b"edit");
        assert_eq!(read_file(&c, inode), expected);
    }
    {
        let fs = BackupFS::open_snapshot(config(data.path()), "original").unwrap();
        assert_eq!(
            read_file(&ctrl(&fs), inode),
            vec![0x31; 4 * CHUNK_SIZE as usize]
        );
    }
    {
        let mut fs = BackupFS::new(config(data.path())).unwrap();
        fs.delete_snapshot("original").unwrap();
    }
    let fs = BackupFS::new(config(data.path())).unwrap();
    assert_eq!(read_file(&ctrl(&fs), inode), expected);
}

#[test]
fn retired_legacy_files_cannot_resurrect_after_catalog_publication() {
    let data = tempfile::tempdir().unwrap();
    let inode = create_legacy_block_file(data.path());
    let old =
        crate::superblock::Superblock::load(&data.path().join("superblock"), "password", true)
            .unwrap();
    let original = crate::objects::ObjectId::block(&old.key, inode.0, 3)
        .path(data.path(), None)
        .unwrap();
    let raw = std::fs::read(&original).unwrap();
    let (key, ecc) = {
        let mut fs = BackupFS::new(config(data.path())).unwrap();
        let c = ctrl(&fs);
        fs.create_snapshot("original").unwrap();
        let mut contents = Contents::open(c.clone(), inode).unwrap();
        contents.resize(CHUNK_SIZE).unwrap();
        contents.flush().unwrap();
        (*c.key(), c.ecc())
    };
    let mut log = crate::seglog::SegmentLog::open(data.path().join("segments"), key).unwrap();
    let mut catalog =
        crate::snapshot::Catalog::load(log.directory(), &key, log.catalog_reference()).unwrap();
    catalog.entries.clear();
    let reference = catalog.write(log.directory(), &key, ecc).unwrap();
    log.publish_catalog(reference, catalog.pins()).unwrap();
    let live = HashSet::from([crate::objects::ObjectId::block(&key, inode.0, 0)]);
    let packed = log.roots().content.keys().copied().collect();
    log.retain_storage(&live, &packed).unwrap();
    log.save_checkpoint(&[]).unwrap();
    drop(log);
    assert!(!original.exists());
    std::fs::write(&original, raw).unwrap();
    let fs = BackupFS::new(config(data.path())).unwrap();
    let c = ctrl(&fs);
    assert!(c.block_path(ContentId(inode.0), 3).exists());
    let mut contents = Contents::open(c.clone(), inode).unwrap();
    contents.resize(4 * CHUNK_SIZE).unwrap();
    contents.flush().unwrap();
    let bytes = read_file(&c, inode);
    assert_eq!(
        &bytes[..CHUNK_SIZE as usize],
        vec![0x31; CHUNK_SIZE as usize]
    );
    assert!(bytes[CHUNK_SIZE as usize..].iter().all(|&byte| byte == 0));
}

#[test]
fn snapshots_share_converted_legacy_objects_without_copying() {
    let data = tempfile::tempdir().unwrap();
    let inode = create_legacy_block_file(data.path());
    let mut fs = BackupFS::new(config(data.path())).unwrap();
    let c = ctrl(&fs);
    let stable = c.block_path(ContentId(inode.0), 0);
    let converted = std::fs::canonicalize(c.resolve_block_path(ContentId(inode.0), 0)).unwrap();
    let before = objects(data.path());
    fs.create_snapshot("legacy").unwrap();
    assert_eq!(objects(data.path()), before);
    blockstore::write_block(
        &c,
        ContentId(inode.0),
        0,
        b"latest",
        crate::compress::Codec::Zstd(2),
        true,
    )
    .unwrap();
    fs.create_snapshot("latest").unwrap();
    c.collect_garbage().unwrap();
    assert!(!stable.exists());
    assert!(converted.exists());
    fs.delete_snapshot("latest").unwrap();
    assert!(converted.exists());
    drop(c);
    drop(fs);
    let fs = BackupFS::open_snapshot(config(data.path()), "legacy").unwrap();
    let c = ctrl(&fs);
    assert_eq!(
        blockstore::read_block(&c, ContentId(inode.0), 0)
            .unwrap()
            .unwrap(),
        vec![0x31; CHUNK_SIZE as usize]
    );
    assert_eq!(
        blockstore::read_block(&c, ContentId(inode.0), 1)
            .unwrap()
            .unwrap(),
        vec![0x31; CHUNK_SIZE as usize]
    );
    drop(c);
    drop(fs);
    let mut fs = BackupFS::new(config(data.path())).unwrap();
    fs.delete_snapshot("legacy").unwrap();
    assert!(!converted.exists());
}

#[test]
fn snapshot_flushes_dirty_handler_metadata_and_open_content() {
    let data = tempfile::tempdir().unwrap();
    let mut fs = BackupFS::new(config(data.path())).unwrap();
    let c = ctrl(&fs);
    let inode = create_file(&c, b"old");
    let metadata_inode = create_file(&c, b"metadata");
    let h = fs.handler.get_mut().unwrap();
    let fh = h.fopen(inode, true, true, |_, _| Ok(())).unwrap();
    let contents = h.handle(fh).unwrap().contents.clone();
    contents.lock().unwrap().write_at(b"new", 0).unwrap();
    let mut attrs = h.load_inode(metadata_inode).unwrap();
    attrs
        .attrs
        .xattrs
        .insert(b"user.pending".to_vec(), b"saved".to_vec());
    h.save_inode(&attrs).unwrap();
    fs.create_snapshot("flushed").unwrap();
    contents.lock().unwrap().write_at(b"later", 0).unwrap();
    drop(contents);
    fs.handler.get_mut().unwrap().fclose(fh).unwrap();
    drop(c);
    drop(fs);
    let fs = BackupFS::open_snapshot(config(data.path()), "flushed").unwrap();
    let c = ctrl(&fs);
    assert_eq!(read_file(&c, inode), b"new");
    let attrs: InodeAttributes = c.load(metadata_inode).unwrap();
    assert_eq!(
        attrs.attrs.xattrs.get(b"user.pending".as_slice()).unwrap(),
        b"saved"
    );
}

#[test]
fn durable_replay_floor_prevents_pinned_history_from_resurrecting_a_deleted_inode() {
    let data = tempfile::tempdir().unwrap();
    let key = chacha20::Key::from([0x29; 32]);
    let dir = data.path().join("segments");
    std::fs::create_dir(&dir).unwrap();
    crate::state::State::create(
        &dir,
        &key,
        crate::vault::EccParams::default(),
        &Default::default(),
        crate::state::Counters {
            next_seq: 1,
            max_inode: 0,
        },
        None,
    )
    .unwrap();
    let mut log = crate::seglog::SegmentLog::open_sized(
        dir.clone(),
        key,
        4096,
        crate::vault::EccParams::default(),
        false,
    )
    .unwrap();
    let attrs = InodeAttributes::new(Inode(12), None, FileData::Inline(b"historical".to_vec()));
    log.put(Inode(12), &attrs.attrs).unwrap();
    let view = log.save_checkpoint(&[]).unwrap();
    log.pin(log.roots().segments().collect());
    for i in 100..200 {
        log.put(Inode(i), &attrs.attrs).unwrap();
    }
    log.tombstone(Inode(12)).unwrap();
    for i in 200..300 {
        log.put(Inode(i), &attrs.attrs).unwrap();
    }
    log.compact(0.0).unwrap();
    log.sync().unwrap();
    drop(log);
    let mut log = crate::seglog::SegmentLog::open_sized(
        dir,
        key,
        4096,
        crate::vault::EccParams::default(),
        true,
    )
    .unwrap();
    assert!(log.load(Inode(12)).unwrap().is_none());
    log.adopt_view(view).unwrap();
    let FileData::Inline(bytes) = log.load(Inode(12)).unwrap().unwrap().contents else {
        panic!()
    };
    assert_eq!(bytes, b"historical");
}

#[test]
fn truncate_then_regrow_without_an_intermediate_fsync_clears_cached_and_packed_tails() {
    let data = tempfile::tempdir().unwrap();
    let fs = BackupFS::new(config(data.path())).unwrap();
    let c = ctrl(&fs);
    for size in [12000, 2 * CHUNK_SIZE as usize] {
        let inode = create_file(&c, &vec![9; size]);
        let mut contents = Contents::open(c.clone(), inode).unwrap();
        let mut cached = [0; 128];
        contents.read_exact_at(&mut cached, 0).unwrap();
        contents.resize(17).unwrap();
        contents.resize(size as u64).unwrap();
        let mut bytes = vec![0; size];
        contents.read_exact_at(&mut bytes, 0).unwrap();
        assert_eq!(&bytes[..17], &[9; 17]);
        assert!(bytes[17..].iter().all(|&b| b == 0));
    }
}

#[test]
fn failed_snapshot_flush_retains_dirty_blocks_for_retry() {
    let data = tempfile::tempdir().unwrap();
    let mut fs = BackupFS::new(config(data.path())).unwrap();
    let c = ctrl(&fs);
    let inode = create_file(&c, b"");
    let h = fs.handler.get_mut().unwrap();
    let fh = h.fopen(inode, true, true, |_, _| Ok(())).unwrap();
    let contents = h.handle(fh).unwrap().contents.clone();
    let expected = vec![0x87; 2 * CHUNK_SIZE as usize];
    contents.lock().unwrap().write_at(&expected, 0).unwrap();
    std::fs::write(data.path().join("contents"), b"write failure").unwrap();
    assert!(fs.create_snapshot("failed").is_err());
    assert!(fs.list_snapshots().unwrap().is_empty());
    std::fs::remove_file(data.path().join("contents")).unwrap();
    fs.create_snapshot("retried").unwrap();
    drop(contents);
    fs.handler.get_mut().unwrap().fclose(fh).unwrap();
    drop(c);
    drop(fs);
    let fs = BackupFS::open_snapshot(config(data.path()), "retried").unwrap();
    assert_eq!(read_file(&ctrl(&fs), inode), expected);
}
