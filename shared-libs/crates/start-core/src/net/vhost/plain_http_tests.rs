use bytes::Bytes;
use http::{HeaderMap, Request, Response, StatusCode};
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use super::*;
use crate::net::host::binding::ProxyAuth;
use crate::net::request_origin::RequestOrigin;

type ObservedRequest = (HeaderMap, RequestOrigin);

async fn backend() -> (
    SocketAddr,
    mpsc::UnboundedReceiver<ObservedRequest>,
    NonDetachingJoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (sent, requests) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let sent = sent.clone();
            tokio::spawn(async move {
                let mut extensions = http::Extensions::new();
                extensions.insert(TcpMetadata {
                    peer_addr: stream.peer_addr().unwrap(),
                    local_addr: stream.local_addr().unwrap(),
                });
                let origin = RequestOrigin::from_extensions(&extensions);
                hyper_util::server::conn::auto::Builder::new(TokioExecutor::new())
                    .serve_connection(
                        TokioIo::new(stream),
                        service_fn(move |req: Request<Incoming>| {
                            sent.send((req.headers().clone(), origin.clone())).unwrap();
                            async { Ok::<_, hyper::Error>(Response::new(Full::new(Bytes::new()))) }
                        }),
                    )
                    .await
                    .ok();
            });
        }
    });
    (addr, requests, task.into())
}

