use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UnixListener, UnixStream};

use super::*;
use crate::net::host::binding::ProxyOptions;
use crate::net::http_proxy::tests::gateway;
use crate::net::vhost::ProxyTarget;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::TmpDir;
use crate::util::sync::{SyncMutex, Watch};

#[cfg(target_os = "linux")]
#[path = "mount_tests.rs"]
mod mount_tests;

#[path = "tls_tests.rs"]
mod tls_tests;

fn dns() -> Arc<crate::net::dns::DnsController> {
    Arc::new(crate::net::dns::DnsController::new(Watch::new(
        Default::default(),
    )))
}

#[tokio::test]
async fn gateway_routes_isolate_public_only_and_container_source_lifetimes() {
    use crate::net::request_origin::RequestOrigin;
    let sockets = sockets().await;
    let dns = dns();
    let tor_ip = [10, 0, 3, 9].into();
    let tor = dns
        .add_service(
            Some("tor".parse().unwrap()),
            tor_ip,
            Some("fd00::9".parse().unwrap()),
        )
        .unwrap();
    let mut routes = BindingRoutes::new(dns.clone());
    let network = |name: &str| HttpProxyGateway::Network {
        gateway_id: name.parse().unwrap(),
    };
    let url = |name: &str| HttpProxyGateway::UrlPlugin {
        package_id: name.parse().unwrap(),
    };
    let policies = vec![
        HttpProxyChain {
            gateway: network("eth0"),
            chain: vec![stage("waf")],
            public_only: true,
        },
        HttpProxyChain {
            gateway: network("eth1"),
            chain: vec![stage("waf")],
            public_only: false,
        },
        HttpProxyChain {
            gateway: url("tor"),
            chain: vec![stage("waf")],
            public_only: false,
        },
        HttpProxyChain {
            gateway: url("replacement"),
            chain: vec![stage("waf")],
            public_only: false,
        },
    ];
    let target = upstream("127.0.0.1:1".parse().unwrap());
    routes
        .reconcile(&sockets, &binding(), &policies, &target)
        .await;
    let mut origin = RequestOrigin {
        peer_ip: Some("198.51.100.9".parse().unwrap()),
        gateway: Some("eth0".parse().unwrap()),
        public: true,
        ..Default::default()
    };
    let eth0 = routes.target.select(&origin).unwrap().unwrap();
    origin.public = false;
    assert!(routes.target.select(&origin).unwrap().is_none());
    origin.gateway = Some("eth1".parse().unwrap());
    let eth1 = routes.target.select(&origin).unwrap().unwrap();
    assert!(!Arc::ptr_eq(&eth0, &eth1));
    origin.gateway = Some("eth0".parse().unwrap());
    for ip in [
        std::net::IpAddr::V4(tor_ip),
        "fd00::9".parse().unwrap(),
        "::ffff:10.0.3.9".parse().unwrap(),
    ] {
        origin.peer_ip = Some(ip);
        let selected = routes.target.select(&origin).unwrap().unwrap();
        assert!(!Arc::ptr_eq(&selected, &eth0));
        assert!(!Arc::ptr_eq(&selected, &eth1));
    }
    let source = routes.target.select(&origin).unwrap().unwrap();
    let projection = routes.target.clone();
    routes
        .reconcile(&sockets, &binding(), &policies, &target)
        .await;
    assert!(Arc::ptr_eq(&projection, &routes.target));
    drop(tor);
    assert!(routes.target.select(&origin).is_err());
    let _replacement = dns
        .add_service(
            Some("replacement".parse().unwrap()),
            tor_ip,
            Some("fd00::9".parse().unwrap()),
        )
        .unwrap();
    let reassigned = routes.target.select(&origin).unwrap().unwrap();
    assert!(!Arc::ptr_eq(&source, &reassigned));
    let _internal = dns
        .add_service(
            Some("internal-client".parse().unwrap()),
            [10, 0, 3, 10].into(),
            None,
        )
        .unwrap();
    origin.peer_ip = Some("10.0.3.10".parse().unwrap());
    origin.public = true;
    assert!(routes.target.select(&origin).unwrap().is_none());
    sockets
        .providers
        .send_modify(|providers| providers.get_mut("waf").unwrap().running = false);
    routes
        .reconcile(&sockets, &binding(), &policies, &target)
        .await;
    origin.peer_ip = Some("198.51.100.9".parse().unwrap());
    origin.public = false;
    assert!(routes.target.select(&origin).unwrap().is_none());
    origin.public = true;
    let unavailable = routes.target.select(&origin).unwrap().unwrap();
    let (mut client, driver) =
        hyper::client::conn::http1::handshake(TokioIo::new(unavailable.connect().await))
            .await
            .unwrap();
    let _driver: NonDetachingJoinHandle<_> = tokio::spawn(driver).into();
    assert_eq!(client.send_request(request()).await.unwrap().status(), 503);
}

