use std::cmp::min;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;

use async_compression::tokio::bufread::GzipEncoder;
use axum::Router;
use axum::body::Body;
use axum::extract::{self as x, Request};
use axum::response::Response;
use axum::routing::{any, get};
use base64::Engine;
use base64::display::Base64Display;
use digest::Digest;
use futures::future::ready;
use http::header::{
    ACCEPT_ENCODING, ACCEPT_RANGES, CACHE_CONTROL, CONNECTION, CONTENT_ENCODING, CONTENT_LENGTH,
    CONTENT_RANGE, CONTENT_TYPE, ETAG, IF_NONE_MATCH, RANGE, VARY,
};
use http::request::Parts as RequestParts;
use http::{HeaderValue, Method, StatusCode};
use imbl_value::InternedString;
use include_dir::Dir;
use new_mime_guess::MimeGuess;
use openssl::hash::MessageDigest;
use openssl::x509::X509;
use rpc_toolkit::{Context, HttpServer, ParentHandler, Server};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, BufReader};
use tokio_util::io::ReaderStream;
use url::Url;

use crate::context::{DiagnosticContext, InitContext, RpcContext, SetupContext};
use crate::hostname::ServerHostname;
use crate::middleware::auth::Auth;
use crate::middleware::auth::signature::verify_request_signature;
use crate::middleware::cors::Cors;
use crate::middleware::db::SyncDb;
use crate::prelude::*;
use crate::rpc_continuations::{Guid, RpcContinuations};
use crate::s9pk::S9pk;
use crate::s9pk::merkle_archive::file_contents::FileContents;
use crate::s9pk::merkle_archive::source::FileSource;
use crate::s9pk::merkle_archive::source::http::HttpSource;
use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;
use crate::util::io::{maybe_open_file, open_file};
use crate::util::serde::BASE64;
use crate::{PackageId, main_api};

const NOT_FOUND: &[u8] = b"Not Found";
const METHOD_NOT_ALLOWED: &[u8] = b"Method Not Allowed";
const NOT_AUTHORIZED: &[u8] = b"Not Authorized";
const INTERNAL_SERVER_ERROR: &[u8] = b"Internal Server Error";
const IMMUTABLE_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";
const REVALIDATE_CACHE_CONTROL: &str = "no-cache";
const IMMUTABLE_ASSETS_MANIFEST: &str = "immutable-assets.txt";

pub const EMPTY_DIR: Dir<'_> = Dir::new("", &[]);

pub trait UiContext: Context + AsRef<RpcContinuations> + Clone + Sized {
    fn ui_dir() -> &'static Dir<'static>;
    fn api() -> ParentHandler<Self>;
    fn middleware(server: Server<Self>) -> HttpServer<Self>;
    fn extend_router(self, router: Router) -> Router {
        router
    }
    /// Applies layers after the UI fallback is installed.
    fn apply_outer_layers(self, router: Router) -> Router {
        router
    }
}

pub static UI_CELL: OnceLock<Dir<'static>> = OnceLock::new();

impl UiContext for RpcContext {
    fn ui_dir() -> &'static Dir<'static> {
        UI_CELL.get().unwrap_or(&EMPTY_DIR)
    }
    fn api() -> ParentHandler<Self> {
        main_api()
    }
    fn middleware(server: Server<Self>) -> HttpServer<Self> {
        server
            .middleware(Cors::new())
            .middleware(Auth::new().with_local_auth().with_signature_auth())
            .middleware(SyncDb::new())
    }
    fn extend_router(self, router: Router) -> Router {
        router
            .nest("/s9pk", s9pk_router(self.clone()))
            .route("/static/local-root-ca.crt", {
                let ctx = self.clone();
                get(move || {
                    let ctx = ctx.clone();
                    async move {
                        ctx.account
                            .peek(|account| cert_send(&account.root_ca_cert, &account.hostname))
                    }
                })
            })
            .route("/manifest.webmanifest", {
                let ctx = self.clone();
                get(move || {
                    let ctx = ctx.clone();
                    async move {
                        ctx.account
                            .peek(|account| webmanifest_send(Self::ui_dir(), &account.hostname))
                    }
                })
            })
            .route(
                "/static/local-root-ca.mobileconfig",
                get(move || {
                    let ctx = self.clone();
                    async move {
                        ctx.account.peek(|account| {
                            mobileconfig_send(&account.root_ca_cert, &account.hostname)
                        })
                    }
                }),
            )
    }
    fn apply_outer_layers(self, router: Router) -> Router {
        crate::net::domain_redirect::redirect_service_domains(self, router)
    }
}

impl UiContext for InitContext {
    fn ui_dir() -> &'static Dir<'static> {
        UI_CELL.get().unwrap_or(&EMPTY_DIR)
    }
    fn api() -> ParentHandler<Self> {
        main_api()
    }
    fn middleware(server: Server<Self>) -> HttpServer<Self> {
        server.middleware(Cors::new())
    }
}

impl UiContext for DiagnosticContext {
    fn ui_dir() -> &'static Dir<'static> {
        UI_CELL.get().unwrap_or(&EMPTY_DIR)
    }
    fn api() -> ParentHandler<Self> {
        main_api()
    }
    fn middleware(server: Server<Self>) -> HttpServer<Self> {
        server.middleware(Cors::new())
    }
}

