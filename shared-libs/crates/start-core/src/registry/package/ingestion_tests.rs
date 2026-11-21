use std::collections::BTreeMap;
use std::sync::Mutex;

use axum::body::Body;
use axum::extract::Path as RoutePath;
use axum::response::Response;
use axum::routing::get;
use ed25519_dalek::SigningKey;
use serde_json::Value;

use super::test_utils::{context, registry_server};
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

#[tokio::test]
async fn registry_cli_filters_signatures_on_upload_mirror_and_promotion() {
    use crate::context::config::{ClientConfig, Profiles};
    use crate::developer::write_signing_key;
    use crate::registry::package::add::cli_add_package_impl;
    use crate::registry::package::promote::{CliPromoteParams, cli_promote};

    let uploader_key = SigningKey::from_bytes(&[12; 32]);
    let uploader = AnyVerifyingKey::Ed25519(uploader_key.verifying_key());
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path().join("first"), uploader.clone()).await;
    let target = context(temp.path().join("second"), uploader.clone()).await;
    let embedded: AnyVerifyingKey = serde_json::from_value(vectors()["embedded"].clone()).unwrap();
    let publisher: AnyVerifyingKey =
        serde_json::from_value(vectors()["endorsements"][0]["signer"].clone()).unwrap();
    for (registry, keys) in [
        (&ctx, BTreeSet::from([embedded.clone(), publisher.clone()])),
        (&target, BTreeSet::from([publisher.clone()])),
    ] {
        registry
            .db
            .mutate(|db| {
                db.as_index_mut().as_signers_mut().add_signer(&SignerInfo {
                    name: "Publisher".into(),
                    contact: vec![],
                    keys,
                    gpg_exports: Default::default(),
                })
            })
            .await
            .result
            .unwrap();
    }
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
    cli_add_package_impl(
        cli.clone(),
        local_package().await,
        vec![host.join("base").unwrap()],
        false,
        None,
        Vec::new(),
    )
    .await
    .unwrap();
    let after = ctx.db.peek().await.as_index().de().unwrap();
    assert_eq!(before.signers, after.signers);
    assert_eq!(before.trust_signers, after.trust_signers);
    let version: crate::util::VersionString = "1.0.0:0".parse().unwrap();
    let asset = &after.package.packages["trust-fixture"].versions[&version].s9pks[0].1;
    assert_eq!(
        asset.signatures.keys().collect::<BTreeSet<_>>(),
        [&uploader, &embedded, &publisher].into()
    );
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
    assert_eq!(
        promoted.signatures.keys().collect::<BTreeSet<_>>(),
        [&uploader, &publisher].into()
    );
    for (key, signature) in &promoted.signatures {
        assert_eq!(Some(signature), asset.signatures.get(key));
    }
    assert_eq!(promoted.urls, mirrored.urls);

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
            true,
            None,
            Vec::new(),
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
async fn private_s9pks_reach_only_their_readers_and_a_source_publisher_promotes_them() {
    use crate::context::config::{ClientConfig, Profiles};
    use crate::developer::write_signing_key;
    use crate::registry::package::add::{cli_add_package_impl, derive_keystream};
    use crate::registry::package::promote::{CliPromoteParams, cli_promote};
    use crate::registry::reader::ReaderKey;
    use crate::registry::tests::rpc;
    use crate::s9pk::merkle_archive::source::encrypted::KeyStreamInfo;
    use crate::sign::{AnySigningKey, Signer};

    let identity =
        |seed: u8| Signer::Key(AnySigningKey::Ed25519(SigningKey::from_bytes(&[seed; 32])));
    let uploader_key = SigningKey::from_bytes(&[13; 32]);
    let uploader = AnyVerifyingKey::Ed25519(uploader_key.verifying_key());
    let publisher = identity(13);
    let (reader, stranger, source_admin) = (identity(14), identity(15), identity(20));
    let reader_key = reader.verifying_key().await.unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = context(
        temp.path().join("source"),
        source_admin.verifying_key().await.unwrap(),
    )
    .await;
    let target = context(temp.path().join("target"), uploader.clone()).await;
    source
        .db
        .mutate(|db| {
            db.as_index_mut().as_readers_mut().add_reader(&ReaderKey {
                label: "Reader".into(),
                key: reader_key.clone(),
            })?;
            let signer = db.as_index_mut().as_signers_mut().add_signer(&SignerInfo {
                name: "Publisher".into(),
                contact: vec![],
                keys: [uploader.clone()].into(),
                gpg_exports: Default::default(),
            })?;
            db.as_index_mut().as_package_mut().as_packages_mut().insert(
                &"trust-fixture".parse().unwrap(),
                &crate::registry::package::index::PackageInfo {
                    authorized: [(signer, exver::VersionRange::any())].into(),
                    ..Default::default()
                },
            )
        })
        .await
        .result
        .unwrap();

    let key_path = temp.path().join("identity.pem");
    write_signing_key(&uploader_key, &key_path).await.unwrap();
    let (source_url, source_task) = registry_server(source.clone()).await;
    let (target_url, target_task) = registry_server(target.clone()).await;
    let cli = CliContext::init(ClientConfig {
        id_key_path: Some(key_path),
        registry: Some(Profiles(BTreeMap::from([
            ("default".into(), source_url.to_string()),
            ("destination".into(), target_url.to_string()),
        ]))),
        ..Default::default()
    })
    .unwrap();
    let key = derive_keystream(&cli, &local_package().await)
        .await
        .unwrap();
    assert_eq!(
        key,
        KeyStreamInfo::derive(
            &Signer::Key(AnySigningKey::Ed25519(uploader_key.clone())),
            &local_package()
                .await
                .as_archive()
                .commitment()
                .await
                .unwrap(),
        )
        .await
        .unwrap()
    );
    let plaintext = temp.path().join("base.s9pk");
    tokio::fs::write(&plaintext, fixture("base.s9pk"))
        .await
        .unwrap();
    let mut encrypted = Vec::new();
    crate::s9pk::merkle_archive::source::encrypted::KeyStreamSource::new(
        crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile::from(
            crate::util::io::open_file(&plaintext).await.unwrap(),
        ),
        Some(key.clone()),
    )
    .copy_all_to(&mut encrypted)
    .await
    .unwrap();
    assert_ne!(encrypted, fixture("base.s9pk"));
    let (host, _, host_task) = server([("private".into(), encrypted)].into()).await;
    let hosted = vec![host.join("private").unwrap()];

    assert!(
        cli_add_package_impl(
            cli.clone(),
            local_package().await,
            hosted.clone(),
            true,
            None,
            Vec::new()
        )
        .await
        .is_err()
    );
    cli_add_package_impl(
        cli.clone(),
        local_package().await,
        hosted.clone(),
        false,
        Some(key.clone()),
        vec![reader_key.clone()],
    )
    .await
    .unwrap();

    let version = "1.0.0:0";
    let assert_visibility = |url: Url, identities: Vec<(Option<Signer>, bool, bool)>| {
        let (key, reader_key) = (key.clone(), reader_key.clone());
        async move {
            for (identity, visible, sees_readers) in identities {
                let identity = identity.as_ref();
                let package = rpc(
                    &url,
                    "package.get",
                    imbl_value::json!({"id": "trust-fixture"}),
                    identity,
                )
                .await;
                let best = &package["result"]["best"];
                assert_eq!(!best[version].is_null(), visible, "{package}");
                if visible {
                    let asset = &best[version]["s9pks"][0][1];
                    assert_eq!(asset["private"], to_value(&key).unwrap());
                    assert_eq!(asset.get("readers").is_some(), sees_readers, "{asset}");
                }
                let index = rpc(&url, "package.index", imbl_value::json!({}), identity).await;
                let indexed = &index["result"]["packages"]["trust-fixture"];
                assert_eq!(!indexed.is_null(), visible, "{index}");
                if visible {
                    assert_eq!(
                        indexed["versions"][version]["s9pks"][0][1]
                            .get("readers")
                            .is_some(),
                        sees_readers,
                        "{index}"
                    );
                }
                let export = rpc(
                    &url,
                    "package.version.export",
                    imbl_value::json!({"id": "trust-fixture", "version": version}),
                    identity,
                )
                .await;
                if sees_readers {
                    let exported = &export["result"];
                    let asset = &exported["info"]["s9pks"][0][1];
                    assert_eq!(asset["private"], to_value(&key).unwrap(), "{export}");
                    let reader_id = asset["readers"][0].clone();
                    assert_eq!(
                        from_value::<AnyVerifyingKey>(
                            exported["readers"][reader_id.as_str().unwrap()]["key"].clone()
                        )
                        .unwrap(),
                        reader_key,
                        "{export}"
                    );
                    let missing = rpc(
                        &url,
                        "package.version.export",
                        imbl_value::json!({"id": "trust-fixture", "version": "9.9.9:0"}),
                        identity,
                    )
                    .await;
                    assert_eq!(
                        missing["error"]["code"],
                        imbl_value::json!(ErrorKind::NotFound as i32),
                        "{missing}"
                    );
                } else {
                    let expected = if visible {
                        ErrorKind::Authorization
                    } else {
                        ErrorKind::NotFound
                    };
                    assert_eq!(
                        export["error"]["code"],
                        imbl_value::json!(expected as i32),
                        "{export}"
                    );
                }
            }
        }
    };
    assert_visibility(
        source_url.clone(),
        vec![
            (None, false, false),
            (Some(stranger.clone()), false, false),
            (Some(reader.clone()), true, false),
            (Some(publisher.clone()), true, true),
            (Some(source_admin.clone()), true, true),
        ],
    )
    .await;
    let stored = source.db.peek().await.as_index().de().unwrap();
    let asset =
        &stored.package.packages["trust-fixture"].versions[&version.parse().unwrap()].s9pks[0].1;
    assert_eq!(asset.private.as_ref(), Some(&key));
    assert_eq!(asset.readers, stored.readers.keys().cloned().collect());

    cli_promote(
        cli.clone(),
        CliPromoteParams {
            from: None,
            to: Some("destination".into()),
            id: "trust-fixture".parse().unwrap(),
            version: version.parse().unwrap(),
        },
    )
    .await
    .unwrap();
    let promoted = target.db.peek().await.as_index().de().unwrap();
    let asset =
        &promoted.package.packages["trust-fixture"].versions[&version.parse().unwrap()].s9pks[0].1;
    assert_eq!(asset.private.as_ref(), Some(&key));
    assert_eq!(asset.urls, hosted);
    assert_eq!(
        promoted.readers.values().cloned().collect::<Vec<_>>(),
        [ReaderKey {
            label: "Reader".into(),
            key: reader_key.clone(),
        }]
    );
    assert_eq!(asset.readers, promoted.readers.keys().cloned().collect());
    assert_visibility(
        target_url.clone(),
        vec![
            (None, false, false),
            (Some(stranger.clone()), false, false),
            (Some(reader.clone()), true, false),
            (Some(publisher.clone()), true, true),
        ],
    )
    .await;

    cli_promote(
        cli.clone(),
        CliPromoteParams {
            from: None,
            to: Some("destination".into()),
            id: "trust-fixture".parse().unwrap(),
            version: version.parse().unwrap(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        target
            .db
            .peek()
            .await
            .as_index()
            .as_readers()
            .de()
            .unwrap()
            .len(),
        1
    );
    host_task.abort();
    source_task.abort();
    target_task.abort();
}

#[tokio::test]
async fn registry_rejects_readers_of_public_or_unknown_keys_before_fetching() {
    use crate::registry::reader::ReaderKey;
    use crate::s9pk::merkle_archive::source::encrypted::KeyStreamInfo;

    let uploader_key = SigningKey::from_bytes(&[12; 32]);
    let uploader = AnyVerifyingKey::Ed25519(uploader_key.verifying_key());
    let known = AnyVerifyingKey::Ed25519(SigningKey::from_bytes(&[16; 32]).verifying_key());
    let unknown = AnyVerifyingKey::Ed25519(SigningKey::from_bytes(&[17; 32]).verifying_key());
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path().into(), uploader.clone()).await;
    ctx.db
        .mutate(|db| {
            db.as_index_mut().as_readers_mut().add_reader(&ReaderKey {
                label: "Known".into(),
                key: known.clone(),
            })
        })
        .await
        .result
        .unwrap();
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
    let (url, requests, task) = server([("base".into(), fixture("base.s9pk"))].into()).await;
    let keystream = KeyStreamInfo {
        key: crate::util::serde::Base64([5; 32]),
        iv: crate::util::serde::Base64([6; 16]),
    };
    for (private, readers, kind) in [
        (None, vec![known.clone()], ErrorKind::InvalidRequest),
        (Some(keystream), vec![known, unknown], ErrorKind::NotFound),
    ] {
        let error = add_package(
            ctx.clone(),
            AddPackageParams {
                urls: vec![url.join("base").unwrap()],
                uploader: uploader.clone(),
                commitment,
                signature: signature.clone(),
                signatures: Default::default(),
                private,
                readers,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind, kind, "{error}");
    }
    assert!(requests.lock().unwrap().is_empty());
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
    assert_eq!(before, after);
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
                    uploader: uploader.clone(),
                    commitment,
                    signature: signature.clone(),
                    signatures: local_package().await.signatures().await.unwrap(),
                    private: None,
                    readers: Vec::new(),
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
                    uploader: uploader.clone(),
                    commitment,
                    signature: signature.clone(),
                    signatures: Default::default(),
                    private: None,
                    readers: Vec::new(),
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
            uploader: uploader.clone(),
            commitment,
            signature,
            signatures: Default::default(),
            private: None,
            readers: Vec::new(),
        },
    )
    .await
    .unwrap();
    let index = ctx.db.peek().await.as_index().de().unwrap();
    let version: crate::util::VersionString = "1.0.0:0".parse().unwrap();
    let asset = &index.package.packages["trust-fixture"].versions[&version].s9pks[0].1;
    assert_eq!(asset.urls, urls);
    assert_eq!(
        asset.signatures.keys().collect::<BTreeSet<_>>(),
        [&uploader].into()
    );
    assert!(requests.lock().unwrap().iter().all(Option::is_none));
    task.abort();
}

