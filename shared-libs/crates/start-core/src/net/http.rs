use std::collections::HashMap;
use std::convert::Infallible;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures::FutureExt;
use http::{HeaderMap, HeaderValue};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::{Body as HyperBody, Incoming};
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use tokio::sync::Mutex;

use crate::net::host::binding::ProxyAuth;
use crate::prelude::*;
use crate::util::io::ReadWriter;
use crate::util::serde::MaybeUtf8String;

/// Body type returned by the proxy service: either an upstream response body
/// (`hyper::body::Incoming`) or a synthetic 401 body (`Full<Bytes>`), unified
/// through `BoxBody`.
type ProxyBody = BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>;

fn box_incoming(b: Incoming) -> ProxyBody {
    b.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })
        .boxed()
}

fn box_full(bytes: Bytes) -> ProxyBody {
    Full::new(bytes).map_err(|e: Infallible| match e {}).boxed()
}

/// Pre-compiled view of a [`ProxyAuth`] used on the proxy hot-path.
///
/// We hash on the *full* `Authorization` header value (e.g.
/// `"Basic dXNlcjpwYXNz"`), so each request is one hashmap lookup and one
/// `HeaderMap::get` — no allocation, no base64, no per-credential loop.
#[derive(Clone)]
pub struct AuthGate {
    /// Map of valid `Authorization` header value → optional `X-Forwarded-User`.
    /// `None` for Bearer (no user concept), `Some(username)` for Basic.
    valid: Arc<HashMap<HeaderValue, Option<HeaderValue>>>,
    /// Pre-built `WWW-Authenticate` challenge sent on 401 responses.
    challenge: HeaderValue,
}

/// Sanitize a package-supplied realm into something that fits inside an
/// RFC 7230 `quoted-string`: drop `"` and `\` (the only two
/// metacharacters in `quoted-string`) and any control bytes. We don't
/// percent-encode — a realm is operator-facing, so silently dropping
/// junk is friendlier than rejecting the whole binding for a stray
/// quote.
fn sanitize_realm(realm: &str) -> String {
    realm
        .chars()
        .filter(|c| *c != '"' && *c != '\\' && !c.is_control())
        .collect()
}

impl AuthGate {
    pub fn from_auth(auth: &ProxyAuth) -> Result<Self, Error> {
        use base64::Engine;
        let mut valid: HashMap<HeaderValue, Option<HeaderValue>> = HashMap::new();
        let (scheme, realm) = match auth {
            ProxyAuth::Bearer { tokens, realm } => {
                for token in tokens {
                    let v = HeaderValue::from_str(&format!("Bearer {token}"))
                        .with_kind(ErrorKind::InvalidRequest)?;
                    valid.insert(v, None);
                }
                ("Bearer", realm.as_deref())
            }
            ProxyAuth::Basic { credentials, realm } => {
                for cred in credentials {
                    let raw = format!("{}:{}", cred.username, cred.password);
                    let encoded = base64::engine::general_purpose::STANDARD.encode(raw.as_bytes());
                    let header = HeaderValue::from_str(&format!("Basic {encoded}"))
                        .with_kind(ErrorKind::InvalidRequest)?;
                    let user = HeaderValue::from_str(&cred.username)
                        .with_kind(ErrorKind::InvalidRequest)?;
                    valid.insert(header, Some(user));
                }
                ("Basic", realm.as_deref())
            }
        };
        let realm = realm.map(sanitize_realm);
        let realm = realm.as_deref().unwrap_or("StartOS");
        let challenge = HeaderValue::from_str(&format!("{scheme} realm=\"{realm}\""))
            .with_kind(ErrorKind::InvalidRequest)?;
        Ok(Self {
            valid: Arc::new(valid),
            challenge,
        })
    }

