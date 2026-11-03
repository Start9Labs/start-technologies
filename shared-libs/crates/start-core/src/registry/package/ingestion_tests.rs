use std::collections::BTreeMap;
use std::sync::Mutex;

use axum::body::Body;
use axum::extract::Path as RoutePath;
use axum::response::Response;
use axum::routing::get;
use ed25519_dalek::SigningKey;
use serde_json::Value;

use super::*;
use crate::registry::package::add::{AddMirrorParams, AddPackageParams, add_mirror, add_package};
use crate::rpc_continuations::Guid;
use crate::s9pk::S9pk;
use crate::s9pk::merkle_archive::source::http::HttpSource;
use crate::s9pk::merkle_archive::source::{ArchiveSource, TmpSource};
use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;
use crate::sign::ed25519::Ed25519ph;
use crate::sign::{AnySignature, SignatureScheme};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/s9pk-trailer")
            .join(name),
    )
    .unwrap()
}
fn vectors() -> Value {
    serde_json::from_slice(&fixture("vectors.json")).unwrap()
}

fn larger_bound_fixture() -> (Vec<u8>, MerkleArchiveCommitment) {
    let mut bytes = fixture("base.s9pk");
    let mut commitment: MerkleArchiveCommitment =
        serde_json::from_value(vectors()["commitment"].clone()).unwrap();
    commitment.root_maxsize += 1;
    let signature = Ed25519ph
        .sign_commitment(&SigningKey::from_bytes(&[7; 32]), &commitment, "s9pk")
        .unwrap();
    bytes[35..99].copy_from_slice(&signature.to_bytes());
    bytes[131..139].copy_from_slice(&commitment.root_maxsize.to_be_bytes());
    (bytes, commitment)
}

async fn context(datadir: PathBuf, uploader: AnyVerifyingKey) -> RegistryContext {
    let db =
        TypedPatchDb::load_unchecked(PatchDb::open(datadir.join("registry.db")).await.unwrap());
    let mut database = RegistryDatabase::init();
    let guid = Guid::new();
    database.admins.insert(guid.clone());
    database.index.trust_signers.insert(guid.clone());
    database.index.signers.insert(
        guid,
        SignerInfo {
            name: "Uploader".into(),
            contact: vec![],
            keys: [uploader].into(),
            gpg_exports: Default::default(),
        },
    );
    db.put(&ROOT, &database).await.unwrap();
    RegistryContext(Arc::new(RegistryContextSeed {
        hostnames: vec!["localhost".into()],
        listen: "127.0.0.1:0".parse().unwrap(),
        db,
        datadir,
        startwrt: false,
        rpc_continuations: RpcContinuations::new(),
        open_authed_continuations: OpenAuthedContinuations::new(),
        auth_sig_nonce_cache: SyncMutex::new(Default::default()),
        client: Client::new(),
        shutdown: tokio::sync::broadcast::channel(1).0,
        metrics_db: SyncMutex::new(Connection::open_in_memory().unwrap()),
        security_feed: None,
    }))
}

type Requests = Arc<Mutex<Vec<Option<(usize, usize)>>>>;