pub static SETUP_WIZARD_CELL: OnceLock<Dir<'static>> = OnceLock::new();

impl UiContext for SetupContext {
    fn ui_dir() -> &'static Dir<'static> {
        SETUP_WIZARD_CELL.get().unwrap_or(&EMPTY_DIR)
    }
    fn api() -> ParentHandler<Self> {
        main_api()
    }
    fn middleware(server: Server<Self>) -> HttpServer<Self> {
        server.middleware(Cors::new())
    }
}

pub fn rpc_router<C: Context + Clone + AsRef<RpcContinuations>>(
    ctx: C,
    server: HttpServer<C>,
) -> Router {
    Router::new()
        .route("/rpc/{*path}", any(server))
        .route(
            "/ws/rpc/{guid}",
            any({
                let ctx = ctx.clone();
                move |x::Path(guid): x::Path<Guid>,
                      ws: axum::extract::ws::WebSocketUpgrade| async move {
                    match AsRef::<RpcContinuations>::as_ref(&ctx).get_ws_handler(&guid).await {
                        Some(cont) => ws.on_upgrade(cont),
                        _ => not_found(),
                    }
                }
            }),
        )
        .route(
            "/rest/rpc/{guid}",
            any({
                let ctx = ctx.clone();
                move |x::Path(guid): x::Path<Guid>, request: x::Request| async move {
                    match AsRef::<RpcContinuations>::as_ref(&ctx).get_rest_handler(&guid).await {
                        None => not_found(),
                        Some(cont) => cont(request).await.unwrap_or_else(server_error),
                    }
                }
            }),
        )
}

/// Matches an exact path listed in `immutable-assets.txt`.
pub fn is_ui_asset_immutable(ui_dir: &Dir<'_>, path: &Path) -> bool {
    let Some(path) = path.to_str() else {
        return false;
    };
    ui_dir
        .get_file(IMMUTABLE_ASSETS_MANIFEST)
        .and_then(|file| std::str::from_utf8(file.contents()).ok())
        .is_some_and(|assets| assets.lines().any(|asset| asset == path))
}

/// Matches paths whose final segment has no extension.
pub fn is_ui_route(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|name| !name.contains('.'))
}

fn serve_ui(req: Request, ui_dir: &'static Dir<'static>) -> Result<Response, Error> {
    let (request_parts, _body) = req.into_parts();
    match &request_parts.method {
        &Method::GET | &Method::HEAD => {
            let uri_path = request_parts
                .uri
                .path()
                .strip_prefix('/')
                .unwrap_or(request_parts.uri.path());

            let file = ui_dir.get_file(uri_path).or_else(|| {
                is_ui_route(uri_path)
                    .then(|| ui_dir.get_file("index.html"))
                    .flatten()
            });

            match file {
                Some(file) => FileData::from_embedded(&request_parts, file, ui_dir)
                    .into_response(&request_parts),
                None => Ok(not_found()),
            }
        }
        _ => Ok(method_not_allowed()),
    }
}

/// Hardening headers on every UI-origin response. The CSP is the backstop
/// that keeps an XSS from exfiltrating or persistently abusing the enrolled
/// signing key: same-origin scripts and connections only, no framing, no
/// plugin content. `'unsafe-inline'` styles are required by the `<style>`
/// tags Angular injects at runtime.
async fn add_security_headers(mut res: Response) -> Response {
    let headers = res.headers_mut();
    headers.insert(
        http::header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
             img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self'; \
             object-src 'none'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'",
        ),
    );
    headers.insert(
        http::header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    res
}

pub fn ui_router<C: UiContext>(ctx: C) -> Router {
    let server = C::middleware(Server::new(
        {
            let ctx = ctx.clone();
            move || ready(Ok(ctx.clone()))
        },
        C::api(),
    ));
    let router = ctx
        .clone()
        .extend_router(rpc_router(ctx.clone(), server))
        .fallback(any(|request: Request| async move {
            serve_ui(request, C::ui_dir()).unwrap_or_else(server_error)
        }));
    // Security headers cover responses produced by context layers.
    ctx.apply_outer_layers(router)
        .layer(axum::middleware::map_response(add_security_headers))
}

pub fn refresher() -> Router {
    Router::new().fallback(get(|request: Request| async move {
        let res = include_bytes!("./refresher.html");
        FileData {
            data: Body::from(&res[..]),
            content_range: None,
            e_tag: None,
            cache_control: Some(REVALIDATE_CACHE_CONTROL),
            encoding: None,
            len: Some(res.len() as u64),
            mime: Some("text/html".into()),
            digest: None,
        }
        .into_response(&request.into_parts().0)
        .unwrap_or_else(server_error)
    }))
}

