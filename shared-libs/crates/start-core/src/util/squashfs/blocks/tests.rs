use std::io::Cursor;

use proptest::prelude::*;

use super::*;

const REFERENCE_BLOCK: &[u8] = &[
    30, 0, 40, 181, 47, 253, 32, 32, 173, 0, 0, 2, 66, 4, 10, 240, 25, 3, 255, 255, 191, 156, 148,
    50, 5, 207, 245, 252, 241, 143, 69, 0,
];
const REFERENCE_INODE: &[u8] = &[
    1, 0, 237, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 2, 0, 0,
    0,
];

fn encode(bytes: &[u8]) -> Vec<u8> {
    let mut writer = MetadataBlocksWriter::new(Vec::new());
    Write::write_all(&mut writer, bytes).unwrap();
    Write::flush(&mut writer).unwrap();
    writer.writer
}

fn decode(bytes: &[u8]) -> io::Result<Vec<u8>> {
    let mut result = Vec::new();
    Read::read_to_end(&mut MetadataBlocksReader::new(bytes), &mut result)?;
    Ok(result)
}

#[test]
fn reads_squashfs_tools_4_6_1_inode_block() {
    // mksquashfs: empty root, zstd, all-root, all-time 0, no-xattrs, no-exports.
    assert_eq!(decode(REFERENCE_BLOCK).unwrap(), REFERENCE_INODE);
}

#[test]
fn reads_literal_uncompressed_blocks() {
    assert_eq!(decode(b"\x03\x80abc\x02\x80de").unwrap(), b"abcde");
}

#[test]
fn block_boundaries() {
    for size in [0, 1, 8191, 8192, 8193, 16384, 32771] {
        let bytes = vec![0x53; size];
        assert_eq!(decode(&encode(&bytes)).unwrap(), bytes);
    }
}

#[test]
fn compressed_frames_fit_kernel_block_workspace() {
    for level in [3, 15, 22] {
        for size in [
            32,
            1024,
            8191,
            METADATA_SIZE,
            65536,
            131071,
            131072,
            1048576,
        ] {
            let block = encode_block(&vec![b'a'; size], level).unwrap();
            assert!(block.compressed);
            let content_size = zstd::zstd_safe::get_frame_content_size(&block.bytes)
                .unwrap()
                .unwrap();
            assert_eq!(content_size, size as u64);
            let single_segment = block.bytes[4] & 0x20 != 0;
            let window_size = if single_segment {
                content_size
            } else {
                let descriptor = block.bytes[5];
                let base = 1u64 << (10 + (descriptor >> 3));
                base + (base / 8) * u64::from(descriptor & 7)
            };
            assert!(window_size <= size.max(1024) as u64);
        }
    }
}

#[test]
fn incompressible_blocks_preserve_raw_fallback() {
    let bytes: Vec<u8> = (0..=255).collect();
    let block = encode_block(&bytes, 15).unwrap();
    assert!(!block.compressed);
    assert_eq!(block.bytes, bytes);
}

#[test]
fn flushes_append_without_gaps() {
    let mut writer = MetadataBlocksWriter::new(Cursor::new(Vec::new()));
    let mut expected = Vec::new();
    let mut random = 123456789u64;
    let input: Vec<u8> = (0..METADATA_SIZE)
        .map(|_| {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            random as u8
        })
        .collect();
    for bytes in [&input[..], b"short", &input[..], b"last"] {
        Write::write_all(&mut writer, bytes).unwrap();
        Write::flush(&mut writer).unwrap();
        expected.extend_from_slice(bytes);
    }
    let encoded = writer.writer.into_inner();
    assert_eq!(&encoded[..2], &0xa000u16.to_le_bytes());
    assert_eq!(&encoded[2..8194], &input);
    assert_eq!(decode(&encoded).unwrap(), expected);
}

#[test]
fn rejects_truncation_and_invalid_frames() {
    for bytes in [
        &b"\x03"[..],
        b"\x03\x80ab",
        b"\x00\x00",
        b"\x01\xa0",
        b"\x04\x00junk",
    ] {
        assert!(decode(bytes).is_err(), "accepted {bytes:?}");
    }
    for size in 1..REFERENCE_BLOCK.len() {
        assert!(decode(&REFERENCE_BLOCK[..size]).is_err());
    }
    let mut extra = REFERENCE_BLOCK.to_vec();
    extra[0] += 1;
    extra.push(0);
    assert!(decode(&extra).is_err());
}

#[test]
fn rejects_oversized_decompression() {
    let frame = encode_block(&vec![0; METADATA_SIZE + 1], 3).unwrap();
    assert!(frame.compressed);
    let mut bytes = (frame.bytes.len() as u16).to_le_bytes().to_vec();
    bytes.extend(frame.bytes);
    assert!(decode(&bytes).is_err());
}

#[test]
fn metadata_references_are_physical_block_and_decoded_offset() {
    let mut table = MetadataTable::new(3);
    assert_eq!(table.reference().unwrap(), 0);
    table.write_all(&vec![b'a'; METADATA_SIZE]).unwrap();
    let second = table.reference().unwrap();
    assert_eq!(second & 0xffff, 0);
    assert!(second >> 16 > 0);
    table.write_all(b"tail").unwrap();
    assert_eq!(table.reference().unwrap(), second + 4);
    let bytes = table.finish().unwrap();
    assert_eq!(decode(&bytes[(second >> 16) as usize..]).unwrap(), b"tail");
}

