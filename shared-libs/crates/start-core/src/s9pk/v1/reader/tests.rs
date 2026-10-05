use std::io::Cursor;

use super::*;

#[tokio::test]
async fn read_handle_appends_to_filled_buffers_without_crossing_the_section() {
    let mut source = Cursor::new(b"ABC".to_vec());
    let mut pos = 0;
    let mut handle = ReadHandle {
        pos: &mut pos,
        range: 0..2,
        rdr: &mut source,
    };
    let mut bytes = [0; 9];
    let mut buf = ReadBuf::new(&mut bytes);
    buf.put_slice(b"prefix!");
    std::future::poll_fn(|cx| Pin::new(&mut handle).poll_read(cx, &mut buf))
        .await
        .unwrap();
    assert_eq!(buf.filled(), b"prefix!AB");
    let mut next = [0; 1];
    assert_eq!(handle.read(&mut next).await.unwrap(), 0);
    drop(handle);
    assert_eq!(pos, 2);
    assert_eq!(source.position(), 2);
}

#[tokio::test]
async fn read_handle_supports_read_to_end_across_buffer_growth() {
    let expected: Vec<u8> = (0..=255).cycle().take(65537).collect();
    let mut bytes = expected.clone();
    bytes.extend_from_slice(b"next section");
    let mut source = Cursor::new(bytes);
    let mut pos = 0;
    let mut handle = ReadHandle {
        pos: &mut pos,
        range: 0..expected.len() as u64,
        rdr: &mut source,
    };
    let mut output = Vec::new();
    handle.read_to_end(&mut output).await.unwrap();
    assert_eq!(output, expected);
    drop(handle);
    assert_eq!(pos, expected.len() as u64);
    assert_eq!(source.position(), expected.len() as u64);
}
