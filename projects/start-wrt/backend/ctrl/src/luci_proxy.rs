use axum::body::Body;
use axum::extract::{OriginalUri, Request};
use axum::http::{HeaderMap, Response, StatusCode};
use axum::Extension;

/// Shared reqwest client for proxying requests to uhttpd.
#[derive(Clone)]
pub struct ProxyClient(pub reqwest::Client);

/// Reverse-proxy handler: forwards requests to uhttpd on localhost:8080.
/// Follows redirects internally (up to 5 hops), collecting Set-Cookie headers
/// and forwarding them on subsequent requests. This avoids browser cookie issues
/// where SameSite=strict cookies set during a 302 response aren't sent on the
/// redirected GET.
pub async fn handler(
    Extension(ProxyClient(client)): Extension<ProxyClient>,
    OriginalUri(original_uri): OriginalUri,
    req: Request,
) -> Response<Body> {
    let method = req.method().clone();
    let path_and_query = original_uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or(original_uri.path());

    let headers = req.headers().clone();

    let body = req.into_body();
    let body_bytes = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return error_response(StatusCode::BAD_REQUEST, "request body too large"),
    };

    let fwd_headers = reqwest_headers(&headers);

    let mut current_url = format!("http://127.0.0.1:8080{path_and_query}");
    let mut current_method = reqwest_method(&method);
    let mut current_body: Option<axum::body::Bytes> = if body_bytes.is_empty() {
        None
    } else {
        Some(body_bytes)
    };
    let mut all_set_cookies: Vec<reqwest::header::HeaderValue> = Vec::new();
    let mut extra_cookies: Vec<String> = Vec::new(); // "name=value" pairs from Set-Cookie

    tracing::debug!(
        "luci proxy {} {} (body_len={})",
        method,
        current_url,
        current_body.as_ref().map_or(0, |b| b.len())
    );

    for hop in 0..5u8 {
        let mut req_builder = client
            .request(current_method.clone(), &current_url)
            .headers(fwd_headers.clone());

        // Merge extra cookies from previous Set-Cookie headers into the cookie header,
        // deduplicating by name so new Set-Cookie values replace stale browser cookies.
        if !extra_cookies.is_empty() {
            let mut cookies: Vec<(String, String)> = fwd_headers
                .get("cookie")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .split(';')
                .filter_map(|c| {
                    let c = c.trim();
                    let (name, value) = c.split_once('=')?;
                    Some((name.to_string(), value.to_string()))
                })
                .collect();

            for extra in &extra_cookies {
                if let Some((name, value)) = extra.split_once('=') {
                    cookies.retain(|(n, _)| n != name);
                    cookies.push((name.to_string(), value.to_string()));
                }
            }

            let cookie_str: String = cookies
                .iter()
                .map(|(n, v)| format!("{n}={v}"))
                .collect::<Vec<_>>()
                .join("; ");
            req_builder = req_builder.header("cookie", &cookie_str);
        }

        if let Some(body) = current_body.take() {
            req_builder = req_builder.body(body);
        }

        let upstream = match req_builder.send().await {
            Ok(resp) => resp,
            Err(e) => {
                tracing::error!("luci proxy error (hop {hop}): {e}");
                return error_response(StatusCode::BAD_GATEWAY, "upstream unavailable");
            }
        };

        let status = upstream.status();
        tracing::debug!("luci proxy hop {hop}: {current_method} {current_url} → {status}");

        // Collect Set-Cookie headers from this hop
        for value in upstream.headers().get_all("set-cookie") {
            all_set_cookies.push(value.clone());
            // Parse "name=value" from "name=value; path=...; ..."
            if let Ok(s) = value.to_str() {
                if let Some(nv) = s.split(';').next() {
                    extra_cookies.push(nv.to_string());
                }
            }
        }

        if !status.is_redirection() {
            // Final response — return to browser with all accumulated Set-Cookie headers
            return build_final_response(upstream, &all_set_cookies).await;
        }

        // Follow redirect
        match upstream.headers().get("location") {
            Some(loc) => {
                current_url = resolve_location(&current_url, loc);
                current_method = reqwest::Method::GET;
                tracing::debug!("luci proxy following redirect → {current_url}");
            }
            None => {
                // Redirect without Location — return as-is
                return build_final_response(upstream, &all_set_cookies).await;
            }
        }
    }

    tracing::error!("luci proxy: too many redirects");
    error_response(StatusCode::BAD_GATEWAY, "too many redirects")
}

