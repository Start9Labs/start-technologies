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
use crate::s9pk::merkle_archive::source::ArchiveSource;
use crate::s9pk::merkle_archive::source::http::HttpSource;
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

pub(super) async fn context(datadir: PathBuf, uploader: AnyVerifyingKey) -> RegistryContext {
    tokio::fs::create_dir_all(&datadir).await.unwrap();
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
        hostnames: vec!["localhost".into(), "127.0.0.1".into()],
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

async fn local_package() -> S9pk {
    let file = tempfile::tempfile().unwrap();
    let mut file = tokio::fs::File::from_std(file);
    use tokio::io::AsyncWriteExt;
    file.write_all(&fixture("endorsed.s9pk")).await.unwrap();
    S9pk::from_file(file).await.unwrap()
}

async fn registry_server(
    ctx: RegistryContext,
) -> (Url, crate::util::future::NonDetachingJoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, crate::registry::registry_router(ctx))
            .await
            .unwrap();
    });
    (url, task.into())
}

#[tokio::test]
async fn registry_cli_transports_local_endorsements_to_stripped_host_and_preserves_mirror_and_promotion()
 {
    use crate::context::config::{ClientConfig, Profiles};
    use crate::developer::write_signing_key;
    use crate::registry::package::add::cli_add_package_impl;
    use crate::registry::package::promote::{CliPromoteParams, cli_promote};

    let uploader_key = SigningKey::from_bytes(&[12; 32]);
    let uploader = AnyVerifyingKey::Ed25519(uploader_key.verifying_key());
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path().join("first"), uploader.clone()).await;
    let before = ctx.db.peek().await.as_index().de().unwrap();
    let (host, requests, host_task) = server(
        [
            ("base".into(), fixture("base.s9pk")),
            ("mirror".into(), fixture("base.s9pk")),
        ]
        .into(),
    )
    .await;
    let (registry, registry_task) = registry_server(ctx.clone()).await;
    let target = context(temp.path().join("second"), uploader.clone()).await;
    let (destination, destination_task) = registry_server(target.clone()).await;
    let key_path = temp.path().join("identity.pem");
    write_signing_key(&uploader_key, &key_path).await.unwrap();
    let cli = CliContext::init(ClientConfig {
        id_key_path: Some(key_path.clone()),
        registry: Some(Profiles(BTreeMap::from([
            ("default".into(), registry.to_string()),
            ("source".into(), registry.to_string()),
            ("destination".into(), destination.to_string()),
        ]))),
        ..Default::default()
    })
    .unwrap();
    let torrent: Url = "magnet:?xt=urn:btih:0123456789012345678901234567890123456789"
        .parse()
        .unwrap();
    cli_add_package_impl(
        cli.clone(),
        local_package().await,
        vec![host.join("base").unwrap()],
        Some(torrent.clone()),
        false,
    )
    .await
    .unwrap();
    let after = ctx.db.peek().await.as_index().de().unwrap();
    assert_eq!(before.signers, after.signers);
    assert_eq!(before.trust_signers, after.trust_signers);
    let version: crate::util::VersionString = "1.0.0:0".parse().unwrap();
    let asset = &after.package.packages["trust-fixture"].versions[&version].s9pks[0].1;
    assert_eq!(asset.signatures.len(), 5);
    assert_eq!(asset.torrent, Some(torrent.clone()));
    for (key, signature) in &asset.signatures {
        key.verify_commitment(&asset.commitment, "s9pk", signature)
            .unwrap();
    }
    assert!(requests.lock().unwrap().iter().all(Option::is_none));

    let local_path = temp.path().join("local.s9pk");
    tokio::fs::write(&local_path, fixture("endorsed.s9pk"))
        .await
        .unwrap();
    let cli_server = rpc_toolkit::Server::new(
        {
            let cli = cli.clone();
            move || {
                let cli = cli.clone();
                async move { Ok(cli) }
            }
        },
        crate::registry::registry_api(),
    );
    cli_server.handle_command("package.add-mirror", imbl_value::json!({"file":local_path,"url":host.join("mirror").unwrap(),"noVerify":true})).await.unwrap();
    let mirrored = ctx.db.peek().await.as_index().de().unwrap();
    let mirrored = &mirrored.package.packages["trust-fixture"].versions[&version].s9pks[0].1;
    assert_eq!(mirrored.signatures, asset.signatures);
    assert_eq!(mirrored.urls.len(), 2);
    assert_eq!(mirrored.torrent, Some(torrent.clone()));

    cli_promote(
        cli.clone(),
        CliPromoteParams {
            from: Some("source".into()),
            to: Some("destination".into()),
            id: "trust-fixture".parse().unwrap(),
            version: version.clone(),
        },
    )
    .await
    .unwrap();
    let promoted = target.db.peek().await.as_index().de().unwrap();
    let promoted = &promoted.package.packages["trust-fixture"].versions[&version].s9pks[0].1;
    assert_eq!(promoted.signatures, asset.signatures);
    assert_eq!(promoted.urls, mirrored.urls);
    assert_eq!(promoted.torrent, Some(torrent));

    ctx.db
        .mutate(|db| db.as_admins_mut().ser(&BTreeSet::<Guid>::new()))
        .await
        .result
        .unwrap();
    assert!(
        cli_add_package_impl(
            cli,
            local_package().await,
            vec![host.join("base").unwrap()],
            None,
            true
        )
        .await
        .is_err()
    );
    assert_eq!(
        ctx.db.peek().await.as_index().de().unwrap().signers,
        before.signers
    );
    host_task.abort();
    registry_task.abort();
    destination_task.abort();
}

