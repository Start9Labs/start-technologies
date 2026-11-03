use std::sync::Arc;

use ed25519_dalek::{Signer as _, SigningKey};
use serde_json::{Value, json};

use super::*;
use crate::s9pk::{S9pk, load_with_signers};
use crate::sign::ed25519::Ed25519ph;
use crate::sign::{SignatureScheme, append_commitment, signing_message};

fn fixtures() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/s9pk-trailer")
}
fn bytes(name: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(name)).unwrap()
}
fn vectors() -> Value {
    serde_json::from_slice(&bytes("vectors.json")).unwrap()
}
fn tlv(kind: u16, value: &[u8]) -> Vec<u8> {
    let mut bytes = kind.to_be_bytes().to_vec();
    bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
    bytes.extend_from_slice(value);
    bytes
}
fn with_region(mut base: Vec<u8>, region: &[u8]) -> Vec<u8> {
    base.extend_from_slice(region);
    base.extend_from_slice(&(region.len() as u64).to_be_bytes());
    base.extend_from_slice(MAGIC);
    base
}
async fn read(bytes: Vec<u8>) -> S9pk<crate::s9pk::merkle_archive::source::Section<Arc<[u8]>>> {
    S9pk::deserialize(&Arc::<[u8]>::from(bytes), None)
        .await
        .unwrap()
}
fn good() -> Vec<u8> {
    tlv(
        1,
        &serde_json::to_vec(&vectors()["endorsements"][0]).unwrap(),
    )
}
fn key(index: usize) -> AnyVerifyingKey {
    serde_json::from_value(vectors()["endorsements"][index]["signer"].clone()).unwrap()
}