/// Build the final axum Response from the upstream reqwest response,
/// including all accumulated Set-Cookie headers from the redirect chain.
async fn build_final_response(
    upstream: reqwest::Response,
    all_set_cookies: &[reqwest::header::HeaderValue],
) -> Response<Body> {
    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let resp_headers = upstream.headers().clone();
    let resp_bytes = upstream.bytes().await.unwrap_or_default();

    let mut response = Response::builder().status(status);

    // Forward response headers, skipping hop-by-hop and set-cookie (we add those separately)
    for (key, value) in &resp_headers {
        let name = key.as_str();
        if matches!(
            name,
            "transfer-encoding" | "connection" | "keep-alive" | "upgrade" | "set-cookie"
        ) {
            continue;
        }
        response = response.header(key, value);
    }

    // Add ALL Set-Cookie headers from every hop in the chain
    for value in all_set_cookies {
        response = response.header("set-cookie", value);
    }

    response.body(Body::from(resp_bytes)).unwrap()
}

/// Resolve a Location header value against the current URL.
/// Handles absolute URLs, absolute paths, and relative paths.
fn resolve_location(current_url: &str, location: &reqwest::header::HeaderValue) -> String {
    let loc = location.to_str().unwrap_or("");

    if loc.starts_with("http://") || loc.starts_with("https://") {
        // Absolute URL — rewrite to go through our proxy to uhttpd
        loc.replace("http://localhost:8080", "http://127.0.0.1:8080")
            .to_string()
    } else if loc.starts_with('/') {
        // Absolute path — prepend origin
        format!("http://127.0.0.1:8080{loc}")
    } else {
        // Relative path — resolve against current URL's directory
        if let Some(base) = current_url.rfind('/') {
            format!("{}/{loc}", &current_url[..base])
        } else {
            format!("http://127.0.0.1:8080/{loc}")
        }
    }
}

fn error_response(status: StatusCode, msg: &str) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(Body::from(msg.to_string()))
        .unwrap()
}

fn reqwest_method(m: &axum::http::Method) -> reqwest::Method {
    reqwest::Method::from_bytes(m.as_str().as_bytes()).unwrap_or(reqwest::Method::GET)
}

/// Convert axum headers to reqwest headers, skipping hop-by-hop and
/// proxy-problematic headers that uhttpd may reject or misinterpret.
fn reqwest_headers(headers: &HeaderMap) -> reqwest::header::HeaderMap {
    let mut out = reqwest::header::HeaderMap::with_capacity(headers.len());
    for (key, value) in headers {
        let name = key.as_str();
        // Skip headers that must not be forwarded through a proxy:
        // - host: reqwest sets the correct Host from the target URL
        // - content-length: reqwest computes it from the body bytes
        // - hop-by-hop headers per RFC 2616 §13.5.1
        if matches!(
            name,
            "host"
                | "content-length"
                | "connection"
                | "keep-alive"
                | "transfer-encoding"
                | "te"
                | "trailer"
                | "upgrade"
        ) {
            continue;
        }
        // Strip StartWRT's "session" cookie so uhttpd only sees LuCI cookies
        if name == "cookie" {
            let filtered: Vec<&str> = value
                .to_str()
                .unwrap_or("")
                .split(';')
                .map(|c| c.trim())
                .filter(|c| !c.starts_with("session="))
                .collect();
            if !filtered.is_empty() {
                if let Ok(val) = reqwest::header::HeaderValue::from_str(&filtered.join("; ")) {
                    out.insert(reqwest::header::COOKIE, val);
                }
            }
            continue;
        }
        if let Ok(rname) = reqwest::header::HeaderName::from_bytes(key.as_str().as_bytes()) {
            if let Ok(val) = reqwest::header::HeaderValue::from_bytes(value.as_bytes()) {
                out.insert(rname, val);
            }
        }
    }
    out
}
