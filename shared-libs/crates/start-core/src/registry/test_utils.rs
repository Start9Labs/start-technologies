pub(super) use super::test_context as context;
use super::*;

pub(super) async fn registry_server(
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