#[tokio::test]
async fn removing_registry_signers_or_keys_prunes_stored_package_signatures() {
    use crate::registry::admin::{
        EditSignerParams, RemoveSignerParams, edit_signer, remove_signer,
    };
    use crate::registry::package::index::tests::version_info_signed_by;

    let (publisher, mut info) = version_info_signed_by(1);
    let (retired, retired_info) = version_info_signed_by(2);
    let (removed, removed_info) = version_info_signed_by(3);
    info.merge_with(retired_info, true).unwrap();
    info.merge_with(removed_info, true).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path().into(), publisher.clone()).await;
    let id: crate::PackageId = "test".parse().unwrap();
    let version: crate::util::VersionString = "1.0.0:0".parse().unwrap();
    let signer = ctx
        .db
        .mutate(|db| {
            let signer = db.as_index_mut().as_signers_mut().add_signer(&SignerInfo {
                name: "Retiring publisher".into(),
                contact: vec![],
                keys: [retired.clone(), removed.clone()].into(),
                gpg_exports: Default::default(),
            })?;
            db.as_index_mut()
                .as_package_mut()
                .as_packages_mut()
                .upsert(&id, || Ok(Default::default()))?
                .as_versions_mut()
                .insert(&version, &info)?;
            Ok(signer)
        })
        .await
        .result
        .unwrap();

    let edit: EditSignerParams = serde_json::from_value(serde_json::json!({
        "id": signer,
        "addContact": [],
        "addKey": [],
        "removeContact": [],
        "removeKey": [retired],
    }))
    .unwrap();
    edit_signer(ctx.clone(), edit).await.unwrap();
    let index = ctx.db.peek().await.as_index().de().unwrap();
    assert_eq!(
        index.package.packages[&id].versions[&version].s9pks[0]
            .1
            .signatures
            .keys()
            .collect::<BTreeSet<_>>(),
        [&publisher, &removed].into()
    );

    remove_signer(ctx.clone(), RemoveSignerParams { id: signer })
        .await
        .unwrap();
    let index = ctx.db.peek().await.as_index().de().unwrap();
    assert_eq!(
        index.package.packages[&id].versions[&version].s9pks[0]
            .1
            .signatures
            .keys()
            .collect::<BTreeSet<_>>(),
        [&publisher].into()
    );
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
