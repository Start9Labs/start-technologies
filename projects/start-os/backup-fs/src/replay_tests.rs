use std::path::Path;

use crate::blockstore::{self, CHUNK_SIZE};
use crate::contents::Contents;
use crate::ctrl::Controller;
use crate::inode::{ContentId, FileData, Inode, InodeAttributes};
use crate::state::State;
use crate::{BackupFS, BackupFSOptions};

fn options(data: &Path) -> BackupFSOptions {
    BackupFSOptions {
        data_dir: data.to_owned(),
        password: "snapshots".into(),
        readonly: false,
        setuid_support: false,
        idmapped: false,
        file_size_padding: None,
    }
}

fn ctrl(fs: &BackupFS) -> Controller {
    fs.handler.lock().unwrap().ctrl().clone()
}

fn read_inode(fs: &BackupFS, inode: u64) -> Vec<u8> {
    let mut contents = Contents::open(ctrl(fs), Inode(inode)).unwrap();
    let mut bytes = vec![0; contents.inode.attrs.size as usize];
    contents.read_exact_at(&mut bytes, 0).unwrap();
    bytes
}

fn same_bytes(actual: &[u8], expected: &[u8]) {
    assert_eq!(actual.len(), expected.len());
    assert_eq!(
        actual
            .iter()
            .zip(expected)
            .position(|(actual, expected)| actual != expected),
        None
    );
}

#[test]
fn store_lock_rejects_symlinks_without_truncating_their_targets() {
    let parent = tempfile::tempdir().unwrap();
    let data = parent.path().join("store");
    std::fs::create_dir(&data).unwrap();
    let target = parent.path().join("target");
    std::fs::write(&target, b"untouched").unwrap();
    std::os::unix::fs::symlink(&target, data.join(".lock")).unwrap();
    assert!(BackupFS::new(options(&data)).is_err());
    assert_eq!(std::fs::read(target).unwrap(), b"untouched");
}

#[test]
fn object_reads_reject_older_versions_other_objects_and_missing_current_files() {
    let data = tempfile::tempdir().unwrap();
    let fs = BackupFS::new(options(data.path())).unwrap();
    let c = ctrl(&fs);
    let inode = c.next_inode().unwrap();
    let mut attrs = InodeAttributes::new(inode, None, FileData::File(ContentId(inode.0)));
    attrs.attrs.size = 3 * CHUNK_SIZE;
    c.save(&attrs).unwrap();
    blockstore::write_block(
        &c,
        ContentId(inode.0),
        0,
        b"version-one",
        crate::compress::Codec::None,
        true,
    )
    .unwrap();
    let old_path = c.resolve_block_path(ContentId(inode.0), 0);
    let old = std::fs::read(old_path).unwrap();
    blockstore::write_block(
        &c,
        ContentId(inode.0),
        0,
        b"version-two",
        crate::compress::Codec::None,
        true,
    )
    .unwrap();
    blockstore::write_block(
        &c,
        ContentId(inode.0),
        1,
        b"healthy",
        crate::compress::Codec::None,
        true,
    )
    .unwrap();
    let current_path = c.resolve_block_path(ContentId(inode.0), 0);
    let current = std::fs::read(&current_path).unwrap();
    let other = std::fs::read(c.resolve_block_path(ContentId(inode.0), 1)).unwrap();
    for replacement in [&old, &other] {
        std::fs::write(&current_path, replacement).unwrap();
        assert!(blockstore::read_block(&c, ContentId(inode.0), 0).is_err());
        assert_eq!(
            blockstore::read_block(&c, ContentId(inode.0), 1)
                .unwrap()
                .unwrap(),
            b"healthy"
        );
    }
    std::fs::remove_file(&current_path).unwrap();
    assert!(blockstore::read_block(&c, ContentId(inode.0), 0).is_err());
    assert!(blockstore::read_block(&c, ContentId(inode.0), 2)
        .unwrap()
        .is_none());
    std::fs::write(&current_path, &current).unwrap();
    let mut repaired = current;
    let middle = repaired.len() / 2;
    repaired[middle] ^= 1;
    std::fs::write(&current_path, repaired).unwrap();
    assert_eq!(
        blockstore::read_block(&c, ContentId(inode.0), 0)
            .unwrap()
            .unwrap(),
        b"version-two"
    );
    drop(c);
    drop(fs);
    let fs = BackupFS::new(options(data.path())).unwrap();
    assert_eq!(
        blockstore::read_block(&ctrl(&fs), ContentId(inode.0), 0)
            .unwrap()
            .unwrap(),
        b"version-two"
    );
}