async fn sockets() -> HttpProxySockets {
    let profiles = BTreeMap::from([(
        "standard".into(),
        BTreeSet::from([HttpProxyProtocol::Http1, HttpProxyProtocol::Http2]),
    )]);
    HttpProxySockets {
        root: Arc::new(TmpDir::new().await.unwrap()),
        providers: Watch::new(BTreeMap::from([
            (
                "waf".parse().unwrap(),
                ProviderState {
                    running: true,
                    profiles: profiles.clone(),
                },
            ),
            (
                "anubis".parse().unwrap(),
                ProviderState {
                    running: true,
                    profiles,
                },
            ),
        ])),
        _watcher: tokio::spawn(std::future::pending()).into(),
    }
}

fn binding() -> HttpProxyBinding {
    HttpProxyBinding {
        package_id: "app".parse().unwrap(),
        host_id: "web".parse().unwrap(),
        internal_port: 8080,
    }
}

fn stage(provider: &str) -> HttpProxyStage {
    HttpProxyStage {
        package_id: provider.parse().unwrap(),
        profile_id: "standard".into(),
    }
}

fn upstream(address: std::net::SocketAddr) -> ProxyTarget {
    ProxyTarget {
        public_v4: BTreeSet::new(),
        public_v6: BTreeSet::new(),
        public_v6_gateways: BTreeSet::new(),
        private: BTreeSet::new(),
        acme: None,
        addr: address,
        addr_v6: None,
        proxy: Some(ProxyOptions::Http {
            add_x_forwarded_headers: true,
            auth: None,
        }),
        http_chain: None,
        connect_ssl: None,
        alpn: None,
        passthrough: false,
        preserve_source_ip: false,
    }
}

async fn echo_application() -> (ProxyTarget, NonDetachingJoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = upstream(listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let _ = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new())
                    .serve_connection(
                        TokioIo::new(stream),
                        service_fn(|request: Request<hyper::body::Incoming>| async move {
                            assert_eq!(
                                request.uri().path_and_query().unwrap().as_str(),
                                "/upload?x=1"
                            );
                            assert_eq!(
                                crate::net::http::request_authority(&request)
                                    .unwrap()
                                    .as_str(),
                                "app.example"
                            );
                            assert_eq!(request.headers()["x-forwarded-for"], "203.0.113.9");
                            let body = request.into_body().collect().await?.to_bytes();
                            Ok::<_, hyper::Error>(
                                Response::builder()
                                    .header("set-cookie", "proof=accepted; HttpOnly")
                                    .body(Full::new(body))
                                    .unwrap(),
                            )
                        }),
                    )
                    .await;
            });
        }
    });
    (target, server.into())
}

