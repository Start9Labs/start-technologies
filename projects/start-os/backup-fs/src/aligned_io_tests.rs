use super::*;

#[test]
fn range_reader_reuses_its_open_file_and_switches_to_other_segments() {
    let data = tempfile::tempdir().unwrap();
    let first = data.path().join("first");
    let second = data.path().join("second");
    std::fs::write(&first, vec![0x31; 8192]).unwrap();
    std::fs::write(&second, vec![0x42; 8192]).unwrap();
    let mut reader = RangeReader::default();
    let mut bytes = [0; 16];
    reader.read_exact(&first, 0, &mut bytes).unwrap();
    assert_eq!(bytes, [0x31; 16]);
    std::fs::remove_file(&first).unwrap();
    reader.read_exact(&first, 4096, &mut bytes).unwrap();
    assert_eq!(bytes, [0x31; 16]);
    reader.read_exact(&second, 0, &mut bytes).unwrap();
    assert_eq!(bytes, [0x42; 16]);
}

#[test]
fn range_reader_serves_reverse_and_straddling_reads() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("segment");
    let contents: Vec<u8> = (0..3 * WINDOW + 100).map(|i| (i % 251) as u8).collect();
    std::fs::write(&path, &contents).unwrap();
    let mut reader = RangeReader::default();
    for (offset, len) in [
        (2 * WINDOW + 50, 40),
        (WINDOW - 10, 30),
        (10, 20),
        (WINDOW / 2, WINDOW + 7),
        (3 * WINDOW + 60, 40),
    ] {
        let mut bytes = vec![0; len];
        reader.read_exact(&path, offset as u64, &mut bytes).unwrap();
        assert_eq!(bytes, contents[offset..offset + len]);
    }
    let mut past = [0; 8];
    assert_eq!(
        reader
            .read_exact(&path, contents.len() as u64 - 4, &mut past)
            .unwrap_err()
            .kind(),
        io::ErrorKind::UnexpectedEof
    );
}
