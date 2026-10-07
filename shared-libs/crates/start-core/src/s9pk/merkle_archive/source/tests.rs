use std::sync::Arc;

use tokio::io::AsyncReadExt;

use super::{ArchiveSource, FileSource};

#[tokio::test]
async fn section_slices_stop_at_the_section_boundary() {
    let source: Arc<[u8]> = Arc::from(&b"before-payload-after"[..]);
    let section = source.section(7, 7);
    for (offset, size, expected) in [
        (0, 100, &b"payload"[..]),
        (0, 0, &b""[..]),
        (2, 3, &b"ylo"[..]),
        (5, u64::MAX, &b"ad"[..]),
        (7, 1, &b""[..]),
        (8, 100, &b""[..]),
        (u64::MAX, 1, &b""[..]),
    ] {
        let mut reader = section.slice(offset, size).await.unwrap();
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await.unwrap();
        assert_eq!(bytes, expected, "offset {offset}, size {size}");
    }
}

#[tokio::test]
async fn section_slice_reports_absolute_offset_overflow() {
    let source: Arc<[u8]> = Arc::from(&b"data"[..]);
    let section = source.section(u64::MAX, 10);
    assert!(section.slice(1, 1).await.is_err());
}