async fn copying_stage(
    sockets: &HttpProxySockets,
    provider: &str,
    order: Arc<SyncMutex<Vec<String>>>,
) -> NonDetachingJoinHandle<()> {
    let stage = stage(provider);
    let root = sockets.prepare_provider(&stage.package_id).await.unwrap();
    let listener =
        UnixListener::bind(binding().socket_path(&root, "in", &gateway(), &stage.profile_id))
            .unwrap();
    let next = binding().socket_path(&root, "next", &gateway(), &stage.profile_id);
    let provider = provider.to_owned();
    tokio::spawn(async move {
        loop {
            let (mut from, _) = listener.accept().await.unwrap();
            order.mutate(|order| order.push(provider.clone()));
            let next = next.clone();
            tokio::spawn(async move {
                if let Ok(mut to) = UnixStream::connect(next).await {
                    let _ = tokio::io::copy_bidirectional(&mut from, &mut to).await;
                }
            });
        }
    })
    .into()
}

fn request() -> Request<Full<Bytes>> {
    Request::builder()
        .method("POST")
        .uri("http://app.example/upload?x=1")
        .header("host", "app.example")
        .header("x-forwarded-for", "203.0.113.9")
        .body(Full::new(Bytes::from_static(b"streamed upload")))
        .unwrap()
}

#[tokio::test]
async fn unrelated_profiles_do_not_invalidate_an_existing_chain() {
    let sockets = sockets().await;
    let target = upstream("127.0.0.1:1".parse().unwrap());
    let stages = [stage("waf")];
    let route = ChainRoute::new(&sockets, &binding(), &gateway(), &stages, target.clone()).await;
    sockets.providers.send_modify(|providers| {
        providers
            .get_mut("waf")
            .unwrap()
            .profiles
            .insert("other".into(), BTreeSet::from([HttpProxyProtocol::Http1]));
    });
    assert!(route.matches(&stages, &sockets.providers.read(), &target));
    sockets.providers.send_modify(|providers| {
        providers
            .get_mut("waf")
            .unwrap()
            .profiles
            .remove("standard");
    });
    assert!(!route.matches(&stages, &sockets.providers.read(), &target));
}

#[tokio::test]
async fn unix_chain_preserves_body_cookies_authority_and_order_then_reorders() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let sockets = sockets().await;
        let order = Arc::new(SyncMutex::new(Vec::new()));
        let (target, _app) = echo_application().await;
        let _waf = copying_stage(&sockets, "waf", order.clone()).await;
        let _anubis = copying_stage(&sockets, "anubis", order.clone()).await;
        for stages in [
            [stage("waf"), stage("anubis")],
            [stage("anubis"), stage("waf")],
        ] {
            order.replace(Vec::new());
            let route =
                ChainRoute::new(&sockets, &binding(), &gateway(), &stages, target.clone()).await;
            let (mut client, connection) =
                hyper::client::conn::http1::handshake(TokioIo::new(route.target.connect().await))
                    .await
                    .unwrap();
            let driver = tokio::spawn(connection);
            let response = client.send_request(request()).await.unwrap();
            assert_eq!(response.status(), 200);
            assert_eq!(response.headers()["set-cookie"], "proof=accepted; HttpOnly");
            assert_eq!(
                response.into_body().collect().await.unwrap().to_bytes(),
                "streamed upload"
            );
            assert_eq!(
                order.peek(Clone::clone),
                stages
                    .iter()
                    .map(|s| s.package_id.to_string())
                    .collect::<Vec<_>>()
            );
            drop(route);
            tokio::time::timeout(Duration::from_secs(1), driver)
                .await
                .expect("HTTP/1 connection outlived route cancellation")
                .ok();
        }
    })
    .await
    .expect("Unix chain stalled");
}

#[tokio::test]
async fn unix_chain_carries_http2_streams() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let sockets = sockets().await;
        let (target, _app) = echo_application().await;
        let _waf = copying_stage(&sockets, "waf", Arc::new(SyncMutex::new(Vec::new()))).await;
        let route =
            ChainRoute::new(&sockets, &binding(), &gateway(), &[stage("waf")], target).await;
        let (mut client, connection) = hyper::client::conn::http2::handshake(
            TokioExecutor::new(),
            TokioIo::new(route.target.connect().await),
        )
        .await
        .unwrap();
        let driver = tokio::spawn(connection);
        let response = client.send_request(request()).await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            response.into_body().collect().await.unwrap().to_bytes(),
            "streamed upload"
        );
        drop(route);
        tokio::time::timeout(Duration::from_secs(1), driver)
            .await
            .expect("HTTP/2 connection outlived route cancellation")
            .ok();
    })
    .await
    .expect("HTTP/2 chain stalled");
}