#[test]
fn replaying_only_the_snapshot_catalog_cannot_remove_a_newer_name() {
    let data = tempfile::tempdir().unwrap();
    let mut fs = BackupFS::new(options(data.path())).unwrap();
    fs.create_snapshot("older").unwrap();
    let c = ctrl(&fs);
    let (state, _) = State::open(&data.path().join("segments"), c.key()).unwrap();
    let older =
        std::fs::read(state.catalog().unwrap().path(&data.path().join("segments"))).unwrap();
    fs.create_snapshot("newer").unwrap();
    let (state, _) = State::open(&data.path().join("segments"), c.key()).unwrap();
    let current = state.catalog().unwrap().path(&data.path().join("segments"));
    drop(state);
    drop(c);
    drop(fs);
    std::fs::write(current, older).unwrap();
    assert!(BackupFS::new(options(data.path())).is_err());
}

#[test]
fn frozen_legacy_index_cache_is_used_only_for_its_matching_segment_inventory() {
    use std::io::Write;
    let data = tempfile::tempdir().unwrap();
    crate::legacy::fixture(data.path(), 2);
    let sb =
        crate::superblock::Superblock::load(&data.path().join("superblock"), "snapshots", true)
            .unwrap();
    let source = crate::legacy::Source::load(data.path(), &sb.key).unwrap();
    assert!(source.roots.index.contains_key(&2));
    assert!(source.roots.index.contains_key(&3));
    assert!(source.roots.index.contains_key(&4));
    assert!(source.next_seq > 1);
    let mut segment = std::fs::OpenOptions::new()
        .append(true)
        .open(data.path().join("segments/0000000000000000.seg"))
        .unwrap();
    segment.write_all(b"incomplete tail").unwrap();
    assert!(crate::legacy::Source::load(data.path(), &sb.key)
        .unwrap()
        .roots
        .index
        .is_empty());
    let fs = BackupFS::new(options(data.path())).unwrap();
    assert_eq!(read_inode(&fs, 2), b"frozen-inline");
}

#[test]
fn frozen_format_two_upgrades_without_losing_inline_packed_block_or_attribute_data() {
    let data = tempfile::tempdir().unwrap();
    crate::legacy::fixture(data.path(), 2);
    let fs = BackupFS::new(options(data.path())).unwrap();
    assert_eq!(read_inode(&fs, 2), b"frozen-inline");
    same_bytes(&read_inode(&fs, 3), &vec![0x42; 12000]);
    same_bytes(&read_inode(&fs, 4), &vec![0x31; 4 * CHUNK_SIZE as usize]);
    let attrs: InodeAttributes = ctrl(&fs).load(Inode(3)).unwrap();
    assert_eq!(
        (attrs.attrs.uid, attrs.attrs.gid, attrs.attrs.mode),
        (123, 456, 0o6751)
    );
    assert_eq!(
        attrs.attrs.xattrs.get(b"user.fixture".as_slice()).unwrap(),
        b"original"
    );
    drop(fs);
    let mut readonly = options(data.path());
    readonly.readonly = true;
    let fs = BackupFS::new(readonly).unwrap();
    same_bytes(&read_inode(&fs, 4), &vec![0x31; 4 * CHUNK_SIZE as usize]);
}

fn check_frozen_snapshots(data: &Path) {
    let original = vec![0x31; 4 * CHUNK_SIZE as usize];
    let fs = BackupFS::open_snapshot(options(data), "before").unwrap();
    same_bytes(&read_inode(&fs, 4), &original);
    drop(fs);
    let mut edited = original;
    edited[CHUNK_SIZE as usize + 123..CHUNK_SIZE as usize + 132].copy_from_slice(b"versioned");
    let fs = BackupFS::open_snapshot(options(data), "after").unwrap();
    same_bytes(&read_inode(&fs, 4), &edited);
}

