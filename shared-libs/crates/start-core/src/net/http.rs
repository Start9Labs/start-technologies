use std::collections::HashMap;
use std::convert::Infallible;
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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
use tokio::io::AsyncReadExt;
use tokio::sync::{Mutex, Notify, oneshot};
use tokio_util::io::InspectReader;

use crate::net::host::binding::ProxyAuth;
use crate::prelude::*;
use crate::util::io::{BackTrackingIO, IOHook, ReadWriter};
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

struct RelayProgress {
    bodies: AtomicUsize,
    flushed: AtomicBool,
    done: Notify,
}

impl RelayProgress {
    fn new() -> Self {
        Self {
            bodies: AtomicUsize::new(0),
            flushed: AtomicBool::new(true),
            done: Notify::new(),
        }
    }

    fn start(self: &Arc<Self>) -> Relaying {
        self.flushed.store(false, Ordering::Relaxed);
        self.bodies.fetch_add(1, Ordering::Relaxed);
        Relaying(self.clone())
    }

    fn did_flush(&self) {
        if self.bodies.load(Ordering::Relaxed) == 0 {
            self.flushed.store(true, Ordering::Relaxed);
            self.done.notify_one();
        }
    }

    fn is_flushed(&self) -> bool {
        self.flushed.load(Ordering::Relaxed)
    }
}

struct Relaying(Arc<RelayProgress>);

impl Relaying {
    fn hold(self, body: ProxyBody) -> ProxyBody {
        RelayedBody {
            body,
            _relaying: self,
        }
        .boxed()
    }
}

impl Drop for Relaying {
    fn drop(&mut self) {
        self.0.bodies.fetch_sub(1, Ordering::Relaxed);
    }
}

struct RelayedBody {
    body: ProxyBody,
    _relaying: Relaying,
}

impl HyperBody for RelayedBody {
    type Data = Bytes;
    type Error = <ProxyBody as HyperBody>::Error;

    fn poll_frame(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<hyper::body::Frame<Bytes>, Self::Error>>> {
        std::pin::Pin::new(&mut self.get_mut().body).poll_frame(cx)
    }

    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }

