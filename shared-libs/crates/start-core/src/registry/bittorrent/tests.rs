use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::routing::get;
use chrono::Utc;
use librqbit::{CreateTorrentOptions, ListenerOptions};
use tokio::net::TcpListener;

use super::*;
use crate::progress::FullProgressTracker;
use crate::registry::asset::RegistryAsset;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::serde::Base64;

#[test]
fn torrent_arguments_preserve_parent_command_descriptions() {
    use clap::Args;

    let command = TorrentDownloadConfig::augment_args(
        clap::Command::new("parent")
            .about("Parent command")
            .long_about("Parent command details"),
    );
    assert_eq!(command.get_about().unwrap().to_string(), "Parent command");
    assert_eq!(
        command.get_long_about().unwrap().to_string(),
        "Parent command details"
    );
}

struct Swarm {
    _tmp: TmpDir,
    _seed: TorrentSession,
    _server: NonDetachingJoinHandle<()>,
    url: Url,
    magnet: Url,
    torrent: bytes::Bytes,
    content: Vec<u8>,
    http_hits: Arc<AtomicUsize>,
}

impl Swarm {
    async fn new() -> Result<Self, Error> {
        Self::with_content((0..1037).map(|_| rand::random::<u8>()).collect()).await
    }

    async fn with_content(content: Vec<u8>) -> Result<Self, Error> {
        let tmp = TmpDir::new().await?;
        tokio::fs::write(tmp.join("payload.bin"), &content).await?;
        let torrent = librqbit::create_torrent(
            &tmp.join("payload.bin"),
            CreateTorrentOptions::default(),
            &librqbit::spawn_utils::BlockingSpawner::new(num_cpus::get()),
        )
        .await
        .map_err(torrent_error)?
        .as_bytes()
        .map_err(torrent_error)?;
        Self::serve_torrent(tmp, content, torrent).await
    }

    async fn hybrid() -> Result<Self, Error> {
        let content = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testdata/s9pk-trailer/base.s9pk"
        ))
        .to_vec();
        let torrent = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testdata/bittorrent/base-hybrid.torrent"
        ));
        let tmp = TmpDir::new().await?;
        tokio::fs::write(tmp.join("base.s9pk"), &content).await?;
        let mut swarm = Self::serve_torrent(tmp, content, torrent.to_vec().into()).await?;
        swarm.magnet.query_pairs_mut().append_pair(
            "xt",
            "urn:btmh:12208485f107f399acd3087f4043f4075bd7ca878ffbe18eb41a64b2d86669545762",
        );
        Ok(swarm)
    }

    async fn serve_torrent(
        tmp: TmpDir,
        content: Vec<u8>,
        bytes: bytes::Bytes,
    ) -> Result<Self, Error> {
        use librqbit_bencode::{BencodeValueOwned, bencode_serialize_to_writer};

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let url: Url = format!("http://{}/", listener.local_addr()?).parse()?;
        let BencodeValueOwned::Dict(mut metadata) =
            librqbit_bencode::from_bytes(&bytes).map_err(torrent_error)?
        else {
            panic!("torrent fixture is not a dictionary");
        };
        metadata.insert(
            b"announce-list".as_slice().into(),
            BencodeValueOwned::List(vec![BencodeValueOwned::List(vec![
                BencodeValueOwned::Bytes(url.join("announce")?.as_str().as_bytes().into()),
            ])]),
        );
        let mut torrent = Vec::new();
        bencode_serialize_to_writer(&BencodeValueOwned::Dict(metadata), &mut torrent)
            .map_err(torrent_error)?;
        let torrent: bytes::Bytes = torrent.into();
        let meta = librqbit::torrent_from_bytes(&torrent).map_err(torrent_error)?;
        let magnet = librqbit::Magnet::from_id20(
            meta.info_hash,
            vec![url.join("announce")?.to_string()],
            None,
        )
        .to_string()
        .parse()?;
        let seed = TorrentSession::new(
            tmp.to_path_buf(),
            SessionOptions {
                dht: None,
                listen: Some(ListenerOptions {
                    listen_addr: "127.0.0.1:0".parse().unwrap(),
                    ..Default::default()
                }),
                disable_trackers: true,
                disable_local_service_discovery: true,
                ..Default::default()
            },
        )
        .await?;
        seed.0
            .add_torrent(
                AddTorrent::from_bytes(torrent.clone()),
                Some(AddTorrentOptions {
                    overwrite: true,
                    ..Default::default()
                }),
            )
            .await
            .map_err(torrent_error)?
            .into_handle()
            .unwrap()
            .wait_until_initialized()
            .await
            .map_err(torrent_error)?;
        let port = seed.0.listen_addr().unwrap().port();
        let mut peers = b"d8:intervali1e5:peers6:".to_vec();
        peers.extend([127, 0, 0, 1]);
        peers.extend(port.to_be_bytes());
        peers.push(b'e');
        let hits = Arc::new(AtomicUsize::new(0));
        let alternate = vec![42u8; content.len()];
        let router = Router::new()
            .route(
                "/alternate-asset",
                get(move || {
                    let content = alternate.clone();
                    async move { content }
                }),
            )
            .route(
                "/metadata.torrent",
                get({
                    let torrent = torrent.clone();
                    move || {
                        let torrent = torrent.clone();
                        async move { torrent }
                    }
                }),
            )
            .route("/invalid.torrent", get(|| async { "invalid torrent" }))
            .route(
                "/head-only",
                get(|| async { axum::http::StatusCode::BAD_GATEWAY })
                    .head(|| async { axum::http::StatusCode::OK }),
            )
            .route(
                "/original-package",
                get(|| async { package_bytes().await.unwrap().0 }),
            )
            .route(
                "/announce",
                get(move || {
                    let peers = peers.clone();
                    async move { peers }
                }),
            )
            .route(
                "/asset",
                get({
                    let content = content.clone();
                    let hits = hits.clone();
                    move || {
                        let content = content.clone();
                        hits.fetch_add(1, Ordering::SeqCst);
                        async move { content }
                    }
                }),
            );
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Ok(Self {
            _tmp: tmp,
            _seed: seed,
            _server: server.into(),
            url,
            magnet,
            torrent,
            content,
            http_hits: hits,
        })
    }

    fn asset(&self, torrent: Url) -> RegistryAsset<Blake3Commitment> {
        RegistryAsset {
            published_at: Utc::now(),
            urls: vec![self.url.join("asset").unwrap()],
            torrent: Some(torrent),
            commitment: Blake3Commitment {
                hash: Base64(*blake3::hash(&self.content).as_bytes()),
                size: self.content.len() as u64,
            },
            signatures: HashMap::new(),
        }
    }

    async fn download(
        &self,
        asset: &RegistryAsset<Blake3Commitment>,
        client: &TorrentClient,
    ) -> Result<Vec<u8>, Error> {
        let mut content = Vec::new();
        let progress = FullProgressTracker::new().add_phase("Downloading".into(), Some(100));
        timeout(
            Duration::from_secs(45),
            asset.download(Client::new(), Some(client), &mut content, progress),
        )
        .await
        .with_kind(ErrorKind::Timeout)??;
        Ok(content)
    }
}