#[tokio::test]
async fn registry_rejects_submitted_invalid_signatures_atomically_and_defaults_old_requests() {
    let uploader_key = SigningKey::from_bytes(&[12; 32]);
    let uploader = AnyVerifyingKey::Ed25519(uploader_key.verifying_key());
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path().into(), uploader.clone()).await;
    let local = local_package().await;
    let commitment = local.as_archive().commitment().await.unwrap();
    let signature = AnySignature::Ed25519ph(
        Ed25519ph
            .sign_commitment(&uploader_key, &commitment, "s9pk")
            .unwrap(),
    );
    let (url, requests, task) = server([("base".into(), fixture("base.s9pk"))].into()).await;
    let old = serde_json::json!({"urls":[url.join("base").unwrap()],"__Auth_signer":uploader,"commitment":commitment,"signature":signature});
    let params: AddPackageParams = serde_json::from_value(old.clone()).unwrap();
    assert!(params.signatures.is_empty());
    let mut supplied = local.signatures().await.unwrap();
    let good_key = supplied.keys().next().unwrap().clone();
    supplied.insert(
        good_key,
        AnySignature::Ed25519ph(ed25519_dalek::Signature::from_bytes(&[0; 64])),
    );
    let mut invalid = old.clone();
    invalid["signatures"] = serde_json::to_value(supplied).unwrap();
    assert!(
        add_package(ctx.clone(), serde_json::from_value(invalid).unwrap())
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
    add_package(ctx.clone(), params).await.unwrap();
    let before = serde_json::to_value(ctx.db.peek().await.as_index().de().unwrap()).unwrap();
    let mut invalid = serde_json::json!({"url":url.join("base").unwrap(),"__Auth_signer":uploader,"commitment":commitment,"signature":signature,"signatures":local.signatures().await.unwrap()});
    let mirror: AddMirrorParams = serde_json::from_value(invalid.clone()).unwrap();
    add_mirror(ctx.clone(), mirror).await.unwrap();
    invalid["signatures"] = serde_json::json!({uploader.to_string():AnySignature::Ed25519ph(ed25519_dalek::Signature::from_bytes(&[0;64]))});
    let after = serde_json::to_value(ctx.db.peek().await.as_index().de().unwrap()).unwrap();
    assert!(
        add_mirror(ctx.clone(), serde_json::from_value(invalid).unwrap())
            .await
            .is_err()
    );
    assert_eq!(
        serde_json::to_value(ctx.db.peek().await.as_index().de().unwrap()).unwrap(),
        after
    );
    assert_ne!(before, after);
    assert!(requests.lock().unwrap().iter().all(Option::is_none));
    task.abort();
}

