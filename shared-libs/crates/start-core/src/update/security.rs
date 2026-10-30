use std::collections::BTreeMap;
use std::time::Duration;

use exver::Version;
use futures::{SinkExt, StreamExt};
use patch_db::{Dump, Revision};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use url::Url;

use crate::context::RpcContext;
use crate::db::model::{Database, DatabaseModel};
use crate::notifications::{NotificationLevel, OsUpdateAvailable, notify};
use crate::prelude::*;
use crate::registry::os::security::SecurityFeedState;
use crate::s9pk::git_hash::GitHash;

pub type SecurityNotifications = BTreeMap<Url, BTreeMap<Version, Option<GitHash>>>;

pub async fn watch_updates(
    db: TypedPatchDb<Database>,
    context: impl Fn() -> Option<RpcContext> + Send + Sync,
) {
    let mut registry = db
        .watch("/public/ui/startosRegistry".parse().unwrap())
        .await;
    let mut backoff = 30;
    tokio::time::sleep(Duration::from_secs(rand::random_range(0..30))).await;
    loop {
        let selected = registry
            .peek_and_mark_seen()
            .with_kind(ErrorKind::Database)
            .and_then(from_value::<Url>);
        if let Ok(selected) = selected {
            tokio::select! {
                result = subscribe(&db, &selected, &context, &mut backoff) => {
                    if let Err(error) = result {
                        tracing::debug!(?error, registry = %selected, "Security update subscription disconnected");
                    }
                }
                result = registry.changed() => {
                    if result.is_err() { return; }
                    backoff = 30;
                    continue;
                }
            }
        }
        let delay = Duration::from_secs(rand::random_range(backoff / 2..=backoff));
        backoff = (backoff * 2).min(300);
        tokio::select! {
            _ = tokio::time::sleep(delay) => (),
            result = registry.changed() => {
                if result.is_err() { return; }
                backoff = 30;
            }
        }
    }
}

fn subscription_url(registry: &Url) -> Result<Url, Error> {
    let mut url = registry.join("os/security")?;
    let scheme = match url.scheme() {
        "https" => "wss",
        "http" => "ws",
        _ => {
            return Err(Error::new(
                eyre!("unsupported registry scheme"),
                ErrorKind::ParseUrl,
            ));
        }
    };
    url.set_scheme(scheme)
        .map_err(|_| Error::new(eyre!("invalid registry scheme"), ErrorKind::ParseUrl))?;
    Ok(url)
}

async fn subscribe(
    db: &TypedPatchDb<Database>,
    registry: &Url,
    context: &(impl Fn() -> Option<RpcContext> + Sync),
    backoff: &mut u64,
) -> Result<(), Error> {
    let url = subscription_url(registry)?;
    let config = WebSocketConfig::default()
        .read_buffer_size(4096)
        .write_buffer_size(0)
        .max_write_buffer_size(4096)
        .max_message_size(Some(1024 * 1024))
        .max_frame_size(Some(1024 * 1024));
    let socket = {
        let Some(context) = context() else {
            return Ok(());
        };
        tokio::time::timeout(
            Duration::from_secs(30),
            context.client.websocket(url, config),
        )
        .await
        .with_kind(ErrorKind::Network)??
    };
    receive_updates(socket, db, registry, backoff).await
}

async fn receive_updates<S: AsyncRead + AsyncWrite + Unpin>(
    mut socket: WebSocketStream<S>,
    db: &TypedPatchDb<Database>,
    registry: &Url,
    backoff: &mut u64,
) -> Result<(), Error> {
    let mut state = None;
    loop {
        let message = tokio::time::timeout(Duration::from_secs(120), socket.next())
            .await
            .with_kind(ErrorKind::Network)?
            .transpose()
            .with_kind(ErrorKind::Network)?;
        match message {
            Some(Message::Text(text)) => {
                let value = match &mut state {
                    None => &*state.insert(
                        serde_json::from_str::<Dump>(&text)
                            .with_kind(ErrorKind::Deserialization)?
                            .value,
                    ),
                    Some(value) => {
                        let revision: Revision =
                            serde_json::from_str(&text).with_kind(ErrorKind::Deserialization)?;
                        patch_db::json_patch::patch(value, &revision.patch)
                            .with_kind(ErrorKind::Database)?;
                        &*value
                    }
                };
                let feed: SecurityFeedState = from_value(value.clone())?;
                db.mutate(|db| {
                    notify_security_updates(
                        db,
                        registry,
                        &feed,
                        crate::version::COMMIT_HASH.trim(),
                        &crate::PLATFORM,
                    )
                })
                .await
                .result?;
                *backoff = 30;
            }
            Some(Message::Ping(_)) => {
                tokio::time::timeout(Duration::from_secs(30), socket.flush())
                    .await
                    .with_kind(ErrorKind::Network)?
                    .with_kind(ErrorKind::Network)?;
            }
            Some(Message::Close(_)) | None => return Ok(()),
            _ => (),
        }
    }
}

