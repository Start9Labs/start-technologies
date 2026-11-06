use std::time::Duration;

use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Full};
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::Role;
use tracing::instrument::WithSubscriber;
use tracing::subscriber::NoSubscriber;

use super::*;

struct FollowedJournal {
    logs: Logs,
    entries: broadcast::Sender<String>,
}

impl Write for FollowedJournal {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.logs.write_all(bytes)?;
        let _ = self
            .entries
            .send(String::from_utf8_lossy(bytes).into_owned());
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

async fn follow_websocket(entries: &broadcast::Sender<String>) {
    let (server_io, client_io) = tokio::io::duplex(4096);
    let mut server = WebSocketStream::from_raw_socket(server_io, Role::Server, None).await;
    let mut client = WebSocketStream::from_raw_socket(client_io, Role::Client, None)
        .with_subscriber(NoSubscriber::default())
        .await;
    let mut journal = entries.subscribe();
    let feed = tokio::spawn(async move {
        for _ in 0..4 {
            let entry = journal.recv().await.unwrap();
            server.send(Message::Text(entry.into())).await.unwrap();
        }
    });
    tracing::trace!(target: "start_core::net::vhost", "follow websocket seed");
    let first = client
        .next()
        .with_subscriber(NoSubscriber::default())
        .await
        .unwrap()
        .unwrap();
    assert!(first.to_text().unwrap().contains("follow websocket seed"));
    let extra = tokio::time::timeout(
        Duration::from_millis(250),
        client.next().with_subscriber(NoSubscriber::default()),
    )
    .await;
    feed.abort();
    assert!(
        extra.is_err(),
        "WebSocket transport generated followed entries: {extra:?}"
    );
}

async fn follow_http2(entries: &broadcast::Sender<String>) {
    let (client_io, server_io) = tokio::io::duplex(4096);
    let server = tokio::spawn(async move {
        hyper::server::conn::http2::Builder::new(TokioExecutor::new())
            .serve_connection(
                TokioIo::new(server_io),
                hyper::service::service_fn(
                    |request: hyper::Request<hyper::body::Incoming>| async move {
                        let body = request.into_body().collect().await?.to_bytes();
                        Ok::<_, hyper::Error>(hyper::Response::new(Full::new(body)))
                    },
                ),
            )
            .await
            .unwrap();
    });
    let (mut client, connection) =
        hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(client_io))
            .with_subscriber(NoSubscriber::default())
            .await
            .unwrap();
    let driver = tokio::spawn(connection.with_subscriber(NoSubscriber::default()));
    let mut journal = entries.subscribe();
    let (sent, mut received) = mpsc::unbounded_channel();
    let feed = tokio::spawn(async move {
        for _ in 0..4 {
            let entry = journal.recv().await.unwrap();
            let request = hyper::Request::builder()
                .uri("http://journal.test/")
                .body(Full::new(Bytes::from(entry)))
                .unwrap();
            let response = client
                .send_request(request)
                .with_subscriber(NoSubscriber::default())
                .await
                .unwrap();
            let body = response
                .into_body()
                .collect()
                .with_subscriber(NoSubscriber::default())
                .await
                .unwrap()
                .to_bytes();
            sent.send(body).unwrap();
        }
    });
    tracing::trace!(target: "start_core::net::vhost", "follow http2 seed");
    let first = received.recv().await.unwrap();
    assert!(String::from_utf8_lossy(&first).contains("follow http2 seed"));
    let extra = tokio::time::timeout(Duration::from_millis(250), received.recv()).await;
    feed.abort();
    driver.abort();
    server.abort();
    assert!(
        extra.is_err(),
        "HTTP/2 transport generated followed entries: {extra:?}"
    );
}

#[test]
fn followed_log_transport_does_not_generate_journal_entries() {
    const CHILD: &str = "STARTOS_TEST_FOLLOWED_TRACE";
    let test = format!(
        "{}::followed_log_transport_does_not_generate_journal_entries",
        module_path!().split_once("::").unwrap().1
    );
    if std::env::var(CHILD).as_deref() != Ok(test.as_str()) {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &test, "--nocapture"])
            .env(CHILD, &test)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stdout.contains("running 1 test"), "{stdout}\n{stderr}");
        assert!(output.status.success(), "{stdout}\n{stderr}");
        return;
    }

    let logs = Logs::default();
    let (entries, _) = broadcast::channel(256);
    let (writer, guard) = NonBlockingBuilder::default().finish(FollowedJournal {
        logs: logs.clone(),
        entries: entries.clone(),
    });
    let (subscriber, filter) =
        StartOSLogger::base_subscriber(writer, EnvFilter::new(packaged_filter()));
    subscriber.init();
    filter.set_level(LogLevel::Trace).unwrap();
    assert_eq!(log::max_level(), log::LevelFilter::Trace);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(10), async {
            follow_websocket(&entries).await;
            follow_http2(&entries).await;
        })
        .await
        .unwrap();
    });
    drop(runtime);
    drop(guard);
    let text = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    assert_eq!(text.lines().count(), 2, "{text}");
}