    /// Validate the `Authorization` header. On success returns the optional
    /// `X-Forwarded-User` value to inject. On failure returns the 401
    /// response that should be sent back to the client.
    fn check(&self, headers: &HeaderMap) -> Result<Option<HeaderValue>, Response<ProxyBody>> {
        match headers.get(http::header::AUTHORIZATION) {
            Some(v) => match self.valid.get(v) {
                Some(user) => Ok(user.clone()),
                None => Err(self.unauthorized()),
            },
            None => Err(self.unauthorized()),
        }
    }

    fn unauthorized(&self) -> Response<ProxyBody> {
        Response::builder()
            .status(http::StatusCode::UNAUTHORIZED)
            .header(http::header::WWW_AUTHENTICATE, self.challenge.clone())
            .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(box_full(Bytes::from_static(b"401 Unauthorized")))
            .expect("static 401 response is well-formed")
    }
}

/// Apply the OS reverse-proxy header policy to an incoming request.
///
/// - Validates the `AuthGate` if present, short-circuiting with a 401 on
///   failure.
/// - Strips any client-supplied `X-Forwarded-*` and `X-Forwarded-User`
///   headers so a downstream service can trust them.
/// - Optionally adds `X-Forwarded-For` / `X-Forwarded-Proto`.
/// - On successful Basic auth, sets `X-Forwarded-User` to the authenticated
///   username.
fn apply_request_policy<B>(
    req: &mut Request<B>,
    src_ip: Option<IpAddr>,
    add_forwarded: bool,
    gate: Option<&AuthGate>,
) -> Result<(), Response<ProxyBody>> {
    // Always strip client-supplied forwarded identity. This function is
    // only ever reached on the HTTP-aware proxy path, which is only
    // entered when the binding actually owns these headers, so there is
    // nothing to preserve.
    let h = req.headers_mut();
    h.remove("X-Forwarded-User");
    h.remove("X-Forwarded-For");
    h.remove("X-Forwarded-Proto");

    let user = match gate {
        Some(g) => g.check(req.headers())?,
        None => None,
    };

    let h = req.headers_mut();
    if add_forwarded {
        h.insert("X-Forwarded-Proto", HeaderValue::from_static("https"));
        if let Some(src_ip) = src_ip
            .map(|s| s.to_string())
            .as_deref()
            .and_then(|s| HeaderValue::from_str(s).ok())
        {
            h.insert("X-Forwarded-For", src_ip);
        }
    }
    if let Some(user) = user {
        h.insert("X-Forwarded-User", user);
    }
    Ok(())
}

pub async fn handle_http_on_https(stream: impl ReadWriter + Unpin + 'static) -> Result<(), Error> {
    use axum::body::Body;
    use axum::extract::Request;
    use axum::response::Response;
    use http::Uri;

    use crate::net::static_server::server_error;

    hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new())
        .serve_connection(
            hyper_util::rt::TokioIo::new(stream),
            hyper_util::service::TowerToHyperService::new(axum::Router::new().fallback(
                axum::routing::method_routing::any(move |req: Request| async move {
                    match async move {
                        let host = req
                            .headers()
                            .get(http::header::HOST)
                            .and_then(|host| host.to_str().ok());
                        if let Some(host) = host {
                            let uri = Uri::from_parts({
                                let mut parts = req.uri().to_owned().into_parts();
                                parts.scheme = Some("https".parse()?);
                                parts.authority = Some(host.parse()?);
                                parts
                            })?;
                            Response::builder()
                                .status(http::StatusCode::TEMPORARY_REDIRECT)
                                .header(http::header::LOCATION, uri.to_string())
                                .body(Body::default())
                        } else {
                            Response::builder()
                                .status(http::StatusCode::BAD_REQUEST)
                                .body(Body::from("Host header required"))
                        }
                    }
                    .await
                    {
                        Ok(a) => a,
                        Err(e) => {
                            tracing::warn!("Error redirecting http request on ssl port: {e}");
                            tracing::error!("{e:?}");
                            server_error(Error::new(e, ErrorKind::Network))
                        }
                    }
                }),
            )),
        )
        .await
        .map_err(|e| Error::new(color_eyre::eyre::Report::msg(e), ErrorKind::Network))
}

