use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::time::{Instant, timeout};

use super::reachable_ip;
use crate::error::ErrorKind;

#[tokio::test(start_paused = true)]
async fn connection_deadline_includes_pending_name_resolution() {
    let (sender, receiver) = oneshot::channel::<()>();
    let lookup = async move {
        let _sender = sender;
        std::future::pending::<io::Result<std::vec::IntoIter<SocketAddr>>>().await
    };
    let started = Instant::now();
    let error = timeout(Duration::from_secs(31), reachable_ip(lookup))
        .await
        .expect("name resolution exceeded the connection deadline")
        .unwrap_err();

    assert_eq!(error.kind, ErrorKind::Timeout);
    assert_eq!(started.elapsed(), Duration::from_secs(30));
    assert!(receiver.await.is_err());
}

#[tokio::test(start_paused = true)]
async fn failed_or_empty_resolution_reports_a_network_error_without_waiting() {
    for result in [
        Err(io::Error::from(io::ErrorKind::NotFound)),
        Ok(Vec::<SocketAddr>::new().into_iter()),
    ] {
        let started = Instant::now();
        let error = reachable_ip(std::future::ready(result)).await.unwrap_err();

        assert_eq!(error.kind, ErrorKind::Network);
        assert_eq!(started.elapsed(), Duration::ZERO);
    }
}

#[tokio::test]
async fn successful_probe_returns_the_reachable_address() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    let lookup = std::future::ready(Ok(vec![address].into_iter()));

    assert_eq!(reachable_ip(lookup).await.unwrap(), "127.0.0.1");
}