async fn server(
    files: BTreeMap<String, Vec<u8>>,
) -> (
    Url,
    Requests,
    crate::util::future::NonDetachingJoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url: Url = format!("http://{}/", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let observed = requests.clone();
    let router = axum::Router::new().route(
        "/{name}",
        get(
            move |RoutePath(name): RoutePath<String>,
                  method: axum::http::Method,
                  headers: axum::http::HeaderMap| {
                let files = files.clone();
                let observed = observed.clone();
                async move {
                    let bytes = &files[&name];
                    let head = method == axum::http::Method::HEAD;
                    let range = headers
                        .get(axum::http::header::RANGE)
                        .and_then(|h| h.to_str().ok())
                        .map(|h| {
                            let (start, end) =
                                h.strip_prefix("bytes=").unwrap().split_once('-').unwrap();
                            (
                                start.parse::<usize>().unwrap(),
                                end.parse::<usize>().unwrap(),
                            )
                        });
                    if !head {
                        observed.lock().unwrap().push(range);
                    }
                    let mut response = Response::builder();
                    if name != "no-ranges" {
                        response = response.header("Accept-Ranges", "bytes");
                    }
                    if let Some((start, end)) = range.filter(|_| name != "ignored") {
                        response = response
                            .status(206)
                            .header(
                                "Content-Range",
                                format!(
                                    "bytes {start}-{end}/{}",
                                    bytes.len() + usize::from(name == "bad-range")
                                ),
                            )
                            .header("Content-Length", end - start + 1);
                        response
                            .body(Body::from(bytes[start..=end].to_vec()))
                            .unwrap()
                    } else {
                        response
                            .header("Content-Length", bytes.len())
                            .body(Body::from(if head { vec![] } else { bytes.clone() }))
                            .unwrap()
                    }
                }
            },
        ),
    );
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (url, requests, task.into())
}

#[tokio::test]
async fn registry_ingests_verified_trailer_signatures_from_secondary_urls_without_trust_changes() {
    let uploader_key = SigningKey::from_bytes(&[12; 32]);
    let uploader = AnyVerifyingKey::Ed25519(uploader_key.verifying_key());
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path().into(), uploader.clone()).await;
    let before = ctx.db.peek().await.as_index().de().unwrap();
    let base = fixture("base.s9pk");
    let base_size = base.len();
    let mixed = fixture("mixed.s9pk");
    let package = S9pk::deserialize(&Arc::<[u8]>::from(base.clone()), None)
        .await
        .unwrap();
    let commitment = package.as_archive().commitment().await.unwrap();
    let signature = AnySignature::Ed25519ph(
        Ed25519ph
            .sign_commitment(&uploader_key, &commitment, "s9pk")
            .unwrap(),
    );
    let (url, requests, task) = server(
        [
            ("base".into(), base),
            ("mixed".into(), mixed.clone()),
            ("ignored".into(), mixed.clone()),
        ]
        .into(),
    )
    .await;
    add_package(
        ctx.clone(),
        AddPackageParams {
            urls: vec![url.join("base").unwrap(), url.join("mixed").unwrap()],
            uploader: uploader.clone(),
            commitment,
            signature: signature.clone(),
        },
    )
    .await
    .unwrap();
    let after = ctx.db.peek().await.as_index().de().unwrap();
    assert_eq!(before.signers, after.signers);
    assert_eq!(before.trust_signers, after.trust_signers);
    let info = &after.package.packages["trust-fixture"].versions
        [&"1.0.0:0".parse::<crate::util::VersionString>().unwrap()];
    let asset = &info.s9pks[0].1;
    assert_eq!(asset.signatures.len(), 5);
    assert_eq!(asset.commitment, commitment);
    for (key, signature) in &asset.signatures {
        key.verify_commitment(&commitment, "s9pk", signature)
            .unwrap();
    }
    assert_eq!(
        reqwest::get(url.join("mixed").unwrap())
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .as_ref(),
        mixed
    );
    let ranged: Vec<_> = requests.lock().unwrap().iter().copied().flatten().collect();
    assert_eq!(
        ranged,
        [
            (base_size - 16, base_size - 1),
            (mixed.len() - 16, mixed.len() - 1),
            (base_size, mixed.len() - 17),
        ]
    );
    add_mirror(
        ctx.clone(),
        AddMirrorParams {
            url: url.join("ignored").unwrap(),
            uploader: uploader.clone(),
            commitment,
            signature: signature.clone(),
        },
    )
    .await
    .unwrap();
    let fallback = S9pk::deserialize(
        &Arc::new(
            HttpSource::new(Client::new(), url.join("ignored").unwrap())
                .await
                .unwrap(),
        ),
        None,
    )
    .await
    .unwrap();
    assert_eq!(fallback.signatures().await.unwrap().len(), 4);
    ctx.db
        .mutate(|db| db.as_admins_mut().ser(&BTreeSet::<Guid>::new()))
        .await
        .result
        .unwrap();
    assert!(
        add_package(
            ctx.clone(),
            AddPackageParams {
                urls: vec![url.join("mixed").unwrap()],
                uploader,
                commitment,
                signature
            }
        )
        .await
        .is_err()
    );
    task.abort();
}