fn cli_client() -> TorrentClient {
    TorrentClient::download_only(TorrentDownloadConfig::default(), false)
}

async fn package_asset(swarm: &Swarm) -> Result<RegistryAsset<MerkleArchiveCommitment>, Error> {
    Ok(RegistryAsset {
        published_at: Utc::now(),
        urls: vec![swarm.url.join("asset")?],
        torrent: Some(swarm.url.join("metadata.torrent")?),
        commitment: package_bytes().await?.1,
        signatures: Default::default(),
    })
}

#[tokio::test]
async fn install_metadata_is_ready_while_the_torrent_payload_is_stalled() -> Result<(), Error> {
    let swarm = Swarm::with_content(package_bytes().await?.0).await?;
    swarm._seed.0.stop().await;
    let asset = package_asset(&swarm).await?;
    let progress = FullProgressTracker::new();
    let phase = progress.add_phase("Downloading".into(), Some(100));
    let prepared = timeout(
        Duration::from_secs(5),
        asset.prepare_install(Client::new(), Some(Arc::new(cli_client())), phase),
    )
    .await
    .with_kind(ErrorKind::Timeout)??;
    assert_eq!(
        prepared.metadata.as_archive().commitment().await?,
        asset.commitment
    );
    let id = prepared.metadata.as_manifest().id.clone();
    let state = crate::db::model::package::PackageState::Installing(
        crate::db::model::package::InstallingState {
            installing_info: crate::db::model::package::InstallingInfo {
                new_manifest: prepared.metadata.as_manifest().clone(),
                progress: progress.snapshot(),
            },
        },
    );
    let entries =
        std::collections::BTreeMap::from([(id.clone(), imbl_value::json!({"stateInfo": state}))]);
    let raw = patch_db::PatchDb::in_memory();
    raw.put(
        &patch_db::json_ptr::ROOT,
        &imbl_value::json!({"public": {"packageData": entries}}),
    )
    .await?;
    let db = patch_db::TypedPatchDb::<crate::db::model::Database, Error>::load_unchecked(raw);
    let update_id = id.clone();
    let sync: NonDetachingJoinHandle<_> = tokio::spawn(progress.sync_to_db(
        db.clone(),
        move |model| {
            model
                .as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(&update_id)
                .and_then(|entry| entry.as_state_info_mut().as_installing_info_mut())
                .map(|info| info.as_progress_mut())
        },
        Some(Duration::from_millis(10)),
    ))
    .into();
    assert!(
        timeout(Duration::from_millis(100), prepared.download.clone())
            .await
            .is_err()
    );
    timeout(Duration::from_secs(5), async {
        loop {
            let state = db
                .peek()
                .await
                .as_public()
                .as_package_data()
                .as_idx(&id)
                .unwrap()
                .as_state_info()
                .de()
                .unwrap();
            let crate::db::model::package::PackageState::Installing(state) = state else {
                panic!("install state lost")
            };
            if !matches!(
                state.installing_info.progress.phases[0].progress,
                crate::progress::Progress::NotStarted(_)
            ) {
                assert!(!state.installing_info.progress.overall.is_complete());
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    drop(prepared);
    drop(sync);
    Ok(())
}

#[tokio::test]
async fn install_metadata_uses_a_secondary_mirror_after_a_false_positive_head() -> Result<(), Error>
{
    let swarm = Swarm::with_content(package_bytes().await?.0).await?;
    swarm._seed.0.stop().await;
    let mut asset = package_asset(&swarm).await?;
    asset.urls.insert(0, swarm.url.join("head-only")?);
    let phase = FullProgressTracker::new().add_phase("Downloading".into(), Some(100));
    let prepared = timeout(
        Duration::from_secs(5),
        asset.prepare_install(Client::new(), Some(Arc::new(cli_client())), phase),
    )
    .await
    .with_kind(ErrorKind::Timeout)??;
    assert_eq!(
        prepared.metadata.as_archive().commitment().await?,
        asset.commitment
    );
    drop(prepared);
    Ok(())
}

#[tokio::test]
async fn install_preparation_uses_peer_headers_when_http_is_unavailable() -> Result<(), Error> {
    let swarm = Swarm::with_content(package_bytes().await?.0).await?;
    let mut asset = package_asset(&swarm).await?;
    asset.urls = vec![swarm.url.join("missing")?];
    let phase = FullProgressTracker::new().add_phase("Downloading".into(), Some(100));
    let prepared = timeout(
        Duration::from_secs(10),
        asset.prepare_install(Client::new(), Some(Arc::new(cli_client())), phase),
    )
    .await
    .with_kind(ErrorKind::Timeout)??;
    let loaded = prepared
        .download
        .await
        .map_err(|error| error.clone_output())?;
    assert_eq!(
        loaded.package.as_manifest().id,
        prepared.metadata.as_manifest().id
    );
    let mut bytes = Vec::new();
    loaded
        .source
        .fetch_all()
        .await?
        .read_to_end(&mut bytes)
        .await?;
    assert_eq!(bytes, swarm.content);
    assert!(loaded.source.sharing.is_some());
    assert_eq!(swarm.http_hits.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn install_preparation_reparses_http_fallback_after_a_bad_torrent() -> Result<(), Error> {
    let mut content = package_bytes().await?.0;
    *content.last_mut().unwrap() ^= 1;
    let swarm = Swarm::with_content(content).await?;
    let mut asset = package_asset(&swarm).await?;
    asset.urls = vec![swarm.url.join("original-package")?];
    let phase = FullProgressTracker::new().add_phase("Downloading".into(), Some(100));
    let prepared = asset
        .prepare_install(Client::new(), Some(Arc::new(cli_client())), phase)
        .await?;
    let loaded = prepared
        .download
        .await
        .map_err(|error| error.clone_output())?;
    let mut bytes = Vec::new();
    loaded
        .source
        .fetch_all()
        .await?
        .read_to_end(&mut bytes)
        .await?;
    assert_eq!(bytes, package_bytes().await?.0);
    assert_eq!(
        loaded.source.sharing.as_ref().unwrap().torrent.0,
        swarm.torrent
    );
    Ok(())
}

#[tokio::test]
async fn install_payload_cancellation_also_closes_metadata_readers() -> Result<(), Error> {
    let swarm = Swarm::with_content(package_bytes().await?.0).await?;
    swarm._seed.0.stop().await;
    let asset = package_asset(&swarm).await?;
    let client = cli_client();
    let pending = client
        .begin(
            asset.torrent.as_ref().unwrap(),
            asset.commitment.into(),
            Client::new(),
        )
        .await?
        .unwrap();
    let source = pending.source.as_ref().unwrap().clone();
    let token = source.workspace.session.0.cancellation_token().clone();
    let _metadata_reader = source.fetch(0, 1).await?;
    let mut phase = FullProgressTracker::new().add_phase("Downloading".into(), Some(100));
    let mut completion = Box::pin(pending.complete(&mut phase));
    assert!(
        timeout(Duration::from_millis(100), completion.as_mut())
            .await
            .is_err()
    );
    assert!(!token.is_cancelled());
    drop(completion);
    assert!(token.is_cancelled());
    assert!(source.torrent.stats().progress_bytes < source.length);
    Ok(())
}

#[tokio::test]
async fn install_unpolled_payload_drop_also_closes_metadata_readers() -> Result<(), Error> {
    let swarm = Swarm::with_content(package_bytes().await?.0).await?;
    swarm._seed.0.stop().await;
    let asset = package_asset(&swarm).await?;
    let client = cli_client();
    let pending = client
        .begin(
            asset.torrent.as_ref().unwrap(),
            asset.commitment.into(),
            Client::new(),
        )
        .await?
        .unwrap();
    let source = pending.source.as_ref().unwrap().clone();
    let token = source.workspace.session.0.cancellation_token().clone();
    let _metadata_reader = source.fetch(0, 1).await?;
    let completion = async move {
        let mut phase = FullProgressTracker::new().add_phase("Downloading".into(), Some(100));
        pending.complete(&mut phase).await
    };
    assert!(!token.is_cancelled());
    drop(completion);
    assert!(token.is_cancelled());
    assert!(source.torrent.stats().progress_bytes < source.length);
    Ok(())
}

#[tokio::test]
async fn torrent_metadata_download_is_preferred_to_http() -> Result<(), Error> {
    let swarm = Swarm::new().await?;
    let client = cli_client();
    assert_eq!(
        swarm
            .download(&swarm.asset(swarm.url.join("metadata.torrent")?), &client)
            .await?,
        swarm.content
    );
    assert_eq!(swarm.http_hits.load(Ordering::SeqCst), 0);
    assert!(client.disable_upload);
    client.resume_seeding().await?;
    assert!(client.catalog.get().is_none());
    assert!(client.seeds.read().await.is_none());
    Ok(())
}

#[tokio::test]
async fn http_only_downloads_ignore_advertised_torrents() -> Result<(), Error> {
    let swarm = Swarm::new().await?;
    let asset = swarm.asset(swarm.url.join("metadata.torrent")?);
    let progress = FullProgressTracker::new().add_phase("Downloading".into(), Some(100));
    let mut content = Vec::new();
    asset
        .download(Client::new(), None, &mut content, progress)
        .await?;
    assert_eq!(content, swarm.content);
    assert!(swarm.http_hits.load(Ordering::SeqCst) > 0);
    Ok(())
}

#[tokio::test]
async fn hybrid_metainfo_and_dual_hash_magnets_use_the_compatible_swarm() -> Result<(), Error> {
    use sha2::Digest;

    let swarm = Swarm::hybrid().await?;
    let meta = librqbit::torrent_from_bytes(&swarm.torrent).map_err(torrent_error)?;
    assert_eq!(
        meta.info_hash.as_string(),
        "aa7225bd6288e39b146b3aeb18d4846d9a4b011a"
    );
    assert_eq!(
        hex::encode(sha2::Sha256::digest(meta.info.raw_bytes.as_ref())),
        "8485f107f399acd3087f4043f4075bd7ca878ffbe18eb41a64b2d86669545762"
    );
    let magnet = librqbit::Magnet::parse(swarm.magnet.as_str()).map_err(torrent_error)?;
    assert!(magnet.as_id20().is_some());
    assert!(magnet.as_id32().is_some());
    for torrent in [swarm.url.join("metadata.torrent")?, swarm.magnet.clone()] {
        let client = cli_client();
        assert_eq!(
            swarm.download(&swarm.asset(torrent), &client).await?,
            swarm.content
        );
    }
    assert_eq!(swarm.http_hits.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn magnet_download_is_preferred_to_http() -> Result<(), Error> {
    let swarm = Swarm::new().await?;
    assert_eq!(
        swarm
            .download(&swarm.asset(swarm.magnet.clone()), &cli_client())
            .await?,
        swarm.content
    );
    assert_eq!(swarm.http_hits.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn a_torrent_without_available_peers_falls_back_to_http() -> Result<(), Error> {
    let swarm = Swarm::new().await?;
    let (torrent, _) = swarm
        ._seed
        .0
        .create_and_serve_torrent(
            &swarm._tmp.join("payload.bin"),
            CreateTorrentOptions::default(),
        )
        .await
        .map_err(torrent_error)?;
    let bytes = torrent.as_bytes().map_err(torrent_error)?;
    let expected_metadata = bytes.to_vec();
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let metadata: Url = format!("http://{}/metadata", listener.local_addr()?).parse()?;
    let router = Router::new().route(
        "/metadata",
        get(move || {
            let bytes = bytes.clone();
            async move { bytes }
        }),
    );
    let _server = NonDetachingJoinHandle::from(tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    }));
    let mut content = Vec::new();
    let progress = FullProgressTracker::new().add_phase("Downloading".into(), Some(100));
    let outcome = swarm
        .asset(metadata)
        .download(Client::new(), Some(&cli_client()), &mut content, progress)
        .await?;
    assert_eq!(content, swarm.content);
    assert_eq!(outcome.sharing.unwrap().torrent.0, expected_metadata);
    assert!(swarm.http_hits.load(Ordering::SeqCst) > 0);
    Ok(())
}

#[tokio::test]
async fn existing_artifact_metadata_lookup_does_not_download_a_payload() -> Result<(), Error> {
    let swarm = Swarm::new().await?;
    let client = TorrentClient::for_server(TorrentDownloadConfig::default(), None);
    let asset = swarm.asset(swarm.url.join("metadata.torrent")?);
    let resolved = client
        .resolve(
            asset.torrent.as_ref().unwrap(),
            &asset.commitment.clone().into(),
            Client::new(),
        )
        .await?
        .unwrap();
    assert!(resolved.workspace.is_none());
    assert_eq!(resolved.bytes, swarm.torrent);
    let metadata = asset
        .sharing_metadata(Client::new(), Some(&client))
        .await?
        .unwrap();
    assert_eq!(metadata.torrent.0, swarm.torrent);
    assert_eq!(swarm.http_hits.load(Ordering::SeqCst), 0);
    assert!(client.catalog.get().is_none());
    assert!(client.seeds.read().await.is_none());
    Ok(())
}

#[tokio::test]
async fn slow_metadata_discovery_does_not_block_sharing_or_disable() -> Result<(), Error> {
    struct DiscoveryGuard(Arc<AtomicUsize>);
    impl Drop for DiscoveryGuard {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let catalog = TmpDir::new().await?;
    let client = Arc::new(TorrentClient::for_server(
        TorrentDownloadConfig::default(),
        Some(TorrentSeedConfig {
            directory: catalog.to_path_buf(),
            listen: None,
        }),
    ));
    let db = patch_db::PatchDb::in_memory();
    db.put(&patch_db::json_ptr::ROOT, &imbl_value::json!(false))
        .await?;
    let watch = db
        .watch(patch_db::json_ptr::ROOT.to_owned())
        .await
        .typed::<bool>();
    let started = Arc::new(AtomicUsize::new(0));
    let cancelled = Arc::new(AtomicUsize::new(0));
    let owner = client.clone();
    let (began, ended) = (started.clone(), cancelled.clone());
    let task: NonDetachingJoinHandle<_> = tokio::spawn(async move {
        owner
            .maintain_seeding(watch, move || {
                let (started, cancelled) = (began.clone(), ended.clone());
                async move {
                    let _guard = DiscoveryGuard(cancelled);
                    started.fetch_add(1, Ordering::SeqCst);
                    std::future::pending::<Result<(), Error>>().await
                }
            })
            .await
    })
    .into();
    assert_eq!(started.load(Ordering::SeqCst), 0);
    db.put(&patch_db::json_ptr::ROOT, &imbl_value::json!(true))
        .await?;
    wait_for_seeding(&client, true).await;
    timeout(Duration::from_secs(10), async {
        while started.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    db.put(&patch_db::json_ptr::ROOT, &imbl_value::json!(false))
        .await?;
    wait_for_seeding(&client, false).await;
    assert_eq!(cancelled.load(Ordering::SeqCst), 1);
    drop(task);
    Ok(())
}

#[tokio::test]
async fn invalid_torrent_metadata_falls_back_to_http() -> Result<(), Error> {
    let swarm = Swarm::new().await?;
    assert_eq!(
        swarm
            .download(
                &swarm.asset(swarm.url.join("invalid.torrent")?),
                &cli_client()
            )
            .await?,
        swarm.content
    );
    assert!(swarm.http_hits.load(Ordering::SeqCst) > 0);
    Ok(())
}

#[tokio::test]
async fn torrent_content_must_match_the_registry_commitment_before_fallback() -> Result<(), Error> {
    let swarm = Swarm::new().await?;
    let mut asset = swarm.asset(swarm.url.join("metadata.torrent")?);
    asset.commitment.hash = Base64([7; 32]);
    let mut content = Vec::new();
    let progress = FullProgressTracker::new().add_phase("Downloading".into(), Some(100));
    assert!(
        asset
            .download(Client::new(), Some(&cli_client()), &mut content, progress)
            .await
            .is_err()
    );
    assert!(swarm.http_hits.load(Ordering::SeqCst) > 0);
    Ok(())
}

#[tokio::test]
async fn corrupt_torrent_content_falls_back_without_writing_it_to_the_destination()
-> Result<(), Error> {
    let swarm = Swarm::new().await?;
    let expected = vec![42; swarm.content.len()];
    let mut asset = swarm.asset(swarm.url.join("metadata.torrent")?);
    asset.urls = vec![swarm.url.join("alternate-asset")?];
    asset.commitment.hash = Base64(*blake3::hash(&expected).as_bytes());
    assert_eq!(swarm.download(&asset, &cli_client()).await?, expected);
    Ok(())
}

#[tokio::test]
async fn proxy_and_onion_sources_never_start_peer_downloads() -> Result<(), Error> {
    let swarm = Swarm::new().await?;
    let client = TorrentClient::download_only(
        TorrentDownloadConfig {
            bittorrent: Some(true),
        },
        true,
    );
    assert_eq!(
        swarm
            .download(&swarm.asset(swarm.url.join("metadata.torrent")?), &client)
            .await?,
        swarm.content
    );
    assert!(swarm.http_hits.load(Ordering::SeqCst) > 0);
    assert!(
        cli_client()
            .for_source(&"http://registry.onion/".parse()?)
            .is_none()
    );
    Ok(())
}

#[tokio::test]
async fn unsafe_names_and_wrong_lengths_are_rejected() -> Result<(), Error> {
    let swarm = Swarm::new().await?;
    let asset = swarm.asset(swarm.url.join("metadata.torrent")?);
    validate_metadata(&swarm.torrent, &asset.commitment.clone().into())?;
    let mut wrong_size = asset.commitment.clone();
    wrong_size.size += 1;
    assert!(validate_metadata(&swarm.torrent, &wrong_size.into()).is_err());
    for name in [
        "../escape",
        "nested/file",
        "..",
        "/absolute",
        "nested\\file",
    ] {
        let mut torrent = swarm.torrent.to_vec();
        let original = b"11:payload.bin";
        let start = torrent
            .windows(original.len())
            .position(|bytes| bytes == original)
            .unwrap();
        torrent.splice(
            start..start + original.len(),
            format!("{}:{name}", name.len()).bytes(),
        );
        assert!(
            validate_metadata(&torrent, &asset.commitment.clone().into()).is_err(),
            "{name}"
        );
    }
    Ok(())
}

async fn package_bytes() -> Result<(Vec<u8>, MerkleArchiveCommitment), Error> {
    use crate::s9pk::merkle_archive::directory_contents::DirectoryContents;
    use crate::s9pk::merkle_archive::{Entry, MerkleArchive};
    use crate::util::io::TrackingIO;
    let manifest = serde_json::to_vec(&serde_json::json!({
        "id": "torrent-test", "version": "1.0.0:0", "canMigrateTo": "*", "canMigrateFrom": "*",
        "title": "Torrent test", "description": { "short": "Test", "long": "Test" },
        "releaseNotes": "Test", "license": "MIT", "upstreamRepo": "https://example.test", "images": {}, "volumes": []
    })).with_kind(ErrorKind::Serialization)?;
    let mut contents = DirectoryContents::<Arc<[u8]>>::new();
    contents.insert_path("manifest.json", Entry::file(manifest.into()))?;
    contents.insert_path("README.md", Entry::file(Arc::from(&b"Test package"[..])))?;
    let mut archive = MerkleArchive::new(
        contents,
        ed25519_dalek::SigningKey::from_bytes(&[42; 32]),
        crate::s9pk::v2::SIG_CONTEXT,
    );
    archive.update_hashes(true).await?;
    let commitment = archive.commitment().await?;
    let mut package = S9pk::new(archive, None).await?;
    let mut bytes = Vec::new();
    package
        .serialize(&mut TrackingIO::new(0, &mut bytes), true)
        .await?;
    Ok((bytes, commitment))
}

#[tokio::test]
async fn package_download_parses_and_verifies_the_committed_torrent_archive() -> Result<(), Error> {
    let (bytes, commitment) = package_bytes().await?;
    let swarm = Swarm::with_content(bytes).await?;
    let asset = RegistryAsset {
        published_at: Utc::now(),
        urls: vec![swarm.url.join("asset")?],
        torrent: Some(swarm.url.join("metadata.torrent")?),
        commitment,
        signatures: HashMap::new(),
    };
    let dest = TmpDir::new().await?;
    let progress = FullProgressTracker::new().add_phase("Downloading".into(), Some(100));
    let client = cli_client();
    let (package, source) = asset
        .download_to(
            dest.join("package.s9pk"),
            Client::new(),
            Some(&client),
            progress,
        )
        .await?;
    source.wait_for_buffered().await?;
    assert_eq!(package.as_manifest().id.to_string(), "torrent-test");
    assert_eq!(package.readme().await?.as_deref(), Some("Test package"));
    assert_eq!(
        tokio::fs::read(dest.join("package.s9pk")).await?,
        swarm.content
    );
    assert_eq!(swarm.http_hits.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn stored_hybrid_registration_keeps_metadata_only_and_waits_for_opt_in() -> Result<(), Error>
{
    let swarm = Swarm::hybrid().await?;
    let catalog = TmpDir::new().await?;
    let installed = TmpDir::new().await?;
    let client = TorrentClient::for_server(
        TorrentDownloadConfig::default(),
        Some(TorrentSeedConfig {
            directory: catalog.to_path_buf(),
            listen: None,
        }),
    );
    let asset = swarm.asset(swarm.url.join("metadata.torrent")?);
    let progress = FullProgressTracker::new().add_phase("download".into(), Some(1));
    let mut content = Vec::new();
    let outcome = asset
        .download(Client::new(), Some(&client), &mut content, progress)
        .await?;
    assert_eq!(content, swarm.content);
    assert!(client.catalog.get().is_none());
    let path = installed.join("installed.s9pk");
    tokio::fs::write(&path, content).await?;
    client
        .register_stored(&path, outcome.sharing.as_ref().unwrap())
        .await?;
    assert!(client.seeds.read().await.is_none());
    let meta = validate_metadata(&swarm.torrent, &asset.commitment.into())?;
    assert!(
        catalog
            .join(meta.info_hash.as_string())
            .with_extension("json")
            .exists()
    );
    assert!(!catalog.join(meta.info_hash.as_string()).exists());
    for _ in 0..2 {
        client.resume_seeding().await?;
        assert!(client.seeds.read().await.is_some());
        client.shutdown().await;
        assert!(client.seeds.read().await.is_none());
    }
    assert_eq!(tokio::fs::read(path).await?, swarm.content);
    Ok(())
}

async fn wait_for_seeding(client: &TorrentClient, enabled: bool) {
    timeout(Duration::from_secs(10), async {
        while client.seeds.read().await.is_some() != enabled {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("seeding state did not follow persisted intent");
}

#[tokio::test]
async fn seeding_follows_persisted_intent_and_can_restart_with_an_old_service_reference()
-> Result<(), Error> {
    let cache = TmpDir::new().await?;
    let client = Arc::new(TorrentClient::for_server(
        TorrentDownloadConfig::default(),
        Some(TorrentSeedConfig {
            directory: cache.to_path_buf(),
            listen: None,
        }),
    ));
    let db = patch_db::PatchDb::in_memory();
    let ptr: patch_db::json_ptr::JsonPointer = "/enabled".parse().unwrap();
    db.put(
        &patch_db::json_ptr::ROOT,
        &imbl_value::json!({"enabled": false}),
    )
    .await?;
    let watch = db.watch(ptr.clone()).await.typed::<bool>();
    let owner = client.clone();
    let task: NonDetachingJoinHandle<_> =
        tokio::spawn(
            async move { Box::pin(owner.maintain_seeding(watch, || async { Ok(()) })).await },
        )
        .into();
    db.put(&ptr, &true).await?;
    wait_for_seeding(&client, true).await;
    let old = client.seeds.read().await.clone().unwrap();
    let lease = client.catalog.get().unwrap().clone();
    db.put(&ptr, &false).await?;
    wait_for_seeding(&client, false).await;
    db.put(&ptr, &true).await?;
    wait_for_seeding(&client, true).await;
    assert!(Arc::ptr_eq(&lease, client.catalog.get().unwrap()));
    assert!(!Arc::ptr_eq(
        &old,
        client.seeds.read().await.as_ref().unwrap()
    ));
    db.put(&ptr, &false).await?;
    wait_for_seeding(&client, false).await;
    drop(task);
    Ok(())
}

#[tokio::test]
async fn registration_while_disabled_does_not_revive_a_stopped_service() -> Result<(), Error> {
    let swarm = Swarm::new().await?;
    let catalog = TmpDir::new().await?;
    let client = TorrentClient::for_server(
        TorrentDownloadConfig::default(),
        Some(TorrentSeedConfig {
            directory: catalog.to_path_buf(),
            listen: None,
        }),
    );
    client.resume_seeding().await?;
    let old = client.seeds.read().await.clone().unwrap();
    client.shutdown().await;
    let metadata = SharingMetadata {
        torrent: Base64(swarm.torrent.to_vec()),
        commitment: swarm.asset(swarm.magnet.clone()).commitment.into(),
    };
    client
        .register_stored(&swarm._tmp.join("payload.bin"), &metadata)
        .await?;
    old.refresh().await?;
    assert!(client.seeds.read().await.is_none());
    client.resume_seeding().await?;
    assert!(!Arc::ptr_eq(
        &old,
        client.seeds.read().await.as_ref().unwrap()
    ));
    client.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn cancelling_the_seeding_worker_prevents_late_service_publication() -> Result<(), Error> {
    let cache = TmpDir::new().await?;
    let client = Arc::new(TorrentClient::for_server(
        TorrentDownloadConfig::default(),
        Some(TorrentSeedConfig {
            directory: cache.to_path_buf(),
            listen: None,
        }),
    ));
    let db = patch_db::PatchDb::in_memory();
    db.put(&patch_db::json_ptr::ROOT, &imbl_value::json!(true))
        .await?;
    let watch = db
        .watch(patch_db::json_ptr::ROOT.to_owned())
        .await
        .typed::<bool>();
    let publication = client.seeds.write().await;
    let mut worker = Box::pin(client.maintain_seeding(watch, || async { Ok(()) }));
    assert!(futures::poll!(&mut worker).is_pending());
    drop(worker);
    drop(publication);
    client.shutdown().await;
    assert!(client.seeds.read().await.is_none());
    Ok(())
}

#[tokio::test]
async fn http_only_server_cannot_start_retained_seeding() -> Result<(), Error> {
    let client = TorrentClient::for_server(
        TorrentDownloadConfig {
            bittorrent: Some(false),
        },
        None,
    );
    client.resume_seeding().await?;
    assert!(!client.downloads_enabled());
    assert!(client.seeds.read().await.is_none());
    assert!(client.catalog.get().is_none());
    Ok(())
}

#[tokio::test]
async fn legacy_registry_assets_keep_their_wire_shape() -> Result<(), Error> {
    let swarm = Swarm::new().await?;
    let mut asset = swarm.asset(swarm.magnet.clone());
    asset.torrent = None;
    let legacy = serde_json::to_value(&asset).with_kind(ErrorKind::Serialization)?;
    assert!(legacy.get("torrent").is_none());
    let decoded: RegistryAsset<Blake3Commitment> =
        serde_json::from_value(legacy.clone()).with_kind(ErrorKind::Deserialization)?;
    assert!(decoded.torrent.is_none());
    assert_eq!(
        serde_json::to_value(decoded).with_kind(ErrorKind::Serialization)?,
        legacy
    );
    asset.torrent = Some(swarm.magnet.clone());
    let decoded: RegistryAsset<Blake3Commitment> =
        serde_json::from_value(serde_json::to_value(asset).with_kind(ErrorKind::Serialization)?)
            .with_kind(ErrorKind::Deserialization)?;
    assert_eq!(decoded.torrent, Some(swarm.magnet));
    Ok(())
}

#[test]
fn publishing_accepts_magnets_and_metadata_urls_but_not_local_files() {
    for url in [
        "https://example.test/metadata.torrent",
        "magnet:?xt=urn:btih:0123456789012345678901234567890123456789",
    ] {
        validate_torrent_url(&url.parse().unwrap()).unwrap();
    }
    for url in [
        "file:///etc/passwd",
        "magnet:?dn=missing-hash",
        "ftp://example.test/asset",
    ] {
        assert!(validate_torrent_url(&url.parse().unwrap()).is_err());
    }
}

#[tokio::test]
async fn dropping_a_download_session_cancels_its_tasks() -> Result<(), Error> {
    let tmp = TmpDir::new().await?;
    let session = TorrentSession::new(
        tmp.to_path_buf(),
        SessionOptions {
            dht: None,
            ..session_options()
        },
    )
    .await?;
    let token = session.0.cancellation_token().clone();
    drop(session);
    assert!(token.is_cancelled());
    Ok(())
}
