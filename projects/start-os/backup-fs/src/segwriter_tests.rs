use super::*;

fn frame(seed: u8, len: usize) -> Vec<u8> {
    (0..len).map(|i| seed.wrapping_add(i as u8)).collect()
}

fn read(dir: &Path, id: u64) -> Vec<u8> {
    std::fs::read(segment_path(dir, id)).unwrap()
}

#[test]
fn frames_stay_readable_before_and_after_their_writes_complete() {
    let dir = tempfile::tempdir().unwrap();
    let mut writer = SegmentWriter::new(dir.path(), 64 << 20, 3);
    let mut written = Vec::new();
    for seed in 0..40u8 {
        let bytes = frame(seed, 300_000 + seed as usize * 8);
        let (segment, offset) = writer.append(Stream::Data, bytes.clone()).unwrap();
        assert_eq!(segment, 3);
        assert_eq!(&*writer.pending(segment, offset).unwrap(), bytes.as_slice());
        written.push((offset, bytes));
    }
    let mut expected = 0;
    for (offset, bytes) in &written {
        assert_eq!(*offset, expected);
        expected += bytes.len() as u64;
    }
    writer.sync().unwrap();
    let on_disk = read(dir.path(), 3);
    assert_eq!(on_disk.len() as u64, expected.div_ceil(ALIGN) * ALIGN);
    for (offset, bytes) in &written {
        let offset = *offset as usize;
        assert_eq!(&on_disk[offset..offset + bytes.len()], bytes.as_slice());
        assert!(writer.pending(3, offset as u64).is_none());
    }
    assert_eq!(writer.take_lengths()[&3], on_disk.len() as u64);
    assert!(!writer.has_unsynced());
}

#[test]
fn sync_pads_to_alignment_and_later_frames_start_on_a_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let mut writer = SegmentWriter::new(dir.path(), 64 << 20, 0);
    let first = writer.append(Stream::Metadata, frame(1, 100)).unwrap();
    writer.sync().unwrap();
    let second = writer.append(Stream::Metadata, frame(2, 200)).unwrap();
    assert_eq!(first, (0, 0));
    assert_eq!(second, (0, ALIGN));
    writer.sync().unwrap();
    let on_disk = read(dir.path(), 0);
    assert_eq!(on_disk.len() as u64, 2 * ALIGN);
    assert_eq!(&on_disk[..100], frame(1, 100).as_slice());
    assert!(on_disk[100..ALIGN as usize].iter().all(|&b| b == 0));
    assert_eq!(
        &on_disk[ALIGN as usize..ALIGN as usize + 200],
        frame(2, 200).as_slice()
    );
}

#[test]
fn streams_use_separate_segments_and_rotate_at_the_limit() {
    let dir = tempfile::tempdir().unwrap();
    let mut writer = SegmentWriter::new(dir.path(), 8192, 10);
    let meta = writer.append(Stream::Metadata, frame(1, 4000)).unwrap();
    let data = writer.append(Stream::Data, frame(2, 4000)).unwrap();
    let rotated = writer.append(Stream::Data, frame(3, 6000)).unwrap();
    let oversized = writer.append(Stream::Data, frame(4, 20000)).unwrap();
    assert_eq!(meta, (10, 0));
    assert_eq!(data, (11, 0));
    assert_eq!(rotated, (12, 0));
    assert_eq!(oversized, (13, 0));
    assert_eq!(
        writer
            .active_segments()
            .collect::<std::collections::BTreeSet<_>>(),
        [10, 13].into()
    );
    writer.sync().unwrap();
    let lengths = writer.take_lengths();
    for id in [10, 11, 12, 13] {
        assert_eq!(lengths[&id], read(dir.path(), id).len() as u64);
    }
    assert_eq!(&read(dir.path(), 13)[..20000], frame(4, 20000).as_slice());
}

#[test]
fn idle_writers_create_no_segments() {
    let dir = tempfile::tempdir().unwrap();
    let mut writer = SegmentWriter::new(dir.path(), 8192, 0);
    writer.sync().unwrap();
    assert!(writer.take_lengths().is_empty());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn a_failed_backing_write_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let mut writer = SegmentWriter::new(dir.path(), 64 << 20, 0xfa11_0000);
    let (segment, offset) = writer.append(Stream::Data, frame(9, 5000)).unwrap();
    FAIL_SEGMENT.store(segment, std::sync::atomic::Ordering::SeqCst);
    let result = writer.sync();
    FAIL_SEGMENT.store(u64::MAX, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(result.unwrap_err().to_errno(), libc::ENOSPC);
    assert_eq!(
        writer
            .append(Stream::Data, frame(1, 10))
            .unwrap_err()
            .to_errno(),
        libc::ENOSPC
    );
    assert!(writer.sync().is_err());
    assert_eq!(
        &*writer.pending(segment, offset).unwrap(),
        frame(9, 5000).as_slice()
    );
}