async fn proxy(
    ip: IpAddr,
    backend: SocketAddr,
    proxy: ProxyOptions,
    max_connections: usize,
) -> (SocketAddr, PlainHttpProxy) {
    let listener = TcpListener::bind((ip, 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let gateway: GatewayId = "lo".parse().unwrap();
    let info = NetworkInterfaceInfo {
        ip_info: Some(Arc::new(crate::db::model::public::IpInfo {
            subnets: [IpNet::new(ip, if ip.is_ipv4() { 8 } else { 128 }).unwrap()]
                .into_iter()
                .collect(),
            ..Default::default()
        })),
        ..Default::default()
    };
    let private = BTreeSet::from([ip]);
    let requirements = VHostBindRequirements {
        private_ips: private.clone(),
        ..Default::default()
    };
    let listener = VHostBindListener {
        ip_info: Watch::new([(gateway.clone(), info.clone())].into_iter().collect()),
        port: addr.port(),
        bind_reqs: Watch::new(requirements),
        listeners: BTreeMap::from([(addr, (listener, GatewayInfo { id: gateway, info }))]),
        retry: None,
    };
    let target = ProxyTarget {
        public_v4: BTreeSet::new(),
        public_v6: BTreeSet::new(),
        public_v6_gateways: BTreeSet::new(),
        private,
        acme: None,
        addr: backend,
        addr_v6: None,
        proxy: Some(proxy),
        connect_ssl: None,
        alpn: None,
        passthrough: false,
        preserve_source_ip: false,
    };
    (
        addr,
        PlainHttpProxy::serve(listener, target, max_connections),
    )
}

async fn client(
    addr: SocketAddr,
) -> (
    hyper::client::conn::http1::SendRequest<Empty<Bytes>>,
    NonDetachingJoinHandle<()>,
) {
    let stream = TcpStream::connect(addr).await.unwrap();
    let (client, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .unwrap();
    let task = tokio::spawn(async {
        connection.await.ok();
    });
    (client, task.into())
}

fn request(auth: Option<&str>) -> Request<Empty<Bytes>> {
    let mut request = Request::builder()
        .uri("/")
        .header("Host", "service.test")
        .header("X-Forwarded-For", "forged")
        .header("X-Forwarded-Proto", "https")
        .header("X-Forwarded-User", "forged")
        .header("X-Forwarded-Host", "forged");
    if let Some(auth) = auth {
        request = request.header("Authorization", auth);
    }
    request.body(Empty::new()).unwrap()
}

#[tokio::test]
async fn plaintext_listener_enforces_auth_and_preserves_origin_in_both_families() {
    tokio::time::timeout(Duration::from_secs(10), async {
        for ip in ["127.0.0.1", "::1"] {
            for headers in [false, true] {
                let (upstream, mut requests, _backend) = backend().await;
                let (addr, _proxy) = proxy(
                    ip.parse().unwrap(),
                    upstream,
                    ProxyOptions::Http {
                        add_x_forwarded_headers: headers,
                        auth: Some(ProxyAuth::Bearer {
                            tokens: vec!["secret".into()],
                            realm: None,
                        }),
                    },
                    4,
                )
                .await;
                let (mut client, _connection) = client(addr).await;
                for auth in [None, Some("Bearer wrong")] {
                    let response = client.send_request(request(auth)).await.unwrap();
                    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
                    response.into_body().collect().await.unwrap();
                    assert!(requests.try_recv().is_err());
                }
                let response = client
                    .send_request(request(Some("Bearer secret")))
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::OK);
                let (received, origin) = requests.recv().await.unwrap();
                assert_eq!(
                    received.get("X-Forwarded-For").map(|v| v.to_str().unwrap()),
                    headers.then_some(ip)
                );
                for name in ["X-Forwarded-Proto", "X-Forwarded-User", "X-Forwarded-Host"] {
                    assert!(!received.contains_key(name));
                }
                assert_eq!(origin.peer_ip, Some(addr.ip()));
                assert_eq!(origin.local_ip, Some(addr.ip()));
                assert_eq!(origin.gateway, Some("lo".parse().unwrap()));
                assert!(!origin.public);
                let response = client.send_request(request(None)).await.unwrap();
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
                assert!(requests.try_recv().is_err());
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn prior_knowledge_h2c_enforces_policy_on_each_stream_in_both_families() {
    tokio::time::timeout(Duration::from_secs(10), async {
        for ip in ["127.0.0.1", "::1"] {
            for headers in [false, true] {
                let (upstream, mut requests, _backend) = backend().await;
                let (addr, _proxy) = proxy(
                    ip.parse().unwrap(),
                    upstream,
                    ProxyOptions::Http {
                        add_x_forwarded_headers: headers,
                        auth: Some(ProxyAuth::Basic {
                            credentials: vec![crate::net::host::binding::BasicCredential {
                                username: "user".into(),
                                password: "secret".into(),
                            }],
                            realm: None,
                        }),
                    },
                    4,
                )
                .await;
                let stream = TcpStream::connect(addr).await.unwrap();
                let (mut client, connection) = hyper::client::conn::http2::handshake(
                    TokioExecutor::new(),
                    TokioIo::new(stream),
                )
                .await
                .unwrap();
                let _connection: NonDetachingJoinHandle<()> = tokio::spawn(async {
                    connection.await.ok();
                })
                .into();
                let h2_request = |auth| {
                    let mut request = request(auth);
                    *request.uri_mut() = "http://service.test/".parse().unwrap();
                    request
                };
                for auth in [None, Some("Basic invalid")] {
                    let response = client.send_request(h2_request(auth)).await.unwrap();
                    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
                    response.into_body().collect().await.unwrap();
                    assert!(requests.try_recv().is_err());
                }
                let response = client
                    .send_request(h2_request(Some("Basic dXNlcjpzZWNyZXQ=")))
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::OK);
                response.into_body().collect().await.unwrap();
                let (received, origin) = requests.recv().await.unwrap();
                assert_eq!(
                    received.get("X-Forwarded-For").map(|v| v.to_str().unwrap()),
                    headers.then_some(ip)
                );
                assert_eq!(received.get("X-Forwarded-User").unwrap(), "user");
                assert!(!received.contains_key("X-Forwarded-Proto"));
                assert!(!received.contains_key("X-Forwarded-Host"));
                assert_eq!(origin.peer_ip, Some(addr.ip()));
                let response = client.send_request(h2_request(None)).await.unwrap();
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
                response.into_body().collect().await.unwrap();
                assert!(requests.try_recv().is_err());
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn dropping_a_plaintext_proxy_cancels_existing_streams_and_releases_its_listener() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let (upstream, mut requests, _backend) = backend().await;
        let (addr, server) = proxy(
            "127.0.0.1".parse().unwrap(),
            upstream,
            ProxyOptions::Http {
                add_x_forwarded_headers: false,
                auth: None,
            },
            4,
        )
        .await;
        let (mut client, _connection) = client(addr).await;
        assert_eq!(
            client.send_request(request(None)).await.unwrap().status(),
            StatusCode::OK
        );
        requests.recv().await.unwrap();
        drop(server);
        assert!(client.send_request(request(None)).await.is_err());
        tokio::task::yield_now().await;
        assert!(TcpStream::connect(addr).await.is_err());
        let _reclaimed = TcpListener::bind(addr).await.unwrap();
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn plaintext_connection_cap_evicts_an_idle_connection() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let (upstream, mut requests, _backend) = backend().await;
        let (addr, _server) = proxy(
            "127.0.0.1".parse().unwrap(),
            upstream,
            ProxyOptions::Http {
                add_x_forwarded_headers: false,
                auth: None,
            },
            1,
        )
        .await;
        let (mut old, _old_connection) = client(addr).await;
        assert_eq!(
            old.send_request(request(None)).await.unwrap().status(),
            StatusCode::OK
        );
        requests.recv().await.unwrap();
        let (mut fresh, _fresh_connection) = client(addr).await;
        assert_eq!(
            fresh.send_request(request(None)).await.unwrap().status(),
            StatusCode::OK
        );
        requests.recv().await.unwrap();
        assert!(old.send_request(request(None)).await.is_err());
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn invalid_auth_configuration_fails_closed_before_any_http_request_reaches_upstream() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let (upstream, mut requests, _backend) = backend().await;
        let (addr, _server) = proxy(
            "127.0.0.1".parse().unwrap(),
            upstream,
            ProxyOptions::Http {
                add_x_forwarded_headers: true,
                auth: Some(ProxyAuth::Bearer {
                    tokens: vec!["bad\ntoken".into()],
                    realm: None,
                }),
            },
            1,
        )
        .await;
        let mut client = TcpStream::connect(addr).await.unwrap();
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: service.test\r\n\r\n")
            .await
            .unwrap();
        let mut byte = [0];
        assert!(client.read(&mut byte).await.map_or(true, |n| n == 0));
        assert!(requests.try_recv().is_err());
    })
    .await
    .unwrap();
}