#[tokio::test]
async fn registry_does_not_advertise_secondary_signatures_against_a_different_actual_bound() {
    let uploader_key = SigningKey::from_bytes(&[12; 32]);
    let uploader = AnyVerifyingKey::Ed25519(uploader_key.verifying_key());
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path().into(), uploader.clone()).await;
    let (larger, commitment) = larger_bound_fixture();
    let signature = AnySignature::Ed25519ph(
        Ed25519ph
            .sign_commitment(&uploader_key, &commitment, "s9pk")
            .unwrap(),
    );
    let (url, _, task) = server(
        [
            ("larger".into(), larger),
            ("mixed".into(), fixture("mixed.s9pk")),
        ]
        .into(),
    )
    .await;
    add_package(
        ctx.clone(),
        AddPackageParams {
            urls: vec![url.join("larger").unwrap(), url.join("mixed").unwrap()],
            uploader,
            commitment,
            signature,
        },
    )
    .await
    .unwrap();
    let index = ctx.db.peek().await.as_index().de().unwrap();
    let info = &index.package.packages["trust-fixture"].versions
        [&"1.0.0:0".parse::<crate::util::VersionString>().unwrap()];
    assert_eq!(info.s9pks[0].1.signatures.len(), 2);
    for (key, signature) in &info.s9pks[0].1.signatures {
        key.verify_commitment(&commitment, "s9pk", signature)
            .unwrap();
    }
    task.abort();
}

#[tokio::test]
async fn registry_rejects_mirror_that_exceeds_the_actual_indexed_bound() {
    let uploader_key = SigningKey::from_bytes(&[12; 32]);
    let uploader = AnyVerifyingKey::Ed25519(uploader_key.verifying_key());
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path().into(), uploader.clone()).await;
    let (larger, commitment) = larger_bound_fixture();
    let signature = AnySignature::Ed25519ph(
        Ed25519ph
            .sign_commitment(&uploader_key, &commitment, "s9pk")
            .unwrap(),
    );
    let (url, _, task) = server(
        [
            ("base".into(), fixture("base.s9pk")),
            ("larger".into(), larger),
        ]
        .into(),
    )
    .await;
    assert!(
        add_package(
            ctx.clone(),
            AddPackageParams {
                urls: vec![url.join("base").unwrap(), url.join("larger").unwrap()],
                uploader,
                commitment,
                signature,
            }
        )
        .await
        .is_err()
    );
    assert!(
        ctx.db
            .peek()
            .await
            .as_index()
            .de()
            .unwrap()
            .package
            .packages
            .is_empty()
    );
    task.abort();
}

#[tokio::test]
async fn registry_optional_malformed_trailer_does_not_block_authorized_ingestion() {
    let uploader_key = SigningKey::from_bytes(&[12; 32]);
    let uploader = AnyVerifyingKey::Ed25519(uploader_key.verifying_key());
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path().into(), uploader.clone()).await;
    let commitment = serde_json::from_value(vectors()["overlapCommitment"].clone()).unwrap();
    let signature = AnySignature::Ed25519ph(
        Ed25519ph
            .sign_commitment(&uploader_key, &commitment, "s9pk")
            .unwrap(),
    );
    let (url, _, task) = server([("overlap".into(), fixture("overlap.s9pk"))].into()).await;
    add_package(
        ctx.clone(),
        AddPackageParams {
            urls: vec![url.join("overlap").unwrap()],
            uploader,
            commitment,
            signature,
        },
    )
    .await
    .unwrap();
    let index = ctx.db.peek().await.as_index().de().unwrap();
    let info = &index.package.packages["trust-fixture"].versions
        [&"1.0.0:0".parse::<crate::util::VersionString>().unwrap()];
    assert_eq!(info.s9pks[0].1.signatures.len(), 2);
    task.abort();
}