    fn size_hint(&self) -> hyper::body::SizeHint {
        self.body.size_hint()
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

/// Headers synthesized from the client connection, independently of authentication.
#[derive(Clone, Copy)]
pub enum ForwardedHeaders {
    /// Adds no connection headers; authenticated Basic users still receive `X-Forwarded-User`.
    Disabled,
    /// Adds `X-Forwarded-For` when the client IP is available.
    Plaintext,
    /// Adds `X-Forwarded-Proto: https` and the available client IP.
    Ssl,
}

fn apply_request_policy<B>(
    req: &mut Request<B>,
    src_ip: Option<IpAddr>,
    forwarded_headers: ForwardedHeaders,
    gate: Option<&AuthGate>,
) -> Result<(), Response<ProxyBody>> {
    let forwarded: Vec<_> = req
        .headers()
        .keys()
        .filter(|name| name.as_str().starts_with("x-forwarded-"))
        .cloned()
        .collect();
    for name in forwarded {
        req.headers_mut().remove(name);
    }

    let user = match gate {
        Some(g) => g.check(req.headers())?,
        None => None,
    };

    let h = req.headers_mut();
    if matches!(forwarded_headers, ForwardedHeaders::Ssl) {
        h.insert("X-Forwarded-Proto", HeaderValue::from_static("https"));
    }
    if !matches!(forwarded_headers, ForwardedHeaders::Disabled) {
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

const H2_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";
const HTTP_HEADER_TIMEOUT: Duration = Duration::from_secs(60);

/// Proxies HTTP/1 or prior-knowledge HTTP/2 using the client's framing.
pub async fn run_plain_http_proxy<F, T>(
    from: F,
    to: T,
    src_ip: Option<IpAddr>,
    forwarded_headers: ForwardedHeaders,
    gate: Option<AuthGate>,
) -> Result<(), Error>
where
    F: ReadWriter + Unpin + Send + 'static,
    T: ReadWriter + Unpin + Send + 'static,
{
    let mut from = BackTrackingIO::new(from);
    let http2 = match tokio::time::timeout(HTTP_HEADER_TIMEOUT, async {
        for expected in H2_PREFACE {
            if from.read_u8().await? != *expected {
                return Ok::<_, std::io::Error>(false);
            }
        }
        Ok(true)
    })
    .await
    .with_kind(ErrorKind::Network)?
    {
        Ok(http2) => http2,
        Err(error)
            if error.kind() == std::io::ErrorKind::UnexpectedEof
                && from.read_buffer().is_empty() =>
        {
            return Ok(());
        }
        Err(error) => return Err(Error::new(error, ErrorKind::Network)),
    };
    let _ = from.rewind();
    if http2 {
        run_http2_proxy(from, to, src_ip, forwarded_headers, gate).await
    } else {
        run_http1_proxy(from, to, src_ip, forwarded_headers, gate).await
    }
}

pub async fn run_http_proxy<F, T>(
    from: F,
    to: T,
    alpn: Option<MaybeUtf8String>,
    src_ip: Option<IpAddr>,
    forwarded_headers: ForwardedHeaders,
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
        run_http2_proxy(from, to, src_ip, forwarded_headers, gate).await
    } else {
        run_http1_proxy(from, to, src_ip, forwarded_headers, gate).await
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
    forwarded_headers: ForwardedHeaders,
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
                        apply_request_policy(&mut req, src_ip, forwarded_headers, gate.as_ref())
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
    forwarded_headers: ForwardedHeaders,
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
    let progress = Arc::new(RelayProgress::new());
    let flush_progress = progress.clone();
    let mut from = IOHook::new(from);
    // Hyper drains its write buffer before flushing the transport.
    from.post_flush(move || flush_progress.did_flush());
    let svc_progress = progress.clone();
    // Hyper disarms `header_read_timeout` during a body or upgrade.
    let from = hyper::server::conn::http1::Builder::new()
        .timer(TokioTimer::new())
        .header_read_timeout(HTTP_HEADER_TIMEOUT)
        .serve_connection(
            TokioIo::new(from),
            service_fn(move |mut req| {
                let client = client.clone();
                let gate = gate.clone();
                // The upstream driver can finish before `send_request` resumes.
                let relaying = svc_progress.start();
                async move {
                    if let Err(resp) =
                        apply_request_policy(&mut req, src_ip, forwarded_headers, gate.as_ref())
                    {
                        return Ok::<_, hyper::Error>(resp.map(|body| relaying.hold(body)));
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
                                if kind
                                    .as_ref()
                                    .and_then(|k| k.to_str().ok())
                                    .is_some_and(|kind| {
                                        kind.eq_ignore_ascii_case("HTTP/2.0")
                                            || kind.eq_ignore_ascii_case("h2c")
                                    })
                                {
                                    run_http2_proxy(
                                        TokioIo::new(from),
                                        TokioIo::new(to),
                                        src_ip,
                                        forwarded_headers,
                                        gate,
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
    let mut upstream_result = None;
    loop {
        tokio::select! {
            res = from.as_mut() => return Ok(res?),
            res = to.as_mut() => upstream_result = Some(res),
            _ = progress.done.notified(), if upstream_result.is_some() => {}
        }
        // Mid-response shutdown can rewrite `Connection: Upgrade` or discard buffered bytes.
        if progress.is_flushed() {
            if let Some(res) = upstream_result.take() {
                res?;
                from.as_mut().graceful_shutdown();
            }
        }
    }
}

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

    async fn read_head<S: tokio::io::AsyncRead + Unpin>(s: &mut S) -> String {
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

    async fn upstream_framing(alpn: Option<&str>) -> String {
        let (mut client, client_facing) = tokio::io::duplex(4096);
        let (backend_facing, mut backend) = tokio::io::duplex(4096);

        tokio::spawn(run_http_proxy(
            client_facing,
            backend_facing,
            alpn.map(|a| MaybeUtf8String(a.as_bytes().to_vec())),
            None,
            ForwardedHeaders::Ssl,
            None,
        ));
        if alpn != Some("h2") {
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
            ForwardedHeaders::Disabled,
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
            ForwardedHeaders::Disabled,
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
            ForwardedHeaders::Disabled,
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

    /// RFC 6455 clients reject a 101 rewritten to `Connection: close`.
    #[tokio::test]
    async fn relayed_101_keeps_its_upgrade_header_and_authenticated_policy() {
        for forwarded_headers in [
            ForwardedHeaders::Disabled,
            ForwardedHeaders::Plaintext,
            ForwardedHeaders::Ssl,
        ] {
            tokio::time::timeout(Duration::from_secs(5), async {
                let (mut client, client_facing) = tokio::io::duplex(16);
                let (backend_facing, mut backend) = tokio::io::duplex(4096);
                let proxy = tokio::spawn(run_http1_proxy(
                    client_facing,
                    backend_facing,
                    Some(CLIENT_IP.parse().unwrap()),
                    forwarded_headers,
                    Some(basic_gate()),
                ));
                let backend = tokio::spawn(async move {
                    let head = read_head(&mut backend).await;
                    assert!(head.contains("upgrade: websocket"), "{head}");
                    assert_policy_headers(
                        &parse_request_headers(&head),
                        forwarded_headers,
                        Some("alice"),
                    );
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
                    .write_all(http1_spoofed_request(Some("websocket")).as_bytes())
                    .await
                    .unwrap();
                let head = read_head(&mut client).await;
                assert!(head.starts_with("http/1.1 101"), "{head:?}");
                assert!(head.contains("connection: upgrade"), "{head:?}");
                assert!(!head.contains("connection: close"), "{head:?}");
                client.write_all(b"ping").await.unwrap();
                let mut echoed = [0u8; 4];
                client.read_exact(&mut echoed).await.unwrap();
                assert_eq!(&echoed, b"ping");
                backend.await.unwrap();
                proxy.abort();
            })
            .await
            .expect("authenticated WebSocket tunnel stalled");
        }
    }

    #[tokio::test]
    async fn backend_close_waits_for_a_backpressured_response() {
        for connection in ["Connection: close\r\n", ""] {
            tokio::time::timeout(Duration::from_secs(5), async {
                let body = vec![b'x'; 224_476];
                let (mut client, client_facing) = tokio::io::duplex(128);
                let (backend_facing, mut backend) = tokio::io::duplex(512 * 1024);
                let proxy = tokio::spawn(run_http1_proxy(
                    client_facing,
                    backend_facing,
                    None,
                    ForwardedHeaders::Disabled,
                    None,
                ));
                let expected = body.clone();
                let backend = tokio::spawn(async move {
                    read_head(&mut backend).await;
                    backend
                        .write_all(
                            format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n{connection}\r\n",
                                body.len(),
                            )
                            .as_bytes(),
                        )
                        .await
                        .unwrap();
                    backend.write_all(&body).await.unwrap();
                });
                client
                    .write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
                    .await
                    .unwrap();
                backend.await.unwrap();
                let head = read_head(&mut client).await;
                assert_eq!(
                    head.contains("connection: close"),
                    !connection.is_empty(),
                    "{head:?}"
                );
                let mut received = Vec::new();
                client.read_to_end(&mut received).await.unwrap();
                assert_eq!(received.len(), expected.len());
                assert_eq!(received, expected);
                proxy.await.unwrap().unwrap();
            })
            .await
            .expect("response or EOF stalled");
        }
    }

    #[tokio::test]
    async fn backend_close_waits_for_chunked_data_and_trailers() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let body = Bytes::from(vec![b'x'; 224_476]);
            let (client, client_facing) = tokio::io::duplex(128);
            let (backend_facing, mut backend) = tokio::io::duplex(512 * 1024);
            let proxy = tokio::spawn(run_http1_proxy(
                client_facing, backend_facing, None, ForwardedHeaders::Disabled, None,
            ));
            let expected = body.clone();
            let backend = tokio::spawn(async move {
                read_head(&mut backend).await;
                backend.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\nTrailer: X-Checksum\r\n\r\n").await.unwrap();
                backend.write_all(format!("{:x}\r\n", body.len()).as_bytes()).await.unwrap();
                backend.write_all(&body).await.unwrap();
                backend.write_all(b"\r\n0\r\nx-checksum: complete\r\n\r\n").await.unwrap();
            });
            let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(client)).await.unwrap();
            let connection = tokio::spawn(connection);
            let response = sender.send_request(Request::builder().uri("/").header("Host", "x").header("TE", "trailers").body(Full::new(Bytes::new())).unwrap()).await.unwrap();
            let collected = response.into_body().collect().await.unwrap();
            assert_eq!(collected.trailers().unwrap()["x-checksum"], "complete");
            assert_eq!(collected.to_bytes(), expected);
            backend.await.unwrap();
            connection.await.unwrap().unwrap();
            proxy.await.unwrap().unwrap();
        }).await.expect("chunked response or EOF stalled");
    }

    #[tokio::test]
    async fn backend_close_follows_head_and_empty_responses() {
        for (request, response) in [
            ("HEAD", "HTTP/1.1 200 OK\r\nContent-Length: 224476\r\n\r\n"),
            ("GET", "HTTP/1.1 204 No Content\r\n\r\n"),
        ] {
            tokio::time::timeout(Duration::from_secs(5), async {
                let (mut client, client_facing) = tokio::io::duplex(16);
                let (backend_facing, mut backend) = tokio::io::duplex(4096);
                let proxy = tokio::spawn(run_http1_proxy(
                    client_facing,
                    backend_facing,
                    None,
                    ForwardedHeaders::Disabled,
                    None,
                ));
                let backend = tokio::spawn(async move {
                    read_head(&mut backend).await;
                    backend.write_all(response.as_bytes()).await.unwrap();
                });
                client
                    .write_all(format!("{request} / HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes())
                    .await
                    .unwrap();
                backend.await.unwrap();
                let head = read_head(&mut client).await;
                assert!(
                    head.starts_with(
                        response
                            .split("\r\n")
                            .next()
                            .unwrap()
                            .to_lowercase()
                            .as_str()
                    ),
                    "{head:?}"
                );
                assert!(!head.contains("connection: close"), "{head:?}");
                if request == "HEAD" {
                    assert!(head.contains("content-length: 224476"), "{head:?}");
                }
                assert_eq!(client.read(&mut [0u8; 1]).await.unwrap(), 0);
                proxy.await.unwrap().unwrap();
            })
            .await
            .expect("empty response or EOF stalled");
        }
    }

    #[tokio::test]
    async fn idle_backend_close_waits_for_an_auth_response() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let (mut client, client_facing) = tokio::io::duplex(16);
            let (backend_facing, backend) = tokio::io::duplex(4096);
            let gate = AuthGate::from_auth(&ProxyAuth::Bearer {
                tokens: vec!["secret".to_owned()],
                realm: None,
            })
            .unwrap();
            let proxy = tokio::spawn(run_http1_proxy(
                client_facing,
                backend_facing,
                None,
                ForwardedHeaders::Disabled,
                Some(gate),
            ));
            client
                .write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
                .await
                .unwrap();
            let head = read_head(&mut client).await;
            assert!(head.starts_with("http/1.1 401"));
            assert!(head.contains("content-length: 16"), "{head:?}");
            assert!(!head.contains("transfer-encoding"), "{head:?}");
            drop(backend);
            let mut received = Vec::new();
            client.read_to_end(&mut received).await.unwrap();
            assert_eq!(received, b"401 Unauthorized");
            assert!(proxy.await.unwrap().is_err());
        })
        .await
        .expect("auth response or EOF stalled");
    }

    struct GatedFlush {
        io: DuplexStream,
        release: Option<oneshot::Receiver<()>>,
        started: Option<oneshot::Sender<()>>,
        wrote: bool,
        shutdown: Arc<AtomicBool>,
    }

    impl tokio::io::AsyncRead for GatedFlush {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::pin::Pin::new(&mut self.get_mut().io).poll_read(cx, buf)
        }
    }

    impl tokio::io::AsyncWrite for GatedFlush {
        fn poll_write(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
            buf: &[u8],
        ) -> std::task::Poll<std::io::Result<usize>> {
            let this = self.get_mut();
            let n = futures::ready!(std::pin::Pin::new(&mut this.io).poll_write(cx, buf)?);
            this.wrote |= n != 0;
            std::task::Poll::Ready(Ok(n))
        }

        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            let this = self.get_mut();
            if this.wrote {
                if let Some(started) = this.started.take() {
                    started.send(()).unwrap();
                }
                if let Some(release) = &mut this.release {
                    futures::ready!(std::pin::Pin::new(release).poll(cx)).unwrap();
                    this.release = None;
                }
            }
            std::pin::Pin::new(&mut this.io).poll_flush(cx)
        }

        fn poll_shutdown(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            let this = self.get_mut();
            this.shutdown.store(true, Ordering::Relaxed);
            std::pin::Pin::new(&mut this.io).poll_shutdown(cx)
        }
    }

    #[tokio::test]
    async fn backend_close_waits_for_the_transport_flush() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let (mut client, io) = tokio::io::duplex(4096);
            let (backend_facing, mut backend) = tokio::io::duplex(4096);
            let (release, released) = oneshot::channel();
            let (started, flushing) = oneshot::channel();
            let shutdown = Arc::new(AtomicBool::new(false));
            let mut proxy = tokio::spawn(run_http1_proxy(
                GatedFlush {
                    io,
                    release: Some(released),
                    started: Some(started),
                    wrote: false,
                    shutdown: shutdown.clone(),
                },
                backend_facing,
                None,
                ForwardedHeaders::Disabled,
                None,
            ));
            let backend = tokio::spawn(async move {
                read_head(&mut backend).await;
                backend
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nhi",
                    )
                    .await
                    .unwrap();
            });
            client
                .write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
                .await
                .unwrap();
            backend.await.unwrap();
            flushing.await.unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(50), &mut proxy)
                    .await
                    .is_err()
            );
            assert!(!shutdown.load(Ordering::Relaxed));
            release.send(()).unwrap();
            assert!(read_head(&mut client).await.starts_with("http/1.1 200 ok"));
            let mut received = Vec::new();
            client.read_to_end(&mut received).await.unwrap();
            assert_eq!(received, b"hi");
            proxy.await.unwrap().unwrap();
            assert!(shutdown.load(Ordering::Relaxed));
        })
        .await
        .expect("transport flush or EOF stalled");
    }

    #[tokio::test]
    async fn tls_close_follows_the_complete_backpressured_response() {
        use tokio_rustls::rustls::pki_types::ServerName;
        use tokio_rustls::{TlsAcceptor, TlsConnector};

        use crate::net::tls::client_config_no_verify;
        use crate::net::tls::test::{provider, self_signed_for_loopback, server_config};

        tokio::time::timeout(Duration::from_secs(10), async {
            let (key, cert) = self_signed_for_loopback();
            let mut config = server_config(&key, &cert);
            config.send_tls13_tickets = 0;
            let acceptor = TlsAcceptor::from(Arc::new(config));
            let connector =
                TlsConnector::from(Arc::new(client_config_no_verify(provider()).unwrap()));
            let (client_io, server_io) = tokio::io::duplex(128);
            let (client, from) = tokio::time::timeout(Duration::from_secs(2), async {
                tokio::join!(
                    connector.connect(ServerName::try_from("localhost").unwrap(), client_io),
                    acceptor.accept(server_io),
                )
            })
            .await
            .expect("TLS handshake stalled");
            let mut client = client.unwrap();
            let (backend_facing, mut backend) = tokio::io::duplex(512 * 1024);
            let proxy = tokio::spawn(run_http1_proxy(
                from.unwrap(),
                backend_facing,
                Some(CLIENT_IP.parse().unwrap()),
                ForwardedHeaders::Ssl,
                Some(basic_gate()),
            ));
            let body = vec![b'x'; 224_476];
            let expected = body.clone();
            let backend = tokio::spawn(async move {
                let head = read_head(&mut backend).await;
                assert_policy_headers(
                    &parse_request_headers(&head),
                    ForwardedHeaders::Ssl,
                    Some("alice"),
                );
                backend
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
                backend.write_all(&body).await.unwrap();
            });
            client
                .write_all(http1_spoofed_request(None).as_bytes())
                .await
                .unwrap();
            client.flush().await.unwrap();
            backend.await.unwrap();
            assert!(read_head(&mut client).await.starts_with("http/1.1 200 ok"));
            let mut received = Vec::new();
            client.read_to_end(&mut received).await.unwrap();
            assert_eq!(received, expected);
            proxy.await.unwrap().unwrap();
        })
        .await
        .expect("TLS response or close_notify stalled");
    }

    #[tokio::test]
    async fn relay_completion_requires_a_flush_after_the_latest_body() {
        let progress = Arc::new(RelayProgress::new());
        assert!(progress.is_flushed());
        let first = progress.start();
        progress.did_flush();
        assert!(!progress.is_flushed());
        drop(first);
        assert!(!progress.is_flushed());
        progress.did_flush();
        assert!(progress.is_flushed());
        let second = progress.start();
        progress.done.notified().await;
        assert!(!progress.is_flushed());
        drop(second);
        assert!(!progress.is_flushed());
        progress.did_flush();
        assert!(progress.is_flushed());
    }

    use super::{ENABLE_CONNECT_PROTOCOL, H2_SETTINGS_FRAME as SETTINGS_FRAME};
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
                ForwardedHeaders::Disabled,
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
            ForwardedHeaders::Disabled,
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
            run_http2_proxy(
                client_facing,
                backend_facing,
                None,
                ForwardedHeaders::Disabled,
                None,
            ),
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
            ForwardedHeaders::Disabled,
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

    const CLIENT_IP: &str = "192.0.2.42";
    const BASIC_AUTH: &str = "Basic YWxpY2U6aHVudGVyMg==";
    const SPOOFED_HEADERS: &[&str] = &[
        "X-Forwarded-For",
        "X-Forwarded-Proto",
        "X-Forwarded-User",
        "X-Forwarded-Host",
        "X-Forwarded-Port",
        "X-Forwarded-Prefix",
        "X-Forwarded-Custom",
    ];

    fn basic_gate() -> AuthGate {
        AuthGate::from_auth(&ProxyAuth::Basic {
            credentials: vec![BasicCredential {
                username: "alice".into(),
                password: "hunter2".into(),
            }],
            realm: None,
        })
        .unwrap()
    }

    fn spoofed_request(auth: Option<&str>) -> Request<Full<Bytes>> {
        let mut req = req_with_auth(auth).map(|()| Full::new(Bytes::new()));
        *req.uri_mut() = "http://service.test/path?query=value".parse().unwrap();
        req.headers_mut()
            .insert("Host", HeaderValue::from_static("service.test"));
        req.headers_mut()
            .insert("X-Preserved", HeaderValue::from_static("untouched"));
        for &name in SPOOFED_HEADERS {
            req.headers_mut()
                .append(name, HeaderValue::from_static("forged"));
            req.headers_mut()
                .append(name, HeaderValue::from_static("also-forged"));
        }
        req
    }

    fn http1_spoofed_request(upgrade: Option<&str>) -> String {
        let req = spoofed_request(Some(BASIC_AUTH));
        let mut head = String::from("GET /path?query=value HTTP/1.1\r\n");
        for (name, value) in req.headers() {
            head.push_str(&format!("{name}: {}\r\n", value.to_str().unwrap()));
        }
        if let Some(upgrade) = upgrade {
            head.push_str(&format!("Connection: Upgrade\r\nUpgrade: {upgrade}\r\n"));
        }
        head.push_str("\r\n");
        head
    }

    fn parse_request_headers(head: &str) -> HeaderMap {
        head.lines()
            .skip(1)
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| {
                (
                    http::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                    HeaderValue::from_str(value.trim()).unwrap(),
                )
            })
            .collect()
    }

