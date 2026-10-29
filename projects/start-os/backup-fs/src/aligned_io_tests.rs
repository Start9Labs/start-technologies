use super::*;

struct RecordingFile {
    file: std::fs::File,
    reads: RefCell<Vec<(usize, u64)>>,
    writes: RefCell<Vec<(usize, u64)>>,
    short_writes: bool,
    interrupt_once: std::cell::Cell<bool>,
    fail_write: std::cell::Cell<bool>,
}

impl AsRawFd for RecordingFile {
    fn as_raw_fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }
}

impl FileExt for RecordingFile {
    fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<usize> {
        assert_eq!(buf.as_ptr() as usize % BLOCK_SIZE, 0);
        assert_eq!(buf.len() % BLOCK_SIZE, 0);
        assert_eq!(offset % BLOCK_SIZE as u64, 0);
        self.reads.borrow_mut().push((buf.len(), offset));
        self.file.read_at(buf, offset)
    }

    fn write_at(&self, buf: &[u8], offset: u64) -> io::Result<usize> {
        assert_eq!(buf.as_ptr() as usize % BLOCK_SIZE, 0);
        assert_eq!(buf.len() % BLOCK_SIZE, 0);
        assert_eq!(offset % BLOCK_SIZE as u64, 0);
        self.writes.borrow_mut().push((buf.len(), offset));
        if self.interrupt_once.replace(false) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if self.fail_write.replace(false) {
            return Err(io::ErrorKind::WriteZero.into());
        }
        let len = if self.short_writes {
            BLOCK_SIZE.min(buf.len())
        } else {
            buf.len()
        };
        self.file.write_at(&buf[..len], offset)
    }
}

fn buffered(initial: &[u8], written_len: Option<u64>) -> BufferedDirectFile<RecordingFile> {
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(initial).unwrap();
    BufferedDirectFile {
        file: Some(RecordingFile {
            file,
            reads: RefCell::new(Vec::new()),
            writes: RefCell::new(Vec::new()),
            short_writes: false,
            interrupt_once: std::cell::Cell::new(false),
            fail_write: std::cell::Cell::new(false),
        }),
        state: RefCell::new(BufState {
            buf: AlignedBuf::new(BUF_CAP),
            base: u64::MAX,
            valid: 0,
            loaded: 0,
            dirty_start: usize::MAX,
            dirty_end: 0,
            pos: 0,
            ring: None,
            flush_chunk: NETFS_FLUSH_CHUNK,
            written_len,
        }),
    }
}

fn disk_bytes(file: &RecordingFile) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut file = file.file.try_clone().unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();
    file.read_to_end(&mut bytes).unwrap();
    bytes
}

#[test]
fn new_output_crosses_windows_without_edge_reads() {
    let mut output = buffered(&[], Some(0));
    let bytes: Vec<_> = (0..BUF_CAP + 173).map(|i| (i % 251) as u8).collect();
    output.write_all(&bytes).unwrap();
    output.flush().unwrap();
    assert!(output.file().reads.borrow().is_empty());
    let stored = disk_bytes(output.file());
    assert_eq!(&stored[..bytes.len()], bytes);
    assert!(stored[bytes.len()..].iter().all(|&b| b == 0));
    assert_eq!(output.file().writes.borrow().len(), 17);
}

#[test]
fn existing_output_preserves_both_unaligned_edges() {
    let mut output = buffered(&vec![0x5a; 2 * BLOCK_SIZE], None);
    output.seek(SeekFrom::Start(7)).unwrap();
    output.write_all(b"replacement").unwrap();
    output.flush().unwrap();
    assert_eq!(&*output.file().reads.borrow(), &[(BLOCK_SIZE, 0)]);
    let stored = disk_bytes(output.file());
    assert_eq!(&stored[..7], &[0x5a; 7]);
    assert_eq!(&stored[7..18], b"replacement");
    assert!(stored[18..].iter().all(|&b| b == 0x5a));
}

#[test]
fn new_output_revisiting_persisted_data_preserves_edges() {
    let mut output = buffered(&[], Some(0));
    output.write_all(&vec![0x5a; BUF_CAP + BLOCK_SIZE]).unwrap();
    output.flush().unwrap();
    output.seek(SeekFrom::Start(7)).unwrap();
    output.write_all(b"new").unwrap();
    output.flush().unwrap();
    assert_eq!(&*output.file().reads.borrow(), &[(BLOCK_SIZE, 0)]);
    let stored = disk_bytes(output.file());
    assert_eq!(&stored[..7], &[0x5a; 7]);
    assert_eq!(&stored[7..10], b"new");
    assert!(stored[10..].iter().all(|&b| b == 0x5a));
}

#[test]
fn fallback_retries_interruptions_and_short_writes_without_losing_data() {
    let mut output = buffered(&[], Some(0));
    output.file.as_mut().unwrap().short_writes = true;
    output.file().interrupt_once.set(true);
    output
        .write_all(&vec![0x5a; NETFS_FLUSH_CHUNK + 17])
        .unwrap();
    output.flush().unwrap();
    let stored = disk_bytes(output.file());
    assert!(stored[..NETFS_FLUSH_CHUNK + 17].iter().all(|&b| b == 0x5a));
    assert!(stored[NETFS_FLUSH_CHUNK + 17..].iter().all(|&b| b == 0));
    assert!(output.file().reads.borrow().is_empty());
}

#[test]
fn failed_flush_does_not_advance_known_output_length() {
    let mut output = buffered(&[], Some(0));
    output.file().fail_write.set(true);
    output.write_all(b"new").unwrap();
    assert_eq!(output.flush().unwrap_err().kind(), io::ErrorKind::WriteZero);
    assert_eq!(output.state.borrow().written_len, Some(0));
    output.flush().unwrap();
    assert!(output.file().reads.borrow().is_empty());
    assert_eq!(&disk_bytes(output.file())[..3], b"new");
}
