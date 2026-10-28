use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use axum::extract::ws::{Message, Utf8Bytes, WebSocketUpgrade};
use axum::response::Response;
use exver::{Version, VersionRange};
use futures::SinkExt;
use imbl_value::InternedString;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tokio::time::timeout;

use crate::prelude::*;
use crate::registry::RegistryDatabase;
use crate::registry::os::index::OsIndex;
use crate::s9pk::git_hash::GitHash;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::net::WebSocket;

/// Security releases with an available SquashFS image, keyed by version.
pub type SecurityCatalog = BTreeMap<Version, SecurityUpdate>;

/// Compatibility and platform availability for an OS security release.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityUpdate {
    pub git_hash: Option<GitHash>,
    pub source_version: VersionRange,
    pub platforms: BTreeSet<InternedString>,
}

const SEND_TIMEOUT: Duration = Duration::from_secs(15);

fn catalog(index: OsIndex) -> SecurityCatalog {
    index
        .versions
        .0
        .into_iter()
        .filter(|(_, info)| info.security && !info.squashfs.is_empty())
        .map(|(version, info)| {
            (
                version,
                SecurityUpdate {
                    git_hash: info.git_hash,
                    source_version: info.source_version,
                    platforms: info.squashfs.into_keys().collect(),
                },
            )
        })
        .collect()
}

pub(crate) async fn init(
    db: &TypedPatchDb<RegistryDatabase>,
) -> Result<(watch::Receiver<Utf8Bytes>, NonDetachingJoinHandle<()>), Error> {
    let mut db_watch = db
        .watch("/index/os".parse().unwrap())
        .await
        .typed::<OsIndex>();
    let mut current = catalog(db_watch.peek_and_mark_seen()?.de()?);
    let (sender, receiver) = watch::channel(
        serde_json::to_string(&current)
            .with_kind(ErrorKind::Serialization)?
            .into(),
    );
    let task = tokio::spawn(async move {
        let result: Result<(), Error> = async {
            loop {
                db_watch.changed().await?;
                let next = catalog(db_watch.peek_and_mark_seen()?.de()?);
                if next != current {
                    sender.send_replace(
                        serde_json::to_string(&next)
                            .with_kind(ErrorKind::Serialization)?
                            .into(),
                    );
                    current = next;
                }
            }
        }
        .await;
        if let Err(error) = result {
            tracing::error!(%error, "Registry security catalog watch stopped");
        }
    });
    Ok((receiver, task.into()))
}

pub(crate) fn upgrade(ws: WebSocketUpgrade, receiver: watch::Receiver<Utf8Bytes>) -> Response {
    ws.read_buffer_size(2048)
        .write_buffer_size(0)
        .max_message_size(1024)
        .max_frame_size(1024)
        .on_upgrade(move |socket| subscribe(WebSocket::new(socket), receiver))
}

