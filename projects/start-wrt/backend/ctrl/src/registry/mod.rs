pub mod asset;
pub mod device_info;
pub mod os;
pub mod signer;

use http::HeaderValue;
use reqwest::Client;
use serde_json::Value;
use url::Url;

use crate::Error;

// ── Registry RPC client ──────────────────────────────────────────────

/// Make a JSON-RPC call to the Start9 registry.
///
/// This mirrors start-os's `call_remote` in `middleware/auth/signature.rs`.
pub async fn call_registry_rpc(
    client: &Client,
    registry_url: &str,
    method: &str,
    params: Value,
    device_info_header: Option<HeaderValue>,
) -> Result<Value, Error> {
    // Build JSON-RPC request
    let rpc_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": method,
        "params": params,
    });
    let body = serde_json::to_vec(&rpc_req)
        .map_err(|e| Error::other(format!("serializing RPC request: {e}")))?;

    // Build URL: {registry}/rpc/v0
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

    // Add device info header
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

    // Extract result from JSON-RPC response
    if let Some(error) = rpc_res.get("error") {
        return Err(Error::other(format!("registry RPC error: {error}")));
    }
    rpc_res
        .get("result")
        .cloned()
        .ok_or_else(|| Error::other("registry response missing 'result' field"))
}