struct ShortIo<T> {
    inner: T,
    pending: bool,
    shutdown: bool,
}
impl<T> ShortIo<T> {
    fn new(inner: T) -> Self {
        Self {
            inner,
            pending: false,
            shutdown: false,
        }
    }
    fn suspend(&mut self, cx: &Context<'_>) -> bool {
        self.pending = !self.pending;
        if self.pending {
            cx.waker().wake_by_ref();
        }
        self.pending
    }
}
impl<T: Read> Read for ShortIo<T> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = bytes.len().min(3);
        self.inner.read(&mut bytes[..count])
    }
}
impl<T: Write> Write for ShortIo<T> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.inner.write(&bytes[..bytes.len().min(3)])
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
impl<T: AsyncRead + Unpin> AsyncRead for ShortIo<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.suspend(cx) {
            return Poll::Pending;
        }
        let count = bytes.remaining().min(3);
        let mut short = ReadBuf::new(&mut bytes.initialize_unfilled()[..count]);
        ready!(Pin::new(&mut this.inner).poll_read(cx, &mut short))?;
        let count = short.filled().len();
        bytes.advance(count);
        Poll::Ready(Ok(()))
    }
}
impl<T: AsyncWrite + Unpin> AsyncWrite for ShortIo<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if this.suspend(cx) {
            return Poll::Pending;
        }
        Pin::new(&mut this.inner).poll_write(cx, &bytes[..bytes.len().min(3)])
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.suspend(cx) {
            return Poll::Pending;
        }
        Pin::new(&mut this.inner).poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.shutdown = true;
        Pin::new(&mut this.inner).poll_shutdown(cx)
    }
}

#[test]
fn sync_short_reads_and_writes() {
    let bytes: Vec<u8> = (0..20000).map(|i| (i % 251) as u8).collect();
    let mut writer = MetadataBlocksWriter::new(ShortIo::new(Vec::new()));
    Write::write_all(&mut writer, &bytes).unwrap();
    Write::flush(&mut writer).unwrap();
    let mut reader = MetadataBlocksReader::new(ShortIo::new(Cursor::new(writer.writer.inner)));
    let mut decoded = Vec::new();
    Read::read_to_end(&mut reader, &mut decoded).unwrap();
    assert_eq!(decoded, bytes);
}

#[tokio::test]
async fn async_short_pending_reads_and_writes() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let bytes: Vec<u8> = (0..20000).map(|i| (i % 251) as u8).collect();
    let mut writer = MetadataBlocksWriter::new(ShortIo::new(Vec::new()));
    AsyncWriteExt::write_all(&mut writer, &bytes).await.unwrap();
    AsyncWriteExt::shutdown(&mut writer).await.unwrap();
    assert!(writer.writer.shutdown);
    let encoded = writer.writer.inner;
    assert_eq!(decode(&encoded).unwrap(), bytes);
    let mut reader = MetadataBlocksReader::new(ShortIo::new(Cursor::new(encoded)));
    let mut decoded = Vec::new();
    AsyncReadExt::read_to_end(&mut reader, &mut decoded)
        .await
        .unwrap();
    assert_eq!(decoded, bytes);
}

#[tokio::test]
async fn async_truncation_and_zero_length_reads() {
    use tokio::io::AsyncReadExt;
    for bytes in [&b"\x03"[..], b"\x03\x80ab", b"\x00\x00", b"\x04\x00junk"] {
        let mut reader = MetadataBlocksReader::new(ShortIo::new(bytes));
        assert_eq!(AsyncReadExt::read(&mut reader, &mut []).await.unwrap(), 0);
        assert!(
            AsyncReadExt::read_to_end(&mut reader, &mut Vec::new())
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn cancelled_flush_resumes_without_duplicate_output() {
    use tokio::io::AsyncWriteExt;
    let mut writer = MetadataBlocksWriter::new(ShortIo::new(Vec::new()));
    AsyncWriteExt::write_all(&mut writer, b"cancelled then resumed")
        .await
        .unwrap();
    {
        let flush = AsyncWriteExt::flush(&mut writer);
        tokio::pin!(flush);
        assert!(futures::poll!(&mut flush).is_pending());
        assert!(futures::poll!(&mut flush).is_pending());
    }
    assert_eq!(writer.output_position, 3);
    AsyncWriteExt::flush(&mut writer).await.unwrap();
    assert_eq!(
        decode(&writer.writer.inner).unwrap(),
        b"cancelled then resumed"
    );
}

#[test]
fn sync_write_zero_is_an_error() {
    let mut writer = MetadataBlocksWriter::new(&mut [][..]);
    Write::write_all(&mut writer, b"data").unwrap();
    assert_eq!(
        Write::flush(&mut writer).unwrap_err().kind(),
        io::ErrorKind::WriteZero
    );
}

proptest! {
    #[test]
    fn chunked_roundtrip(bytes in prop::collection::vec(any::<u8>(), 0..50000), chunk in 1usize..1000) {
        let mut writer = MetadataBlocksWriter::new(Vec::new());
        for bytes in bytes.chunks(chunk) { Write::write_all(&mut writer, bytes).unwrap(); }
        Write::flush(&mut writer).unwrap();
        let mut reader = MetadataBlocksReader::new(&writer.writer[..]);
        let mut decoded = Vec::new();
        let mut buffer = vec![0; chunk];
        loop {
            let count = Read::read(&mut reader, &mut buffer).unwrap();
            if count == 0 { break; }
            decoded.extend_from_slice(&buffer[..count]);
        }
        prop_assert_eq!(decoded, bytes);
    }
}
