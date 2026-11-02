use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroUsize;
use std::time::Duration;

use axum::extract::ws::{Message, Utf8Bytes, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use exver::{Version, VersionRange};
use futures::SinkExt;
use imbl_value::InternedString;
use patch_db::PatchDb;
use patch_db::json_ptr::ROOT;
use serde::{Deserialize, Serialize};
use tokio::time::timeout;

use crate::prelude::*;
use crate::registry::os::index::OsIndex;
use crate::registry::{FullIndex, RegistryDatabase};
use crate::s9pk::git_hash::GitHash;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::net::WebSocket;

/// Security releases with an available SquashFS image, keyed by version.
pub type SecurityCatalog = BTreeMap<Version, SecurityUpdate>;

/// Registry display name and available OS security releases.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityFeedState {
    pub name: Option<String>,
    pub updates: SecurityCatalog,
}

/// Compatibility and platform availability for an OS security release.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityUpdate {
    pub git_hash: Option<GitHash>,
    pub source_version: VersionRange,
    pub platforms: BTreeSet<InternedString>,
}

const SEND_TIMEOUT: Duration = Duration::from_secs(15);
const SUBSCRIPTION_CAPACITY: NonZeroUsize = NonZeroUsize::new(32).unwrap();

pub(crate) struct SecurityFeed {
    pub db: PatchDb,
    _sync: NonDetachingJoinHandle<()>,
}

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

fn state(index: Model<FullIndex>) -> Result<SecurityFeedState, Error> {
    Ok(SecurityFeedState {
        name: index.as_name().de()?,
        updates: catalog(index.as_os().de()?),
    })
}

pub(crate) async fn init(db: &TypedPatchDb<RegistryDatabase>) -> Result<SecurityFeed, Error> {
    let mirror = PatchDb::in_memory();
    let mut os = db.watch("/index/os".parse().unwrap()).await;
    let mut name = db.watch("/index/name".parse().unwrap()).await;
    os.peek_and_mark_seen()?;
    name.peek_and_mark_seen()?;
    mirror
        .put(&ROOT, &state(db.peek().await.into_index())?)
        .await?;
    let updates = mirror.clone();
    let source = db.clone();
    let task = tokio::spawn(async move {
        let result: Result<(), Error> = async {
            loop {
                tokio::select! {
                    result = os.changed() => result?,
                    result = name.changed() => result?,
                }
                updates
                    .put(&ROOT, &state(source.peek().await.into_index())?)
                    .await?;
            }
        }
        .await;
        if let Err(error) = result {
            tracing::error!(%error, "Registry security catalog watch stopped");
        }
    });
    Ok(SecurityFeed {
        db: mirror,
        _sync: task.into(),
    })
}

pub(crate) fn upgrade(ws: WebSocketUpgrade, db: Option<PatchDb>) -> Response {
    let Some(db) = db else {
        return StatusCode::FORBIDDEN.into_response();
    };
    ws.read_buffer_size(2048)
        .write_buffer_size(0)
        .max_message_size(1024)
        .max_frame_size(1024)
        .on_upgrade(move |socket| subscribe(WebSocket::new(socket), db))
}

async fn send_json(socket: &mut WebSocket, json: Bytes) -> Result<(), Error> {
    let text = Utf8Bytes::try_from(json).with_kind(ErrorKind::Serialization)?;
    timeout(SEND_TIMEOUT, socket.send(Message::Text(text)))
        .await
        .with_kind(ErrorKind::Network)?
        .with_kind(ErrorKind::Network)
}