#[test]
fn frozen_format_three_preserves_named_views_and_live_deletion_overrides() {
    let data = tempfile::tempdir().unwrap();
    crate::legacy::fixture(data.path(), 3);
    let fs = BackupFS::new(options(data.path())).unwrap();
    let mut expected = vec![0x31; 2 * CHUNK_SIZE as usize];
    expected[CHUNK_SIZE as usize + 123..CHUNK_SIZE as usize + 132].copy_from_slice(b"versioned");
    same_bytes(&read_inode(&fs, 4), &expected);
    assert_eq!(fs.list_snapshots().unwrap().len(), 2);
    drop(fs);
    check_frozen_snapshots(data.path());
    let mut fs = BackupFS::new(options(data.path())).unwrap();
    fs.delete_snapshot("before").unwrap();
    drop(fs);
    let fs = BackupFS::open_snapshot(options(data.path()), "after").unwrap();
    let mut expected = vec![0x31; 4 * CHUNK_SIZE as usize];
    expected[CHUNK_SIZE as usize + 123..CHUNK_SIZE as usize + 132].copy_from_slice(b"versioned");
    same_bytes(&read_inode(&fs, 4), &expected);
}

#[test]
fn interrupted_upgrade_does_not_replay_migration_pointers_over_live_deletions() {
    let data = tempfile::tempdir().unwrap();
    crate::legacy::fixture(data.path(), 3);
    let blocker = data.path().join("segments/state.tmp");
    std::fs::create_dir(&blocker).unwrap();
    assert!(BackupFS::new(options(data.path())).is_err());
    let mut readonly = options(data.path());
    readonly.readonly = true;
    assert!(BackupFS::new(readonly).is_err());
    std::fs::remove_dir(blocker).unwrap();
    let fs = BackupFS::new(options(data.path())).unwrap();
    let c = ctrl(&fs);
    let mut contents = Contents::open(c.clone(), Inode(4)).unwrap();
    contents.resize(4 * CHUNK_SIZE).unwrap();
    contents.flush().unwrap();
    c.syncfs().unwrap();
    let mut expected = vec![0; 4 * CHUNK_SIZE as usize];
    expected[..2 * CHUNK_SIZE as usize].fill(0x31);
    expected[CHUNK_SIZE as usize + 123..CHUNK_SIZE as usize + 132].copy_from_slice(b"versioned");
    same_bytes(&read_inode(&fs, 4), &expected);
    drop(contents);
    drop(c);
    drop(fs);
    check_frozen_snapshots(data.path());
}

#[test]
fn pending_marker_resume_uses_published_state_instead_of_rebuilding_legacy_sources() {
    let data = tempfile::tempdir().unwrap();
    crate::legacy::fixture(data.path(), 2);
    let blocker = data.path().join("segments/state.tmp");
    std::fs::create_dir(&blocker).unwrap();
    assert!(BackupFS::new(options(data.path())).is_err());
    let primary = data.path().join("superblock");
    let replicas = crate::superblock::replica_paths(&primary);
    let pending: Vec<_> = replicas
        .iter()
        .map(|path| std::fs::read(path).unwrap())
        .collect();
    std::fs::remove_dir(blocker).unwrap();
    let fs = BackupFS::new(options(data.path())).unwrap();
    let c = ctrl(&fs);
    let mut contents = Contents::open(c.clone(), Inode(4)).unwrap();
    contents.write_at(b"newer", 0).unwrap();
    contents.flush().unwrap();
    c.syncfs().unwrap();
    drop(contents);
    drop(c);
    drop(fs);
    let anchor = data.path().join("segments/state");
    let before = std::fs::read(&anchor).unwrap();
    for (path, bytes) in replicas.iter().zip(pending) {
        std::fs::write(path, bytes).unwrap();
    }
    crate::superblock::Superblock::open_or_create(&primary, "snapshots", false).unwrap();
    assert_eq!(std::fs::read(anchor).unwrap(), before);
    let fs = BackupFS::new(options(data.path())).unwrap();
    assert_eq!(&read_inode(&fs, 4)[..5], b"newer");
}
