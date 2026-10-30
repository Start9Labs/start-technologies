use std::fs::OpenOptions;

use super::*;
use crate::inode::{FileData, InodeAttributes};

fn key() -> Key {
    Key::from([0x92; 32])
}

fn initial(data: &Path) -> SegmentLog {
    let dir = data.join("segments");
    std::fs::create_dir(&dir).unwrap();
    State::create(
        &dir,
        &key(),
        EccParams::default(),
        &Roots::default(),
        Counters {
            next_seq: 1,
            max_inode: 0,
        },
        None,
    )
    .unwrap();
    SegmentLog::open_sized(dir, key(), 4096, EccParams::default(), false).unwrap()
}

fn attrs(bytes: &[u8]) -> Attributes {
    InodeAttributes::new(Inode(7), None, FileData::Inline(bytes.to_vec())).attrs
}

fn inline(log: &SegmentLog, id: u64) -> Vec<u8> {
    let FileData::Inline(bytes) = log.load(Inode(id)).unwrap().unwrap().contents else {
        panic!()
    };
    bytes
}

#[test]
fn bulk_reads_use_physical_order_and_validate_each_expected_key() {
    let data = tempfile::tempdir().unwrap();
    let mut log = initial(data.path());
    for id in [3, 1, 2] {
        log.put(Inode(id), &attrs(b"value")).unwrap();
    }
    log.sync().unwrap();
    let mut observed = Vec::new();
    log.visit_records(
        [1, 2, 3]
            .into_iter()
            .map(|id| (RecordKey::Inode(id), log.roots().index.get(&id).unwrap())),
        |record| {
            observed.push(record.key());
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(
        observed,
        vec![
            RecordKey::Inode(3),
            RecordKey::Inode(1),
            RecordKey::Inode(2)
        ]
    );
    let reference = log.roots().index.get(&1).unwrap();
    assert!(log
        .visit_records(
            std::iter::once((RecordKey::Inode(99), reference)),
            |_| panic!("wrong identity reached caller")
        )
        .is_err());
}

#[test]
fn bulk_results_separate_corrupt_records_from_backing_io_failures() {
    let data = tempfile::tempdir().unwrap();
    let mut log = initial(data.path());
    log.put(Inode(7), &attrs(b"damaged")).unwrap();
    log.put(Inode(8), &attrs(b"healthy")).unwrap();
    log.sync().unwrap();
    let location = log.roots().index[&7];
    let path = segment_path(log.directory(), location.segment);
    OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .write_all_at(&vec![0; location.len as usize], location.offset)
        .unwrap();
    let mut corrupt = 0;
    let mut healthy = Vec::new();
    log.visit_record_results(
        log.roots()
            .index
            .iter()
            .map(|(&id, loc)| (RecordKey::Inode(id), loc)),
        |record| {
            match record {
                Ok(record) => healthy.push(record.key()),
                Err(_) => corrupt += 1,
            }
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(corrupt, 1);
    assert_eq!(healthy, [RecordKey::Inode(8)]);

    std::fs::remove_file(path).unwrap();
    assert!(log
        .visit_record_results(
            std::iter::once((RecordKey::Inode(7), &location)),
            |_| panic!("backing I/O failure reached visitor"),
        )
        .is_err());
}

#[test]
fn idle_writable_opens_do_not_create_data_segments() {
    let data = tempfile::tempdir().unwrap();
    let log = initial(data.path());
    assert!(list_segments(log.directory()).unwrap().is_empty());
    drop(log);
    let log = SegmentLog::open(data.path().join("segments"), key()).unwrap();
    assert!(list_segments(log.directory()).unwrap().is_empty());
}

#[test]
fn committed_records_and_tombstones_survive_checkpoint_and_reopen() {
    let data = tempfile::tempdir().unwrap();
    let mut log = initial(data.path());
    log.put(Inode(7), &attrs(b"retained")).unwrap();
    log.put(Inode(8), &attrs(b"removed")).unwrap();
    log.tombstone(Inode(8)).unwrap();
    log.save_checkpoint(&[]).unwrap();
    drop(log);
    let log = SegmentLog::open(data.path().join("segments"), key()).unwrap();
    assert_eq!(inline(&log, 7), b"retained");
    assert!(log.load(Inode(8)).unwrap().is_none());
    assert_eq!(log.max_inode(), 8);
}

#[test]
fn clean_close_commits_fast_updates_without_replaying_data_tails() {
    let data = tempfile::tempdir().unwrap();
    let mut log = initial(data.path());
    log.put(Inode(7), &attrs(b"saved on close")).unwrap();
    drop(log);
    let log = SegmentLog::open(data.path().join("segments"), key()).unwrap();
    assert_eq!(inline(&log, 7), b"saved on close");
}

#[test]
fn a_replayed_valid_frame_is_rejected_even_after_renumbering() {
    let data = tempfile::tempdir().unwrap();
    let mut log = initial(data.path());
    let old = attrs(b"earlier");
    log.put(Inode(7), &old).unwrap();
    let old_loc = log.roots.index[&7];
    let old_frame = log.read_raw_frame(old_loc).unwrap();
    let mut current = old.clone();
    current.contents = FileData::Inline(b"current".to_vec());
    log.put(Inode(7), &current).unwrap();
    log.put(Inode(8), &attrs(b"healthy")).unwrap();
    log.sync().unwrap();
    let loc = log.roots.index[&7];
    assert_eq!(loc.len, old_loc.len);
    let file = OpenOptions::new()
        .write(true)
        .open(segment_path(log.directory(), loc.segment))
        .unwrap();
    file.write_all_at(&old_frame, loc.offset).unwrap();
    log.drop_read_caches();
    assert!(log.load(Inode(7)).is_err());
    let old_payload = Frames::new(&old_frame).next().unwrap().payload;
    let renumbered = encode_frame(loc.seq, old_payload);
    file.write_all_at(&renumbered, loc.offset).unwrap();
    log.drop_read_caches();
    assert!(log.load(Inode(7)).is_err());
    assert_eq!(inline(&log, 8), b"healthy");
    drop(log);
    let log = SegmentLog::open(data.path().join("segments"), key()).unwrap();
    assert!(log.load(Inode(7)).is_err());
    assert_eq!(inline(&log, 8), b"healthy");
}

#[test]
fn missing_indexed_data_does_not_fall_back_or_disappear_from_the_index() {
    let data = tempfile::tempdir().unwrap();
    let mut log = initial(data.path());
    log.put(Inode(7), &attrs(b"older")).unwrap();
    log.put(Inode(7), &attrs(b"latest")).unwrap();
    log.sync().unwrap();
    let loc = log.roots.index[&7];
    std::fs::remove_file(segment_path(log.directory(), loc.segment)).unwrap();
    drop(log);
    let log = SegmentLog::open(data.path().join("segments"), key()).unwrap();
    assert!(log.contains(Inode(7)));
    assert!(log.load(Inode(7)).is_err());
}

#[test]
fn unindexed_authenticated_tail_is_ignored() {
    let data = tempfile::tempdir().unwrap();
    let mut log = initial(data.path());
    log.put(Inode(7), &attrs(b"committed")).unwrap();
    log.sync().unwrap();
    let later = seal_inode(
        &key(),
        EccParams::default(),
        Inode(7),
        &attrs(b"uncommitted"),
    )
    .unwrap();
    let path = segment_path(log.directory(), log.roots.index[&7].segment);
    let end = std::fs::metadata(&path).unwrap().len();
    OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .write_all_at(&encode_frame(log.next_seq, &later.sealed.bytes), end)
        .unwrap();
    drop(log);
    let log = SegmentLog::open(data.path().join("segments"), key()).unwrap();
    assert_eq!(inline(&log, 7), b"committed");
}

#[test]
fn ecc_repairs_data_without_invalidating_its_parent_reference() {
    let data = tempfile::tempdir().unwrap();
    let mut log = initial(data.path());
    log.put(Inode(7), &attrs(b"recoverable")).unwrap();
    log.sync().unwrap();
    let loc = log.roots.index[&7];
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(segment_path(log.directory(), loc.segment))
        .unwrap();
    let offset = loc.offset + HEADER_LEN as u64 + loc.payload_len as u64 / 2;
    let mut byte = [0];
    file.read_exact_at(&mut byte, offset).unwrap();
    byte[0] ^= 1;
    file.write_all_at(&byte, offset).unwrap();
    log.drop_read_caches();
    assert_eq!(inline(&log, 7), b"recoverable");
}

#[test]
fn edited_crc_header_lengths_return_an_error_without_allocating_from_them() {
    let data = tempfile::tempdir().unwrap();
    let mut log = initial(data.path());
    log.put(Inode(7), &attrs(b"bounded")).unwrap();
    log.sync().unwrap();
    let loc = log.roots.index[&7];
    let mut bytes = log.read_raw_frame(loc).unwrap();
    bytes[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    let crc = crc32fast::hash(&bytes[8..24]);
    bytes[4..8].copy_from_slice(&crc.to_le_bytes());
    OpenOptions::new()
        .write(true)
        .open(segment_path(log.directory(), loc.segment))
        .unwrap()
        .write_all_at(&bytes, loc.offset)
        .unwrap();
    log.drop_read_caches();
    assert!(log.load(Inode(7)).is_err());
}

#[test]
fn frame_scanner_resynchronizes_and_sequence_overflow_is_an_error() {
    let payload = b"payload";
    let mut bytes = b"torn bytes".to_vec();
    bytes.extend_from_slice(&encode_frame(4, payload));
    let frames: Vec<_> = Frames::new(&bytes).collect();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].seq, 4);
    assert_eq!(frames[0].payload, payload);
    assert!(next_sequence(u64::MAX).is_err());
}