async fn subscribe(mut socket: WebSocket, db: PatchDb) {
    let Ok((dump, mut revisions)) = db
        .dump_and_sub_json(ROOT.to_owned(), SUBSCRIPTION_CAPACITY)
        .await
    else {
        return;
    };
    if send_json(&mut socket, dump).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            revision = revisions.recv() => {
                let Ok(Some(revision)) = revision else { break };
                if send_json(&mut socket, revision).await.is_err() {
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

    async fn changed(receiver: &mut patch_db::DbWatch) -> SecurityFeedState {
        timeout(Duration::from_secs(2), receiver.changed())
            .await
            .unwrap()
            .unwrap();
        from_value(receiver.peek_and_mark_seen().unwrap()).unwrap()
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
    async fn disabled_feed_refuses_upgrades_without_changing_release_metadata() {
        let (_dir, db) = database().await;
        put_index(&db, &index(release())).await;
        let (url, _server) = serve(None).await;
        for _ in 0..2 {
            let error = connect_async(&url).await.unwrap_err();
            let tokio_tungstenite::tungstenite::Error::Http(response) = error else {
                panic!("expected refusal")
            };
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
            assert!(response.body().as_ref().is_none_or(Vec::is_empty));
        }
        let mut info = release();
        info.git_hash = Some("replacement-build".parse().unwrap());
        put_index(&db, &index(info)).await;
        let stored = db.peek().await.into_index().into_os().de().unwrap();
        let published = stored.versions.0.into_values().next().unwrap();
        assert!(published.security);
        assert_eq!(published.git_hash.as_deref(), Some("replacement-build"));
        let enabled = init(&db).await.unwrap();
        assert_eq!(
            enabled
                .db
                .get::<SecurityFeedState, _, _>(&ROOT)
                .await
                .unwrap()
                .updates
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn mirror_tracks_metadata_and_availability_without_unrelated_revisions() {
        let (_dir, db) = database().await;
        put_index(&db, &index(release())).await;
        let feed = init(&db).await.unwrap();
        let mirror = &feed.db;
        let mut receiver = mirror.watch(ROOT.to_owned()).await;
        let initial = receiver.peek_and_mark_seen().unwrap();
        assert_eq!(
            from_value::<SecurityFeedState>(initial.clone())
                .unwrap()
                .updates
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
        let renamed = changed(&mut receiver).await;
        assert_eq!(renamed.name.as_deref(), Some("Renamed"));
        assert_eq!(
            renamed.updates,
            from_value::<SecurityFeedState>(initial).unwrap().updates
        );
        let initial = to_value(&renamed).unwrap();
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
        assert_eq!(mirror.sequence().await, 2);
        assert_eq!(mirror.clone().dump(&ROOT).await.value, initial);

        let mut info = release();
        info.git_hash = Some("def456".parse().unwrap());
        put_index(&db, &index(info)).await;
        assert_eq!(
            changed(&mut receiver)
                .await
                .updates
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
        let update = changed(&mut receiver)
            .await
            .updates
            .into_values()
            .next()
            .unwrap();
        assert_eq!(update.source_version, ">=0.4.0".parse().unwrap());
        assert_eq!(update.platforms, ["aarch64".into(), "x86_64".into()].into());

        let mut info = release();
        info.squashfs.clear();
        put_index(&db, &index(info)).await;
        assert!(changed(&mut receiver).await.updates.is_empty());
        put_index(&db, &index(release())).await;
        assert_eq!(changed(&mut receiver).await.updates.len(), 1);
        let mut info = release();
        info.security = false;
        put_index(&db, &index(info)).await;
        assert!(changed(&mut receiver).await.updates.is_empty());
    }

    type ClientSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

    async fn serve(db: Option<PatchDb>) -> (String, NonDetachingJoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/os/security", listener.local_addr().unwrap());
        let router = Router::new().route(
            "/os/security",
            get(move |ws: WebSocketUpgrade| {
                let db = db.clone();
                async move { upgrade(ws, db) }
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
    async fn raw_websocket_sends_atomic_dump_then_revisions_and_reconnects() {
        let (_dir, db) = database().await;
        let feed = init(&db).await.unwrap();
        let (url, _server) = serve(Some(feed.db.clone())).await;
        let (mut first, _) = connect_async(&url).await.unwrap();
        let mut state: patch_db::Dump =
            serde_json::from_str(&receive(&mut first).await.into_text().unwrap()).unwrap();
        assert_eq!(
            state.value,
            imbl_value::json!({"name": null, "updates": {}})
        );
        let (mut second, _) = connect_async(&url).await.unwrap();
        let second_dump: patch_db::Dump =
            serde_json::from_str(&receive(&mut second).await.into_text().unwrap()).unwrap();
        assert_eq!(second_dump.id, state.id);
        put_index(&db, &index(release())).await;
        let first_update = receive(&mut first).await.into_text().unwrap();
        assert_eq!(
            first_update,
            receive(&mut second).await.into_text().unwrap()
        );
        let revision: patch_db::Revision = serde_json::from_str(&first_update).unwrap();
        assert_eq!(revision.id, state.id + 1);
        patch_db::json_patch::patch(&mut state.value, &revision.patch).unwrap();
        assert_eq!(
            state.value["updates"]["0.4.1"]["gitHash"],
            imbl_value::json!("abc123")
        );
        let mut info = release();
        info.git_hash = Some("replacement-build".parse().unwrap());
        put_index(&db, &index(info)).await;
        let update = receive(&mut first).await.into_text().unwrap();
        assert_eq!(update, receive(&mut second).await.into_text().unwrap());
        let revision: patch_db::Revision = serde_json::from_str(&update).unwrap();
        assert_eq!(
            serde_json::to_value(&revision.patch).unwrap(),
            serde_json::json!([
                { "op": "replace", "path": "/updates/0.4.1/gitHash", "value": "replacement-build" }
            ])
        );
        patch_db::json_patch::patch(&mut state.value, &revision.patch).unwrap();
        first.close(None).await.unwrap();
        assert!(matches!(receive(&mut first).await, ClientMessage::Close(_)));
        let (mut reconnected, _) = connect_async(&url).await.unwrap();
        let dump: patch_db::Dump =
            serde_json::from_str(&receive(&mut reconnected).await.into_text().unwrap()).unwrap();
        assert_eq!(dump.id, revision.id);
        assert_eq!(dump.value, state.value);
        put_index(&db, &OsIndex::default()).await;
        let removed = receive(&mut second).await.into_text().unwrap();
        assert_eq!(
            removed,
            receive(&mut reconnected).await.into_text().unwrap()
        );
        let revision: patch_db::Revision = serde_json::from_str(&removed).unwrap();
        patch_db::json_patch::patch(&mut state.value, &revision.patch).unwrap();
        assert_eq!(
            state.value,
            imbl_value::json!({"name": null, "updates": {}})
        );
    }

    #[tokio::test]
    async fn lagging_socket_closes_and_a_new_connection_gets_a_fresh_dump() {
        let mirror = PatchDb::in_memory();
        mirror
            .put(&ROOT, &SecurityFeedState::default())
            .await
            .unwrap();
        let (url, _server) = serve(Some(mirror.clone())).await;
        let (mut socket, _) = connect_async(&url).await.unwrap();
        receive(&mut socket).await;
        for i in 0..=SUBSCRIPTION_CAPACITY.get() {
            let mut info = release();
            info.git_hash = Some(format!("build-{i}").parse().unwrap());
            mirror
                .put(
                    &ROOT,
                    &SecurityFeedState {
                        name: None,
                        updates: catalog(index(info)),
                    },
                )
                .await
                .unwrap();
        }
        assert!(matches!(
            receive(&mut socket).await,
            ClientMessage::Close(_)
        ));
        let (mut reconnected, _) = connect_async(&url).await.unwrap();
        let dump: patch_db::Dump =
            serde_json::from_str(&receive(&mut reconnected).await.into_text().unwrap()).unwrap();
        assert_eq!(
            dump.value["updates"]["0.4.1"]["gitHash"],
            imbl_value::json!("build-32")
        );
        mirror
            .put(&ROOT, &SecurityFeedState::default())
            .await
            .unwrap();
        let revision: patch_db::Revision =
            serde_json::from_str(&receive(&mut reconnected).await.into_text().unwrap()).unwrap();
        assert_eq!(revision.id, dump.id + 1);
    }

    #[tokio::test]
    async fn outbound_dump_exceeds_inbound_limit() {
        let large = SecurityFeedState {
            name: Some("Large Registry".into()),
            updates: SecurityCatalog::from_iter((0..100).map(|i| {
                (
                    format!("0.4.{i}").parse().unwrap(),
                    SecurityUpdate {
                        git_hash: None,
                        source_version: VersionRange::default(),
                        platforms: ["x86_64".into()].into(),
                    },
                )
            })),
        };
        assert!(serde_json::to_string(&large).unwrap().len() > 1024);
        let mirror = PatchDb::in_memory();
        mirror.put(&ROOT, &large).await.unwrap();
        let (url, _server) = serve(Some(mirror)).await;
        let (mut socket, _) = connect_async(&url).await.unwrap();
        let dump: patch_db::Dump =
            serde_json::from_str(&receive(&mut socket).await.into_text().unwrap()).unwrap();
        assert_eq!(dump.value, to_value(&large).unwrap());
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
        let mirror = PatchDb::in_memory();
        mirror
            .put(&ROOT, &SecurityFeedState::default())
            .await
            .unwrap();
        let (url, _server) = serve(Some(mirror)).await;
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
