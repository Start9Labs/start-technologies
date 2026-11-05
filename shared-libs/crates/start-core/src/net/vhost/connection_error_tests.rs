use tokio::io::AsyncWriteExt;
use tracing::instrument::WithSubscriber;

use super::*;

type Logs = Arc<std::sync::Mutex<Vec<u8>>>;

struct LogWriter(Logs);

impl std::io::Write for LogWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(super) fn capture_logs() -> (Logs, tracing::Dispatch) {
    let logs = Logs::default();
    let writer = logs.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter("start_core=trace")
        .without_time()
        .with_ansi(false)
        .with_writer(move || LogWriter(writer.clone()))
        .finish();
    (logs, tracing::Dispatch::new(subscriber))
}

fn target(addr: SocketAddr) -> ProxyTarget {
    ProxyTarget {
        public_v4: BTreeSet::new(),
        public_v6: BTreeSet::new(),
        public_v6_gateways: BTreeSet::new(),
        private: BTreeSet::new(),
        acme: None,
        addr,
        addr_v6: None,
        proxy: None,
        connect_ssl: None,
        alpn: None,
        passthrough: false,
        preserve_source_ip: false,
    }
}

#[tokio::test]
async fn refused_backend_is_silent_and_connects_when_listening() {
    for ip in ["127.0.0.1", "::1"] {
        let ip: IpAddr = ip.parse().unwrap();
        let socket = if ip.is_ipv4() {
            tokio::net::TcpSocket::new_v4().unwrap()
        } else {
            tokio::net::TcpSocket::new_v6().unwrap()
        };
        socket.bind((ip, 0).into()).unwrap();
        let addr = socket.local_addr().unwrap();
        let target = target(addr);
        let (logs, dispatch) = capture_logs();

        assert!(
            target
                .connect_plain()
                .with_subscriber(dispatch.clone())
                .await
                .is_none()
        );
        assert!(logs.lock().unwrap().is_empty());

        let listener = socket.listen(1).unwrap();
        let (stream, deadline) = target
            .connect_plain()
            .with_subscriber(dispatch)
            .await
            .unwrap();
        assert_eq!(stream.peer_addr().unwrap(), addr);
        assert!(deadline > tokio::time::Instant::now());
        let (accepted, _) = listener.accept().await.unwrap();
        assert_eq!(accepted.peer_addr().unwrap(), stream.local_addr().unwrap());
        assert!(logs.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn other_backend_connect_errors_are_silent() {
    let addr = "255.255.255.255:1".parse().unwrap();
    let target = target(addr);
    let (logs, dispatch) = capture_logs();

    assert!(
        target
            .connect_plain()
            .with_subscriber(dispatch)
            .await
            .is_none()
    );
    assert!(logs.lock().unwrap().is_empty());
}

#[cfg(target_os = "linux")]
#[tokio::test]
#[ignore = "requires CAP_NET_RAW or CAP_NET_ADMIN"]
async fn transparent_connection_errors_are_silent() {
    for ip in ["127.0.0.1", "::1"] {
        let ip: IpAddr = ip.parse().unwrap();
        let target_socket = if ip.is_ipv4() {
            tokio::net::TcpSocket::new_v4().unwrap()
        } else {
            tokio::net::TcpSocket::new_v6().unwrap()
        };
        target_socket.bind((ip, 0).into()).unwrap();
        let target = target_socket.local_addr().unwrap();
        let source_listener = TcpListener::bind((ip, 0)).await.unwrap();
        let client = source_listener.local_addr().unwrap();
        crate::net::transparent::transparent_socket(client, target).unwrap();
        let (logs, dispatch) = capture_logs();
        assert!(
            ProxyTarget::connect_transparent(client, target)
                .with_subscriber(dispatch.clone())
                .await
                .is_none()
        );
        assert!(logs.lock().unwrap().is_empty());
        assert!(
            ProxyTarget::connect_transparent((ip, 0).into(), target)
                .with_subscriber(dispatch)
                .await
                .is_none()
        );
        assert!(logs.lock().unwrap().is_empty());

        let backend = TcpListener::bind((ip, 0)).await.unwrap();
        let target = backend.local_addr().unwrap();
        let stream = crate::net::transparent::transparent_connect((ip, 0).into(), target)
            .await
            .unwrap();
        let (accepted, _) = backend.accept().await.unwrap();
        assert_eq!(accepted.peer_addr().unwrap(), stream.local_addr().unwrap());
        assert_eq!(stream.local_addr().unwrap().ip(), ip);
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
#[ignore = "requires both CAP_NET_RAW and CAP_NET_ADMIN to be absent"]
async fn transparent_socket_setup_failure_warns() {
    let client = "127.0.0.1:0".parse().unwrap();
    let target = "127.0.0.1:1".parse().unwrap();
    let error = crate::net::transparent::transparent_socket(client, target).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    let (logs, dispatch) = capture_logs();
    assert!(
        ProxyTarget::connect_transparent(client, target)
            .with_subscriber(dispatch)
            .await
            .is_none()
    );
    let logs = String::from_utf8(logs.lock().unwrap().clone()).unwrap();
    assert!(logs.contains("WARN"), "{logs}");
    assert!(logs.contains("transparent egress setup"), "{logs}");
    assert!(logs.contains("connecting plainly"), "{logs}");
}

#[tokio::test]
async fn malformed_http_from_clients_and_services_is_silent() {
    tokio::time::timeout(Duration::from_secs(5), async {
        for ssl in [false, true] {
            for bad_client in [false, true] {
                let (from, mut client) = tokio::io::duplex(4096);
                let (to, mut service) = tokio::io::duplex(4096);
                let mut target = target("127.0.0.1:1".parse().unwrap());
                target.proxy = Some(ProxyOptions::Http {
                    add_x_forwarded_headers: false,
                    auth: None,
                });
                if bad_client {
                    client.write_all(b"invalid HTTP\r\n\r\n").await.unwrap();
                } else {
                    client
                        .write_all(b"GET / HTTP/1.1\r\nHost: service.test\r\n\r\n")
                        .await
                        .unwrap();
                    service.write_all(b"invalid HTTP\r\n\r\n").await.unwrap();
                }
                let (logs, dispatch) = capture_logs();
                target
                    .relay(Box::pin(from), Box::pin(to), None, None, ssl)
                    .with_subscriber(dispatch)
                    .await;
                assert!(logs.lock().unwrap().is_empty());
            }
        }
    })
    .await
    .unwrap();
}
