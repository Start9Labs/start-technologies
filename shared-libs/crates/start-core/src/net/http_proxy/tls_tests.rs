use tokio_rustls::TlsAcceptor;

use super::*;
use crate::net::tls::client_config;
use crate::net::tls::test::{provider, self_signed_for_loopback, server_config};
use crate::net::vhost::upstream_alpn_tests::try_negotiate_target;

#[tokio::test]
async fn tls_ingress_negotiates_only_protocols_in_the_chain() {
    tokio::time::timeout(Duration::from_secs(10), async {
        for (supported, offered, expected) in [
            (
                HttpProxyProtocol::Http1,
                vec!["h2", "http/1.1"],
                Ok(Some("http/1.1")),
            ),
            (HttpProxyProtocol::Http1, vec!["h2"], Err(())),
            (HttpProxyProtocol::Http1, vec![], Ok(None)),
            (
                HttpProxyProtocol::Http2,
                vec!["h2", "http/1.1"],
                Ok(Some("h2")),
            ),
            (HttpProxyProtocol::Http2, vec!["http/1.1"], Err(())),
            (HttpProxyProtocol::Http2, vec![], Err(())),
        ] {
            let sockets = sockets().await;
            sockets.providers.send_modify(|providers| {
                providers
                    .get_mut("waf")
                    .unwrap()
                    .profiles
                    .insert("standard".into(), BTreeSet::from([supported]));
            });
            let mut target = upstream("127.0.0.1:1".parse().unwrap());
            let route =
                ChainRoute::new(&sockets, &binding(), &[stage("waf")], target.clone()).await;
            target.http_chain = Some(route.target.clone());
            let negotiated =
                try_negotiate_target(target, &["h3", "h2", "http/1.1"], &offered, b"").await;
            assert_eq!(
                negotiated.map_err(|_| ()),
                expected.map(|p| p.map(str::to_owned))
            );
        }
    })
    .await
    .expect("chain TLS negotiation stalled");
}

async fn tls_application(
    alpn: &[&str],
) -> (
    ProxyTarget,
    tokio::sync::oneshot::Receiver<Option<Vec<u8>>>,
    NonDetachingJoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (key, cert) = self_signed_for_loopback();
    let mut target = upstream(listener.local_addr().unwrap());
    target.connect_ssl = Some(Arc::new(
        client_config(provider(), [cert.as_ref()]).unwrap(),
    ));
    let mut config = server_config(&key, &cert);
    config.alpn_protocols = alpn.iter().map(|p| p.as_bytes().to_vec()).collect();
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let (report, negotiated) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        if let Ok(stream) = acceptor.accept(stream).await {
            let _ = report.send(stream.get_ref().1.alpn_protocol().map(<[u8]>::to_vec));
            let _ = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new())
                .serve_connection(
                    TokioIo::new(stream),
                    service_fn(|_: Request<hyper::body::Incoming>| async {
                        Ok::<_, hyper::Error>(Response::new(Full::new(Bytes::from_static(
                            b"secure upstream",
                        ))))
                    }),
                )
                .await;
        }
    });
    (target, negotiated, server.into())
}

async fn query(
    route: &ChainRoute,
    protocol: &HttpProxyProtocol,
) -> Response<hyper::body::Incoming> {
    let stream = TokioIo::new(route.target.connect().await);
    match protocol {
        HttpProxyProtocol::Http1 => {
            let (mut client, connection) =
                hyper::client::conn::http1::handshake(stream).await.unwrap();
            tokio::spawn(connection);
            client.send_request(request()).await.unwrap()
        }
        HttpProxyProtocol::Http2 => {
            let (mut client, connection) =
                hyper::client::conn::http2::handshake(TokioExecutor::new(), stream)
                    .await
                    .unwrap();
            tokio::spawn(connection);
            client.send_request(request()).await.unwrap()
        }
    }
}

#[tokio::test]
async fn unix_continuation_retains_upstream_tls_validation_and_framing() {
    tokio::time::timeout(Duration::from_secs(10), async {
        for protocol in [HttpProxyProtocol::Http1, HttpProxyProtocol::Http2] {
            let sockets = sockets().await;
            let _waf = copying_stage(&sockets, "waf", Arc::new(SyncMutex::new(Vec::new()))).await;
            let (target, negotiated, _app) = tls_application(&["h2", "http/1.1"]).await;
            let route = ChainRoute::new(&sockets, &binding(), &[stage("waf")], target).await;
            let response = query(&route, &protocol).await;
            assert_eq!(response.status(), 200);
            assert_eq!(
                response.into_body().collect().await.unwrap().to_bytes(),
                "secure upstream"
            );
            assert_eq!(negotiated.await.unwrap().as_deref(), Some(protocol.alpn()));

            let (mut target, negotiated, _app) = tls_application(&["h2", "http/1.1"]).await;
            target.connect_ssl = Some(Arc::new(
                client_config(provider(), std::iter::empty()).unwrap(),
            ));
            drop(route);
            let route = ChainRoute::new(&sockets, &binding(), &[stage("waf")], target).await;
            assert_eq!(query(&route, &protocol).await.status(), 503);
            assert!(negotiated.await.is_err());
        }
    })
    .await
    .expect("TLS continuation stalled");
}

#[tokio::test]
async fn http2_continuation_rejects_a_backend_that_does_not_negotiate_http2() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let sockets = sockets().await;
        let _waf = copying_stage(&sockets, "waf", Arc::new(SyncMutex::new(Vec::new()))).await;
        let (target, negotiated, _app) = tls_application(&[]).await;
        let route = ChainRoute::new(&sockets, &binding(), &[stage("waf")], target).await;
        assert_eq!(query(&route, &HttpProxyProtocol::Http2).await.status(), 503);
        assert_eq!(negotiated.await.unwrap(), None);
    })
    .await
    .expect("HTTP/2 ALPN rejection stalled");
}
