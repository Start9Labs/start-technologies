use std::net::Ipv4Addr;
use std::time::Duration;

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::time::{Instant, timeout};

use super::reachable_ip;
use crate::prelude::*;

#[tokio::test(start_paused = true)]
async fn connection_deadline_includes_pending_name_resolution() {
    let (sender, receiver) = oneshot::channel::<()>();
    let connect = async move {
        let _sender = sender;
        std::future::pending::<Result<TcpStream, Error>>().await
    };
    let started = Instant::now();
    let error = timeout(Duration::from_secs(31), reachable_ip(connect))
        .await
        .expect("name resolution exceeded the connection deadline")
        .unwrap_err();

    assert_eq!(error.kind, ErrorKind::Timeout);
    assert_eq!(started.elapsed(), Duration::from_secs(30));
    assert!(receiver.await.is_err());
}

#[tokio::test]
async fn successful_connection_returns_the_reachable_address() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    let connect = async move {
        TcpStream::connect(address)
            .await
            .with_kind(ErrorKind::Network)
    };

    assert_eq!(reachable_ip(connect).await.unwrap(), "127.0.0.1");
}