pub async fn run_http_proxy<F, T>(
    from: F,
    to: T,
    alpn: Option<MaybeUtf8String>,
    src_ip: Option<IpAddr>,
    add_forwarded: bool,
    gate: Option<AuthGate>,
) -> Result<(), Error>
where
    F: ReadWriter + Unpin + Send + 'static,
    T: ReadWriter + Unpin + Send + 'static,
{
    if alpn
        .as_ref()
        .map(|alpn| alpn.0.as_slice() == b"h2")
        .unwrap_or(false)
    {
        run_http2_proxy(from, to, src_ip, add_forwarded, gate).await
    } else {
        run_http1_proxy(from, to, src_ip, add_forwarded, gate).await
    }
}

pub async fn run_http2_proxy<F, T>(
    from: F,
    to: T,
    src_ip: Option<IpAddr>,
    add_forwarded: bool,
    gate: Option<AuthGate>,
) -> Result<(), Error>
where
    F: ReadWriter + Unpin + Send + 'static,
    T: ReadWriter + Unpin + Send + 'static,
{
    let (client, to) = hyper::client::conn::http2::Builder::new(TokioExecutor::new())
        .timer(TokioTimer::new())
        .adaptive_window(true)
        .keep_alive_interval(Duration::from_secs(25))
        .keep_alive_timeout(Duration::from_secs(300))
        .handshake(TokioIo::new(to))
        .await?;
    let from = hyper::server::conn::http2::Builder::new(TokioExecutor::new())
        .timer(TokioTimer::new())
        .enable_connect_protocol()
        .adaptive_window(true)
        .keep_alive_interval(Duration::from_secs(25)) // Add this
        .keep_alive_timeout(Duration::from_secs(300))
        .serve_connection(
            TokioIo::new(from),
            service_fn(move |mut req| {
                let mut client = client.clone();
                let gate = gate.clone();
                async move {
                    if let Err(resp) =
                        apply_request_policy(&mut req, src_ip, add_forwarded, gate.as_ref())
                    {
                        return Ok::<_, hyper::Error>(resp);
                    }

                    let upgrade = if req.method() == http::method::Method::CONNECT
                        && req.extensions().get::<hyper::ext::Protocol>().is_some()
                    {
                        Some(hyper::upgrade::on(&mut req))
                    } else {
                        None
                    };

                    let mut res = match client.send_request(req).await {
                        Ok(r) => r,
                        Err(e) => return Err(e),
                    };

                    if let Some(from) = upgrade {
                        let to = hyper::upgrade::on(&mut res);
                        tokio::task::spawn(async move {
                            if let Some((from, to)) = futures::future::try_join(from, to).await.ok()
                            {
                                tokio::io::copy_bidirectional(
                                    &mut TokioIo::new(from),
                                    &mut TokioIo::new(to),
                                )
                                .await
                                .ok();
                            }
                        });
                    }

                    Ok::<_, hyper::Error>(res.map(box_incoming))
                }
            }),
        );
    futures::future::try_join(from.boxed(), to.boxed()).await?;

    Ok(())
}

