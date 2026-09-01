use std::collections::HashMap;
use std::convert::Infallible;
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use bytes::Bytes;
use futures::FutureExt;
use http::uri::{Authority, Scheme};
use http::{HeaderMap, HeaderValue};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::{Body as HyperBody, Incoming};
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use tokio::sync::{Mutex, Notify, oneshot};
use tokio_util::io::InspectReader;

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

/// Marks a response as still on its way to the client. `disable_keep_alive`
/// only closes the connection outright while it is idle; called mid-response
/// it stamps `Connection: close` on the head instead. Held by the relayed
/// body, so it drops once that head and body have been written.
struct Relaying {
    count: Arc<AtomicUsize>,
    done: Arc<Notify>,
}

impl Relaying {
    fn start(count: Arc<AtomicUsize>, done: Arc<Notify>) -> Self {
        count.fetch_add(1, Ordering::Relaxed);
        Self { count, done }
    }

    fn hold(self, body: ProxyBody) -> ProxyBody {
        body.map_frame(move |frame| {
            let _ = &self;
            frame
        })
        .boxed()
    }
}

impl Drop for Relaying {
    fn drop(&mut self) {
        if self.count.fetch_sub(1, Ordering::Relaxed) == 1 {
            self.done.notify_one();
        }
    }
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

pub fn request_authority<B>(req: &Request<B>) -> Option<Authority> {
    req.uri().authority().cloned().or_else(|| {
        req.headers()
            .get(http::header::HOST)?
            .to_str()
            .ok()?
            .parse()
            .ok()
    })
}

pub fn https_redirect_uri(uri: &http::Uri, authority: Authority) -> Result<http::Uri, http::Error> {
    let path_and_query = uri
        .path_and_query()
        .filter(|path| path.as_str() != "*")
        .map_or("/", |path| path.as_str());
    http::Uri::builder()
        .scheme(Scheme::HTTPS)
        .authority(authority)
        .path_and_query(path_and_query)
        .build()
}

pub async fn handle_http_on_https(stream: impl ReadWriter + Unpin + 'static) -> Result<(), Error> {
    use axum::body::Body;
    use axum::extract::Request;
    use axum::response::Response;

    use crate::net::static_server::server_error;

    hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new())
        .serve_connection(
            hyper_util::rt::TokioIo::new(stream),
            hyper_util::service::TowerToHyperService::new(axum::Router::new().fallback(
                axum::routing::method_routing::any(move |req: Request| async move {
                    match async move {
                        if let Some(authority) = request_authority(&req) {
                            let target = https_redirect_uri(req.uri(), authority)?;
                            Response::builder()
                                .status(http::StatusCode::TEMPORARY_REDIRECT)
                                .header(http::header::LOCATION, target.to_string())
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

const H2_SETTINGS_FRAME: u8 = 0x4;
const ENABLE_CONNECT_PROTOCOL: u16 = 0x8;
const MAX_SETTINGS_PAYLOAD: usize = 16_384;

type OpeningSettingsResult = Result<bool, &'static str>;

struct OpeningSettingsObserver {
    result: Option<oneshot::Sender<OpeningSettingsResult>>,
    header: [u8; 9],
    header_len: usize,
    payload_remaining: usize,
    entry: [u8; 6],
    entry_len: usize,
    extended_connect: bool,
}

impl OpeningSettingsObserver {
    fn new(result: oneshot::Sender<OpeningSettingsResult>) -> Self {
        Self {
            result: Some(result),
            header: [0; 9],
            header_len: 0,
            payload_remaining: 0,
            entry: [0; 6],
            entry_len: 0,
            extended_connect: false,
        }
    }

    fn inspect(&mut self, mut bytes: &[u8]) {
        if self.result.is_none() {
            return;
        }

        if self.header_len < self.header.len() {
            let len = bytes.len().min(self.header.len() - self.header_len);
            self.header[self.header_len..self.header_len + len].copy_from_slice(&bytes[..len]);
            self.header_len += len;
            bytes = &bytes[len..];
            if self.header_len < self.header.len() {
                return;
            }
            self.payload_remaining = ((self.header[0] as usize) << 16)
                | ((self.header[1] as usize) << 8)
                | self.header[2] as usize;
            if self.header[3] != H2_SETTINGS_FRAME {
                return self.finish(Err("backend did not open with SETTINGS"));
            }
            if self.header[4] & 0x1 != 0 {
                return self.finish(Err("backend opening SETTINGS was an acknowledgement"));
            }
            let stream_id = u32::from_be_bytes([
                self.header[5],
                self.header[6],
                self.header[7],
                self.header[8],
            ]) & 0x7fff_ffff;
            if stream_id != 0 {
                return self.finish(Err("backend opening SETTINGS used a nonzero stream"));
            }
            if self.payload_remaining > MAX_SETTINGS_PAYLOAD {
                return self.finish(Err("backend opening SETTINGS exceeded the maximum payload"));
            }
            if self.payload_remaining % self.entry.len() != 0 {
                return self.finish(Err("backend opening SETTINGS had a partial entry"));
            }
            if self.payload_remaining == 0 {
                return self.finish(Ok(false));
            }
        }

        while !bytes.is_empty() && self.result.is_some() {
            let len = bytes.len().min(self.entry.len() - self.entry_len);
            self.entry[self.entry_len..self.entry_len + len].copy_from_slice(&bytes[..len]);
            self.entry_len += len;
            self.payload_remaining -= len;
            bytes = &bytes[len..];

            if self.entry_len == self.entry.len() {
                let id = u16::from_be_bytes([self.entry[0], self.entry[1]]);
                let value = u32::from_be_bytes([
                    self.entry[2],
                    self.entry[3],
                    self.entry[4],
                    self.entry[5],
                ]);
                if id == ENABLE_CONNECT_PROTOCOL {
                    match value {
                        0 => self.extended_connect = false,
                        1 => self.extended_connect = true,
                        _ => {
                            return self.finish(Err(
                                "backend opening SETTINGS had an invalid ENABLE_CONNECT_PROTOCOL",
                            ));
                        }
                    }
                }
                self.entry_len = 0;
            }

            if self.payload_remaining == 0 {
                self.finish(Ok(self.extended_connect));
            }
        }
    }

    fn finish(&mut self, result: OpeningSettingsResult) {
        if let Some(sender) = self.result.take() {
            let _ = sender.send(result);
        }
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
    // Hyper fixes ENABLE_CONNECT_PROTOCOL when it creates the frontend connection.
    let (settings_sender, mut settings_receiver) = oneshot::channel();
    let mut settings_observer = OpeningSettingsObserver::new(settings_sender);
    let to = InspectReader::new(to, move |bytes| settings_observer.inspect(bytes));
    let (client, to) = hyper::client::conn::http2::Builder::new(TokioExecutor::new())
        .timer(TokioTimer::new())
        .adaptive_window(true)
        .keep_alive_interval(Duration::from_secs(25))
        .keep_alive_timeout(Duration::from_secs(300))
        .handshake(TokioIo::new(to))
        .await?;
    let mut to = Box::pin(to.fuse());
    let settings = tokio::select! {
        settings = &mut settings_receiver => {
            settings.map_err(|e| Error::new(e, ErrorKind::Network))?
        }
        res = to.as_mut() => match settings_receiver.try_recv() {
            Ok(settings) => settings,
            Err(_) => {
                res?;
                return Err(Error::new(
                    eyre!("backend http2 connection ended before opening SETTINGS"),
                    ErrorKind::Network,
                ));
            }
        }
    };
    let extended_connect =
        settings.map_err(|message| Error::new(eyre!(message), ErrorKind::Network))?;
    let from = {
        let mut server = hyper::server::conn::http2::Builder::new(TokioExecutor::new());
        server
            .timer(TokioTimer::new())
            .adaptive_window(true)
            .keep_alive_interval(Duration::from_secs(25))
            .keep_alive_timeout(Duration::from_secs(300));
        if extended_connect {
            server.enable_connect_protocol();
        }
        server.serve_connection(
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
        )
    };
    let mut from = Box::pin(from);
    loop {
        tokio::select! {
            res = from.as_mut() => return Ok(res?),
            res = to.as_mut() => {
                if let Err(e) = res {
                    tracing::warn!("Error proxying http2 connection to backend: {e}");
                    tracing::debug!("{e:?}");
                }
                from.as_mut().graceful_shutdown();
            }
        }
    }
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
    // Non-zero while a relayed response is still being written to the client.
    let relaying = Arc::new(AtomicUsize::new(0));
    let relayed = Arc::new(Notify::new());
    let svc_relaying = relaying.clone();
    let svc_relayed = relayed.clone();
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
                let relaying = svc_relaying.clone();
                let relayed = svc_relayed.clone();
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

                    // Taken before `send_request`, because the upstream
                    // connection can resolve in the same poll that delivers the
                    // response — before this future is resumed.
                    let relaying = Relaying::start(relaying, relayed);

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

                    Ok::<_, hyper::Error>(res.map(|b| relaying.hold(box_incoming(b))))
                }
            }),
        );
    let mut from = Box::pin(from.with_upgrades());
    let mut to = Box::pin(to.with_upgrades().fuse());
    let mut deferred = false;
    loop {
        tokio::select! {
            res = from.as_mut() => return Ok(res?),
            res = to.as_mut() => {
                res?;
                // The backend is gone. Hand the client the EOF now, the way the
                // splice path does, instead of leaving it a connection that
                // looks reusable and aborts the next request it carries. But
                // `disable_keep_alive` only closes outright when the connection
                // is idle — mid-response it stamps `Connection: close` on the
                // head instead, which is a header the backend never sent, and
                // on a 101 it lands on top of `Connection: Upgrade`.
                if relaying.load(Ordering::Relaxed) == 0 {
                    from.as_mut().graceful_shutdown();
                } else {
                    deferred = true;
                }
            }
            _ = relayed.notified(), if deferred => {
                deferred = false;
                from.as_mut().graceful_shutdown();
            }
        }
    }
}

// Silence unused-import lints that may show up depending on feature flags.
#[allow(dead_code)]
fn _assert_body_bounds() {
    fn assert_body<B: HyperBody + Send + 'static>() {}
    assert_body::<ProxyBody>();
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

    use super::*;
    use crate::net::host::binding::BasicCredential;

    async fn read_head(s: &mut DuplexStream) -> String {
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            if s.read(&mut byte).await.unwrap() == 0 {
                break;
            }
            head.push(byte[0]);
        }
        String::from_utf8_lossy(&head).to_lowercase()
    }

    /// The bytes this proxy writes upstream for one connection whose
    /// client-facing handshake negotiated `alpn`.
    async fn upstream_framing(alpn: Option<&str>) -> String {
        let (mut client, client_facing) = tokio::io::duplex(4096);
        let (backend_facing, mut backend) = tokio::io::duplex(4096);

        // A target reaches `run_http_proxy` only when it adds forwarded headers
        // or gates auth.
        tokio::spawn(run_http_proxy(
            client_facing,
            backend_facing,
            alpn.map(|a| MaybeUtf8String(a.as_bytes().to_vec())),
            None,
            true,
            None,
        ));
        if alpn != Some("h2") {
            // Only h2 opens with a preface.
            client
                .write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
                .await
                .unwrap();
        }

        let mut head = [0u8; 14];
        tokio::time::timeout(Duration::from_secs(5), backend.read_exact(&mut head))
            .await
            .expect("the proxy opens the upstream leg")
            .unwrap();
        String::from_utf8_lossy(&head).into_owned()
    }

    /// The proxy writes h2 framing upstream only when the client-facing
    /// handshake selected h2.
    #[tokio::test]
    async fn the_negotiated_alpn_frames_the_upstream_leg() {
        assert_eq!(upstream_framing(Some("h2")).await, "PRI * HTTP/2.0");
        assert_eq!(upstream_framing(Some("http/1.1")).await, "GET / HTTP/1.1");
        assert_eq!(upstream_framing(None).await, "GET / HTTP/1.1");
    }

    /// A backend closing its idle keep-alive leg (Apache's `KeepAliveTimeout`
    /// is 5s) must reach the client as an EOF, not as an abort on its next
    /// request.
    #[tokio::test]
    async fn upstream_close_is_relayed_to_the_client() {
        let (mut client, client_facing) = tokio::io::duplex(4096);
        let (backend_facing, mut backend) = tokio::io::duplex(4096);

        tokio::spawn(run_http1_proxy(
            client_facing,
            backend_facing,
            None,
            false,
            None,
        ));
        tokio::spawn(async move {
            read_head(&mut backend).await;
            backend
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhi")
                .await
                .unwrap();
            drop(backend);
        });

        client
            .write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .unwrap();
        assert!(read_head(&mut client).await.starts_with("http/1.1 200 ok"));
        client.read_exact(&mut [0u8; 2]).await.unwrap();

        let eof = tokio::time::timeout(Duration::from_secs(5), client.read(&mut [0u8; 1]))
            .await
            .expect(
                "client leg outlived the upstream: it is now a connection that looks reusable \
                 and will abort the next request written to it",
            );
        assert_eq!(eof.unwrap(), 0);
    }

    /// A backend that answers and closes in the same breath still gets its
    /// response relayed verbatim: `Connection: close` is a header it never
    /// sent, and the close is carried by closing, not by editing the reply.
    #[tokio::test]
    async fn a_relayed_response_is_not_rewritten_when_the_backend_closes() {
        let (mut client, client_facing) = tokio::io::duplex(4096);
        let (backend_facing, mut backend) = tokio::io::duplex(4096);

        tokio::spawn(run_http1_proxy(
            client_facing,
            backend_facing,
            None,
            false,
            None,
        ));
        tokio::spawn(async move {
            read_head(&mut backend).await;
            backend
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhi")
                .await
                .unwrap();
            drop(backend);
        });

        client
            .write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .unwrap();
        let head = read_head(&mut client).await;
        assert!(!head.contains("connection: close"), "{head:?}");
        client.read_exact(&mut [0u8; 2]).await.unwrap();

        let eof = tokio::time::timeout(Duration::from_secs(5), client.read(&mut [0u8; 1]))
            .await
            .expect("client leg outlived the upstream");
        assert_eq!(eof.unwrap(), 0);
    }

    /// The proxy relays the backend's own 101 rather than answering the
    /// upgrade itself, so the client hears nothing until the backend has
    /// spoken.
    #[tokio::test]
    async fn upgrade_is_not_answered_before_the_backend_answers() {
        let (mut client, client_facing) = tokio::io::duplex(4096);
        let (backend_facing, mut backend) = tokio::io::duplex(4096);

        tokio::spawn(run_http1_proxy(
            client_facing,
            backend_facing,
            None,
            false,
            None,
        ));
        tokio::spawn(async move {
            read_head(&mut backend).await;
            tokio::time::sleep(Duration::from_millis(200)).await;
            backend
                .write_all(
                    b"HTTP/1.1 101 Switching Protocols\r\n\
                      Connection: Upgrade\r\nUpgrade: websocket\r\n\r\n",
                )
                .await
                .unwrap();
            std::future::pending::<()>().await;
        });

        client
            .write_all(
                b"GET /ws HTTP/1.1\r\nHost: x\r\n\
                  Connection: Upgrade\r\nUpgrade: websocket\r\n\r\n",
            )
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), client.read(&mut [0u8; 1]))
                .await
                .is_err(),
            "the client was answered before the backend had responded",
        );
        assert!(read_head(&mut client).await.starts_with("http/1.1 101"));
    }

    /// Shutting the client leg down while a 101 is in flight makes hyper
    /// replace `Connection: Upgrade` with `Connection: close`, which
    /// conformant WebSocket clients reject (RFC 6455 §4.1). The tunnel still
    /// carries bytes either way, so assert on the header too.
    #[tokio::test]
    async fn relayed_101_keeps_its_upgrade_header() {
        let (mut client, client_facing) = tokio::io::duplex(4096);
        let (backend_facing, mut backend) = tokio::io::duplex(4096);

        tokio::spawn(run_http1_proxy(
            client_facing,
            backend_facing,
            None,
            false,
            None,
        ));
        tokio::spawn(async move {
            assert!(read_head(&mut backend).await.contains("upgrade: websocket"));
            backend
                .write_all(
                    b"HTTP/1.1 101 Switching Protocols\r\n\
                      Connection: Upgrade\r\nUpgrade: websocket\r\n\r\n",
                )
                .await
                .unwrap();
            let mut framed = [0u8; 4];
            backend.read_exact(&mut framed).await.unwrap();
            backend.write_all(&framed).await.unwrap();
        });

        client
            .write_all(
                b"GET /ws HTTP/1.1\r\nHost: x\r\n\
                  Connection: Upgrade\r\nUpgrade: websocket\r\n\r\n",
            )
            .await
            .unwrap();
        let head = read_head(&mut client).await;
        assert!(head.contains("connection: upgrade"), "{head:?}");
        assert!(!head.contains("connection: close"), "{head:?}");

        client.write_all(b"ping").await.unwrap();
        let mut echoed = [0u8; 4];
        tokio::time::timeout(Duration::from_secs(5), client.read_exact(&mut echoed))
            .await
            .expect("tunnel stalled")
            .unwrap();
        assert_eq!(&echoed, b"ping");
    }

    use super::{ENABLE_CONNECT_PROTOCOL, H2_SETTINGS_FRAME as SETTINGS_FRAME};
    const H2_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";
    const GOAWAY_FRAME: u8 = 0x7;

    async fn write_frame(s: &mut DuplexStream, kind: u8, payload: &[u8]) {
        let len = payload.len() as u32;
        let head = [
            (len >> 16) as u8,
            (len >> 8) as u8,
            len as u8,
            kind,
            0,
            0,
            0,
            0,
            0,
        ];
        s.write_all(&head).await.unwrap();
        s.write_all(payload).await.unwrap();
    }

    async fn write_client_preface(s: &mut DuplexStream, settings: &[(u16, u32)]) {
        s.write_all(H2_PREFACE).await.unwrap();
        let mut payload = Vec::new();
        for &(id, value) in settings {
            payload.extend_from_slice(&id.to_be_bytes());
            payload.extend_from_slice(&value.to_be_bytes());
        }
        write_frame(s, SETTINGS_FRAME, &payload).await;
    }

    async fn read_frame(s: &mut DuplexStream) -> std::io::Result<(u8, Vec<u8>)> {
        let mut head = [0u8; 9];
        s.read_exact(&mut head).await?;
        let len = ((head[0] as usize) << 16) | ((head[1] as usize) << 8) | head[2] as usize;
        let mut payload = vec![0u8; len];
        s.read_exact(&mut payload).await?;
        Ok((head[3], payload))
    }

    async fn read_client_opening(s: &mut DuplexStream) {
        let mut preface = [0; H2_PREFACE.len()];
        s.read_exact(&mut preface).await.unwrap();
        assert_eq!(&preface, H2_PREFACE);

        let mut header = [0; 9];
        s.read_exact(&mut header).await.unwrap();
        assert_eq!(header[3], SETTINGS_FRAME);
        assert_eq!(header[4], 0);
        assert_eq!(&header[5..9], &[0; 4]);
        let len = ((header[0] as usize) << 16) | ((header[1] as usize) << 8) | header[2] as usize;
        let mut payload = vec![0; len];
        s.read_exact(&mut payload).await.unwrap();
    }

    async fn read_opening_settings(s: &mut DuplexStream) -> Vec<u8> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let (kind, payload) = read_frame(s).await.unwrap();
                if kind == SETTINGS_FRAME {
                    return payload;
                }
            }
        })
        .await
        .expect("the proxy never sent its client-facing SETTINGS")
    }

    fn extended_connect_enabled(payload: &[u8]) -> bool {
        payload.chunks_exact(6).any(|entry| {
            u16::from_be_bytes([entry[0], entry[1]]) == ENABLE_CONNECT_PROTOCOL
                && u32::from_be_bytes([entry[2], entry[3], entry[4], entry[5]]) == 1
        })
    }

    #[tokio::test]
    async fn extended_connect_follows_the_backend_opening_settings() {
        for (backend_settings, advertised) in [
            (Vec::new(), false),
            (vec![(ENABLE_CONNECT_PROTOCOL, 1u32)], true),
        ] {
            let (mut client, client_facing) = tokio::io::duplex(4096);
            let (backend_facing, mut backend) = tokio::io::duplex(4096);

            let proxy = tokio::spawn(run_http2_proxy(
                client_facing,
                backend_facing,
                None,
                false,
                None,
            ));
            let backend = tokio::spawn(async move {
                read_client_opening(&mut backend).await;
                let mut payload = Vec::with_capacity(backend_settings.len() * 6);
                for (id, value) in backend_settings {
                    payload.extend_from_slice(&id.to_be_bytes());
                    payload.extend_from_slice(&value.to_be_bytes());
                }
                write_frame(&mut backend, SETTINGS_FRAME, &payload).await;
            });

            write_client_preface(&mut client, &[]).await;
            assert_eq!(
                extended_connect_enabled(&read_opening_settings(&mut client).await),
                advertised,
            );
            backend.await.unwrap();
            drop(client);
            tokio::time::timeout(Duration::from_secs(5), proxy)
                .await
                .expect("proxy did not close with the client")
                .unwrap()
                .unwrap();
        }
    }

    #[tokio::test]
    async fn maximum_opening_settings_is_observed() {
        let (mut client, client_facing) = tokio::io::duplex(4096);
        let (backend_facing, mut backend) = tokio::io::duplex(32 * 1024);
        let proxy = tokio::spawn(run_http2_proxy(
            client_facing,
            backend_facing,
            None,
            false,
            None,
        ));
        let backend = tokio::spawn(async move {
            let mut settings = vec![(0xa000u16, 0u32); 2_730];
            settings[1_365] = (ENABLE_CONNECT_PROTOCOL, 1);
            let mut payload = Vec::with_capacity(settings.len() * 6);
            for (id, value) in settings {
                payload.extend_from_slice(&id.to_be_bytes());
                payload.extend_from_slice(&value.to_be_bytes());
            }
            write_frame(&mut backend, SETTINGS_FRAME, &payload).await;
            read_client_opening(&mut backend).await;
        });

        write_client_preface(&mut client, &[]).await;
        assert!(extended_connect_enabled(
            &read_opening_settings(&mut client).await
        ));
        backend.await.unwrap();
        drop(client);
        tokio::time::timeout(Duration::from_secs(5), proxy)
            .await
            .expect("proxy did not close with the client")
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn backend_failure_before_opening_settings_returns_without_a_client_preface() {
        let (_client, client_facing) = tokio::io::duplex(4096);
        let (backend_facing, backend) = tokio::io::duplex(4096);
        drop(backend);

        let result = tokio::time::timeout(
            Duration::from_secs(1),
            run_http2_proxy(client_facing, backend_facing, None, false, None),
        )
        .await
        .expect("proxy waited for the client after the backend failed");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn established_backend_failure_sends_goaway() {
        let (mut client, client_facing) = tokio::io::duplex(4096);
        let (backend_facing, mut backend) = tokio::io::duplex(4096);

        let proxy = tokio::spawn(run_http2_proxy(
            client_facing,
            backend_facing,
            None,
            false,
            None,
        ));
        let backend = tokio::spawn(async move {
            read_client_opening(&mut backend).await;
            write_frame(&mut backend, SETTINGS_FRAME, &[]).await;
            assert_eq!(read_frame(&mut backend).await.unwrap().0, SETTINGS_FRAME);
        });

        write_client_preface(&mut client, &[]).await;
        backend.await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match read_frame(&mut client).await.unwrap() {
                    (GOAWAY_FRAME, _) => break,
                    _ => {}
                }
            }
        })
        .await
        .expect("no GOAWAY after the upstream errored");
        drop(client);
        tokio::time::timeout(Duration::from_secs(5), proxy)
            .await
            .expect("proxy did not close with the client")
            .unwrap()
            .unwrap();
    }

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
