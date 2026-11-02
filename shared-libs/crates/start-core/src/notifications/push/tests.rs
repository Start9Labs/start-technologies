use std::sync::{Arc, Mutex};

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use openssl::bn::BigNumContext;
use openssl::ec::{EcGroup, EcKey, EcPoint};
use openssl::ecdsa::EcdsaSig;
use openssl::nid::Nid;
use openssl::sha::sha256;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::sync::Notify;

use super::*;
use crate::notifications::{NotificationLevel, notify};

fn subscription(endpoint: &str) -> (PushSubscription, p256::SecretKey, Auth) {
    let key = p256::SecretKey::random(&mut p256::elliptic_curve::rand_core::OsRng);
    let auth: Auth = rand::random::<[u8; 16]>().into();
    (
        PushSubscription {
            endpoint: endpoint.into(),
            keys: PushKeys {
                p256dh: URL_SAFE_NO_PAD.encode(key.public_key().to_encoded_point(false).as_bytes()),
                auth: URL_SAFE_NO_PAD.encode(auth),
            },
        },
        key,
        auth,
    )
}

async fn database() -> (tempfile::TempDir, TypedPatchDb<Database>) {
    let temp = tempfile::tempdir().unwrap();
    let db = TypedPatchDb::load_unchecked(PatchDb::open(temp.path().join("db")).await.unwrap());
    db.put(
        &patch_db::json_ptr::ROOT,
        &imbl_value::json!({
            "private": {"notifications": {}},
            "public": {"serverInfo": {"unreadNotificationCount": 0}}
        }),
    )
    .await
    .unwrap();
    db.put(&"/private/push".parse().unwrap(), &PushState::default())
        .await
        .unwrap();
    (temp, db)
}

async fn store(db: &TypedPatchDb<Database>, subscription: PushSubscription) {
    db.mutate(|db| {
        mutate_state(db, |state| {
            state.private_key()?;
            state.upsert(subscription);
            Ok(())
        })
    })
    .await
    .result
    .unwrap();
}

async fn state(db: &TypedPatchDb<Database>) -> PushState {
    db.peek().await.as_private().as_push().de().unwrap()
}

#[derive(Clone)]
struct Receiver {
    status: Arc<Mutex<StatusCode>>,
    received: Arc<Mutex<Vec<(HeaderMap, Vec<u8>)>>>,
    started: Arc<Notify>,
    release: Option<Arc<Notify>>,
}

async fn receive(
    State(receiver): State<Receiver>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::http::Response<Body> {
    receiver
        .received
        .lock()
        .unwrap()
        .push((headers, body.to_vec()));
    receiver.started.notify_one();
    if let Some(release) = &receiver.release {
        release.notified().await;
    }
    axum::http::Response::builder()
        .status(*receiver.status.lock().unwrap())
        .body(Body::empty())
        .unwrap()
}

async fn receiver(
    status: StatusCode,
    pause: bool,
) -> (
    String,
    Receiver,
    crate::util::future::NonDetachingJoinHandle<()>,
) {
    let receiver = Receiver {
        status: Arc::new(Mutex::new(status)),
        received: Default::default(),
        started: Arc::new(Notify::new()),
        release: pause.then(|| Arc::new(Notify::new())),
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/private-token", listener.local_addr().unwrap());
    let app = axum::Router::new()
        .route("/private-token", post(receive))
        .with_state(receiver.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    })
    .into();
    (endpoint, receiver, server)
}

fn verify_wire(
    headers: &HeaderMap,
    body: &[u8],
    subscription: &PushSubscription,
    key: &p256::SecretKey,
    auth: &Auth,
    public_key: &str,
) -> serde_json::Value {
    assert_eq!(headers["content-encoding"], "aes128gcm");
    assert_eq!(headers["content-type"], "application/octet-stream");
    assert!(headers["ttl"].to_str().unwrap().parse::<u32>().unwrap() <= MAX_AGE as u32);
    assert!(body.len() <= 4096);
    let authorization = headers["authorization"].to_str().unwrap();
    let (jwt, public) = authorization
        .strip_prefix("vapid t=")
        .unwrap()
        .split_once(", k=")
        .unwrap();
    assert_eq!(public, public_key);
    let parts: Vec<_> = jwt.split('.').collect();
    let claims: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
    assert_eq!(
        claims["aud"],
        url::Url::parse(&subscription.endpoint)
            .unwrap()
            .origin()
            .ascii_serialization()
    );
    assert_eq!(claims["sub"], "mailto:support@start9.com");
    let expiry = claims["exp"].as_i64().unwrap();
    assert!(expiry > Utc::now().timestamp());
    assert!(expiry <= Utc::now().timestamp() + MAX_AGE);
    let signature = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
    let signature = EcdsaSig::from_private_components(
        openssl::bn::BigNum::from_slice(&signature[..32]).unwrap(),
        openssl::bn::BigNum::from_slice(&signature[32..]).unwrap(),
    )
    .unwrap();
    let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).unwrap();
    let mut ctx = BigNumContext::new().unwrap();
    let point =
        EcPoint::from_bytes(&group, &URL_SAFE_NO_PAD.decode(public).unwrap(), &mut ctx).unwrap();
    let public = EcKey::from_public_key(&group, &point).unwrap();
    assert!(
        signature
            .verify(
                &sha256(format!("{}.{}", parts[0], parts[1]).as_bytes()),
                &public
            )
            .unwrap()
    );
    let plaintext = web_push_native::decrypt(body.to_vec(), key, auth).unwrap();
    serde_json::from_slice(&plaintext).unwrap()
}

