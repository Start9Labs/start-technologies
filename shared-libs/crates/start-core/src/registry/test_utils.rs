pub(crate) use super::test_context as context;
use super::*;

pub(crate) async fn registry_server(
    ctx: RegistryContext,
) -> (Url, crate::util::future::NonDetachingJoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, crate::registry::registry_router(ctx))
            .await
            .unwrap();
    });
    (url, task.into())
}

pub(crate) async fn socks_client(
    url: &Url,
) -> (
    crate::context::rpc::ReloadableHttpClient,
    crate::util::future::NonDetachingJoinHandle<()>,
) {
    use fast_socks5::server::Socks5ServerProtocol;
    use fast_socks5::util::target_addr::TargetAddr;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = crate::context::rpc::ReloadableHttpClient::new(format!(
        "socks5h://{}",
        listener.local_addr().unwrap()
    ))
    .unwrap();
    let host = url.host_str().unwrap().to_owned();
    let port = url.port_or_known_default().unwrap();
    let task = tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                stream = listener.accept() => {
                    let (stream, _) = stream.unwrap();
                    let host = host.clone();
                    connections.spawn(async move {
                        let (protocol, _, target) = Socks5ServerProtocol::accept_no_auth(stream).await.unwrap().read_command().await.unwrap();
                        if host.parse::<std::net::IpAddr>().is_err() {
                            assert_eq!(target, TargetAddr::Domain(host, port));
                        }
                        let mut upstream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
                        let mut stream = protocol.reply_success("127.0.0.1:0".parse().unwrap()).await.unwrap();
                        let _ = tokio::io::copy_bidirectional(&mut stream, &mut upstream).await;
                    });
                }
                result = connections.join_next(), if !connections.is_empty() => { result.unwrap().unwrap(); }
            }
        }
    }).into();
    (client, task)
}