pub async fn run_http1_proxy<F, T>(
    from: F,
    to: T,
    src_ip: Option<IpAddr>,
    add_forwarded: bool,
    gate: Option<AuthGate>,
) -> Result<(), Error>
where
    F: ReadWriter + Unpin + Send + 'static,
    T: ReadWriter + Unpin + Send + 'static,
{
    let (client, to) = hyper::client::conn::http1::Builder::new()
        .title_case_headers(true)
        .preserve_header_case(true)
        .handshake(TokioIo::new(to))
        .await?;
    let client = Arc::new(Mutex::new(client));
    // hyper disarms `header_read_timeout` while a body or upgrade is in
    // flight, so this can't kill an active stream — only idle keep-alive.
    let from = hyper::server::conn::http1::Builder::new()
        .timer(TokioTimer::new())
        .header_read_timeout(Duration::from_secs(60))
        .serve_connection(
            TokioIo::new(from),
            service_fn(move |mut req| {
                let client = client.clone();
                let gate = gate.clone();
                async move {
                    if let Err(resp) =
                        apply_request_policy(&mut req, src_ip, add_forwarded, gate.as_ref())
                    {
                        return Ok::<_, hyper::Error>(resp);
                    }

                    let upgrade =
                        if req
                            .headers()
                            .get(http::header::CONNECTION)
                            .map_or(false, |h| {
                                h.to_str()
                                    .unwrap_or_default()
                                    .split(",")
                                    .any(|s| s.trim().eq_ignore_ascii_case("upgrade"))
                            })
                        {
                            Some(hyper::upgrade::on(&mut req))
                        } else {
                            None
                        };

                    let mut res = match client.lock().await.send_request(req).await {
                        Ok(r) => r,
                        Err(e) => return Err(e),
                    };

                    if let Some(from) = upgrade {
                        let kind = res
                            .headers()
                            .get(http::header::UPGRADE)
                            .map(|h| h.to_owned());
                        let to = hyper::upgrade::on(&mut res);
                        tokio::task::spawn(async move {
                            if let Some((from, to)) = futures::future::try_join(from, to).await.ok()
                            {
                                if kind.map_or(false, |k| k == "HTTP/2.0") {
                                    // Inner upgraded HTTP/2 connection is the
                                    // same logical request stream — auth was
                                    // already validated and forwarded
                                    // headers were already applied on the
                                    // outer hop. Pass the upgraded stream
                                    // through with no additional policy.
                                    run_http2_proxy(
                                        TokioIo::new(from),
                                        TokioIo::new(to),
                                        src_ip,
                                        false,
                                        None,
                                    )
                                    .await
                                    .ok();
                                } else {
                                    tokio::io::copy_bidirectional(
                                        &mut TokioIo::new(from),
                                        &mut TokioIo::new(to),
                                    )
                                    .await
                                    .ok();
                                }
                            }
                        });
                    }

                    Ok::<_, hyper::Error>(res.map(box_incoming))
                }
            }),
        );
    futures::future::try_join(from.with_upgrades().boxed(), to.with_upgrades().boxed()).await?;

    Ok(())
}

