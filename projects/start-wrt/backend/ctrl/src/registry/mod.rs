pub use startos::registry::{asset, signer};

pub mod device_info;
pub mod os;

use http::HeaderValue;
use reqwest::Client;
use rpc_toolkit::yajrc::{GenericRpcMethod, Id, RpcRequest};
use serde_json::Value;
use url::Url;

use crate::Error;

/// Sends unsigned requests with a 30-second request timeout.
pub async fn call_registry_rpc(
    client: &Client,
    registry_url: &str,
    method: &str,
    params: Value,
    device_info_header: Option<HeaderValue>,
) -> Result<Value, Error> {
    let mut rpc_req = serde_json::to_value(RpcRequest {
        id: Some(Id::Number(0.into())),
        method: GenericRpcMethod::<_, _, Value>::new(method),
        params,
    })?;
    // `RpcRequest` omits the JSON-RPC version tag.
    rpc_req["jsonrpc"] = "2.0".into();
    let body = serde_json::to_vec(&rpc_req)?;

    let url: Url = registry_url
        .parse()
        .map_err(|e| Error::other(format!("invalid registry URL: {e}")))?;
    let mut url = url;
    url.path_segments_mut()
        .map_err(|_| Error::other("cannot extend registry URL path"))?
        .push("rpc")
        .push("v0");

    let mut req = client
        .post(url.clone())
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(http::header::ACCEPT, "application/json")
        .header(http::header::CONTENT_LENGTH, body.len());

    if let Some(di) = device_info_header {
        req = req.header(device_info::DEVICE_INFO_HEADER, di);
    }

    let res = req
        .timeout(std::time::Duration::from_secs(30))
        .body(body)
        .send()
        .await
        .map_err(|e| Error::other(format!("registry request failed: {e}")))?;

    if !res.status().is_success() {
        let status = res.status();
        let txt = res.text().await.unwrap_or_default();
        return Err(Error::other(format!(
            "registry returned {}: {}",
            status, txt
        )));
    }

    let content_type = res
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !content_type.contains("application/json") {
        return Err(Error::other(format!(
            "unexpected content type from registry: {content_type}"
        )));
    }

    let rpc_res: serde_json::Value = res
        .json()
        .await
        .map_err(|e| Error::other(format!("parsing registry response: {e}")))?;

    if let Some(error) = rpc_res.get("error") {
        return Err(Error::other(format!("registry RPC error: {error}")));
    }
    rpc_res
        .get("result")
        .cloned()
        .ok_or_else(|| Error::other("registry response missing 'result' field"))
}
