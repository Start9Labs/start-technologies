use crate::contents::Contents;
use crate::ctrl::Controller;
use crate::inode::{ContentId, FileData, Inode, InodeAttributes};
use crate::BackupFSOptions;

fn controller(data: &tempfile::TempDir) -> Controller {
    padded_controller(data, None)
}

fn padded_controller(data: &tempfile::TempDir, padding: Option<f64>) -> Controller {
    Controller::new(BackupFSOptions {
        data_dir: data.path().to_owned(),
        setuid_support: false,
        password: "test".into(),
        file_size_padding: padding,
        readonly: false,
        idmapped: false,
        enable_snapshots: true,
    })
    .unwrap()
}

#[test]
fn new_block_partial_writes_do_not_read_backing_objects_beyond_eof() {
    let data = tempfile::tempdir().unwrap();
    let ctrl = controller(&data);
    let inode = Inode(2);
    let attrs = InodeAttributes::new(inode, None, FileData::File(ContentId(2)));
    let path = ctrl.block_path(ContentId(2), 0);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"orphaned data outside the logical file").unwrap();
    let mut contents = Contents::open_with_attrs(ctrl, attrs, false).unwrap();
    assert_eq!(contents.write_at(b"new", 128).unwrap(), 3);
    let mut bytes = vec![0xff; 131];
    contents.read_exact_at(&mut bytes, 0).unwrap();
    assert_eq!(&bytes[..128], &[0; 128]);
    assert_eq!(&bytes[128..], b"new");
}

#[test]
fn partial_overwrite_of_existing_block_still_preserves_untouched_bytes() {
    let data = tempfile::tempdir().unwrap();
    let ctrl = controller(&data);
    let inode = Inode(2);
    let attrs = InodeAttributes::new(inode, None, FileData::File(ContentId(2)));
    let mut contents = Contents::open_with_attrs(ctrl.clone(), attrs, false).unwrap();
    contents.write_at(b"before-content-after", 0).unwrap();
    contents.flush().unwrap();
    let mut contents = Contents::open(ctrl, inode).unwrap();
    contents.write_at(b"updated", 7).unwrap();
    let mut bytes = [0; 20];
    contents.read_exact_at(&mut bytes, 0).unwrap();
    assert_eq!(&bytes, b"before-updated-after");
}

#[test]
fn cow_object_writes_publish_only_at_the_requested_commit() {
    use std::io::Write;
    let data = tempfile::tempdir().unwrap();
    let ctrl = controller(&data);
    for (idx, durable) in [false, true].into_iter().enumerate() {
        let id = ctrl.block_id(ContentId(2), idx as u64);
        ctrl.write_object(id, durable, |path, binding| {
            let sealed = crate::vault::seal_bound(b"object", ctrl.key(), ctrl.ecc(), binding);
            let mut output = crate::atomic_file::AtomicFile::create_buffered(path)?;
            output.write_all(&sealed.bytes)?;
            output.save_fast()
        })
        .unwrap();
        let (path, binding) = ctrl.resolve_object(id).unwrap().unwrap();
        assert_eq!(
            crate::vault::open_bound(&std::fs::read(path).unwrap(), ctrl.key(), binding)
                .unwrap()
                .plaintext,
            b"object"
        );
        let (_, roots) =
            crate::state::State::open(&data.path().join("segments"), ctrl.key()).unwrap();
        assert_eq!(roots.objects.contains_key(&id), durable);
        if durable {
            assert_eq!(roots.objects.len(), 2);
        }
    }
}

#[test]
fn framed_padding_stays_outside_logical_data_after_growth() {
    let data = tempfile::tempdir().unwrap();
    let ctrl = padded_controller(&data, Some(1.0));
    let inode = Inode(2);
    let attrs = InodeAttributes::new(inode, None, FileData::File(ContentId(2)));
    let mut contents = Contents::open_with_attrs(ctrl.clone(), attrs, false).unwrap();
    contents.write_at(b"abc", 0).unwrap();
    contents.flush().unwrap();
    let mut contents = Contents::open(ctrl.clone(), inode).unwrap();
    contents.resize(64).unwrap();
    let mut contents = Contents::open(ctrl, inode).unwrap();
    let mut bytes = [0xff; 64];
    contents.read_exact_at(&mut bytes, 0).unwrap();
    assert_eq!(&bytes[..3], b"abc");
    assert_eq!(&bytes[3..], &[0; 61]);
}

fn legacy_padded_contents(ctrl: &Controller) -> Contents {
    let mut attrs = InodeAttributes::new(Inode(2), None, FileData::File(ContentId(2)));
    attrs.attrs.size = 3;
    let mut legacy = b"abc".to_vec();
    legacy.resize(64, 0xaa);
    let stored = crate::compress::compress(&legacy, crate::compress::Codec::None);
    ctrl.write_object(ctrl.block_id(ContentId(2), 0), true, |path, binding| {
        let sealed = crate::vault::seal_bound(&stored, ctrl.key(), ctrl.ecc(), binding);
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, sealed.bytes)?;
        Ok(())
    })
    .unwrap();
    ctrl.save(&attrs).unwrap();
    Contents::open(ctrl.clone(), Inode(2)).unwrap()
}

#[test]
fn legacy_padding_is_clipped_before_resize() {
    let data = tempfile::tempdir().unwrap();
    let ctrl = padded_controller(&data, Some(0.05));
    let mut contents = legacy_padded_contents(&ctrl);
    contents.resize(64).unwrap();
    let mut contents = Contents::open(ctrl, Inode(2)).unwrap();
    let mut bytes = [0xff; 64];
    contents.read_exact_at(&mut bytes, 0).unwrap();
    assert_eq!(&bytes[..3], b"abc");
    assert_eq!(&bytes[3..], &[0; 61]);
}

#[test]
fn legacy_padding_is_clipped_before_allocation_grows_the_file() {
    let data = tempfile::tempdir().unwrap();
    let ctrl = padded_controller(&data, Some(0.05));
    let mut contents = legacy_padded_contents(&ctrl);
    contents.fallocate(0, 64, 0, false).unwrap();
    contents.flush().unwrap();
    let mut contents = Contents::open(ctrl, Inode(2)).unwrap();
    let mut bytes = [0xff; 64];
    contents.read_exact_at(&mut bytes, 0).unwrap();
    assert_eq!(&bytes[..3], b"abc");
    assert_eq!(&bytes[3..], &[0; 61]);
}

#[test]
fn legacy_padding_is_clipped_before_sparse_writes_grow_the_file() {
    let data = tempfile::tempdir().unwrap();
    let ctrl = padded_controller(&data, Some(0.05));
    let mut contents = legacy_padded_contents(&ctrl);
    contents.write_at(b"z", 100).unwrap();
    contents.flush().unwrap();
    let mut contents = Contents::open(ctrl, Inode(2)).unwrap();
    let mut bytes = [0xff; 101];
    contents.read_exact_at(&mut bytes, 0).unwrap();
    assert_eq!(&bytes[..3], b"abc");
    assert_eq!(&bytes[3..100], &[0; 97]);
    assert_eq!(bytes[100], b'z');
}
