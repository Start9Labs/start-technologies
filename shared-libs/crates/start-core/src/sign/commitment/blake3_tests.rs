use std::sync::Arc;
use std::time::Duration;

use tokio::io::AsyncReadExt;

use super::*;

fn abc() -> Blake3Commitment {
    Blake3Commitment {
        hash: crate::util::serde::Base64(
            hex::decode("6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85")
                .unwrap()
                .try_into()
                .unwrap(),
        ),
        size: 3,
    }
}

#[tokio::test]
async fn complete_commitment_rejects_missing_changed_and_trailing_bytes() {
    let bytes: Arc<[u8]> = Arc::from(&b"abc"[..]);
    assert_eq!(Blake3Commitment::create(&bytes).await.unwrap(), abc());
    let mut output = Vec::new();
    abc().copy_to(&bytes, &mut output).await.unwrap();
    assert_eq!(output, b"abc");
    for invalid in [&b""[..], b"ab", b"abd", b"abc\0"] {
        let source: Arc<[u8]> = Arc::from(invalid);
        assert!(abc().check(&source).await.is_err());
    }
}

struct NonterminatingSuffix;

impl ArchiveSource for NonterminatingSuffix {
    type FetchAllReader = tokio::io::Chain<std::io::Cursor<&'static [u8]>, tokio::io::Repeat>;
    type FetchReader = std::io::Cursor<&'static [u8]>;

    async fn fetch_all(&self) -> Result<Self::FetchAllReader, Error> {
        Ok(std::io::Cursor::new(&b"abc"[..]).chain(tokio::io::repeat(0)))
    }

    async fn fetch(&self, _: u64, _: u64) -> Result<Self::FetchReader, Error> {
        panic!("whole-resource verification must not select a prefix")
    }
}

#[tokio::test]
async fn unknown_length_sources_reject_an_extra_byte_without_waiting_for_eof() {
    let mut output = Vec::new();
    let result = tokio::time::timeout(
        Duration::from_secs(1),
        abc().copy_to(&NonterminatingSuffix, &mut output),
    )
    .await
    .unwrap();
    assert!(result.is_err());
    assert!(output.len() <= 3);
}

#[tokio::test]
async fn file_verification_uses_the_same_complete_resource_contract() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("body");
    tokio::fs::write(&path, b"abc").await.unwrap();
    abc().verify_file(&path).await.unwrap();
    tokio::fs::write(&path, b"abc\0").await.unwrap();
    assert!(abc().verify_file(&path).await.is_err());
}
