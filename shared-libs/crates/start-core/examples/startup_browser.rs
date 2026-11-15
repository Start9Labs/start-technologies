use std::sync::Arc;

use start_core::context::InitContext;
use start_core::context::config::ServerConfig;
use start_core::net::dns::DnsController;
use start_core::net::static_server::ui_router;
use start_core::net::web_server::{Acceptor, WebServer};
use start_core::util::sync::Watch;

#[tokio::main]
async fn main() {
    let dns = Arc::new(DnsController::new(Watch::new(Default::default())));
    let context = InitContext::init(&ServerConfig::default(), dns)
        .await
        .unwrap();
    let _phase = context
        .progress
        .add_phase("x".repeat(17 * 1024 * 1024).into(), None);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let _server = WebServer::new(Acceptor::new(listener), ui_router(context));
    println!("http://{address}");
    std::future::pending::<()>().await;
}