#[tokio::test]
async fn absent_stopped_or_missing_profile_never_falls_back_to_application() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let sockets = sockets().await;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = upstream(listener.local_addr().unwrap());
        for stages in [
            vec![stage("missing")],
            vec![stage("waf")],
            vec![HttpProxyStage {
                profile_id: "deleted".into(),
                ..stage("waf")
            }],
        ] {
            let route =
                ChainRoute::new(&sockets, &binding(), &gateway(), &stages, target.clone()).await;
            let (mut client, connection) =
                hyper::client::conn::http1::handshake(TokioIo::new(route.target.connect().await))
                    .await
                    .unwrap();
            let _driver: NonDetachingJoinHandle<_> = tokio::spawn(connection).into();
            assert_eq!(client.send_request(request()).await.unwrap().status(), 503);
        }
        sockets
            .providers
            .send_modify(|providers| providers.get_mut("waf").unwrap().running = false);
        let route =
            ChainRoute::new(&sockets, &binding(), &gateway(), &[stage("waf")], target).await;
        let (mut client, connection) =
            hyper::client::conn::http1::handshake(TokioIo::new(route.target.connect().await))
                .await
                .unwrap();
        let _driver: NonDetachingJoinHandle<_> = tokio::spawn(connection).into();
        assert_eq!(client.send_request(request()).await.unwrap().status(), 503);
        assert!(
            tokio::time::timeout(Duration::from_millis(30), listener.accept())
                .await
                .is_err()
        );
    })
    .await
    .expect("unavailable proxy did not answer");
}

#[tokio::test]
async fn ingress_authentication_precedes_plugins_and_cancellation_closes_the_frontend() {
    use crate::net::host::binding::{BasicCredential, ProxyAuth};
    tokio::time::timeout(Duration::from_secs(5), async {
        let sockets = sockets().await;
        let (mut target, _app) = echo_application().await;
        target.proxy = Some(ProxyOptions::Http {
            add_x_forwarded_headers: true,
            auth: Some(ProxyAuth::Basic {
                credentials: vec![BasicCredential {
                    username: "u".into(),
                    password: "pw".into(),
                }],
                realm: None,
            }),
        });
        let _waf = copying_stage(&sockets, "waf", Arc::new(SyncMutex::new(Vec::new()))).await;
        let route = ChainRoute::new(
            &sockets,
            &binding(),
            &gateway(),
            &[stage("waf")],
            target.clone(),
        )
        .await;
        let chain = route.target.clone();
        let backend = route.target.connect().await;
        let (client, frontend) = tokio::io::duplex(4096);
        let proxy = tokio::spawn(async move {
            target
                .relay(
                    Box::pin(frontend),
                    backend,
                    None,
                    Some("203.0.113.9".parse().unwrap()),
                    false,
                    Some(chain),
                )
                .await;
        });
        let (mut client, driver) = hyper::client::conn::http1::handshake(TokioIo::new(client))
            .await
            .unwrap();
        let driver = tokio::spawn(driver);
        assert_eq!(client.send_request(request()).await.unwrap().status(), 401);
        let mut authorized = request();
        authorized
            .headers_mut()
            .insert("authorization", "Basic dTpwdw==".parse().unwrap());
        authorized
            .headers_mut()
            .insert("x-forwarded-for", "127.0.0.1".parse().unwrap());
        let response = client.send_request(authorized).await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            response.into_body().collect().await.unwrap().to_bytes(),
            "streamed upload"
        );
        drop(route);
        proxy.await.unwrap();
        let _ = driver.await;
    })
    .await
    .expect("authenticated chain stalled");
}