    fn assert_policy_headers(
        headers: &HeaderMap,
        forwarded_headers: ForwardedHeaders,
        user: Option<&str>,
    ) {
        assert_eq!(headers.get("X-Preserved").unwrap(), "untouched");
        assert_eq!(headers.get("Host").unwrap(), "service.test");
        assert_eq!(
            headers.get("X-Forwarded-For").map(|h| h.to_str().unwrap()),
            match forwarded_headers {
                ForwardedHeaders::Disabled => None,
                ForwardedHeaders::Plaintext | ForwardedHeaders::Ssl => Some(CLIENT_IP),
            },
        );
        assert_eq!(
            headers
                .get("X-Forwarded-Proto")
                .map(|h| h.to_str().unwrap()),
            match forwarded_headers {
                ForwardedHeaders::Ssl => Some("https"),
                ForwardedHeaders::Disabled | ForwardedHeaders::Plaintext => None,
            },
        );
        assert_eq!(
            headers.get("X-Forwarded-User").map(|h| h.to_str().unwrap()),
            user,
        );
        for &name in SPOOFED_HEADERS {
            let values: Vec<_> = headers.get_all(name).iter().collect();
            assert!(values.len() <= 1, "duplicate {name}: {values:?}");
            assert!(
                values.iter().all(|v| *v != "forged" && *v != "also-forged"),
                "spoofed {name}: {values:?}",
            );
            if !matches!(
                name,
                "X-Forwarded-For" | "X-Forwarded-Proto" | "X-Forwarded-User"
            ) {
                assert!(values.is_empty(), "unexpected {name}: {values:?}");
            }
        }
    }

