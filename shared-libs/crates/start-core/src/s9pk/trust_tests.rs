use std::io::Cursor;

use ed25519_dalek::SigningKey;

use super::*;
use crate::s9pk::merkle_archive::directory_contents::DirectoryContents;
use crate::s9pk::merkle_archive::{Entry, MerkleArchive};
use crate::s9pk::v2::SIG_CONTEXT;

#[tokio::test]
async fn sideload_enforces_trust_on_verified_archive_signatures() {
    let signer = SigningKey::from_bytes(&[7; 32]);
    let other = SigningKey::from_bytes(&[8; 32]);
    let manifest = serde_json::from_value(serde_json::json!({
        "id": "trust-fixture", "version": "1.0.0:0", "canMigrateTo": "*", "canMigrateFrom": "*",
        "title": "Trust fixture", "description": {"short": "Signer verification fixture", "long": "Signer verification fixture"},
        "releaseNotes": "Fixture", "license": "MIT", "packageRepo": "https://example.com/package", "upstreamRepo": "https://example.com/upstream",
        "osVersion": "0.4.0", "images": {}, "volumes": [], "dependencies": {}, "hardwareRequirements": {"arch": ["x86_64"]}
    })).unwrap();
    let mut contents = DirectoryContents::<Arc<[u8]>>::new();
    for (path, bytes) in [
        ("icon.svg", b"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 32 32\"><circle cx=\"16\" cy=\"16\" r=\"16\" fill=\"green\"/></svg>".as_slice()),
        ("LICENSE.md", b"MIT".as_slice()),
        ("instructions.md", b"Fixture".as_slice()),
        ("javascript.squashfs", b"Fixture".as_slice()),
    ] {
        contents.insert_path(path, Entry::file(Arc::<[u8]>::from(bytes))).unwrap();
    }
    let mut package = S9pk::new_with_manifest(
        MerkleArchive::new(contents, signer.clone(), SIG_CONTEXT),
        None,
        manifest,
    );
    let mut sink = Cursor::new(Vec::new());
    package.serialize(&mut sink, true).await.unwrap();
    let bytes = sink.into_inner();
    if let Some(path) = std::env::var_os("STARTOS_TRUST_FIXTURE") {
        tokio::fs::write(path, &bytes).await.unwrap();
    }
    let source = Arc::<[u8]>::from(bytes.clone());
    for accepted in [
        None,
        Some(vec![AnyVerifyingKey::Ed25519(signer.verifying_key())]),
    ] {
        let loaded = load_with_signers(
            source.clone(),
            || panic!("v2 needs no conversion key"),
            None,
            accepted.as_deref(),
        )
        .await
        .unwrap();
        assert_eq!(loaded.as_archive().signer(), signer.verifying_key());
    }
    for accepted in [
        vec![],
        vec![AnyVerifyingKey::Ed25519(other.verifying_key())],
    ] {
        assert!(
            load_with_signers(
                source.clone(),
                || panic!("v2 needs no conversion key"),
                None,
                Some(&accepted)
            )
            .await
            .is_err()
        );
    }
    let mut corrupted = bytes;
    corrupted[3 + 32] ^= 1;
    assert!(
        load_with_signers(
            Arc::<[u8]>::from(corrupted),
            || panic!("v2 needs no conversion key"),
            None,
            None
        )
        .await
        .is_err()
    );
}