fn s9pk_router(ctx: RpcContext) -> Router {
    Router::new()
        .route("/installed/{s9pk}", {
            let ctx = ctx.clone();
            get(
                |x::Path(s9pk): x::Path<String>, request: Request| async move {
                    if_authorized(&ctx, request, |request| async {
                        let id = s9pk
                            .strip_suffix(".s9pk")
                            .unwrap_or(&s9pk)
                            .parse::<PackageId>()?;
                        let (parts, _) = request.into_parts();
                        match FileData::from_installed_s9pk(
                            &parts,
                            &ctx.db
                                .peek()
                                .await
                                .into_public()
                                .into_package_data()
                                .into_idx(&id)
                                .or_not_found(&id)?
                                .into_s9pk()
                                .de()?,
                        )
                        .await?
                        {
                            Some(file) => file.into_response(&parts),
                            None => Ok(not_found()),
                        }
                    })
                    .await
                    .unwrap_or_else(server_error)
                },
            )
        })
        .route("/installed/{s9pk}/{*path}", {
            let ctx = ctx.clone();
            get(
                |x::Path((s9pk, path)): x::Path<(String, PathBuf)>,
                 x::RawQuery(query): x::RawQuery,
                 request: Request| async move {
                    if_authorized(&ctx, request, |request| async {
                        let id = s9pk
                            .strip_suffix(".s9pk")
                            .unwrap_or(&s9pk)
                            .parse::<PackageId>()?;
                        let s9pk = S9pk::deserialize(
                            &MultiCursorFile::from(
                                open_file(
                                    ctx.db
                                        .peek()
                                        .await
                                        .into_public()
                                        .into_package_data()
                                        .into_idx(&id)
                                        .or_not_found(&id)?
                                        .into_s9pk()
                                        .de()?,
                                )
                                .await?,
                            ),
                            query
                                .as_deref()
                                .map(MerkleArchiveCommitment::from_query)
                                .and_then(|a| a.transpose())
                                .transpose()?
                                .as_ref(),
                        )
                        .await?;
                        let (parts, _) = request.into_parts();
                        match FileData::from_s9pk(&parts, &s9pk, &path).await? {
                            Some(file) => file.into_response(&parts),
                            None => Ok(not_found()),
                        }
                    })
                    .await
                    .unwrap_or_else(server_error)
                },
            )
        })
        .route(
            "/proxy/{url}/{*path}",
            get(
                |x::Path((url, path)): x::Path<(Url, PathBuf)>,
                 x::RawQuery(query): x::RawQuery,
                 request: Request| async move {
                    if_authorized(&ctx, request, |request| async {
                        let s9pk = S9pk::deserialize(
                            &Arc::new(HttpSource::new(ctx.client.get(), url).await?),
                            query
                                .as_deref()
                                .map(MerkleArchiveCommitment::from_query)
                                .and_then(|a| a.transpose())
                                .transpose()?
                                .as_ref(),
                        )
                        .await?;
                        let (parts, _) = request.into_parts();
                        match FileData::from_s9pk(&parts, &s9pk, &path).await? {
                            Some(file) => file.into_response(&parts),
                            None => Ok(not_found()),
                        }
                    })
                    .await
                    .unwrap_or_else(server_error)
                },
            ),
        )
}

async fn if_authorized<
    F: FnOnce(Request) -> Fut,
    Fut: Future<Output = Result<Response, Error>> + Send,
>(
    ctx: &RpcContext,
    mut request: Request,
    f: F,
) -> Result<Response, Error> {
    let path = request.uri().path().to_owned();
    match async {
        let signer = verify_request_signature(ctx, &mut request).await?;
        let key = signer.interned_pem();
        let enrolled = ctx
            .ephemeral_auth_keys
            .peek(|keys| keys.0.contains_key(&*key))
            || ctx
                .db
                .peek()
                .await
                .as_private()
                .as_session_pubkeys()
                .de()?
                .0
                .contains_key(&*key);
        if !enrolled {
            return Err(Error::new(
                eyre!("{}", t!("middleware.auth.unauthorized")),
                ErrorKind::Authorization,
            ));
        }
        Ok(signer)
    }
    .await
    {
        Err(e) => Ok(unauthorized(e, &path)),
        Ok(_) => f(request).await,
    }
}

pub fn unauthorized(err: Error, path: &str) -> Response {
    tracing::warn!("unauthorized for {} @{:?}", err, path);
    tracing::debug!("{:?}", err);
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .body(NOT_AUTHORIZED.into())
        .unwrap()
}

/// HTTP status code 404
pub fn not_found() -> Response {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(NOT_FOUND.into())
        .unwrap()
}

/// HTTP status code 405
pub fn method_not_allowed() -> Response {
    Response::builder()
        .status(StatusCode::METHOD_NOT_ALLOWED)
        .body(METHOD_NOT_ALLOWED.into())
        .unwrap()
}

pub fn server_error(err: Error) -> Response {
    tracing::error!("internal server error: {}", err);
    tracing::debug!("{:?}", err);
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(INTERNAL_SERVER_ERROR.into())
        .unwrap()
}

pub fn bad_request() -> Response {
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .body(Body::empty())
        .unwrap()
}

fn webmanifest_send(
    ui_dir: &'static Dir<'static>,
    hostname: &ServerHostname,
) -> Result<Response, Error> {
    let mut manifest: serde_json::Map<String, serde_json::Value> = serde_json::from_slice(
        ui_dir
            .get_file("manifest.webmanifest")
            .or_not_found("manifest.webmanifest")?
            .contents(),
    )
    .with_kind(ErrorKind::Deserialization)?;
    manifest.insert("name".into(), hostname.as_ref().into());
    manifest.insert("short_name".into(), hostname.as_ref().into());
    let body = serde_json::to_vec(&manifest).with_kind(ErrorKind::Serialization)?;

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/manifest+json")
        .header(CACHE_CONTROL, REVALIDATE_CACHE_CONTROL)
        .header(CONTENT_LENGTH, body.len())
        .body(Body::from(body))
        .with_kind(ErrorKind::Network)
}