#[tokio::test]
#[ignore = "Regenerates the shared, frozen native/TypeScript fixtures"]
async fn regenerate_s9pk_trailer_fixtures() {
    let dir = fixtures();
    std::fs::create_dir_all(&dir).unwrap();
    let base = crate::s9pk::trust_tests::fixture_bytes(&[42; 128]).await;
    let package = read(base.clone()).await;
    let commitment = package.as_archive().commitment().await.unwrap();
    let mut endorsements = Vec::new();
    for (seed, mode) in [(8, "ph"), (9, "pure"), (10, "legacy")] {
        let key = SigningKey::from_bytes(&[seed; 32]);
        let signature = match mode {
            "pure" => AnySignature::Ed25519(key.sign(
                &signing_message(crate::sign::COMMITMENT_SIG_TAG, &commitment, "s9pk").unwrap(),
            )),
            "ph" => AnySignature::Ed25519ph(
                Ed25519ph
                    .sign_commitment(&key, &commitment, "s9pk")
                    .unwrap(),
            ),
            _ => AnySignature::Ed25519(
                Ed25519ph
                    .sign_commitment(&key, &commitment, "s9pk")
                    .unwrap(),
            ),
        };
        endorsements.push(json!({"signer": AnyVerifyingKey::Ed25519(key.verifying_key()), "signature": signature}));
    }
    let mut raw = tlv(65535, b"unknown\0record");
    raw.extend(tlv(1, br#"{"signer":"bad","signature":"bad"}"#));
    for record in &endorsements {
        raw.extend(tlv(1, &serde_json::to_vec(record).unwrap()));
    }
    let alias = json!({"signer": key_from_record(&endorsements[0]).to_string(), "signature": endorsements[0]["signature"]});
    raw.extend(tlv(1, &serde_json::to_vec(&alias).unwrap()));
    let mut extended = serde_json::to_string(&endorsements[1]).unwrap();
    extended.pop();
    extended.push_str(",\"unknown\":{\"large\":18446744073709551615,\"a\":[1,2]}}");
    raw.extend(tlv(1, extended.as_bytes()));
    let endorsed = endorsements
        .iter()
        .flat_map(|r| tlv(1, &serde_json::to_vec(r).unwrap()))
        .collect::<Vec<_>>();
    std::fs::write(
        dir.join("endorsed.s9pk"),
        with_region(base.clone(), &endorsed),
    )
    .unwrap();
    std::fs::write(dir.join("base.s9pk"), &base).unwrap();
    std::fs::write(dir.join("mixed.s9pk"), with_region(base.clone(), &raw)).unwrap();
    let mut message = Vec::new();
    append_commitment(&mut message, &commitment);
    let mut dormant = vec![42; 128];
    dormant[112..120].copy_from_slice(&0_u64.to_be_bytes());
    dormant[120..].copy_from_slice(MAGIC);
    let overlap = crate::s9pk::trust_tests::fixture_bytes(&dormant).await;
    std::fs::write(dir.join("overlap.s9pk"), &overlap).unwrap();
    let overlap_commitment = read(overlap).await.as_archive().commitment().await.unwrap();
    let large_commitment = MerkleArchiveCommitment {
        root_maxsize: 9007199254740993,
        ..commitment
    };
    let large_signature = AnySignature::Ed25519ph(
        Ed25519ph
            .sign_commitment(&SigningKey::from_bytes(&[8; 32]), &large_commitment, "s9pk")
            .unwrap(),
    );
    let invalid_point = AnyVerifyingKey::Ed25519(
        ed25519_dalek::VerifyingKey::from_bytes(&[
            1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0,
        ])
        .unwrap(),
    );
    let mut small_order_r = [0; 64];
    small_order_r[0] = 1;
    let invalid_signature =
        AnySignature::Ed25519ph(ed25519_dalek::Signature::from_bytes(&small_order_r));
    let manifest = json!({
        "overlapCommitment": overlap_commitment,
        "smallR": adversarial_ph(&SigningKey::from_bytes(&[8;32]), &commitment, true),
        "torsionR": adversarial_ph(&SigningKey::from_bytes(&[8;32]), &commitment, false),
        "largeBound": { "bound": "9007199254740993", "signature": large_signature },
        "invalidPoint": { "signer": invalid_point, "signature": invalid_signature },
        "commitment": commitment,
        "messageHex": hex::encode(message),
        "taggedMessageHex": hex::encode(signing_message(crate::sign::COMMITMENT_SIG_TAG, &commitment, "s9pk").unwrap()),
        "embedded": AnyVerifyingKey::Ed25519(package.as_archive().signer()),
        "endorsements": endorsements,
    });
    std::fs::write(
        dir.join("vectors.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
}
fn key_from_record(record: &Value) -> AnyVerifyingKey {
    serde_json::from_value(record["signer"].clone()).unwrap()
}

#[tokio::test]
async fn s9pk_trailer_verified_map_and_trust_policy() {
    let package = read(bytes("mixed.s9pk")).await;
    let signatures = package.signatures().await.unwrap();
    assert_eq!(signatures.len(), 4);
    assert_eq!(
        package
            .diagnostics()
            .iter()
            .map(|d| d.kind)
            .collect::<Vec<_>>(),
        ["invalid-signature"]
    );
    assert_eq!(
        serde_json::to_value(package.as_archive().commitment().await.unwrap()).unwrap(),
        vectors()["commitment"]
    );
    for index in 0..3 {
        let signature = &signatures[&key(index)];
        assert_eq!(matches!(signature, AnySignature::Ed25519(_)), index == 1);
        key(index)
            .verify_commitment(
                &package.as_archive().commitment().await.unwrap(),
                "s9pk",
                signature,
            )
            .unwrap();
    }
    let source = Arc::<[u8]>::from(bytes("mixed.s9pk"));
    for policy in [
        None,
        Some(vec![key(0)]),
        Some(vec![key(1)]),
        Some(vec![key(2)]),
    ] {
        load_with_signers(source.clone(), || panic!(), None, policy.as_deref())
            .await
            .unwrap();
    }
    for policy in [
        vec![],
        vec![AnyVerifyingKey::Ed25519(
            SigningKey::from_bytes(&[1; 32]).verifying_key(),
        )],
    ] {
        assert!(
            load_with_signers(source.clone(), || panic!(), None, Some(&policy))
                .await
                .is_err()
        );
    }
    let mut corrupt = source.to_vec();
    corrupt[35] ^= 1;
    assert!(
        S9pk::deserialize(&Arc::<[u8]>::from(corrupt), None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn s9pk_trailer_bad_records_are_isolated_but_bad_framing_discards_all() {
    let malformed = [
        br#"{"signer":"x","sign\u0065r":"y","signature":"z"}"#.to_vec(),
        br#"{"signer":"x","signature":"y","unknown":{"a":1,"\u0061":2}}"#.to_vec(),
        br#"{"signer":1,"signature":"y"}"#.to_vec(),
        b"[]".to_vec(),
        b"null".to_vec(),
        vec![255],
        format!("{}0{}", "[".repeat(70), "]".repeat(70)).into_bytes(),
    ];
    for record in malformed {
        let mut region = tlv(1, &record);
        region.extend(good());
        let package = read(with_region(bytes("base.s9pk"), &region)).await;
        assert_eq!(package.signatures().await.unwrap().len(), 2);
        assert_eq!(package.diagnostics()[0].kind, "invalid-record");
        assert!(!package.trailer.editable);
    }
    for tail in [
        vec![0],
        [1_u16.to_be_bytes().as_slice(), &u64::MAX.to_be_bytes()].concat(),
    ] {
        let mut region = good();
        region.extend(tail);
        let package = read(with_region(bytes("base.s9pk"), &region)).await;
        assert_eq!(package.signatures().await.unwrap().len(), 1);
        assert_eq!(package.diagnostics()[0].kind, "invalid-framing");
    }
    let mut huge = bytes("base.s9pk");
    huge.extend_from_slice(&u64::MAX.to_be_bytes());
    huge.extend_from_slice(MAGIC);
    assert_eq!(read(huge).await.diagnostics()[0].kind, "invalid-framing");
    for length in [MAX_REGION - 10, MAX_REGION - 9] {
        let package = read(with_region(
            bytes("base.s9pk"),
            &tlv(55, &vec![0; length as usize]),
        ))
        .await;
        assert_eq!(package.diagnostics().is_empty(), length == MAX_REGION - 10);
    }
    let mut unsupported = with_region(bytes("base.s9pk"), &good());
    *unsupported.last_mut().unwrap() = b'2';
    assert_eq!(
        read(unsupported).await.diagnostics()[0].kind,
        "unsupported-version"
    );
    assert_eq!(
        read(bytes("overlap.s9pk")).await.diagnostics()[0].kind,
        "invalid-framing"
    );
}

#[tokio::test]
async fn s9pk_trailer_rejects_noncanonical_key_encodings() {
    let signer = "MCowBQYDK2VwAyEAE5j2LG0aRXxRumpLXz29L2n8qTIWIY3ImX5Ba9F9k8o=";
    for invalid in [
        signer.replace("k8o=", "k8p="),
        format!("{signer} "),
        format!("-----BEGIN PUBLIC KEY-----\n{signer}\n-----END OTHER KEY-----\n"),
    ] {
        assert!(invalid.parse::<AnyVerifyingKey>().is_err());
        let record =
            json!({"signer": invalid, "signature": vectors()["endorsements"][0]["signature"]});
        let package = read(with_region(
            bytes("base.s9pk"),
            &tlv(1, &serde_json::to_vec(&record).unwrap()),
        ))
        .await;
        assert_eq!(package.signatures().await.unwrap().len(), 1);
        assert_eq!(package.diagnostics()[0].kind, "invalid-signature");
    }
}

#[tokio::test]
async fn s9pk_trailer_toc_ranges_and_lazy_files_are_accounted() {
    let mut base = bytes("base.s9pk");
    let position = u64::from_be_bytes(base[139..147].try_into().unwrap()) as usize;
    let size = u64::from_be_bytes(base[147..155].try_into().unwrap()) as usize;
    let toc = base[position..position + size].to_vec();
    let start = base.len();
    let mut region = tlv(55, &toc);
    region.extend(good());
    base[139..147].copy_from_slice(&((start + 10) as u64).to_be_bytes());
    let package = read(with_region(base, &region)).await;
    assert_eq!(package.diagnostics()[0].kind, "invalid-framing");
    assert_eq!(package.signatures().await.unwrap().len(), 1);
    let mut bad = bytes("base.s9pk");
    bad[139..147].copy_from_slice(&u64::MAX.to_be_bytes());
    assert!(
        S9pk::deserialize(&Arc::<[u8]>::from(bad), None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn s9pk_trailer_writer_preserves_raw_base_and_unknowns_and_refuses_damage() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("test.s9pk");
    let record = &vectors()["endorsements"][0];
    let signature: AnySignature = serde_json::from_value(record["signature"].clone()).unwrap();
    let original = bytes("mixed.s9pk");
    tokio::fs::write(&path, &original).await.unwrap();
    for _ in 0..2 {
        Trailer::append(&path, key(0), signature.clone())
            .await
            .unwrap();
    }
    let updated = tokio::fs::read(&path).await.unwrap();
    assert_eq!(
        &updated[..original.len() - 16],
        &original[..original.len() - 16]
    );
    let package = S9pk::open(&path, None).await.unwrap();
    assert_eq!(package.trailer.start, Some(bytes("base.s9pk").len() as u64));
    assert_eq!(package.signatures().await.unwrap().len(), 4);
    for mut bad in [
        bytes("overlap.s9pk"),
        with_region(bytes("base.s9pk"), &tlv(1, b"{}")),
        with_region(bytes("base.s9pk"), &[0]),
    ] {
        for version in [b'1', b'2'] {
            *bad.last_mut().unwrap() = version;
            tokio::fs::write(&path, &bad).await.unwrap();
            assert!(
                Trailer::append(&path, key(0), signature.clone())
                    .await
                    .is_err()
            );
            assert_eq!(tokio::fs::read(&path).await.unwrap(), bad);
        }
    }
    tokio::fs::write(&path, bytes("base.s9pk")).await.unwrap();
    let invalid = AnySignature::Ed25519ph(ed25519_dalek::Signature::from_bytes(&[0; 64]));
    assert!(Trailer::append(&path, key(0), invalid).await.is_err());
    assert_eq!(tokio::fs::read(&path).await.unwrap(), bytes("base.s9pk"));
}

#[tokio::test]
async fn s9pk_trailer_content_rewrite_drops_old_metadata_diagnostically() {
    let mut package = read(bytes("mixed.s9pk")).await;
    package.as_manifest_mut().metadata.title = "Changed".into();
    package
        .as_archive_mut()
        .set_signer(SigningKey::from_bytes(&[7; 32]), "s9pk");
    let mut sink = std::io::Cursor::new(Vec::new());
    package.serialize(&mut sink, true).await.unwrap();
    assert_eq!(package.diagnostics()[0].kind, "discarded");
    let result = read(sink.into_inner()).await;
    assert_eq!(result.signatures().await.unwrap().len(), 1);
    assert!(result.diagnostics().is_empty());
}

#[test]
fn s9pk_trailer_strict_json_rejects_duplicate_keys_and_deep_input() {
    for bad in [
        br#"{"a":1,"a":2}"#.as_slice(),
        br#"{"a":{"x":1,"\u0078":2}}"#,
        b"{} 1",
    ] {
        assert!(strict_json(bad).is_err());
    }
    assert!(strict_json(br#"{"a":[{"x":1}],"big":18446744073709551615}"#).is_ok());
}

#[tokio::test]
async fn s9pk_trailer_commitment_bounds_and_embedded_collision_remain_exact() {
    let package = read(bytes("base.s9pk")).await;
    let commitment = package.as_archive().commitment().await.unwrap();
    assert_eq!(
        commitment.root_sighash.to_string(),
        "CEhWFKyWwTpG30snr5ZcatODIjlEDUuTrfA0uV0ZJ1g"
    );
    assert_eq!(commitment.root_maxsize, 380);
    let large_signature: AnySignature =
        serde_json::from_value(vectors()["largeBound"]["signature"].clone()).unwrap();
    let large = MerkleArchiveCommitment {
        root_maxsize: 9007199254740993,
        ..commitment
    };
    key(0)
        .verify_commitment(&large, "s9pk", &large_signature)
        .unwrap();
    assert!(
        key(0)
            .verify_commitment(
                &MerkleArchiveCommitment {
                    root_maxsize: 9007199254740992,
                    ..large
                },
                "s9pk",
                &large_signature
            )
            .is_err()
    );
    let invalid = &vectors()["invalidPoint"];
    let invalid_key: AnyVerifyingKey = serde_json::from_value(invalid["signer"].clone()).unwrap();
    let invalid_signature: AnySignature =
        serde_json::from_value(invalid["signature"].clone()).unwrap();
    assert!(
        crate::sign::compat::verify_commitment(
            &invalid_key,
            &commitment,
            "s9pk",
            &invalid_signature
        )
        .is_err()
    );
    let embedded = SigningKey::from_bytes(&[7; 32]);
    let pure = AnySignature::Ed25519(
        embedded
            .sign(&signing_message(crate::sign::COMMITMENT_SIG_TAG, &commitment, "s9pk").unwrap()),
    );
    let region = tlv(1, &serde_json::to_vec(&json!({"signer": AnyVerifyingKey::Ed25519(embedded.verifying_key()), "signature": pure})).unwrap());
    let package = read(with_region(bytes("base.s9pk"), &region)).await;
    assert_eq!(package.signatures().await.unwrap().len(), 1);
    assert!(matches!(
        package.signatures().await.unwrap().values().next().unwrap(),
        AnySignature::Ed25519ph(_)
    ));
}

#[derive(Clone)]
struct UnknownSize(Arc<[u8]>);
impl ArchiveSource for UnknownSize {
    type FetchReader = <Arc<[u8]> as ArchiveSource>::FetchReader;
    type FetchAllReader = <Arc<[u8]> as ArchiveSource>::FetchAllReader;
    async fn fetch(&self, position: u64, size: u64) -> Result<Self::FetchReader, Error> {
        self.0.fetch(position, size).await
    }
    async fn fetch_all(&self) -> Result<Self::FetchAllReader, Error> {
        self.0.fetch_all().await
    }
}

#[tokio::test]
async fn s9pk_trailer_unknown_length_is_not_reported_as_absent() {
    let package = S9pk::deserialize(&UnknownSize(Arc::<[u8]>::from(bytes("mixed.s9pk"))), None)
        .await
        .unwrap();
    assert_eq!(package.diagnostics()[0].kind, "unknown-size");
    assert_eq!(package.signatures().await.unwrap().len(), 1);
}

#[tokio::test]
async fn s9pk_trailer_lazy_file_overflow_and_writer_cap_are_enforced() {
    let mut base = bytes("base.s9pk");
    let name = b"dormant.squashfs";
    let position =
        base.windows(name.len()).position(|w| w == name).unwrap() + name.len() + 32 + 8 + 1;
    for offset in [u64::MAX, base.len() as u64] {
        base[position..position + 8].copy_from_slice(&offset.to_be_bytes());
        assert!(
            S9pk::deserialize(&Arc::<[u8]>::from(base.clone()), None)
                .await
                .is_err()
        );
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("full.s9pk");
    let full = with_region(
        bytes("base.s9pk"),
        &tlv(90, &vec![0; MAX_REGION as usize - 10]),
    );
    tokio::fs::write(&path, &full).await.unwrap();
    let signature =
        serde_json::from_value(vectors()["endorsements"][0]["signature"].clone()).unwrap();
    assert!(Trailer::append(&path, key(0), signature).await.is_err());
    assert_eq!(tokio::fs::read(&path).await.unwrap(), full);
}

fn adversarial_ph(
    key: &SigningKey,
    commitment: &MerkleArchiveCommitment,
    small_r: bool,
) -> AnySignature {
    use sha2::{Digest, Sha512};

    let a = key.to_scalar();
    let one = a * a.invert();
    let zero = a - a;
    let nonce = SigningKey::from_bytes(&[15; 32]);
    let r = if small_r { zero } else { nonce.to_scalar() };
    let torsion = ed25519_dalek::VerifyingKey::from_bytes(&[0; 32])
        .unwrap()
        .to_edwards();
    let point = if small_r {
        torsion - torsion
    } else {
        nonce.verifying_key().to_edwards() + torsion
    };
    let encoded_r = point.compress().to_bytes();
    let mut message = Vec::new();
    append_commitment(&mut message, commitment);
    let challenge = Sha512::new()
        .chain_update(b"SigEd25519 no Ed25519 collisions")
        .chain_update([1, 4])
        .chain_update(b"s9pk")
        .chain_update(encoded_r)
        .chain_update(key.verifying_key().as_bytes())
        .chain_update(Sha512::digest(message))
        .finalize();
    let mut k = zero;
    for byte in challenge.iter().rev() {
        for bit in (0..8).rev() {
            k = k + k;
            if byte & (1 << bit) != 0 {
                k += one;
            }
        }
    }
    let s = r + k * a;
    let mut signature = [0; 64];
    signature[..32].copy_from_slice(&encoded_r);
    signature[32..].copy_from_slice(&s.to_bytes());
    AnySignature::Ed25519ph(ed25519_dalek::Signature::from_bytes(&signature))
}

#[tokio::test]
async fn s9pk_trailer_rejects_signatures_that_only_satisfy_the_cofactored_equation() {
    let commitment = read(bytes("base.s9pk"))
        .await
        .as_archive()
        .commitment()
        .await
        .unwrap();
    for kind in ["smallR", "torsionR"] {
        let signature: AnySignature = serde_json::from_value(vectors()[kind].clone()).unwrap();
        assert!(
            crate::sign::compat::verify_commitment(&key(0), &commitment, "s9pk", &signature)
                .is_err()
        );
    }
}

#[derive(Clone)]
struct UnreadableTail(Arc<[u8]>);
impl ArchiveSource for UnreadableTail {
    type FetchReader = <Arc<[u8]> as ArchiveSource>::FetchReader;
    type FetchAllReader = <Arc<[u8]> as ArchiveSource>::FetchAllReader;
    async fn size(&self) -> Option<u64> {
        Some(self.0.len() as u64)
    }
    async fn fetch(&self, position: u64, size: u64) -> Result<Self::FetchReader, Error> {
        if position == self.0.len() as u64 - 16 {
            return Err(Error::new(
                eyre!("fixture read failure"),
                ErrorKind::Network,
            ));
        }
        self.0.fetch(position, size).await
    }
    async fn fetch_all(&self) -> Result<Self::FetchAllReader, Error> {
        self.0.fetch_all().await
    }
}

#[tokio::test]
async fn s9pk_trailer_read_failure_keeps_valid_base_and_refuses_editing() {
    let package = S9pk::deserialize(
        &UnreadableTail(Arc::<[u8]>::from(bytes("mixed.s9pk"))),
        None,
    )
    .await
    .unwrap();
    assert_eq!(package.diagnostics()[0].kind, "read-failed");
    assert_eq!(package.signatures().await.unwrap().len(), 1);
    assert!(!package.trailer.editable);
    let overlap = read(bytes("overlap.s9pk")).await;
    for path in overlap.as_archive().contents().file_paths("") {
        if let Some(file) = overlap.as_archive().contents().get_path(path) {
            if file.as_file().is_some() {
                file.read_file_to_vec().await.unwrap();
            }
        }
    }
}
