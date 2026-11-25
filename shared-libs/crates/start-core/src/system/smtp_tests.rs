use std::net::Ipv4Addr;
use std::time::Duration;

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::time::{Instant, timeout};

use super::{SetSmtpParams, SmtpSecurity, reachable_ip};
use crate::prelude::*;

#[test]
fn setter_preserves_existing_wire_names_and_defaults() {
    for input in [
        serde_json::json!({
            "host": "smtp.example.com", "port": 587,
            "from": "a@example.com", "username": "a",
        }),
        serde_json::json!({
            "server": "smtp.example.com", "port": 587,
            "from": "a@example.com", "login": "a",
        }),
    ] {
        let params: SetSmtpParams = serde_json::from_value(input).unwrap();
        assert_eq!(params.smtp.host, "smtp.example.com");
        assert_eq!(params.smtp.port, 587);
        assert_eq!(params.smtp.from, "a@example.com");
        assert_eq!(params.smtp.username, "a");
        assert!(params.smtp.password.is_none());
        assert!(matches!(params.smtp.security, SmtpSecurity::Starttls));
        assert!(!params.keep_password);
    }
}

#[test]
fn setter_accepts_password_retention_with_legacy_wire_names() {
    let params: SetSmtpParams = serde_json::from_value(serde_json::json!({
        "server": "smtp.example.com", "port": 465,
        "from": "a@example.com", "login": "a",
        "password": "replacement", "keepPassword": true, "security": "tls",
    }))
    .unwrap();

    assert_eq!(params.smtp.password.as_deref(), Some("replacement"));
    assert!(params.keep_password);
    assert!(matches!(params.smtp.security, SmtpSecurity::Tls));
}

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
