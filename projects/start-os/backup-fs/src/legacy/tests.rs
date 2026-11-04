use std::path::Path;

use crate::blockstore::CHUNK_SIZE;
use crate::contents::Contents;
use crate::ctrl::Controller;
use crate::inode::{Inode, InodeAttributes};
use crate::superblock::Superblock;
use crate::{BackupFS, BackupFSOptions};

fn options(data: &Path, readonly: bool) -> BackupFSOptions {
    BackupFSOptions {
        data_dir: data.to_owned(),
        password: "password".into(),
        readonly,
        file_size_padding: Some(0.05),
        setuid_support: false,
        idmapped: false,
    }
}
fn ctrl(fs: &BackupFS) -> Controller {
    fs.handler.lock().unwrap().ctrl().clone()
}
fn read(fs: &BackupFS, inode: u64) -> Vec<u8> {
    let mut contents = Contents::open(ctrl(fs), Inode(inode)).unwrap();
    let mut bytes = vec![0; contents.inode.attrs.size as usize];
    contents.read_exact_at(&mut bytes, 0).unwrap();
    bytes
}

#[test]
fn legacy_superblock_without_segments_opens_without_conversion() {
    let source = tempfile::tempdir().unwrap();
    super::fixture(source.path());
    let data = tempfile::tempdir().unwrap();
    std::fs::copy(
        source.path().join("superblock"),
        data.path().join("superblock"),
    )
    .unwrap();
    assert!(!data.path().join("segments").exists());
    for readonly in [false, true] {
        let fs = BackupFS::new(options(data.path(), readonly)).unwrap();
        assert!(ctrl(&fs).is_legacy());
        assert!(data.path().join("segments").is_dir());
        assert!(!data.path().join("segments/state").exists());
    }
}

#[test]
fn ordinary_legacy_opens_leave_superblocks_and_data_format_unchanged() {
    let data = tempfile::tempdir().unwrap();
    super::fixture(data.path());
    let primary = data.path().join("superblock");
    let before = std::fs::read(&primary).unwrap();
    assert_eq!(
        BackupFS::convert(options(data.path(), true))
            .unwrap_err()
            .to_errno(),
        libc::EROFS
    );
    for readonly in [true, false] {
        let mut fs = BackupFS::new(options(data.path(), readonly)).unwrap();
        assert!(ctrl(&fs).is_legacy());
        assert_eq!(read(&fs, 2), b"frozen-inline");
        assert_eq!(read(&fs, 3), vec![0x42; 12000]);
        assert_eq!(read(&fs, 4), vec![0x31; 4 * CHUNK_SIZE as usize]);
        if !readonly {
            assert!(fs.create_snapshot("requires-conversion").is_err());
        }
        drop(fs);
        assert_eq!(std::fs::read(&primary).unwrap(), before);
        assert!(!data.path().join("segments/state").exists());
    }
}

#[test]
fn legacy_writes_restore_after_reopen_and_survive_explicit_conversion() {
    let data = tempfile::tempdir().unwrap();
    super::fixture(data.path());
    {
        let fs = BackupFS::new(options(data.path(), false)).unwrap();
        assert!(BackupFS::convert(options(data.path(), false)).is_err());
        for (inode, bytes) in [
            (2, b"edited inline".as_slice()),
            (3, b"edited packed".as_slice()),
            (4, b"edited block".as_slice()),
        ] {
            let mut contents = Contents::open(ctrl(&fs), Inode(inode)).unwrap();
            contents.write_at(bytes, 0).unwrap();
            contents.fsync().unwrap();
        }
        let mut contents = Contents::open(ctrl(&fs), Inode(4)).unwrap();
        contents.resize(CHUNK_SIZE + 17).unwrap();
        contents.resize(2 * CHUNK_SIZE).unwrap();
        contents.fsync().unwrap();
    }
    for converted in [false, true] {
        if converted {
            BackupFS::convert(options(data.path(), false)).unwrap();
        }
        let fs = BackupFS::new(options(data.path(), true)).unwrap();
        assert_eq!(&read(&fs, 2)[..13], b"edited inline");
        assert_eq!(&read(&fs, 3)[..13], b"edited packed");
        let bytes = read(&fs, 4);
        assert_eq!(&bytes[..12], b"edited block");
        assert!(bytes[CHUNK_SIZE as usize + 17..]
            .iter()
            .all(|&byte| byte == 0));
        let attrs: InodeAttributes = ctrl(&fs).load(Inode(3)).unwrap();
        assert_eq!(
            (attrs.attrs.uid, attrs.attrs.gid, attrs.attrs.mode),
            (123, 456, 0o6751)
        );
        assert_eq!(
            attrs.attrs.xattrs.get(b"user.fixture".as_slice()).unwrap(),
            b"original"
        );
        assert_eq!(ctrl(&fs).is_legacy(), !converted);
    }
}

#[test]
fn legacy_deletion_tombstones_the_inode_before_removing_its_blocks() {
    let data = tempfile::tempdir().unwrap();
    super::fixture(data.path());
    let fs = BackupFS::new(options(data.path(), false)).unwrap();
    let mut handler = fs.handler.lock().unwrap();
    let mut file: InodeAttributes = handler.ctrl().load(Inode(4)).unwrap();
    file.attrs.parents.clear();
    assert!(handler.gc_inode(&file).unwrap());
    let durable = Controller::new(options(data.path(), true)).unwrap();
    assert!(durable.load::<InodeAttributes>(Inode(4)).is_err());
}

#[test]
fn legacy_password_changes_preserve_format_one() {
    let data = tempfile::tempdir().unwrap();
    super::fixture(data.path());
    {
        let mut fs = BackupFS::new(options(data.path(), false)).unwrap();
        fs.change_password("changed password").unwrap();
    }
    let primary = data.path().join("superblock");
    assert!(Superblock::load(&primary, "password", true).is_err());
    assert_eq!(
        Superblock::load(&primary, "changed password", true)
            .unwrap()
            .format_version,
        1
    );
    let mut config = options(data.path(), true);
    config.password = "changed password".into();
    assert_eq!(read(&BackupFS::new(config).unwrap(), 2), b"frozen-inline");
}