#[tokio::test]
async fn http_source_preserves_pooled_bulk_reads_and_forwards_tail_capability() {
    use tokio::io::AsyncReadExt;

    let bytes = fixture("endorsed.s9pk");
    let (url, requests, task) = server([("endorsed".into(), bytes.clone())].into()).await;
    let source = TmpSource::new(
        Arc::new(crate::util::io::TmpDir::new().await.unwrap()),
        Arc::new(
            HttpSource::new(Client::new(), url.join("endorsed").unwrap())
                .await
                .unwrap(),
        ),
    );
    for (position, length) in [(0, 8), (8, 8), (80, 4)] {
        let mut read = vec![0; length];
        source
            .fetch(position as u64, length as u64)
            .await
            .unwrap()
            .read_exact(&mut read)
            .await
            .unwrap();
        assert_eq!(read, bytes[position..position + length]);
    }
    assert_eq!(*requests.lock().unwrap(), [None]);
    let mut footer = [0; 16];
    source
        .fetch_tail(bytes.len() as u64 - 16, 16)
        .await
        .unwrap()
        .read_exact(&mut footer)
        .await
        .unwrap();
    assert_eq!(&footer[8..], b"S9PKEXT1");
    let mut empty = Vec::new();
    source
        .fetch_tail(0, 0)
        .await
        .unwrap()
        .read_to_end(&mut empty)
        .await
        .unwrap();
    assert!(empty.is_empty());
    assert_eq!(
        *requests.lock().unwrap(),
        [None, Some((bytes.len() - 16, bytes.len() - 1))]
    );
    source.gc().await.unwrap();
    task.abort();
}

#[tokio::test]
async fn http_source_tail_falls_back_without_ranges_or_when_they_are_ignored() {
    for name in ["no-ranges", "ignored"] {
        let (url, requests, task) = server([(name.into(), fixture("endorsed.s9pk"))].into()).await;
        let source = Arc::new(
            HttpSource::new(Client::new(), url.join(name).unwrap())
                .await
                .unwrap(),
        );
        let package = S9pk::deserialize(&source, None).await.unwrap();
        assert_eq!(package.signatures().await.unwrap().len(), 4);
        assert!(package.diagnostics().is_empty());
        let ranged = requests
            .lock()
            .unwrap()
            .iter()
            .filter(|range| range.is_some())
            .count();
        assert_eq!(ranged, if name == "no-ranges" { 0 } else { 2 });
        task.abort();
    }
}

#[tokio::test]
async fn http_source_rejects_invalid_tail_range_response_without_rejecting_base() {
    let (url, _, task) = server([("bad-range".into(), fixture("endorsed.s9pk"))].into()).await;
    let source = Arc::new(
        HttpSource::new(Client::new(), url.join("bad-range").unwrap())
            .await
            .unwrap(),
    );
    let package = S9pk::deserialize(&source, None).await.unwrap();
    assert_eq!(package.signatures().await.unwrap().len(), 1);
    assert_eq!(package.diagnostics()[0].kind, "read-failed");
    task.abort();
}

#[tokio::test]
async fn http_source_discovers_the_footer_with_a_direct_range_request() {
    let fixture = fixture("mixed.s9pk");
    let (url, requests, task) = server([("mixed".into(), fixture.clone())].into()).await;
    let source = HttpSource::new(Client::new(), url.join("mixed").unwrap())
        .await
        .unwrap();
    let mut footer = [0; 16];
    use tokio::io::AsyncReadExt;
    source
        .fetch_tail(fixture.len() as u64 - 16, 16)
        .await
        .unwrap()
        .read_exact(&mut footer)
        .await
        .unwrap();
    assert_eq!(&footer[8..], b"S9PKEXT1");
    assert_eq!(
        *requests.lock().unwrap(),
        [Some((fixture.len() - 16, fixture.len() - 1))]
    );
    task.abort();
}