    #[test]
    fn forwarded_headers_are_owned_with_or_without_authentication() {
        let basic = basic_gate();
        let bearer = AuthGate::from_auth(&ProxyAuth::Bearer {
            tokens: vec!["secret".into()],
            realm: None,
        })
        .unwrap();
        for forwarded_headers in [
            ForwardedHeaders::Disabled,
            ForwardedHeaders::Plaintext,
            ForwardedHeaders::Ssl,
        ] {
            for (gate, auth, user) in [
                (None, None, None),
                (Some(&basic), Some(BASIC_AUTH), Some("alice")),
                (Some(&bearer), Some("Bearer secret"), None),
            ] {
                let mut req = spoofed_request(auth);
                apply_request_policy(
                    &mut req,
                    Some(CLIENT_IP.parse().unwrap()),
                    forwarded_headers,
                    gate,
                )
                .unwrap();
                assert_policy_headers(req.headers(), forwarded_headers, user);
                assert_eq!(req.uri(), "http://service.test/path?query=value");
                assert_eq!(
                    req.headers()
                        .get(http::header::AUTHORIZATION)
                        .map(|h| h.to_str().unwrap()),
                    auth,
                );
            }
        }
    }

    #[test]
    fn missing_client_ip_does_not_preserve_a_spoofed_address() {
        for forwarded_headers in [
            ForwardedHeaders::Disabled,
            ForwardedHeaders::Plaintext,
            ForwardedHeaders::Ssl,
        ] {
            let mut req = spoofed_request(None);
            apply_request_policy(&mut req, None, forwarded_headers, None).unwrap();
            assert!(req.headers().get("X-Forwarded-For").is_none());
            assert_eq!(
                req.headers()
                    .get("X-Forwarded-Proto")
                    .map(|h| h.to_str().unwrap()),
                match forwarded_headers {
                    ForwardedHeaders::Ssl => Some("https"),
                    ForwardedHeaders::Disabled | ForwardedHeaders::Plaintext => None,
                },
            );
        }
    }