#[tokio::test]
async fn registry_rejects_endorsements_signed_for_a_different_actual_bound() {
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
            ("base".into(), fixture("base.s9pk")),
        ]
        .into(),
    )
    .await;
    for name in ["larger", "base"] {
        assert!(
            add_package(
                ctx.clone(),
                AddPackageParams {
                    urls: vec![url.join(name).unwrap()],
                    torrent: None,
                    uploader: uploader.clone(),
                    commitment,
                    signature: signature.clone(),
                    signatures: local_package().await.signatures().await.unwrap()
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
    }
    task.abort();
}

#[tokio::test]
async fn registry_validates_every_package_url_without_importing_remote_endorsements() {
    let uploader_key = SigningKey::from_bytes(&[12; 32]);
    let uploader = AnyVerifyingKey::Ed25519(uploader_key.verifying_key());
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path().into(), uploader.clone()).await;
    let commitment = local_package()
        .await
        .as_archive()
        .commitment()
        .await
        .unwrap();
    let signature = AnySignature::Ed25519ph(
        Ed25519ph
            .sign_commitment(&uploader_key, &commitment, "s9pk")
            .unwrap(),
    );
    let (wrong, _) = larger_bound_fixture();
    let (url, requests, task) = server(
        [
            ("base".into(), fixture("base.s9pk")),
            ("wrong-bound".into(), wrong),
            ("wrong-root".into(), fixture("overlap.s9pk")),
            ("truncated".into(), fixture("base.s9pk")[..100].to_vec()),
            ("endorsed".into(), fixture("endorsed.s9pk")),
        ]
        .into(),
    )
    .await;
    for secondary in ["wrong-bound", "wrong-root", "truncated"] {
        assert!(
            add_package(
                ctx.clone(),
                AddPackageParams {
                    urls: vec![url.join("base").unwrap(), url.join(secondary).unwrap()],
                    torrent: None,
                    uploader: uploader.clone(),
                    commitment,
                    signature: signature.clone(),
                    signatures: Default::default(),
                }
            )
            .await
            .is_err(),
            "{secondary}"
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
    }
    let urls = vec![url.join("base").unwrap(), url.join("endorsed").unwrap()];
    add_package(
        ctx.clone(),
        AddPackageParams {
            urls: urls.clone(),
            torrent: None,
            uploader,
            commitment,
            signature,
            signatures: Default::default(),
        },
    )
    .await
    .unwrap();
    let index = ctx.db.peek().await.as_index().de().unwrap();
    let version: crate::util::VersionString = "1.0.0:0".parse().unwrap();
    let asset = &index.package.packages["trust-fixture"].versions[&version].s9pks[0].1;
    assert_eq!(asset.urls, urls);
    assert_eq!(asset.signatures.len(), 2);
    assert!(requests.lock().unwrap().iter().all(Option::is_none));
    task.abort();
}

#[tokio::test]
async fn http_metadata_never_discovers_endorsements_and_reuses_exact_position() {
    use tokio::io::AsyncReadExt;
    let bytes = fixture("endorsed.s9pk");
    let (url, requests, task) = server([("endorsed".into(), bytes.clone())].into()).await;
    let source = Arc::new(
        HttpSource::new(Client::new(), url.join("endorsed").unwrap())
            .await
            .unwrap(),
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
    let package = S9pk::deserialize(&source, None).await.unwrap();
    assert_eq!(package.signatures().await.unwrap().len(), 1);
    assert!(package.diagnostics().is_empty());
    assert!(requests.lock().unwrap().iter().all(Option::is_none));
    assert!(package.as_archive().contents().source_end() < bytes.len() as u64);
    task.abort();
}