fn cert_send(cert: &X509, hostname: &ServerHostname) -> Result<Response, Error> {
    let pem = cert.to_pem()?;
    Response::builder()
        .status(StatusCode::OK)
        .header(
            http::header::ETAG,
            base32::encode(
                base32::Alphabet::Rfc4648 { padding: false },
                &*cert.digest(MessageDigest::sha256())?,
            )
            .to_lowercase(),
        )
        .header(http::header::CONTENT_TYPE, "application/x-x509-ca-cert")
        .header(http::header::CONTENT_LENGTH, pem.len())
        .header(
            http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}.crt\"", hostname.as_ref()),
        )
        .body(Body::from(pem))
        .with_kind(ErrorKind::Network)
}

fn mobileconfig_send(cert: &X509, hostname: &ServerHostname) -> Result<Response, Error> {
    let der = cert.to_der()?;
    let fingerprint = hex::encode(&*cert.digest(MessageDigest::sha256())?);
    let cert_uuid = format_uuid_from_hex(&fingerprint[..32]);
    let profile_uuid = format_uuid_from_hex(&fingerprint[32..64]);
    let der_b64 = BASE64.encode(&der);
    let host = hostname.as_ref();

    let plist = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \t<key>PayloadContent</key>\n\
         \t<array>\n\
         \t\t<dict>\n\
         \t\t\t<key>PayloadCertificateFileName</key>\n\
         \t\t\t<string>{host}.crt</string>\n\
         \t\t\t<key>PayloadContent</key>\n\
         \t\t\t<data>{der_b64}</data>\n\
         \t\t\t<key>PayloadDescription</key>\n\
         \t\t\t<string>Adds the StartOS root certificate authority for {host}.</string>\n\
         \t\t\t<key>PayloadDisplayName</key>\n\
         \t\t\t<string>{host} Root Certificate</string>\n\
         \t\t\t<key>PayloadIdentifier</key>\n\
         \t\t\t<string>com.start9.ca.cert.{cert_uuid}</string>\n\
         \t\t\t<key>PayloadType</key>\n\
         \t\t\t<string>com.apple.security.root</string>\n\
         \t\t\t<key>PayloadUUID</key>\n\
         \t\t\t<string>{cert_uuid}</string>\n\
         \t\t\t<key>PayloadVersion</key>\n\
         \t\t\t<integer>1</integer>\n\
         \t\t</dict>\n\
         \t</array>\n\
         \t<key>PayloadDescription</key>\n\
         \t<string>Trusts the root certificate authority for {host}.</string>\n\
         \t<key>PayloadDisplayName</key>\n\
         \t<string>StartOS Root CA ({host})</string>\n\
         \t<key>PayloadIdentifier</key>\n\
         \t<string>com.start9.ca.profile.{profile_uuid}</string>\n\
         \t<key>PayloadType</key>\n\
         \t<string>Configuration</string>\n\
         \t<key>PayloadUUID</key>\n\
         \t<string>{profile_uuid}</string>\n\
         \t<key>PayloadVersion</key>\n\
         \t<integer>1</integer>\n\
         </dict>\n\
         </plist>\n",
    );

    Response::builder()
        .status(StatusCode::OK)
        .header(
            http::header::ETAG,
            base32::encode(
                base32::Alphabet::Rfc4648 { padding: false },
                &*cert.digest(MessageDigest::sha256())?,
            )
            .to_lowercase(),
        )
        .header(
            http::header::CONTENT_TYPE,
            "application/x-apple-aspen-config",
        )
        .header(http::header::CONTENT_LENGTH, plist.len())
        .header(
            http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}.mobileconfig\"", host),
        )
        .body(Body::from(plist))
        .with_kind(ErrorKind::Network)
}

fn format_uuid_from_hex(hex32: &str) -> String {
    format!(
        "{}-{}-{}-{}-{}",
        &hex32[0..8],
        &hex32[8..12],
        &hex32[12..16],
        &hex32[16..20],
        &hex32[20..32],
    )
}

fn accepts_encoding(req: &RequestParts, encoding: &str) -> bool {
    req.headers
        .get_all(ACCEPT_ENCODING)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .flat_map(|h| h.split(','))
        .filter_map(|e| e.split(';').next())
        .any(|e| e.trim() == encoding)
}

fn if_none_match(req: &RequestParts, e_tag: &str) -> bool {
    req.headers
        .get_all(IF_NONE_MATCH)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .flat_map(|h| h.split(','))
        .any(|candidate| candidate.trim() == e_tag)
}

/// Ignores a range it cannot satisfy.
fn parse_range(header: &HeaderValue, len: u64) -> Option<(u64, u64)> {
    let (start, end) = header
        .to_str()
        .ok()?
        .strip_prefix("bytes=")?
        .split_once('-')?;
    let last = len.checked_sub(1)?;
    let (start, end) = if start.is_empty() {
        (len.saturating_sub(end.parse().ok()?), last)
    } else if end.is_empty() {
        (start.parse().ok()?, last)
    } else {
        (start.parse().ok()?, min(end.parse().ok()?, last))
    };
    (start <= end).then_some((start, end))
}