#[tokio::test]
async fn http2_is_rejected_instead_of_bypassing_an_http1_only_profile() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let sockets = sockets().await;
        sockets.providers.send_modify(|providers| {
            providers
                .get_mut("waf")
                .unwrap()
                .profiles
                .get_mut("standard")
                .unwrap()
                .remove(&HttpProxyProtocol::Http2);
        });
        let (target, _app) = echo_application().await;
        let _waf = copying_stage(&sockets, "waf", Arc::new(SyncMutex::new(Vec::new()))).await;
        let route = ChainRoute::new(
            &sockets,
            &binding(),
            &gateway(),
            &[stage("waf")],
            target.clone(),
        )
        .await;
        let chain = route.target.clone();
        let backend = route.target.connect().await;
        let (client, frontend) = tokio::io::duplex(4096);
        let proxy = tokio::spawn(async move {
            target
                .relay(Box::pin(frontend), backend, None, None, false, Some(chain))
                .await;
        });
        let (mut client, connection) =
            hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(client))
                .await
                .unwrap();
        let _driver: NonDetachingJoinHandle<_> = tokio::spawn(connection).into();
        assert_eq!(client.send_request(request()).await.unwrap().status(), 505);
        drop(route);
        proxy.await.unwrap();
    })
    .await
    .expect("unsupported protocol was not rejected");
}

#[tokio::test]
async fn challenge_response_short_circuits_the_chain() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let sockets = sockets().await;
        let root = sockets.prepare_provider(&stage("anubis").package_id).await.unwrap();
        let listener = UnixListener::bind(binding().socket_path(&root, "in", &gateway(), "standard")).unwrap();
        let challenge = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut byte = [0];
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") { stream.read_exact(&mut byte).await.unwrap(); head.push(byte[0]); }
            stream.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 9\r\nSet-Cookie: challenge=1\r\n\r\nchallenge").await.unwrap();
        });
        let (target, _app) = echo_application().await;
        let route = ChainRoute::new(&sockets, &binding(), &gateway(), &[stage("anubis")], target).await;
        let (mut client, connection) = hyper::client::conn::http1::handshake(TokioIo::new(route.target.connect().await)).await.unwrap();
        let _driver: NonDetachingJoinHandle<_> = tokio::spawn(connection).into();
        let response = client.send_request(request()).await.unwrap();
        assert_eq!(response.status(), 403);
        assert_eq!(response.headers()["set-cookie"], "challenge=1");
        assert_eq!(response.into_body().collect().await.unwrap().to_bytes(), "challenge");
        challenge.await.unwrap();
    }).await.expect("challenge stalled");
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn ingress_connections_refuse_provider_symlinks() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let dir = TmpDir::new().await.unwrap();
        let host_socket = dir.join("host.sock");
        let _host = UnixListener::bind(&host_socket).unwrap();
        let planted = dir.join("planted.sock");
        std::os::unix::fs::symlink(&host_socket, &planted).unwrap();
        assert!(connect_ingress(&planted).await.is_err());
        connect_ingress(&host_socket).await.unwrap();
        let target = ChainTarget {
            head: Some(planted.clone()),
            protocols: HttpProxyProtocol::all(),
            cancel: CancellationToken::new(),
        };
        let (frontend, continuation) = tokio::io::duplex(4096);
        let _relay: NonDetachingJoinHandle<_> = tokio::spawn(async move {
            NextHop::Unix(planted)
                .relay(Box::pin(continuation))
                .await
                .unwrap();
        })
        .into();
        let streams: [AcceptStream; 2] = [target.connect().await, Box::pin(frontend)];
        for stream in streams {
            let (mut client, connection) =
                hyper::client::conn::http1::handshake(TokioIo::new(stream))
                    .await
                    .unwrap();
            let _driver: NonDetachingJoinHandle<_> = tokio::spawn(connection).into();
            assert_eq!(client.send_request(request()).await.unwrap().status(), 503);
        }
    })
    .await
    .expect("symlink rejection stalled");
}