#[tokio::test]
async fn encrypted_loopback_post_and_vapid_signature() {
    let (endpoint, receiver, _server) = receiver(StatusCode::CREATED, false).await;
    let (mut subscription, key, auth) = subscription(&endpoint);
    validate(&mut subscription).unwrap();
    let mut state = PushState::default();
    let public = state.public_key().unwrap();
    let persisted = serde_json::to_string(&state).unwrap();
    let mut restored: PushState = serde_json::from_str(&persisted).unwrap();
    assert_eq!(public, restored.public_key().unwrap());
    let stored = StoredSubscription {
        id: new_guid().to_string(),
        subscription: subscription.clone(),
    };
    let payload = payload("Title", "Body").unwrap();
    assert_eq!(
        deliver(
            &Client::new(),
            &stored,
            state.vapid_private_key.as_ref(),
            &payload,
            MAX_AGE as u32
        )
        .await,
        Delivery::Delivered
    );
    let received = receiver.received.lock().unwrap();
    let (headers, body) = &received[0];
    let plaintext = verify_wire(headers, body, &subscription, &key, &auth, &public);
    assert_eq!(
        plaintext,
        serde_json::from_str::<serde_json::Value>(&payload).unwrap()
    );
    assert_eq!(
        plaintext["notification"]["data"]["onActionClick"]["default"]["url"],
        "notifications"
    );
    assert_eq!(
        plaintext["notification"]["data"]["onActionClick"]["default"]["operation"],
        "navigateLastFocusedOrOpen"
    );
    assert_eq!(plaintext["notification"]["title"], "Title");
    assert_eq!(plaintext["notification"]["body"], "Body");
}

#[test]
fn validates_real_keys_and_permits_private_endpoints() {
    for endpoint in [
        "http://127.0.0.1:8080/push",
        "https://192.168.1.2/push",
        "https://[::1]:8081/push",
        "https://push.example/path?token=secret",
    ] {
        let (mut s, _, _) = subscription(endpoint);
        validate(&mut s).unwrap();
    }
    for endpoint in ["file:///secret", "ftp://localhost", "not a url"] {
        let (mut s, _, _) = subscription(endpoint);
        let e = validate(&mut s).unwrap_err();
        assert!(!format!("{e:?}").contains(endpoint));
    }
    let (s, _, _) = subscription("https://example.com/push");
    for keys in [
        PushKeys {
            p256dh: URL_SAFE_NO_PAD.encode([4u8; 65]),
            auth: s.keys.auth.clone(),
        },
        PushKeys {
            p256dh: s.keys.p256dh.clone(),
            auth: URL_SAFE_NO_PAD.encode([0u8; 15]),
        },
        PushKeys {
            p256dh: "secret".into(),
            auth: s.keys.auth.clone(),
        },
    ] {
        let mut s = PushSubscription { keys, ..s.clone() };
        assert!(validate(&mut s).is_err());
    }
    assert!(
        serde_json::from_value::<PushSubscription>(json!({"endpoint":"https://example.com/push"}))
            .is_err()
    );
}

