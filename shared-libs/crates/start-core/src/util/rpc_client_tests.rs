use std::os::unix::fs::symlink;

use tokio::net::UnixListener;

use super::*;

#[tokio::test]
async fn container_rpc_reconnect_refuses_a_replaced_socket_symlink() {
    let dir = crate::util::io::TmpDir::new().await.unwrap();
    let service = dir.join("service.sock");
    let listener = UnixListener::bind(&service).unwrap();
    let client = UnixRpcClient::new(service.clone());
    let connected = client.pool.clone().get().await.unwrap();
    let accepted = listener.accept().await.unwrap();
    connected.destroy();
    drop(accepted);

    let sensitive = dir.join("host.sock");
    let _host = UnixListener::bind(&sensitive).unwrap();
    std::fs::remove_file(&service).unwrap();
    symlink(&sensitive, &service).unwrap();
    assert!(client.pool.clone().get().await.is_err());
}

#[tokio::test]
async fn container_rpc_reconnect_accepts_a_new_real_socket() {
    let dir = crate::util::io::TmpDir::new().await.unwrap();
    let service = dir.join("service.sock");
    let listener = UnixListener::bind(&service).unwrap();
    let client = UnixRpcClient::new(service.clone());
    let connected = client.pool.clone().get().await.unwrap();
    let accepted = listener.accept().await.unwrap();
    connected.destroy();
    drop((accepted, listener));

    std::fs::remove_file(&service).unwrap();
    let replacement = UnixListener::bind(&service).unwrap();
    let reconnected = client.pool.clone().get().await.unwrap();
    let accepted = replacement.accept().await.unwrap();
    drop((reconnected, accepted));
}
