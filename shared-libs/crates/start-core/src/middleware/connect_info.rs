use std::net::SocketAddr;

use axum::extract::Request;
use axum::response::Response;
use imbl_value::json;
use rpc_toolkit::Middleware;
use serde::Deserialize;

#[derive(Clone, Default)]
pub struct ConnectInfo {
    peer_addr: Option<SocketAddr>,
    local_addr: Option<SocketAddr>,
}
impl ConnectInfo {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Deserialize)]
pub struct Metadata {
    get_connect_info: bool,
}

impl<Context: Send + Sync + 'static> Middleware<Context> for ConnectInfo {
    type Metadata = Metadata;
    async fn process_http_request(
        &mut self,
        _: &Context,
        request: &mut Request,
    ) -> Result<(), Response> {
        if let Some(axum::extract::ConnectInfo((peer, local))) = request.extensions().get().cloned()
        {
            self.peer_addr = Some(peer);
            self.local_addr = Some(local);
        }
        Ok(())
    }
    async fn process_rpc_request(
        &mut self,
        _: &Context,
        metadata: Self::Metadata,
        request: &mut rpc_toolkit::RpcRequest,
    ) -> Result<(), rpc_toolkit::RpcResponse> {
        if metadata.get_connect_info {
            if let Some(peer_addr) = self.peer_addr {
                super::params_object(&mut request.params)?
                    .insert("__ConnectInfo_peer_addr".into(), json!(peer_addr));
            }
            if let Some(local_addr) = self.local_addr {
                super::params_object(&mut request.params)?
                    .insert("__ConnectInfo_local_addr".into(), json!(local_addr));
            }
        }
        Ok(())
    }
}