fn notify_security_updates(
    db: &mut DatabaseModel,
    registry: &Url,
    feed: &SecurityFeedState,
    installed_hash: &str,
    platform: &str,
) -> Result<(), Error> {
    let installed = db.as_public().as_server_info().as_version().de()?;
    let mut notified = db.as_private().as_os_security_notifications().de()?;
    let releases = notified.entry(registry.clone()).or_default();
    let registry_name = feed
        .name
        .clone()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| t!("update.security.selected-registry").to_string());
    for (version, update) in &feed.updates {
        if !update.platforms.contains(platform)
            || !installed.satisfies(&update.source_version)
            || !super::is_os_update(
                version,
                update.git_hash.as_ref(),
                &installed,
                installed_hash,
            )
            || releases.get(version) == Some(&update.git_hash)
        {
            continue;
        }
        notify(
            db,
            None,
            NotificationLevel::Warning,
            t!("update.security.available-title").to_string(),
            t!(
                "update.security.available-message",
                version = version.to_string(),
                registry = registry_name.as_str()
            )
            .to_string(),
            OsUpdateAvailable {},
        )?;
        releases.insert(version.clone(), update.git_hash.clone());
    }
    db.as_private_mut()
        .as_os_security_notifications_mut()
        .ser(&notified)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use imbl_value::json;
    use patch_db::ModelExt;

    use super::*;
    use crate::registry::os::security::SecurityUpdate;

    fn database() -> DatabaseModel {
        let mut db = Model::from_value(json!({
            "public": { "serverInfo": { "version": "0.4.0.5", "unreadNotificationCount": 0 } },
            "private": { "notifications": {}, "osSecurityNotifications": {} }
        }));
        db.as_private_mut()
            .as_push_mut()
            .ser(&crate::notifications::push::PushState::default())
            .unwrap();
        db
    }

    fn catalog(
        version: &str,
        hash: Option<&str>,
        source: &str,
        platform: &str,
    ) -> SecurityFeedState {
        SecurityFeedState {
            name: Some("Test Registry".into()),
            updates: [(
                version.parse().unwrap(),
                SecurityUpdate {
                    git_hash: hash.map(|h| h.parse().unwrap()),
                    source_version: source.parse().unwrap(),
                    platforms: BTreeSet::from([platform.into()]),
                },
            )]
            .into_iter()
            .collect(),
        }
    }

    fn count(db: &DatabaseModel) -> usize {
        db.as_private().as_notifications().de().unwrap().0.len()
    }

    #[test]
    fn registry_names_are_display_only_and_renames_do_not_repeat_notifications() {
        let mut db = database();
        let registry: Url = "https://registry.test/".parse().unwrap();
        let other: Url = "https://other-registry.test/".parse().unwrap();
        let mut feed = catalog("0.4.0.6", Some("new-build"), "*", "x86_64");
        notify_security_updates(&mut db, &registry, &feed, "installed-build", "x86_64").unwrap();
        let notifications = db.as_private().as_notifications().de().unwrap();
        let message = &notifications.0.values().next().unwrap().message;
        assert!(message.contains("Test Registry"));
        assert!(!message.contains(registry.as_str()));
        notify_security_updates(&mut db, &other, &feed, "installed-build", "x86_64").unwrap();
        assert_eq!(count(&db), 2);
        feed.name = Some("Renamed Registry".into());
        notify_security_updates(&mut db, &registry, &feed, "installed-build", "x86_64").unwrap();
        assert_eq!(count(&db), 2);
        feed.updates.values_mut().next().unwrap().git_hash =
            Some("replacement-build".parse().unwrap());
        notify_security_updates(&mut db, &registry, &feed, "installed-build", "x86_64").unwrap();
        let notifications = db.as_private().as_notifications().de().unwrap();
        assert_eq!(notifications.0.len(), 3);
        assert!(
            notifications
                .0
                .values()
                .last()
                .unwrap()
                .message
                .contains("Renamed Registry")
        );
    }

    #[test]
    fn unnamed_registries_use_a_localized_label_without_exposing_the_url() {
        let registry: Url = "https://registry.test/".parse().unwrap();
        for name in [None, Some(String::new())] {
            let mut db = database();
            let mut feed = catalog("0.4.0.6", Some("new-build"), "*", "x86_64");
            feed.name = name;
            notify_security_updates(&mut db, &registry, &feed, "installed-build", "x86_64")
                .unwrap();
            let notifications = db.as_private().as_notifications().de().unwrap();
            let message = &notifications.0.values().next().unwrap().message;
            assert!(message.contains(&t!("update.security.selected-registry").to_string()));
            assert!(!message.contains(registry.as_str()));
        }
    }

    #[test]
    fn notifies_once_across_reconnect_restart_and_notification_deletion() {
        let mut db = database();
        let registry: Url = "https://registry.test".parse().unwrap();
        let update = catalog("0.4.0.6", Some("new-build"), ">=0.4.0", "x86_64");
        notify_security_updates(&mut db, &registry, &update, "installed-build", "x86_64").unwrap();
        assert_eq!(count(&db), 1);
        assert_eq!(
            db.as_public()
                .as_server_info()
                .as_unread_notification_count()
                .de()
                .unwrap(),
            1
        );
        let notifications = db.as_private().as_notifications().de().unwrap();
        let notification = notifications.0.values().next().unwrap();
        assert_eq!(
            notification.code,
            <OsUpdateAvailable as crate::notifications::NotificationType>::CODE
        );
        assert_eq!(notification.data, json!({ "kind": "os-update-available" }));
        db = Model::from_value(db.as_value().clone());
        notify_security_updates(&mut db, &registry, &update, "installed-build", "x86_64").unwrap();
        assert_eq!(count(&db), 1);
        db.as_private_mut()
            .as_notifications_mut()
            .ser(&crate::notifications::Notifications::new())
            .unwrap();
        notify_security_updates(&mut db, &registry, &update, "installed-build", "x86_64").unwrap();
        assert_eq!(count(&db), 0);
    }

    #[test]
    fn security_notifications_enqueue_browser_push_once() {
        use base64::Engine;
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use web_push_native::p256;
        use web_push_native::p256::elliptic_curve::sec1::ToEncodedPoint;

        use crate::notifications::push::{PushKeys, PushState, PushSubscription};

        let mut db = database();
        let key = p256::SecretKey::from_slice(&[19; 32]).unwrap();
        let subscription = PushSubscription {
            endpoint: "https://push.example.com/subscription".into(),
            keys: PushKeys {
                p256dh: URL_SAFE_NO_PAD.encode(key.public_key().to_encoded_point(false).as_bytes()),
                auth: URL_SAFE_NO_PAD.encode([29u8; 16]),
            },
        };
        let mut state = to_value(&PushState::default()).unwrap();
        state["vapidPrivateKey"] =
            to_value(&josekit::jwk::Jwk::generate_ec_key(josekit::jwk::P_256).unwrap()).unwrap();
        state["subscriptions"] =
            json!([{ "id": crate::util::new_guid().to_string(), "subscription": subscription }]);
        db.as_private_mut()
            .as_push_mut()
            .ser(&from_value::<PushState>(state).unwrap())
            .unwrap();
        let registry: Url = "https://registry.test/".parse().unwrap();
        let update = catalog("0.4.0.6", Some("new-build"), "*", "x86_64");
        notify_security_updates(&mut db, &registry, &update, "installed-build", "x86_64").unwrap();
        db = Model::from_value(db.as_value().clone());
        notify_security_updates(&mut db, &registry, &update, "installed-build", "x86_64").unwrap();
        assert_eq!(count(&db), 1);
        let pending = db.as_private().as_push().as_value()["pending"]
            .as_array()
            .unwrap();
        assert_eq!(pending.len(), 1);
        let payload: serde_json::Value =
            serde_json::from_str(pending[0]["payload"].as_str().unwrap()).unwrap();
        let notification = db
            .as_private()
            .as_notifications()
            .de()
            .unwrap()
            .0
            .into_values()
            .next()
            .unwrap();
        assert_eq!(
            payload["notification"]["title"].as_str(),
            Some(notification.title.as_str())
        );
        assert_eq!(
            payload["notification"]["body"].as_str(),
            Some(notification.message.as_str())
        );
    }

    #[test]
    fn filters_installed_older_incompatible_and_other_platform_updates() {
        let registry: Url = "https://registry.test".parse().unwrap();
        for update in [
            catalog("0.4.0.4", Some("older-build"), "*", "x86_64"),
            catalog("0.4.0.5", Some("installed-build"), "*", "x86_64"),
            catalog("0.4.0.5", None, "*", "x86_64"),
            catalog("0.4.0.6", Some("new-build"), ">=0.5.0", "x86_64"),
            catalog("0.4.0.6", Some("new-build"), "*", "aarch64"),
        ] {
            let mut db = database();
            notify_security_updates(&mut db, &registry, &update, "installed-build", "x86_64")
                .unwrap();
            assert_eq!(count(&db), 0);
        }
    }

    #[test]
    fn replacement_builds_and_new_registry_subscriptions_get_notified() {
        let mut db = database();
        let registry: Url = "https://registry.test".parse().unwrap();
        let first = catalog("0.4.0.5", Some("rc-one"), "*", "x86_64");
        let second = catalog("0.4.0.5", Some("rc-two"), "*", "x86_64");
        notify_security_updates(&mut db, &registry, &first, "installed-build", "x86_64").unwrap();
        notify_security_updates(&mut db, &registry, &second, "installed-build", "x86_64").unwrap();
        notify_security_updates(&mut db, &registry, &second, "installed-build", "x86_64").unwrap();
        assert_eq!(count(&db), 2);
        let other: Url = "https://other.test".parse().unwrap();
        notify_security_updates(&mut db, &other, &second, "installed-build", "x86_64").unwrap();
        assert_eq!(count(&db), 3);
    }

    #[tokio::test]
    async fn subscription_applies_revisions_answers_heartbeats_and_deduplicates_reconnects() {
        use patch_db::json_ptr::ROOT;

        let temp = tempfile::tempdir().unwrap();
        let raw = patch_db::PatchDb::open(temp.path().join("db"))
            .await
            .unwrap();
        let db = TypedPatchDb::<Database>::load_unchecked(raw);
        db.put(&ROOT, database().as_value()).await.unwrap();
        let registry: Url = "https://registry.test/".parse().unwrap();
        let feed = patch_db::PatchDb::in_memory();
        feed.put(
            &ROOT,
            &catalog("0.4.0.6", Some("published-build"), "*", &crate::PLATFORM),
        )
        .await
        .unwrap();
        for attempt in 0..2 {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("ws://{}", listener.local_addr().unwrap());
            let feed = feed.clone();
            let server = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
                let dump = feed.dump(&ROOT).await;
                socket
                    .send(Message::Text(serde_json::to_string(&dump).unwrap().into()))
                    .await
                    .unwrap();
                if attempt == 0 {
                    let replacement =
                        catalog("0.4.0.6", Some("replacement-build"), "*", &crate::PLATFORM);
                    for value in [&replacement, &SecurityFeedState::default(), &replacement] {
                        let revision = feed.put(&ROOT, value).await.unwrap().unwrap();
                        socket
                            .send(Message::Text(
                                serde_json::to_string(&revision).unwrap().into(),
                            ))
                            .await
                            .unwrap();
                    }
                }
                socket
                    .send(Message::Ping(vec![1, 2, 3].into()))
                    .await
                    .unwrap();
                assert_eq!(
                    socket.next().await.unwrap().unwrap(),
                    Message::Pong(vec![1, 2, 3].into())
                );
                socket.send(Message::Close(None)).await.unwrap();
            });
            let (socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
            let mut backoff = 300;
            tokio::time::timeout(
                Duration::from_secs(5),
                receive_updates(socket, &db, &registry, &mut backoff),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(backoff, 30);
            server.await.unwrap();
        }
        assert_eq!(count(&db.peek().await), 2);
    }

    #[test]
    fn subscription_url_preserves_registry_prefix_and_uses_tls() {
        for (registry, expected) in [
            ("https://registry.test/", "wss://registry.test/os/security"),
            (
                "http://registry.test:5959/prefix/",
                "ws://registry.test:5959/prefix/os/security",
            ),
        ] {
            assert_eq!(
                subscription_url(&registry.parse().unwrap())
                    .unwrap()
                    .as_str(),
                expected
            );
        }
    }
}