    async fn echo_request_headers(
        req: Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, Infallible> {
        let mut response = Response::new(Full::new(Bytes::new()));
        *response.headers_mut() = req.into_parts().0.headers;
        Ok(response)
    }

    enum PolicyClient {
        Http1(hyper::client::conn::http1::SendRequest<Full<Bytes>>),
        Http2(hyper::client::conn::http2::SendRequest<Full<Bytes>>),
    }

    impl PolicyClient {
        async fn check_auth_and_headers(&mut self, forwarded_headers: ForwardedHeaders) {
            for auth in [
                None,
                Some("Basic invalid"),
                Some(BASIC_AUTH),
                None,
                Some(BASIC_AUTH),
            ] {
                let req = spoofed_request(auth);
                let response = match self {
                    Self::Http1(client) => client.send_request(req).await.unwrap(),
                    Self::Http2(client) => client.send_request(req).await.unwrap(),
                };
                if auth == Some(BASIC_AUTH) {
                    assert_eq!(response.status(), http::StatusCode::OK);
                    assert_policy_headers(response.headers(), forwarded_headers, Some("alice"));
                    assert_eq!(response.headers()[http::header::AUTHORIZATION], BASIC_AUTH);
                    assert!(
                        response
                            .into_body()
                            .collect()
                            .await
                            .unwrap()
                            .to_bytes()
                            .is_empty()
                    );
                } else {
                    assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);
                    assert_eq!(
                        response.headers()[http::header::WWW_AUTHENTICATE],
                        "Basic realm=\"StartOS\""
                    );
                    assert_eq!(
                        response.into_body().collect().await.unwrap().to_bytes(),
                        "401 Unauthorized"
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn http1_and_http2_enforce_auth_and_forwarded_headers_on_each_request() {
        tokio::time::timeout(Duration::from_secs(10), async {
            for alpn in [None, Some("h2")] {
                for forwarded_headers in [
                    ForwardedHeaders::Disabled,
                    ForwardedHeaders::Plaintext,
                    ForwardedHeaders::Ssl,
                ] {
                    let (client, client_facing) = tokio::io::duplex(4096);
                    let (backend_facing, backend) = tokio::io::duplex(4096);
                    let proxy = tokio::spawn(run_http_proxy(
                        client_facing,
                        backend_facing,
                        alpn.map(|a| MaybeUtf8String(a.as_bytes().to_vec())),
                        Some(CLIENT_IP.parse().unwrap()),
                        forwarded_headers,
                        Some(basic_gate()),
                    ));
                    let backend = tokio::spawn(async move {
                        hyper_util::server::conn::auto::Builder::new(TokioExecutor::new())
                            .serve_connection(
                                TokioIo::new(backend),
                                service_fn(echo_request_headers),
                            )
                            .await
                            .unwrap();
                    });
                    let (mut client, connection) = if alpn == Some("h2") {
                        let (sender, connection) = hyper::client::conn::http2::handshake(
                            TokioExecutor::new(),
                            TokioIo::new(client),
                        )
                        .await
                        .unwrap();
                        (
                            PolicyClient::Http2(sender),
                            tokio::spawn(async move {
                                connection.await.unwrap();
                            }),
                        )
                    } else {
                        let (sender, connection) =
                            hyper::client::conn::http1::handshake(TokioIo::new(client))
                                .await
                                .unwrap();
                        (
                            PolicyClient::Http1(sender),
                            tokio::spawn(async move {
                                connection.await.unwrap();
                            }),
                        )
                    };
                    client.check_auth_and_headers(forwarded_headers).await;
                    connection.abort();
                    proxy.abort();
                    backend.abort();
                }
            }
        })
        .await
        .expect("HTTP auth/header policy exchange stalled");
    }

    #[tokio::test]
    async fn http2_upgrade_keeps_auth_and_header_policy_for_subsequent_requests() {
        tokio::time::timeout(Duration::from_secs(10), async {
            for forwarded_headers in [
                ForwardedHeaders::Disabled,
                ForwardedHeaders::Plaintext,
                ForwardedHeaders::Ssl,
            ] {
                for upgrade in ["HTTP/2.0", "h2c", "H2C"] {
                    let (mut client, client_facing) = tokio::io::duplex(4096);
                    let (backend_facing, mut backend) = tokio::io::duplex(4096);
                    let proxy = tokio::spawn(run_http1_proxy(
                        client_facing,
                        backend_facing,
                        Some(CLIENT_IP.parse().unwrap()),
                        forwarded_headers,
                        Some(basic_gate()),
                    ));
                    let backend = tokio::spawn(async move {
                        let head = read_head(&mut backend).await;
                        assert_policy_headers(
                            &parse_request_headers(&head),
                            forwarded_headers,
                            Some("alice"),
                        );
                        let response = format!(
                            "HTTP/1.1 101 Switching Protocols\r\n\
                         Connection: Upgrade\r\nUpgrade: {upgrade}\r\n\r\n"
                        );
                        backend.write_all(response.as_bytes()).await.unwrap();
                        hyper::server::conn::http2::Builder::new(TokioExecutor::new())
                            .serve_connection(
                                TokioIo::new(backend),
                                service_fn(echo_request_headers),
                            )
                            .await
                            .unwrap();
                    });
                    client
                        .write_all(http1_spoofed_request(Some(upgrade)).as_bytes())
                        .await
                        .unwrap();
                    assert!(read_head(&mut client).await.starts_with("http/1.1 101"));
                    let (sender, connection) = hyper::client::conn::http2::handshake(
                        TokioExecutor::new(),
                        TokioIo::new(client),
                    )
                    .await
                    .unwrap();
                    let connection = tokio::spawn(connection);
                    PolicyClient::Http2(sender)
                        .check_auth_and_headers(forwarded_headers)
                        .await;
                    connection.abort();
                    proxy.abort();
                    backend.abort();
                }
            }
        })
        .await
        .expect("HTTP/2 upgrade policy exchange stalled");
    }

    #[tokio::test]
    async fn plaintext_detection_accepts_a_short_http1_request() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let (mut client, from) = tokio::io::duplex(4096);
            let (to, mut backend) = tokio::io::duplex(4096);
            let proxy = tokio::spawn(run_plain_http_proxy(
                from,
                to,
                None,
                ForwardedHeaders::Disabled,
                None,
            ));
            client.write_all(b"GET / HTTP/1.0\r\n\r\n").await.unwrap();
            assert!(read_head(&mut backend).await.starts_with("get / http/1.0"));
            backend
                .write_all(b"HTTP/1.0 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
            drop(backend);
            assert!(read_head(&mut client).await.starts_with("http/1.0 200"));
            proxy.await.unwrap().unwrap();
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn plaintext_detection_replays_a_fragmented_http2_preface() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let (mut client, from) = tokio::io::duplex(4096);
            let (to, mut backend) = tokio::io::duplex(4096);
            let proxy = tokio::spawn(run_plain_http_proxy(
                from,
                to,
                None,
                ForwardedHeaders::Plaintext,
                None,
            ));
            for byte in H2_PREFACE {
                client.write_all(&[*byte]).await.unwrap();
                tokio::task::yield_now().await;
            }
            let mut preface = vec![0; H2_PREFACE.len()];
            backend.read_exact(&mut preface).await.unwrap();
            assert_eq!(preface, H2_PREFACE);
            proxy.abort();
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn plaintext_close_is_clean_before_any_bytes_but_rejects_partial_prefaces() {
        tokio::time::timeout(Duration::from_secs(5), async {
            for prefix in [b"".as_slice(), b"P", b"PRI *"] {
                let (mut client, from) = tokio::io::duplex(4096);
                let (to, _backend) = tokio::io::duplex(4096);
                client.write_all(prefix).await.unwrap();
                client.shutdown().await.unwrap();
                let result =
                    run_plain_http_proxy(from, to, None, ForwardedHeaders::Plaintext, None).await;
                if prefix.is_empty() {
                    result.unwrap();
                } else {
                    assert_eq!(result.unwrap_err().kind, ErrorKind::Network);
                }
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn plaintext_detection_times_out_idle_and_incomplete_prefaces() {
        for prefix in [b"".as_slice(), b"PRI *"] {
            let (mut client, from) = tokio::io::duplex(4096);
            let (to, _backend) = tokio::io::duplex(4096);
            client.write_all(prefix).await.unwrap();
            let start = tokio::time::Instant::now();
            let error = run_plain_http_proxy(from, to, None, ForwardedHeaders::Plaintext, None)
                .await
                .unwrap_err();
            assert_eq!(error.kind, ErrorKind::Network);
            assert_eq!(start.elapsed(), HTTP_HEADER_TIMEOUT);
        }
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

        let mut req = req_with_auth(Some("Basic YWxpY2U6aHVudGVyMg=="));
        apply_request_policy(&mut req, None, ForwardedHeaders::Disabled, Some(&gate)).unwrap();
        assert_eq!(req.headers().get("X-Forwarded-User").unwrap(), "alice");

        let mut req = req_with_auth(Some("Basic Ym9iOnN3b3JkZmlzaA=="));
        apply_request_policy(&mut req, None, ForwardedHeaders::Disabled, Some(&gate)).unwrap();
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

        let mut req = req_with_auth(Some("Basic YWxpY2U6d3Jvbmc="));
        let resp = apply_request_policy(&mut req, None, ForwardedHeaders::Disabled, Some(&gate))
            .unwrap_err();
        assert_eq!(resp.status(), http::StatusCode::UNAUTHORIZED);
        assert_eq!(
            resp.headers().get(http::header::WWW_AUTHENTICATE).unwrap(),
            "Basic realm=\"My App\""
        );

        let mut req = req_with_auth(None);
        let resp = apply_request_policy(&mut req, None, ForwardedHeaders::Disabled, Some(&gate))
            .unwrap_err();
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
        apply_request_policy(&mut req, None, ForwardedHeaders::Disabled, Some(&gate)).unwrap();
        assert!(req.headers().get("X-Forwarded-User").is_none());

        let mut req = req_with_auth(Some("Bearer gamma"));
        let resp = apply_request_policy(&mut req, None, ForwardedHeaders::Disabled, Some(&gate))
            .unwrap_err();
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
        let mut req = req_with_auth(None);
        req.headers_mut()
            .insert("X-Forwarded-User", HeaderValue::from_static("root"));
        apply_request_policy(&mut req, None, ForwardedHeaders::Disabled, None).unwrap();
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
        let resp = apply_request_policy(&mut req, None, ForwardedHeaders::Disabled, Some(&gate))
            .unwrap_err();
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
        apply_request_policy(&mut req, None, ForwardedHeaders::Disabled, Some(&gate)).unwrap();
        assert_eq!(req.headers().get("X-Forwarded-User").unwrap(), "alice");
    }
}