async fn subscribe(mut socket: WebSocket, mut receiver: watch::Receiver<Utf8Bytes>) {
    let initial = receiver.borrow_and_update().clone();
    if !matches!(
        timeout(SEND_TIMEOUT, socket.send(Message::Text(initial))).await,
        Ok(Ok(()))
    ) {
        return;
    }
    loop {
        tokio::select! {
            changed = receiver.changed() => {
                if changed.is_err() {
                    break;
                }
                let latest = receiver.borrow_and_update().clone();
                if !matches!(timeout(SEND_TIMEOUT, socket.send(Message::Text(latest))).await, Ok(Ok(()))) {
                    return;
                }
            }
            message = socket.recv() => {
                match message {
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_))) => {
                        let _ = timeout(SEND_TIMEOUT, socket.flush()).await;
                        return;
                    }
                    _ => break,
                }
            }
        }
    }
    let _ = timeout(SEND_TIMEOUT, socket.normal_close("")).await;
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::routing::get;
    use futures::{SinkExt, StreamExt};
    use patch_db::PatchDb;
    use patch_db::json_ptr::ROOT;
    use tokio::net::{TcpListener, TcpStream};
    use tokio_tungstenite::tungstenite::Message as ClientMessage;
    use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

    use super::*;
    use crate::registry::asset::RegistryAsset;
    use crate::registry::os::index::OsVersionInfo;
    use crate::sign::commitment::blake3::Blake3Commitment;
    use crate::util::io::TmpDir;
    use crate::util::serde::Base64;

    fn asset() -> RegistryAsset<Blake3Commitment> {
        RegistryAsset {
            published_at: chrono::Utc::now(),
            urls: vec![],
            commitment: Blake3Commitment {
                hash: Base64([0; 32]),
                size: 0,
            },
            signatures: Default::default(),
        }
    }

    fn release() -> OsVersionInfo {
        OsVersionInfo {
            security: true,
            git_hash: Some("abc123".parse().unwrap()),
            source_version: "<0.4.1".parse().unwrap(),
            squashfs: [("x86_64".into(), asset())].into(),
            ..Default::default()
        }
    }

    fn index(info: OsVersionInfo) -> OsIndex {
        let mut index = OsIndex::default();
        index.versions.0.insert("0.4.1".parse().unwrap(), info);
        index
    }

    async fn database() -> (TmpDir, TypedPatchDb<RegistryDatabase>) {
        let dir = TmpDir::new().await.unwrap();
        let db =
            TypedPatchDb::load_unchecked(PatchDb::open(dir.join("registry.db")).await.unwrap());
        db.put(&ROOT, &RegistryDatabase::init()).await.unwrap();
        (dir, db)
    }

    async fn put_index(db: &TypedPatchDb<RegistryDatabase>, index: &OsIndex) {
        db.put(
            &"/index/os"
                .parse::<patch_db::json_ptr::JsonPointer>()
                .unwrap(),
            index,
        )
        .await
        .unwrap();
    }

    async fn changed(receiver: &mut watch::Receiver<Utf8Bytes>) -> SecurityCatalog {
        timeout(Duration::from_secs(2), receiver.changed())
            .await
            .unwrap()
            .unwrap();
        serde_json::from_str(&receiver.borrow_and_update()).unwrap()
    }

    #[test]
    fn filters_security_releases_with_squashfs_platforms() {
        let expected = SecurityUpdate {
            git_hash: Some("abc123".parse().unwrap()),
            source_version: "<0.4.1".parse().unwrap(),
            platforms: ["x86_64".into()].into(),
        };
        assert_eq!(
            catalog(index(release())).into_values().next(),
            Some(expected)
        );
        let mut regular = release();
        regular.security = false;
        assert!(catalog(index(regular)).is_empty());
        let mut iso_only = release();
        iso_only.squashfs.clear();
        iso_only.iso.insert("x86_64".into(), asset());
        iso_only.img.insert("aarch64".into(), asset());
        assert!(catalog(index(iso_only)).is_empty());
    }

    #[tokio::test]
    async fn cache_tracks_metadata_and_availability_without_unrelated_fanout() {
        let (_dir, db) = database().await;
        put_index(&db, &index(release())).await;
        let (mut receiver, _task) = init(&db).await.unwrap();
        let initial = receiver.borrow_and_update().clone();
        assert_eq!(
            serde_json::from_str::<SecurityCatalog>(&initial)
                .unwrap()
                .len(),
            1
        );

        db.put(
            &"/index/name"
                .parse::<patch_db::json_ptr::JsonPointer>()
                .unwrap(),
            "Renamed",
        )
        .await
        .unwrap();
        let mut info = release();
        info.headline = "Unrelated headline".into();
        info.release_notes = "Long release notes".repeat(1000).parse().unwrap();
        info.squashfs
            .get_mut("x86_64")
            .unwrap()
            .urls
            .push("https://example.com/os".parse().unwrap());
        put_index(&db, &index(info)).await;
        assert!(
            timeout(Duration::from_millis(50), receiver.changed())
                .await
                .is_err()
        );
        assert_eq!(receiver.borrow().as_ptr(), initial.as_ptr());
        assert_eq!(receiver.clone().borrow().as_ptr(), initial.as_ptr());

        let mut info = release();
        info.git_hash = Some("def456".parse().unwrap());
        put_index(&db, &index(info)).await;
        assert_eq!(
            changed(&mut receiver)
                .await
                .into_values()
                .next()
                .unwrap()
                .git_hash,
            Some("def456".parse().unwrap())
        );

        let mut info = release();
        info.source_version = ">=0.4.0".parse().unwrap();
        info.squashfs.insert("aarch64".into(), asset());
        put_index(&db, &index(info)).await;
        let update = changed(&mut receiver).await.into_values().next().unwrap();
        assert_eq!(update.source_version, ">=0.4.0".parse().unwrap());
        assert_eq!(update.platforms, ["aarch64".into(), "x86_64".into()].into());

        let mut info = release();
        info.squashfs.clear();
        put_index(&db, &index(info)).await;
        assert!(changed(&mut receiver).await.is_empty());
        put_index(&db, &index(release())).await;
        assert_eq!(changed(&mut receiver).await.len(), 1);
        let mut info = release();
        info.security = false;
        put_index(&db, &index(info)).await;
        assert!(changed(&mut receiver).await.is_empty());
    }

    type ClientSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

    async fn serve(receiver: watch::Receiver<Utf8Bytes>) -> (String, NonDetachingJoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/os/security", listener.local_addr().unwrap());
        let router = Router::new().route(
            "/os/security",
            get(move |ws: WebSocketUpgrade| {
                let receiver = receiver.clone();
                async move { upgrade(ws, receiver) }
            }),
        );
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        (url, task.into())
    }

    async fn receive(socket: &mut ClientSocket) -> ClientMessage {
        timeout(Duration::from_secs(2), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
    }

    #[tokio::test]
    async fn raw_websocket_sends_initial_latest_updates_and_reconnects() {
        let (_dir, db) = database().await;
        let (receiver, _cache_task) = init(&db).await.unwrap();
        let (url, _server) = serve(receiver).await;
        let (mut first, _) = connect_async(&url).await.unwrap();
        assert_eq!(receive(&mut first).await.into_text().unwrap(), "{}");
        let (mut second, _) = connect_async(&url).await.unwrap();
        assert_eq!(receive(&mut second).await.into_text().unwrap(), "{}");
        put_index(&db, &index(release())).await;
        let first_update = receive(&mut first).await.into_text().unwrap();
        assert_eq!(
            first_update,
            receive(&mut second).await.into_text().unwrap()
        );
        assert_eq!(
            serde_json::from_str::<SecurityCatalog>(&first_update).unwrap(),
            catalog(index(release()))
        );
        first.close(None).await.unwrap();
        assert!(matches!(receive(&mut first).await, ClientMessage::Close(_)));
        let (mut reconnected, _) = connect_async(&url).await.unwrap();
        assert_eq!(
            receive(&mut reconnected).await.into_text().unwrap(),
            first_update
        );
        put_index(&db, &OsIndex::default()).await;
        assert_eq!(receive(&mut second).await.into_text().unwrap(), "{}");
        assert_eq!(receive(&mut reconnected).await.into_text().unwrap(), "{}");
    }

    #[tokio::test]
    async fn outbound_snapshot_exceeds_inbound_limit() {
        let large: Utf8Bytes =
            serde_json::to_string(&SecurityCatalog::from_iter((0..100).map(|i| {
                (
                    format!("0.4.{i}").parse().unwrap(),
                    SecurityUpdate {
                        git_hash: None,
                        source_version: VersionRange::default(),
                        platforms: ["x86_64".into()].into(),
                    },
                )
            })))
            .unwrap()
            .into();
        assert!(large.len() > 1024);
        let (_sender, receiver) = watch::channel(large.clone());
        let (url, _server) = serve(receiver).await;
        let (mut socket, _) = connect_async(&url).await.unwrap();
        assert_eq!(
            receive(&mut socket).await.into_text().unwrap().as_str(),
            large.as_str()
        );
        socket
            .send(ClientMessage::Text("x".repeat(2048).into()))
            .await
            .unwrap();
        assert!(matches!(
            timeout(Duration::from_secs(2), socket.next()).await,
            Ok(None | Some(Err(_)) | Some(Ok(ClientMessage::Close(_))))
        ));
    }

    #[tokio::test]
    async fn heartbeat_keeps_responsive_clients_and_drops_missing_pongs() {
        let (_sender, receiver) = watch::channel("{}".into());
        let (url, _server) = serve(receiver).await;
        let (mut socket, _) = connect_async(&url).await.unwrap();
        receive(&mut socket).await;
        for _ in 0..13 {
            tokio::time::pause();
            tokio::time::advance(Duration::from_secs(31)).await;
            tokio::time::resume();
            assert!(matches!(receive(&mut socket).await, ClientMessage::Ping(_)));
            socket.flush().await.unwrap();
            socket
                .send(ClientMessage::Ping(vec![1].into()))
                .await
                .unwrap();
            assert!(matches!(receive(&mut socket).await, ClientMessage::Pong(_)));
        }
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(31)).await;
        tokio::time::resume();
        assert!(matches!(receive(&mut socket).await, ClientMessage::Ping(_)));
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(301)).await;
        tokio::time::resume();
        assert!(matches!(
            receive(&mut socket).await,
            ClientMessage::Close(_)
        ));
    }
}