#[test]
fn push_rpc_group_is_absent_from_cli() {
    use rpc_toolkit::CliBindings;

    let command = crate::notifications::notification::<crate::context::CliContext>().cli_command();
    assert!(command.find_subcommand("push").is_none());
    assert!(command.find_subcommand("list").is_some());
}

#[test]
fn empty_titles_remain_visible_and_canonical_endpoints_share_one_identity() {
    let payload: serde_json::Value = serde_json::from_str(&payload("", "body").unwrap()).unwrap();
    assert_eq!(payload["notification"]["title"], "StartOS");
    assert_eq!(payload["notification"]["body"], "body");
    assert_eq!(
        canonical_endpoint("HTTP://LOCALHOST:80/push").unwrap(),
        "http://localhost/push"
    );
    assert_eq!(
        canonical_endpoint("https://[fd00::1]:8443/push").unwrap(),
        "https://[fd00::1]:8443/push"
    );
}

#[tokio::test]
async fn durable_retry_gone_and_local_persistence() {
    let (endpoint, receiver, _server) = receiver(StatusCode::SERVICE_UNAVAILABLE, false).await;
    let (s, _, _) = subscription(&endpoint);
    let (_temp, db) = database().await;
    store(&db, s).await;
    db.mutate(|db| {
        notify(
            db,
            None,
            NotificationLevel::Info,
            "local title".into(),
            "local message".into(),
            "large-data".repeat(10000),
        )
    })
    .await
    .result
    .unwrap();
    drain(&db, &Client::new()).await.unwrap();
    let snapshot = db.peek().await;
    assert_eq!(
        snapshot
            .as_private()
            .as_notifications()
            .keys()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        snapshot
            .as_public()
            .as_server_info()
            .as_unread_notification_count()
            .de()
            .unwrap(),
        1
    );
    assert_eq!(
        snapshot
            .as_private()
            .as_notifications()
            .as_idx(&0)
            .unwrap()
            .as_message()
            .de()
            .unwrap(),
        "local message"
    );
    let queued = state(&db).await;
    assert_eq!(queued.pending.len(), 1);
    assert!(queued.pending[0].retry_at > Utc::now());
    assert!(!queued.pending[0].payload.contains("large-data"));
    drain(&db, &Client::new()).await.unwrap();
    assert_eq!(receiver.received.lock().unwrap().len(), 1);
    *receiver.status.lock().unwrap() = StatusCode::UNAUTHORIZED;
    db.mutate(|db| {
        mutate_state(db, |state| {
            state.pending[0].retry_at = Utc::now();
            Ok(())
        })
    })
    .await
    .result
    .unwrap();
    drain(&db, &Client::new()).await.unwrap();
    assert_eq!(state(&db).await.subscriptions.len(), 1);
    assert_eq!(state(&db).await.pending.len(), 1);
    *receiver.status.lock().unwrap() = StatusCode::GONE;
    db.mutate(|db| {
        mutate_state(db, |state| {
            state.pending[0].retry_at = Utc::now();
            Ok(())
        })
    })
    .await
    .result
    .unwrap();
    drain(&db, &Client::new()).await.unwrap();
    assert!(state(&db).await.subscriptions.is_empty());
    assert!(state(&db).await.pending.is_empty());
    assert_eq!(
        db.peek()
            .await
            .as_private()
            .as_notifications()
            .keys()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn network_failure_retries_without_affecting_notification() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/push", listener.local_addr().unwrap());
    drop(listener);
    let (_temp, db) = database().await;
    store(&db, subscription(&endpoint).0).await;
    db.mutate(|db| {
        notify(
            db,
            None,
            NotificationLevel::Error,
            "Error".into(),
            "detail".into(),
            (),
        )
    })
    .await
    .result
    .unwrap();
    drain(&db, &Client::new()).await.unwrap();
    assert_eq!(state(&db).await.pending.len(), 1);
    assert_eq!(
        db.peek()
            .await
            .as_private()
            .as_notifications()
            .keys()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn fanout_idempotence_and_unique_events_after_notification_deletion() {
    let (_temp, db) = database().await;
    let (s, _, _) = subscription("http://127.0.0.1:8080/a");
    store(&db, s.clone()).await;
    let id = state(&db).await.subscriptions[0].id.clone();
    store(&db, s).await;
    assert_eq!(state(&db).await.subscriptions[0].id, id);
    store(&db, subscription("http://127.0.0.1:8080/b").0).await;
    db.mutate(|db| {
        notify(
            db,
            None,
            NotificationLevel::Info,
            "one".into(),
            "body".into(),
            (),
        )
    })
    .await
    .result
    .unwrap();
    let first: serde_json::Value =
        serde_json::from_str(&state(&db).await.pending[0].payload).unwrap();
    assert_eq!(state(&db).await.pending.len(), 2);
    store(&db, subscription("http://127.0.0.1:8080/c").0).await;
    assert_eq!(state(&db).await.pending.len(), 2);
    db.mutate(|db| {
        db.as_private_mut().as_notifications_mut().remove(&0)?;
        notify(
            db,
            None,
            NotificationLevel::Info,
            "two".into(),
            "body".into(),
            (),
        )
    })
    .await
    .result
    .unwrap();
    let queued = state(&db).await;
    assert_eq!(queued.pending.len(), 5);
    let second: serde_json::Value = serde_json::from_str(&queued.pending[2].payload).unwrap();
    assert_ne!(first["notification"]["tag"], second["notification"]["tag"]);
}

#[tokio::test]
async fn in_flight_gone_cannot_remove_replacement() {
    let (endpoint, receiver, _server) = receiver(StatusCode::GONE, true).await;
    let (_temp, db) = database().await;
    store(&db, subscription(&endpoint).0).await;
    db.mutate(|db| enqueue(db, "title", "message"))
        .await
        .result
        .unwrap();
    let worker_db = db.clone();
    let worker = tokio::spawn(async move {
        drain(&worker_db, &Client::new()).await.unwrap();
    });
    receiver.started.notified().await;
    store(&db, subscription(&endpoint).0).await;
    let replacement = state(&db).await.subscriptions[0].id.clone();
    receiver.release.as_ref().unwrap().notify_one();
    worker.await.unwrap();
    assert_eq!(state(&db).await.subscriptions[0].id, replacement);
    assert!(state(&db).await.pending.is_empty());
}

#[tokio::test]
async fn expiry_queue_bound_and_unsubscribe_prune_independent_of_notifications() {
    let (_temp, db) = database().await;
    let (s, _, _) = subscription("http://127.0.0.1:8080/a");
    store(&db, s.clone()).await;
    db.mutate(|db| {
        mutate_state(db, |state| {
            for _ in 0..MAX_PENDING + 5 {
                state.enqueue("title", "message", Utc::now())?;
            }
            assert_eq!(state.pending.len(), MAX_PENDING);
            state.pending[0].created_at = Utc::now() - chrono::Duration::seconds(MAX_AGE);
            Ok(())
        })
    })
    .await
    .result
    .unwrap();
    let persisted = serde_json::to_vec(&state(&db).await).unwrap();
    let restored: PushState = serde_json::from_slice(&persisted).unwrap();
    assert_eq!(restored.pending.len(), MAX_PENDING);
    db.mutate(|db| {
        mutate_state(db, |state| {
            state.prune(Utc::now());
            assert_eq!(state.pending.len(), MAX_PENDING - 1);
            state.remove_endpoint(&s.endpoint);
            assert!(state.pending.is_empty());
            Ok(())
        })
    })
    .await
    .result
    .unwrap();
}

#[tokio::test]
async fn legacy_missing_push_field_materializes_through_typed_database_load() {
    let (_temp, db) = database().await;
    let account =
        crate::account::AccountInfo::new("test-password", std::time::SystemTime::now(), None)
            .unwrap();
    let database = Database::init(&account, false, None, None).unwrap();
    let mut value = serde_json::to_value(&database).unwrap();
    value["private"].as_object_mut().unwrap().remove("push");
    let public_before = value["public"].clone();
    db.put(&patch_db::json_ptr::ROOT, &value).await.unwrap();
    let db = TypedPatchDb::<Database>::load(PatchDb::clone(&db))
        .await
        .unwrap();
    assert!(state(&db).await.subscriptions.is_empty());
    assert!(state(&db).await.pending.is_empty());
    assert_eq!(
        public_before,
        serde_json::to_value(db.peek().await.as_public()).unwrap()
    );
    db.mutate(|db| {
        notify(
            db,
            None,
            NotificationLevel::Info,
            "title".into(),
            "message".into(),
            (),
        )
    })
    .await
    .result
    .unwrap();
    assert!(state(&db).await.pending.is_empty());
}

#[tokio::test]
async fn long_multibyte_and_escaped_payloads_encrypt_without_truncating_local_notification() {
    let (_temp, db) = database().await;
    let (endpoint, receiver, _server) = receiver(StatusCode::CREATED, false).await;
    let (s, key, auth) = subscription(&endpoint);
    store(&db, s.clone()).await;
    let title = "🦀\"\n".repeat(1000);
    let message = "通知é\u{0000}".repeat(10000);
    db.mutate(|db| {
        notify(
            db,
            None,
            NotificationLevel::Info,
            title.clone(),
            message.clone(),
            (),
        )
    })
    .await
    .result
    .unwrap();
    let mut state = state(&db).await;
    let public = state.public_key().unwrap();
    assert!(state.pending[0].payload.len() <= MAX_PAYLOAD);
    let snapshot = db.peek().await;
    let local = snapshot.as_private().as_notifications().as_idx(&0).unwrap();
    assert_eq!(local.as_title().de().unwrap(), title);
    assert_eq!(local.as_message().de().unwrap(), message);
    drain(&db, &Client::new()).await.unwrap();
    let received = receiver.received.lock().unwrap();
    let decoded = verify_wire(&received[0].0, &received[0].1, &s, &key, &auth, &public);
    assert!(title.starts_with(decoded["notification"]["title"].as_str().unwrap()));
    assert!(message.starts_with(decoded["notification"]["body"].as_str().unwrap()));
}

#[tokio::test]
async fn identical_resubscribe_preserves_queued_event_and_retry_deadline() {
    let (_temp, db) = database().await;
    let (s, _, _) = subscription("http://127.0.0.1:8080/a");
    store(&db, s.clone()).await;
    db.mutate(|db| {
        mutate_state(db, |state| {
            state.enqueue("title", "message", Utc::now())?;
            state.pending[0].retry_at = Utc::now() + chrono::Duration::seconds(60);
            Ok(())
        })
    })
    .await
    .result
    .unwrap();
    let before = serde_json::to_value(state(&db).await).unwrap();
    store(&db, s).await;
    assert_eq!(before, serde_json::to_value(state(&db).await).unwrap());
}

#[tokio::test]
async fn fanout_deliveries_are_concurrent() {
    let (_temp, db) = database().await;
    let (endpoint_a, receiver_a, _server_a) = receiver(StatusCode::CREATED, true).await;
    let (endpoint_b, receiver_b, _server_b) = receiver(StatusCode::CREATED, true).await;
    store(&db, subscription(&endpoint_a).0).await;
    store(&db, subscription(&endpoint_b).0).await;
    db.mutate(|db| enqueue(db, "title", "message"))
        .await
        .result
        .unwrap();
    let worker_db = db.clone();
    let worker = tokio::spawn(async move {
        drain(&worker_db, &Client::new()).await.unwrap();
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        receiver_a.started.notified().await;
        receiver_b.started.notified().await;
    })
    .await
    .unwrap();
    receiver_a.release.as_ref().unwrap().notify_one();
    receiver_b.release.as_ref().unwrap().notify_one();
    worker.await.unwrap();
    assert!(state(&db).await.pending.is_empty());
}

#[tokio::test]
async fn test_notification_uses_existing_registration_and_removes_404() {
    let (_temp, db) = database().await;
    let (endpoint, receiver, _server) = receiver(StatusCode::CREATED, false).await;
    let (s, key, auth) = subscription(&endpoint);
    store(&db, s.clone()).await;
    let public = state(&db).await.public_key().unwrap();
    test_subscription(&db, &Client::new(), &endpoint)
        .await
        .unwrap();
    {
        let received = receiver.received.lock().unwrap();
        let decoded = verify_wire(&received[0].0, &received[0].1, &s, &key, &auth, &public);
        assert_eq!(
            decoded["notification"]["title"],
            t!("notifications.push.test-title").as_ref()
        );
        assert_eq!(
            decoded["notification"]["body"],
            t!("notifications.push.test-body").as_ref()
        );
    }
    assert!(
        db.peek()
            .await
            .as_private()
            .as_notifications()
            .keys()
            .unwrap()
            .is_empty()
    );
    assert!(state(&db).await.pending.is_empty());
    *receiver.status.lock().unwrap() = StatusCode::NOT_FOUND;
    let e = test_subscription(&db, &Client::new(), &endpoint)
        .await
        .unwrap_err();
    assert!(!format!("{e:?}").contains("private-token"));
    assert!(state(&db).await.subscriptions.is_empty());
    assert!(
        test_subscription(&db, &Client::new(), &endpoint)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn pending_and_vapid_identity_survive_database_reopen_and_expire_without_delivery() {
    let (temp, db) = database().await;
    let (endpoint, receiver, _server) = receiver(StatusCode::CREATED, false).await;
    store(&db, subscription(&endpoint).0).await;
    db.mutate(|db| enqueue(db, "title", "message"))
        .await
        .result
        .unwrap();
    let before = serde_json::to_value(state(&db).await).unwrap();
    let underlying = PatchDb::clone(&db);
    drop(db);
    underlying.close().await;
    let db = TypedPatchDb::<Database>::load_unchecked(
        PatchDb::open(temp.path().join("db")).await.unwrap(),
    );
    assert_eq!(before, serde_json::to_value(state(&db).await).unwrap());
    db.mutate(|db| {
        mutate_state(db, |state| {
            state.pending[0].created_at = Utc::now() - chrono::Duration::seconds(MAX_AGE);
            Ok(())
        })
    })
    .await
    .result
    .unwrap();
    drain(&db, &Client::new()).await.unwrap();
    assert!(receiver.received.lock().unwrap().is_empty());
    assert!(state(&db).await.pending.is_empty());
    assert_eq!(state(&db).await.subscriptions.len(), 1);
}

#[test]
fn fresh_database_has_private_push_and_legacy_private_deserialization_defaults() {
    let account =
        crate::account::AccountInfo::new("test-password", std::time::SystemTime::now(), None)
            .unwrap();
    let database = Database::init(&account, false, None, None).unwrap();
    assert!(database.private.push.subscriptions.is_empty());
    let mut private = serde_json::to_value(database.private).unwrap();
    private.as_object_mut().unwrap().remove("push");
    let legacy: crate::db::model::private::Private = serde_json::from_value(private).unwrap();
    assert!(legacy.push.subscriptions.is_empty());
}

#[tokio::test]
async fn unread_count_helper_counts_persisted_seen_bits() {
    let (_temp, db) = database().await;
    db.mutate(|db| {
        notify(
            db,
            None,
            NotificationLevel::Info,
            "one".into(),
            "body".into(),
            (),
        )?;
        notify(
            db,
            None,
            NotificationLevel::Info,
            "two".into(),
            "body".into(),
            (),
        )?;
        db.as_private_mut()
            .as_notifications_mut()
            .as_idx_mut(&0)
            .unwrap()
            .as_seen_mut()
            .ser(&true)?;
        super::super::update_unread_count(db)
    })
    .await
    .result
    .unwrap();
    assert_eq!(
        db.peek()
            .await
            .as_public()
            .as_server_info()
            .as_unread_notification_count()
            .de()
            .unwrap(),
        1
    );
    db.mutate(|db| {
        db.as_private_mut().as_notifications_mut().remove(&1)?;
        super::super::update_unread_count(db)
    })
    .await
    .result
    .unwrap();
    assert_eq!(
        db.peek()
            .await
            .as_public()
            .as_server_info()
            .as_unread_notification_count()
            .de()
            .unwrap(),
        0
    );
}