fn precompressed(
    req: &RequestParts,
    ui_dir: &'static Dir<'static>,
    path: &Path,
    encoding: &'static str,
    extension: &str,
) -> Option<(&'static str, &'static [u8])> {
    if !accepts_encoding(req, encoding) {
        return None;
    }
    let file = ui_dir.get_file(format!("{}.{extension}", path.display()))?;
    Some((encoding, file.contents()))
}

struct FileData {
    data: Body,
    len: Option<u64>,
    content_range: Option<(u64, u64, u64)>,
    encoding: Option<&'static str>,
    e_tag: Option<String>,
    cache_control: Option<&'static str>,
    mime: Option<InternedString>,
    digest: Option<(&'static str, Vec<u8>)>,
}
impl FileData {
    fn from_embedded(
        req: &RequestParts,
        file: &'static include_dir::File<'static>,
        ui_dir: &'static Dir<'static>,
    ) -> Self {
        let path = file.path();
        let size = file.contents().len() as u64;
        let range = req.headers.get(RANGE).and_then(|r| parse_range(r, size));
        let (encoding, data) = if range.is_some() {
            (None, file.contents())
        } else if let Some((encoding, data)) = precompressed(req, ui_dir, path, "br", "br")
            .or_else(|| precompressed(req, ui_dir, path, "gzip", "gz"))
        {
            (Some(encoding), data)
        } else {
            (None, file.contents())
        };
        let data = match range {
            Some((start, end)) => &data[start as usize..=end as usize],
            None => data,
        };

        Self {
            data: if req.method == Method::HEAD {
                Body::empty()
            } else {
                Body::from(data)
            },
            len: Some(data.len() as u64),
            content_range: range.map(|(start, end)| (start, end, size)),
            encoding,
            e_tag: file
                .metadata()
                .map(|metadata| e_tag(path, metadata.modified(), encoding)),
            cache_control: Some(if is_ui_asset_immutable(ui_dir, path) {
                IMMUTABLE_CACHE_CONTROL
            } else {
                REVALIDATE_CACHE_CONTROL
            }),
            mime: MimeGuess::from_path(path)
                .first()
                .map(|m| m.essence_str().into()),
            digest: None,
        }
    }

    fn encode<R: AsyncRead + Send + 'static>(gzip: bool, data: R, len: u64) -> (Option<u64>, Body) {
        if gzip {
            (
                None,
                Body::from_stream(ReaderStream::new(GzipEncoder::new(BufReader::new(data)))),
            )
        } else {
            (Some(len), Body::from_stream(ReaderStream::new(data)))
        }
    }

    async fn from_installed_s9pk(req: &RequestParts, path: &Path) -> Result<Option<Self>, Error> {
        let Some(mut file) = maybe_open_file(path).await? else {
            return Ok(None);
        };
        let metadata = file
            .metadata()
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, path.display().to_string()))?;
        let size = metadata.len();
        let range = req.headers.get(RANGE).and_then(|r| parse_range(r, size));
        let gzip = range.is_none() && accepts_encoding(req, "gzip");
        let encoding = gzip.then_some("gzip");
        let (len, data) = match range {
            Some((start, end)) => {
                let len = end + 1 - start;
                file.seek(std::io::SeekFrom::Start(start)).await?;
                Self::encode(gzip, file.take(len), len)
            }
            None => Self::encode(gzip, file, size),
        };

        Ok(Some(Self {
            data: if req.method == Method::HEAD {
                Body::empty()
            } else {
                data
            },
            len,
            content_range: range.map(|(start, end)| (start, end, size)),
            encoding,
            e_tag: Some(e_tag(path, metadata.modified()?, encoding)),
            cache_control: Some(REVALIDATE_CACHE_CONTROL),
            mime: MimeGuess::from_path(path)
                .first()
                .map(|m| m.essence_str().into()),
            digest: None,
        }))
    }

    async fn from_s9pk<S: FileSource>(
        req: &RequestParts,
        s9pk: &S9pk<S>,
        path: &Path,
    ) -> Result<Option<Self>, Error> {
        let Some(file) = s9pk.as_archive().contents().get_path(path) else {
            return Ok(None);
        };
        let Some(contents) = file.as_file() else {
            return Ok(None);
        };
        let (digest, size) = if let Some((hash, size)) = file.hash() {
            (Some(("blake3", hash.as_bytes().to_vec())), size)
        } else {
            (None, contents.size().await?)
        };

        Ok(Some(
            Self::from_s9pk_contents(req, path, contents, digest, size).await?,
        ))
    }

    async fn from_s9pk_contents<S: FileSource>(
        req: &RequestParts,
        path: &Path,
        contents: &FileContents<S>,
        digest: Option<(&'static str, Vec<u8>)>,
        size: u64,
    ) -> Result<Self, Error> {
        let range = req.headers.get(RANGE).and_then(|r| parse_range(r, size));
        let gzip = range.is_none() && accepts_encoding(req, "gzip");
        let len = range.map_or(size, |(start, end)| end + 1 - start);
        let (len, data) = if req.method == Method::HEAD {
            ((!gzip).then_some(len), Body::empty())
        } else if let Some((start, _)) = range {
            Self::encode(gzip, contents.slice(start, len).await?, len)
        } else {
            Self::encode(gzip, contents.reader().await?.take(len), len)
        };

        Ok(Self {
            data,
            len,
            content_range: range.map(|(start, end)| (start, end, size)),
            encoding: gzip.then_some("gzip"),
            e_tag: None,
            cache_control: None,
            mime: MimeGuess::from_path(path)
                .first()
                .map(|m| m.essence_str().into()),
            digest,
        })
    }

    fn into_response(self, req: &RequestParts) -> Result<Response, Error> {
        let mut builder = Response::builder()
            .header(VARY, "Accept-Encoding")
            .header(ACCEPT_RANGES, "bytes");
        if let Some(mime) = self.mime {
            builder = builder.header(CONTENT_TYPE, &*mime);
        }
        if let Some(e_tag) = &self.e_tag {
            builder = builder.header(ETAG, &**e_tag);
        }
        if let Some(cache_control) = self.cache_control {
            builder = builder.header(CACHE_CONTROL, cache_control);
        }
        if let Some((algorithm, digest)) = self.digest {
            builder = builder.header(
                "File-Digest",
                format!("{algorithm}=:{}:", Base64Display::new(&digest, &BASE64)),
            );
        }
        if req
            .headers
            .get_all(CONNECTION)
            .iter()
            .flat_map(|s| s.to_str().ok())
            .flat_map(|s| s.split(","))
            .any(|s| s.trim() == "keep-alive")
        {
            builder = builder.header(CONNECTION, "keep-alive");
        }

        if self
            .e_tag
            .as_deref()
            .is_some_and(|e_tag| if_none_match(req, e_tag))
        {
            return builder
                .status(StatusCode::NOT_MODIFIED)
                .body(Body::empty())
                .with_kind(ErrorKind::Network);
        }
        if let Some((start, end, size)) = self.content_range {
            builder = builder
                .header(CONTENT_RANGE, format!("bytes {start}-{end}/{size}"))
                .status(StatusCode::PARTIAL_CONTENT);
        }
        if let Some(len) = self.len {
            builder = builder.header(CONTENT_LENGTH, len);
        }
        if let Some(encoding) = self.encoding {
            builder = builder.header(CONTENT_ENCODING, encoding);
        }
        builder.body(self.data).with_kind(ErrorKind::Network)
    }
}