// Silence unused-import lints that may show up depending on feature flags.
#[allow(dead_code)]
fn _assert_body_bounds() {
    fn assert_body<B: HyperBody + Send + 'static>() {}
    assert_body::<ProxyBody>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::host::binding::BasicCredential;

    fn req_with_auth(auth: Option<&str>) -> Request<()> {
        let mut b = Request::builder().uri("/");
        if let Some(a) = auth {
            b = b.header(http::header::AUTHORIZATION, a);
        }
        b.body(()).unwrap()
    }

    #[test]
    fn basic_gate_accepts_listed_credentials_and_forwards_user() {
        let gate = AuthGate::from_auth(&ProxyAuth::Basic {
            credentials: vec![
                BasicCredential {
                    username: "alice".into(),
                    password: "hunter2".into(),
                },
                BasicCredential {
                    username: "bob".into(),
                    password: "swordfish".into(),
                },
            ],
            realm: None,
        })
        .unwrap();

        // "alice:hunter2" -> base64
        let mut req = req_with_auth(Some("Basic YWxpY2U6aHVudGVyMg=="));
        apply_request_policy(&mut req, None, false, Some(&gate)).unwrap();
        assert_eq!(req.headers().get("X-Forwarded-User").unwrap(), "alice");

        // "bob:swordfish"
        let mut req = req_with_auth(Some("Basic Ym9iOnN3b3JkZmlzaA=="));
        apply_request_policy(&mut req, None, false, Some(&gate)).unwrap();
        assert_eq!(req.headers().get("X-Forwarded-User").unwrap(), "bob");
    }

    #[test]
    fn basic_gate_rejects_unknown_and_missing() {
        let gate = AuthGate::from_auth(&ProxyAuth::Basic {
            credentials: vec![BasicCredential {
                username: "alice".into(),
                password: "hunter2".into(),
            }],
            realm: Some("My App".into()),
        })
        .unwrap();

        // wrong password
        let mut req = req_with_auth(Some("Basic YWxpY2U6d3Jvbmc="));
        let resp = apply_request_policy(&mut req, None, false, Some(&gate)).unwrap_err();
        assert_eq!(resp.status(), http::StatusCode::UNAUTHORIZED);
        assert_eq!(
            resp.headers().get(http::header::WWW_AUTHENTICATE).unwrap(),
            "Basic realm=\"My App\""
        );

        // missing header
        let mut req = req_with_auth(None);
        let resp = apply_request_policy(&mut req, None, false, Some(&gate)).unwrap_err();
        assert_eq!(resp.status(), http::StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn bearer_gate_accepts_any_listed_token_and_does_not_set_user() {
        let gate = AuthGate::from_auth(&ProxyAuth::Bearer {
            tokens: vec!["alpha".into(), "beta".into()],
            realm: None,
        })
        .unwrap();

        let mut req = req_with_auth(Some("Bearer alpha"));
        apply_request_policy(&mut req, None, false, Some(&gate)).unwrap();
        assert!(req.headers().get("X-Forwarded-User").is_none());

        let mut req = req_with_auth(Some("Bearer gamma"));
        let resp = apply_request_policy(&mut req, None, false, Some(&gate)).unwrap_err();
        assert_eq!(resp.status(), http::StatusCode::UNAUTHORIZED);
        assert!(
            resp.headers()
                .get(http::header::WWW_AUTHENTICATE)
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("Bearer ")
        );
    }

    #[test]
    fn client_supplied_x_forwarded_user_is_stripped_even_with_no_gate() {
        // With no gate, we still strip X-Forwarded-User so a malicious
        // client can't impersonate someone for an unauthenticated upstream.
        let mut req = req_with_auth(None);
        req.headers_mut()
            .insert("X-Forwarded-User", HeaderValue::from_static("root"));
        apply_request_policy(&mut req, None, false, None).unwrap();
        assert!(req.headers().get("X-Forwarded-User").is_none());
    }

    #[test]
    fn realm_is_sanitized_against_quoted_string_metacharacters() {
        // `"` and `\` are the only metacharacters in an RFC 7230
        // quoted-string. The sanitizer drops them rather than escaping,
        // so a hostile realm can't break out of the challenge header.
        let gate = AuthGate::from_auth(&ProxyAuth::Basic {
            credentials: vec![BasicCredential {
                username: "alice".into(),
                password: "hunter2".into(),
            }],
            realm: Some("hax\"\\\nor".into()),
        })
        .unwrap();
        let mut req = req_with_auth(None);
        let resp = apply_request_policy(&mut req, None, false, Some(&gate)).unwrap_err();
        assert_eq!(
            resp.headers().get(http::header::WWW_AUTHENTICATE).unwrap(),
            "Basic realm=\"haxor\""
        );
    }

    #[test]
    fn client_supplied_x_forwarded_user_is_replaced_by_authenticated_user() {
        let gate = AuthGate::from_auth(&ProxyAuth::Basic {
            credentials: vec![BasicCredential {
                username: "alice".into(),
                password: "hunter2".into(),
            }],
            realm: None,
        })
        .unwrap();
        let mut req = req_with_auth(Some("Basic YWxpY2U6aHVudGVyMg=="));
        req.headers_mut()
            .insert("X-Forwarded-User", HeaderValue::from_static("root"));
        apply_request_policy(&mut req, None, false, Some(&gate)).unwrap();
        assert_eq!(req.headers().get("X-Forwarded-User").unwrap(), "alice");
    }
}
