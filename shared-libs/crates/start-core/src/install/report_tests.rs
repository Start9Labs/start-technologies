use std::num::NonZeroUsize;

use axum::extract::ws::{Message, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::routing::{get, post};
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::*;
use crate::middleware::auth::signature::{AUTH_SIG_HEADER, SignatureHeader};
use crate::registry::device_info::DEVICE_INFO_HEADER;
use crate::registry::metrics::RecordInstallParams;
use crate::sign::session::Target;
use crate::sign::{AnySigningKey, AnyVerifyingKey, Signer};
use crate::util::future::NonDetachingJoinHandle;
use crate::util::net::secure::EncryptedSocket;

type Report = (AnyVerifyingKey, HeaderMap, serde_json::Value);

async fn assert_unlinkable_reports(app: axum::Router, mut received: UnboundedReceiver<Report>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url: Url = format!(
        "http://localhost:{}/catalog/",
        listener.local_addr().unwrap().port()
    )
    .parse()
    .unwrap();
    let _server: NonDetachingJoinHandle<()> =
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }).into();
    let (client, _proxy) = crate::registry::context::test_utils::socks_client(&url).await;
    let mut identities = Vec::new();
    for _ in 0..2 {
        tokio::time::timeout(
            Duration::from_secs(3),
            report_install(
                &client,
                &url,
                RecordInstallParams {
                    id: "hello-world".parse().unwrap(),
                    version: "1.0.0:0".parse().unwrap(),
                },
            ),
        )
        .await
        .unwrap()
        .unwrap();
        let (identity, headers, request) = received.recv().await.unwrap();
        identities.push(identity);
        assert!(!headers.contains_key(DEVICE_INFO_HEADER));
        assert!(!headers.contains_key(reqwest::header::AUTHORIZATION));
        assert_eq!(request["method"], "package.record-install");
        assert_eq!(
            request["params"],
            serde_json::json!({"id": "hello-world", "version": "1.0.0:0"})
        );
    }
    assert_ne!(identities[0], identities[1]);
}

#[tokio::test]
async fn encrypted_install_reports_use_per_report_identities_without_device_metadata() {
    let (send, received) = unbounded_channel();
    let key: Signer = AnySigningKey::generate().into();
    let app = axum::Router::new().route(
        "/catalog/rpc/v2",
        get(move |headers: HeaderMap, upgrade: WebSocketUpgrade| {
            let key = key.clone();
            let send = send.clone();
            async move {
                assert!(!headers.contains_key(AUTH_SIG_HEADER));
                upgrade.on_upgrade(move |socket| async move {
                    let (peer, _, mut socket) = EncryptedSocket::accept(
                        Box::new(socket),
                        &key.into(),
                        &["localhost".into()],
                        NonZeroUsize::new(1024 * 1024).unwrap(),
                        &Target::Rpc,
                        None,
                        |_| async { Ok(()) },
                    )
                    .await
                    .unwrap();
                    let text = socket.next().await.unwrap().unwrap().into_text().unwrap();
                    let request: serde_json::Value = serde_json::from_str(&text).unwrap();
                    let response = serde_json::json!({
                        "jsonrpc": "2.0", "id": request["id"], "result": null
                    });
                    send.send((peer, headers, request)).unwrap();
                    socket
                        .send(Message::Text(response.to_string().into()))
                        .await
                        .unwrap();
                })
            }
        }),
    );
    assert_unlinkable_reports(app, received).await;
}

#[tokio::test]
async fn legacy_install_reports_use_per_report_identities_without_device_metadata() {
    let (send, received) = unbounded_channel();
    let app = axum::Router::new()
        .route("/catalog/rpc/v2", get(|| async { StatusCode::NOT_FOUND }))
        .route(
            "/catalog/rpc/v0",
            post(
                move |headers: HeaderMap, axum::Json(request): axum::Json<serde_json::Value>| {
                    let send = send.clone();
                    async move {
                        let proof =
                            SignatureHeader::from_header(&headers[AUTH_SIG_HEADER]).unwrap();
                        let response = serde_json::json!({
                            "jsonrpc": "2.0", "id": request["id"], "result": null
                        });
                        send.send((proof.signer, headers, request)).unwrap();
                        axum::Json(response)
                    }
                },
            ),
        );
    assert_unlinkable_reports(app, received).await;
}