fn e_tag(path: &Path, modified: SystemTime, encoding: Option<&str>) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(format!(
        "{path:?}:{modified:?}:{}",
        encoding.unwrap_or("identity")
    ));
    let res = hasher.finalize();
    format!(
        "\"{}\"",
        base32::encode(base32::Alphabet::Rfc4648 { padding: false }, res.as_slice()).to_lowercase()
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::body::{Bytes, to_bytes};
    use http::header::HeaderName;
    use include_dir::{DirEntry, File, Metadata};

    use super::*;

    const METADATA: Metadata = Metadata::new(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_secs(1),
    );

    const fn file(path: &'static str, contents: &'static [u8]) -> DirEntry<'static> {
        DirEntry::File(File::new(path, contents).with_metadata(METADATA))
    }

    static TEST_UI_DIR: Dir<'static> = Dir::new(
        "",
        &[
            file("index.html", b"<html>StartOS</html>"),
            file(
                IMMUTABLE_ASSETS_MANIFEST,
                b"main-ABCDEFGH.js\nmedia/font-ABCDEFGH.woff2\n",
            ),
            file("main-ABCDEFGH.js", b"console.log('StartOS')"),
            file("main-ABCDEFGH.js.gz", b"compressed javascript"),
            file("ngsw-worker.js", b"self.addEventListener()"),
            file("assets/logo.svg", b"<svg></svg>"),
            file("media/font-ABCDEFGH.woff2", b"font"),
        ],
    );

    struct UnreadableSource;

    impl FileSource for UnreadableSource {
        type Reader = std::io::Cursor<&'static [u8]>;
        type SliceReader = Self::Reader;

        async fn size(&self) -> Result<u64, Error> {
            panic!("HEAD read the source size")
        }

        async fn reader(&self) -> Result<Self::Reader, Error> {
            panic!("HEAD opened the source reader")
        }

        async fn slice(&self, _: u64, _: u64) -> Result<Self::SliceReader, Error> {
            panic!("HEAD sliced the source")
        }
    }

    fn request(method: Method, uri: &str, headers: &[(HeaderName, &str)]) -> Request {
        let mut request = Request::builder().method(method).uri(uri);
        for (name, value) in headers {
            request = request.header(name, *value);
        }
        request.body(Body::empty()).unwrap()
    }

    fn ui_response(method: Method, uri: &str, headers: &[(HeaderName, &str)]) -> Response {
        serve_ui(request(method, uri, headers), &TEST_UI_DIR).unwrap()
    }

    fn ui_get(uri: &str, headers: &[(HeaderName, &str)]) -> Response {
        ui_response(Method::GET, uri, headers)
    }

    fn header(response: &Response, name: HeaderName) -> &str {
        response
            .headers()
            .get(name)
            .map_or("", |header| header.to_str().unwrap())
    }

    async fn body(response: Response) -> Bytes {
        to_bytes(response.into_body(), usize::MAX).await.unwrap()
    }

    #[test]
    fn immutable_assets_are_declared_by_exact_path() {
        assert!(is_ui_asset_immutable(
            &TEST_UI_DIR,
            Path::new("main-ABCDEFGH.js")
        ));
        assert!(is_ui_asset_immutable(
            &TEST_UI_DIR,
            Path::new("media/font-ABCDEFGH.woff2")
        ));
        assert!(!is_ui_asset_immutable(
            &TEST_UI_DIR,
            Path::new("main-ABCDEFGH.js.gz")
        ));
        assert!(!is_ui_asset_immutable(
            &TEST_UI_DIR,
            Path::new("index.html")
        ));
        assert!(!is_ui_asset_immutable(
            &EMPTY_DIR,
            Path::new("main-ABCDEFGH.js")
        ));
    }

    #[test]
    fn ui_cache_policy_follows_the_manifest() {
        for path in [
            "/",
            "/ngsw-worker.js",
            "/assets/logo.svg",
            "/immutable-assets.txt",
            "/main-ABCDEFGH.js.gz",
        ] {
            let response = ui_get(path, &[]);
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert_eq!(
                header(&response, CACHE_CONTROL),
                REVALIDATE_CACHE_CONTROL,
                "{path}"
            );
            assert!(response.headers().contains_key(ETAG), "{path}");
        }
        for path in ["/main-ABCDEFGH.js", "/media/font-ABCDEFGH.woff2"] {
            let response = ui_get(path, &[]);
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert_eq!(
                header(&response, CACHE_CONTROL),
                IMMUTABLE_CACHE_CONTROL,
                "{path}"
            );
        }

        let gzip = ui_get("/main-ABCDEFGH.js", &[(ACCEPT_ENCODING, "gzip, br")]);
        assert_eq!(header(&gzip, CONTENT_ENCODING), "gzip");
        assert_eq!(header(&gzip, CACHE_CONTROL), IMMUTABLE_CACHE_CONTROL);
    }

    #[tokio::test]
    async fn revalidation_matches_the_representation() {
        let identity = ui_get("/main-ABCDEFGH.js", &[]);
        let identity_e_tag = header(&identity, ETAG).to_owned();
        assert_eq!(header(&identity, VARY), "Accept-Encoding");
        let gzip = ui_get("/main-ABCDEFGH.js", &[(ACCEPT_ENCODING, "gzip")]);
        let gzip_e_tag = header(&gzip, ETAG).to_owned();
        assert_ne!(gzip_e_tag, identity_e_tag);

        let matched = ui_get(
            "/main-ABCDEFGH.js",
            &[(IF_NONE_MATCH, &format!("\"old\", {identity_e_tag}"))],
        );
        assert_eq!(matched.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(header(&matched, ETAG), identity_e_tag);
        assert!(body(matched).await.is_empty());

        let stale = ui_get("/main-ABCDEFGH.js", &[(IF_NONE_MATCH, &gzip_e_tag)]);
        assert_eq!(stale.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn spa_fallback_serves_index_for_routes_only() {
        let index = body(ui_get("/", &[])).await;
        for path in [
            "/settings/general",
            "/settings/general/",
            "/route.name/child",
            "/settings/general?tab=a.b",
        ] {
            let response = ui_get(path, &[]);
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert_eq!(header(&response, CONTENT_TYPE), "text/html", "{path}");
            assert_eq!(body(response).await, index, "{path}");
        }
        for path in ["/missing.js", "/assets/missing/icon.svg", "/.hidden"] {
            assert_eq!(ui_get(path, &[]).status(), StatusCode::NOT_FOUND, "{path}");
        }
        assert_eq!(
            ui_response(Method::POST, "/", &[]).status(),
            StatusCode::METHOD_NOT_ALLOWED
        );
    }

    #[test]
    fn byte_ranges_are_parsed_or_ignored() {
        let parse = |range: &str| parse_range(&HeaderValue::from_str(range).unwrap(), 20);
        assert_eq!(parse("bytes=0-4"), Some((0, 4)));
        assert_eq!(parse("bytes=10-"), Some((10, 19)));
        assert_eq!(parse("bytes=5-50"), Some((5, 19)));
        assert_eq!(parse("bytes=-5"), Some((15, 19)));
        assert_eq!(parse("bytes=-50"), Some((0, 19)));
        for ignored in [
            "bytes=20-",
            "bytes=10-5",
            "bytes=-0",
            "bytes=-",
            "bytes=0-4,10-15",
            "bytes=garbage-",
            "items=0-5",
        ] {
            assert_eq!(parse(ignored), None, "{ignored}");
        }
        assert_eq!(parse_range(&HeaderValue::from_static("bytes=0-"), 0), None);
    }

    #[tokio::test]
    async fn ranged_ui_requests_serve_identity_bytes() {
        let ranged = ui_get(
            "/main-ABCDEFGH.js",
            &[(RANGE, "bytes=0-4"), (ACCEPT_ENCODING, "gzip")],
        );
        assert_eq!(ranged.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(header(&ranged, CONTENT_RANGE), "bytes 0-4/22");
        assert_eq!(header(&ranged, CONTENT_LENGTH), "5");
        assert!(!ranged.headers().contains_key(CONTENT_ENCODING));
        assert_eq!(body(ranged).await, "conso");

        let ignored = ui_get("/main-ABCDEFGH.js", &[(RANGE, "bytes=22-")]);
        assert_eq!(ignored.status(), StatusCode::OK);
        assert!(!ignored.headers().contains_key(CONTENT_RANGE));

        let head = ui_response(Method::HEAD, "/main-ABCDEFGH.js", &[]);
        assert_eq!(head.status(), StatusCode::OK);
        assert_eq!(header(&head, CONTENT_LENGTH), "22");
        assert!(body(head).await.is_empty());
    }

    #[tokio::test]
    async fn s9pk_file_digest_names_the_decoded_file_on_every_response() {
        use async_compression::tokio::bufread::GzipDecoder;

        async fn response(
            bytes: &Arc<[u8]>,
            digest: &[u8],
            headers: &[(HeaderName, &str)],
        ) -> Response {
            let contents = FileContents::new(bytes.clone());
            let parts = request(Method::GET, "/asset.bin", headers).into_parts().0;
            FileData::from_s9pk_contents(
                &parts,
                Path::new("asset.bin"),
                &contents,
                Some(("blake3", digest.to_vec())),
                bytes.len() as u64,
            )
            .await
            .unwrap()
            .into_response(&parts)
            .unwrap()
        }

        let bytes: Arc<[u8]> = Arc::from(&b"signed package bytes"[..]);
        let digest = blake3::hash(&bytes).as_bytes().to_vec();
        let expected = format!("blake3=:{}:", Base64Display::new(&digest, &BASE64));
        let file_digest = HeaderName::from_static("file-digest");

        let full = response(&bytes, &digest, &[]).await;
        assert_eq!(full.status(), StatusCode::OK);
        assert_eq!(header(&full, file_digest.clone()), expected);
        assert_eq!(body(full).await, bytes.as_ref());

        let ranged = response(
            &bytes,
            &digest,
            &[(RANGE, "bytes=2-7"), (ACCEPT_ENCODING, "gzip")],
        )
        .await;
        assert_eq!(ranged.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(header(&ranged, CONTENT_RANGE), "bytes 2-7/20");
        assert!(!ranged.headers().contains_key(CONTENT_ENCODING));
        assert_eq!(header(&ranged, file_digest.clone()), expected);
        assert_eq!(body(ranged).await, &bytes[2..=7]);

        let gzip = response(&bytes, &digest, &[(ACCEPT_ENCODING, "gzip")]).await;
        assert_eq!(header(&gzip, CONTENT_ENCODING), "gzip");
        assert_eq!(header(&gzip, file_digest), expected);
        let mut decoded = Vec::new();
        GzipDecoder::new(BufReader::new(std::io::Cursor::new(body(gzip).await)))
            .read_to_end(&mut decoded)
            .await
            .unwrap();
        assert_eq!(decoded, bytes.as_ref());
    }

    #[tokio::test]
    async fn s9pk_head_does_not_read_contents() {
        for (headers, encoding, length) in [
            (vec![], "", "21"),
            (vec![(ACCEPT_ENCODING, "gzip")], "gzip", ""),
        ] {
            let contents = FileContents::new(UnreadableSource);
            let parts = request(Method::HEAD, "/asset.bin", &headers).into_parts().0;
            let response =
                FileData::from_s9pk_contents(&parts, Path::new("asset.bin"), &contents, None, 21)
                    .await
                    .unwrap()
                    .into_response(&parts)
                    .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(header(&response, CONTENT_ENCODING), encoding);
            assert_eq!(header(&response, CONTENT_LENGTH), length);
            assert!(body(response).await.is_empty());
        }
    }

    #[tokio::test]
    async fn installed_s9pks_revalidate_and_resume() {
        async fn response(path: &Path, headers: &[(HeaderName, &str)]) -> Response {
            let parts = request(Method::GET, "/", headers).into_parts().0;
            FileData::from_installed_s9pk(&parts, path)
                .await
                .unwrap()
                .unwrap()
                .into_response(&parts)
                .unwrap()
        }

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("package.s9pk");
        tokio::fs::write(&path, b"0123456789").await.unwrap();

        let full = response(&path, &[]).await;
        assert_eq!(header(&full, CACHE_CONTROL), REVALIDATE_CACHE_CONTROL);
        let e_tag = header(&full, ETAG).to_owned();
        assert_eq!(body(full).await, "0123456789");

        let not_modified = response(&path, &[(IF_NONE_MATCH, &e_tag)]).await;
        assert_eq!(not_modified.status(), StatusCode::NOT_MODIFIED);

        let gzip = response(&path, &[(ACCEPT_ENCODING, "gzip")]).await;
        assert_eq!(header(&gzip, CONTENT_ENCODING), "gzip");
        assert_ne!(header(&gzip, ETAG), e_tag);

        let ranged = response(&path, &[(RANGE, "bytes=2-5"), (ACCEPT_ENCODING, "gzip")]).await;
        assert_eq!(ranged.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(header(&ranged, CONTENT_RANGE), "bytes 2-5/10");
        assert!(!ranged.headers().contains_key(CONTENT_ENCODING));
        assert_eq!(body(ranged).await, "2345");

        let parts = request(Method::GET, "/", &[]).into_parts().0;
        assert!(
            FileData::from_installed_s9pk(&parts, &dir.path().join("missing.s9pk"))
                .await
                .unwrap()
                .is_none()
        );
    }
}
