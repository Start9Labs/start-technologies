use std::cmp::min;
use std::future::Future;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

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
    CONTENT_RANGE, CONTENT_TYPE, ETAG, IF_NONE_MATCH, IF_RANGE, RANGE, VARY,
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
const IMMUTABLE_UI_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";
const REVALIDATE_CACHE_CONTROL: &str = "no-cache";
const PRIVATE_REVALIDATE_CACHE_CONTROL: &str = "private, no-cache";

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
                get(move |request: Request| {
                    let ctx = ctx.clone();
                    async move {
                        let (request_parts, _body) = request.into_parts();
                        ctx.account.peek(|account| {
                            webmanifest_send(&request_parts, Self::ui_dir(), &account.hostname)
                        })
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

fn is_content_hashed(path: &Path) -> bool {
    if path.components().count() != 1 {
        return false;
    }
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(stem) = file_name
        .strip_suffix(".js")
        .or_else(|| file_name.strip_suffix(".css"))
    else {
        return false;
    };
    let bytes = stem.as_bytes();
    bytes.len() > 9
        && bytes[bytes.len() - 9] == b'-'
        && bytes[bytes.len() - 8..]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-' || *byte == b'_')
}

fn is_route_like(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|name| !name.contains('.'))
}

fn serve_ui_from_dir(req: Request, ui_dir: &'static Dir<'static>) -> Result<Response, Error> {
    let (request_parts, _body) = req.into_parts();
    match &request_parts.method {
        &Method::GET | &Method::HEAD => {
            let uri_path = request_parts
                .uri
                .path()
                .strip_prefix('/')
                .unwrap_or(request_parts.uri.path());

            let file = ui_dir.get_file(uri_path).or_else(|| {
                is_route_like(uri_path)
                    .then(|| ui_dir.get_file("index.html"))
                    .flatten()
            });

            if let Some(file) = file {
                FileData::from_embedded(&request_parts, file, ui_dir).into_response(&request_parts)
            } else {
                Ok(not_found())
            }
        }
        _ => Ok(method_not_allowed()),
    }
}

fn serve_ui<C: UiContext>(req: Request) -> Result<Response, Error> {
    serve_ui_from_dir(req, C::ui_dir())
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
            serve_ui::<C>(request).unwrap_or_else(server_error)
        }));
    // Security headers cover responses produced by context layers.
    ctx.apply_outer_layers(router)
        .layer(axum::middleware::map_response(add_security_headers))
}

pub fn refresher() -> Router {
    Router::new().fallback(get(|request: Request| async move {
        let (request_parts, _) = request.into_parts();
        let file = if RepresentationQualities::from_request(&request_parts).identity <= 0.0 {
            FileData::not_acceptable()
        } else {
            let res = include_bytes!("./refresher.html");
            FileData {
                data: Body::from(&res[..]),
                range: ByteRange::Full,
                e_tag: None,
                cache_control: None,
                encoding: None,
                len: Some(res.len() as u64),
                mime: Some("text/html".into()),
                digest: None,
                status: StatusCode::OK,
            }
        };
        file.into_response(&request_parts)
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
                            &Arc::new(HttpSource::new(ctx.client.clone(), url).await?),
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
    request_parts: &RequestParts,
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

    FileData::from_bytes(
        request_parts,
        Path::new("manifest.webmanifest"),
        "application/manifest+json",
        REVALIDATE_CACHE_CONTROL,
        body,
    )
    .into_response(request_parts)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RepresentationChoice {
    NotAcceptable,
    Identity,
    Gzip,
    Brotli,
}

#[derive(Clone, Copy, Debug)]
struct RepresentationQualities {
    identity: f32,
    gzip: f32,
    brotli: f32,
}

impl RepresentationQualities {
    fn from_request(req: &RequestParts) -> Self {
        let mut identity = None::<f32>;
        let mut gzip = None::<f32>;
        let mut brotli = None::<f32>;
        let mut wildcard = None::<f32>;
        for value in req
            .headers
            .get_all(ACCEPT_ENCODING)
            .iter()
            .filter_map(|header| header.to_str().ok())
            .flat_map(|header| header.split(','))
        {
            let mut parts = value.split(';');
            let name = parts.next().unwrap_or_default().trim();
            let mut quality = 1.0;
            for parameter in parts {
                let Some((key, value)) = parameter.trim().split_once('=') else {
                    continue;
                };
                if key.trim().eq_ignore_ascii_case("q") {
                    quality = value
                        .trim()
                        .parse::<f32>()
                        .ok()
                        .filter(|quality| (0.0..=1.0).contains(quality))
                        .unwrap_or(0.0);
                }
            }
            let target = if name.eq_ignore_ascii_case("identity") {
                &mut identity
            } else if name.eq_ignore_ascii_case("gzip") {
                &mut gzip
            } else if name.eq_ignore_ascii_case("br") {
                &mut brotli
            } else if name == "*" {
                &mut wildcard
            } else {
                continue;
            };
            *target = Some(target.map_or(quality, |current| current.max(quality)));
        }
        Self {
            identity: identity.unwrap_or_else(|| if wildcard == Some(0.0) { 0.0 } else { 1.0 }),
            gzip: gzip.or(wildcard).unwrap_or(0.0),
            brotli: brotli.or(wildcard).unwrap_or(0.0),
        }
    }

    fn select(self, gzip: bool, brotli: bool) -> RepresentationChoice {
        let mut selected =
            (self.identity > 0.0).then_some((self.identity, 0, RepresentationChoice::Identity));
        for (available, quality, priority, choice) in [
            (gzip, self.gzip, 1, RepresentationChoice::Gzip),
            (brotli, self.brotli, 2, RepresentationChoice::Brotli),
        ] {
            if available
                && quality > 0.0
                && selected.is_none_or(|(current, current_priority, _)| {
                    quality > current || (quality == current && priority > current_priority)
                })
            {
                selected = Some((quality, priority, choice));
            }
        }
        selected.map_or(RepresentationChoice::NotAcceptable, |(_, _, choice)| choice)
    }

    fn select_for_range(
        self,
        range: &mut ByteRange,
        gzip_available: bool,
        brotli_available: bool,
    ) -> RepresentationChoice {
        if *range != ByteRange::Full && self.identity > 0.0 {
            RepresentationChoice::Identity
        } else {
            *range = ByteRange::Full;
            self.select(gzip_available, brotli_available)
        }
    }
}

fn if_none_match(req: &RequestParts, current: &str) -> bool {
    let current = current.strip_prefix("W/").unwrap_or(current);
    req.headers
        .get_all(IF_NONE_MATCH)
        .iter()
        .filter_map(|header| header.to_str().ok())
        .flat_map(|header| header.split(','))
        .map(str::trim)
        .any(|candidate| {
            candidate == "*" || candidate.strip_prefix("W/").unwrap_or(candidate) == current
        })
}

fn if_range_matches(req: &RequestParts, current: Option<&str>) -> bool {
    req.headers.get(IF_RANGE).is_none_or(|candidate| {
        current.is_some_and(|current| {
            !current.starts_with("W/")
                && candidate
                    .to_str()
                    .ok()
                    .is_some_and(|candidate| candidate.trim() == current)
        })
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ByteRange {
    Full,
    Satisfiable { start: u64, end: u64, size: u64 },
    Unsatisfiable { size: u64 },
}

fn parse_decimal_saturating(decimal: &str) -> Option<u64> {
    if decimal.is_empty() {
        return None;
    }
    decimal.bytes().try_fold(0u64, |value, digit| {
        digit.is_ascii_digit().then(|| {
            value
                .saturating_mul(10)
                .saturating_add(u64::from(digit - b'0'))
        })
    })
}

fn decimal_less_than(left: &str, right: &str) -> bool {
    let left = left.trim_start_matches('0');
    let right = right.trim_start_matches('0');
    left.len() < right.len() || (left.len() == right.len() && left < right)
}

fn parse_range(header: &HeaderValue, len: u64) -> ByteRange {
    let Some(range) = header
        .to_str()
        .ok()
        .map(str::trim)
        .and_then(|range| {
            range
                .get(..6)
                .filter(|unit| unit.eq_ignore_ascii_case("bytes="))
                .map(|_| range[6..].trim())
        })
        .filter(|range| !range.contains(','))
    else {
        return ByteRange::Full;
    };
    let Some((start, end)) = range.split_once('-') else {
        return ByteRange::Full;
    };
    if start.is_empty() {
        let Some(suffix_len) = parse_decimal_saturating(end) else {
            return ByteRange::Full;
        };
        if suffix_len == 0 {
            return ByteRange::Unsatisfiable { size: len };
        }
        if len == 0 {
            return ByteRange::Full;
        }
        return ByteRange::Satisfiable {
            start: len.saturating_sub(suffix_len),
            end: len - 1,
            size: len,
        };
    }
    let Some(start_value) = parse_decimal_saturating(start) else {
        return ByteRange::Full;
    };
    let parsed_end = if end.is_empty() {
        None
    } else {
        let Some(end_value) = parse_decimal_saturating(end) else {
            return ByteRange::Full;
        };
        if end_value < start_value || (end_value == start_value && decimal_less_than(end, start)) {
            return ByteRange::Full;
        }
        Some(end_value)
    };
    if start_value >= len {
        return ByteRange::Unsatisfiable { size: len };
    }
    let end = min(parsed_end.unwrap_or(len - 1), len - 1);
    ByteRange::Satisfiable {
        start: start_value,
        end,
        size: len,
    }
}

fn requested_range(req: &RequestParts, len: u64, current_e_tag: Option<&str>) -> ByteRange {
    if req.method != Method::GET || !if_range_matches(req, current_e_tag) {
        return ByteRange::Full;
    }
    let mut ranges = req.headers.get_all(RANGE).iter();
    let Some(range) = ranges.next() else {
        return ByteRange::Full;
    };
    if ranges.next().is_some() {
        return ByteRange::Full;
    }
    parse_range(range, len)
}

struct FileData {
    data: Body,
    len: Option<u64>,
    range: ByteRange,
    encoding: Option<&'static str>,
    e_tag: Option<String>,
    cache_control: Option<&'static str>,
    mime: Option<InternedString>,
    digest: Option<(&'static str, Vec<u8>)>,
    status: StatusCode,
}
impl FileData {
    fn not_acceptable() -> Self {
        Self {
            data: Body::empty(),
            len: None,
            range: ByteRange::Full,
            encoding: None,
            e_tag: None,
            cache_control: None,
            mime: None,
            digest: None,
            status: StatusCode::NOT_ACCEPTABLE,
        }
    }

    fn from_bytes(
        req: &RequestParts,
        path: &Path,
        mime: &'static str,
        cache_control: &'static str,
        data: Vec<u8>,
    ) -> Self {
        if RepresentationQualities::from_request(req).identity <= 0.0 {
            return Self::not_acceptable();
        }
        let e_tag = Some(e_tag(path, &data));
        let range = requested_range(req, data.len() as u64, e_tag.as_deref());
        let (body, len) = match range {
            ByteRange::Full => {
                let len = data.len() as u64;
                (Body::from(data), Some(len))
            }
            ByteRange::Satisfiable { start, end, .. } => {
                let data = data[(start as usize)..=(end as usize)].to_vec();
                let len = data.len() as u64;
                (Body::from(data), Some(len))
            }
            ByteRange::Unsatisfiable { .. } => (Body::empty(), Some(0)),
        };
        Self {
            data: if req.method == Method::HEAD {
                Body::empty()
            } else {
                body
            },
            len,
            range,
            encoding: None,
            e_tag,
            cache_control: Some(cache_control),
            mime: Some(mime.into()),
            digest: None,
            status: StatusCode::OK,
        }
    }

    fn from_embedded(
        req: &RequestParts,
        file: &'static include_dir::File<'static>,
        ui_dir: &'static Dir<'static>,
    ) -> Self {
        let path = file.path();
        let identity_e_tag = embedded_e_tag(path, file.contents());
        let qualities = RepresentationQualities::from_request(req);
        let mut range = requested_range(req, file.contents().len() as u64, Some(&identity_e_tag));
        let gzip = ui_dir
            .get_file(format!("{}.gz", path.display()))
            .map(|file| file.contents());
        let brotli = ui_dir
            .get_file(format!("{}.br", path.display()))
            .map(|file| file.contents());
        let choice = qualities.select_for_range(&mut range, gzip.is_some(), brotli.is_some());
        let (encoding, representation) = match choice {
            RepresentationChoice::Identity => (None, file.contents()),
            RepresentationChoice::Gzip => (Some("gzip"), gzip.unwrap()),
            RepresentationChoice::Brotli => (Some("br"), brotli.unwrap()),
            RepresentationChoice::NotAcceptable => return Self::not_acceptable(),
        };
        let e_tag = Some(if encoding.is_none() {
            identity_e_tag
        } else {
            embedded_e_tag(path, representation)
        });
        let (data, len) = match range {
            ByteRange::Full => (
                Body::from(representation),
                Some(representation.len() as u64),
            ),
            ByteRange::Satisfiable { start, end, .. } => {
                let data = &representation[(start as usize)..=(end as usize)];
                (Body::from(data), Some(data.len() as u64))
            }
            ByteRange::Unsatisfiable { .. } => (Body::empty(), Some(0)),
        };

        Self {
            len,
            encoding,
            range,
            data: if req.method == Method::HEAD {
                Body::empty()
            } else {
                data
            },
            e_tag,
            cache_control: Some(if is_content_hashed(path) {
                IMMUTABLE_UI_CACHE_CONTROL
            } else {
                REVALIDATE_CACHE_CONTROL
            }),
            mime: MimeGuess::from_path(path)
                .first()
                .map(|m| m.essence_str().into()),
            digest: None,
            status: StatusCode::OK,
        }
    }

    fn encode<R: AsyncRead + Send + 'static>(
        choice: RepresentationChoice,
        data: R,
        len: u64,
    ) -> (Option<&'static str>, Option<u64>, Body) {
        match choice {
            RepresentationChoice::Gzip => (
                Some("gzip"),
                None,
                Body::from_stream(ReaderStream::new(GzipEncoder::new(BufReader::new(data)))),
            ),
            RepresentationChoice::Identity => {
                (None, Some(len), Body::from_stream(ReaderStream::new(data)))
            }
            RepresentationChoice::NotAcceptable | RepresentationChoice::Brotli => unreachable!(),
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
        let qualities = RepresentationQualities::from_request(req);
        // Installed archive bytes are immutable after atomic publication.
        let identity_e_tag = e_tag(
            path,
            format!(
                "{}:{}:{}:{}:{}:{}",
                *INSTANCE_NONCE,
                metadata.dev(),
                metadata.ino(),
                metadata.len(),
                metadata.ctime(),
                metadata.ctime_nsec(),
            ),
        );
        let mut range = requested_range(req, metadata.len(), Some(&identity_e_tag));
        let choice = qualities.select_for_range(&mut range, true, false);
        if choice == RepresentationChoice::NotAcceptable {
            return Ok(Some(Self::not_acceptable()));
        }
        let e_tag = match choice {
            RepresentationChoice::Identity => identity_e_tag,
            RepresentationChoice::Gzip => {
                format!("W/{}", e_tag(path, format!("{identity_e_tag}:gzip")))
            }
            RepresentationChoice::NotAcceptable | RepresentationChoice::Brotli => unreachable!(),
        };
        let send_payload = req.method != Method::HEAD && !if_none_match(req, &e_tag);

        let (encoding, len, data) = match range {
            ByteRange::Full if send_payload => Self::encode(choice, file, metadata.len()),
            ByteRange::Full => match choice {
                RepresentationChoice::Gzip => (Some("gzip"), None, Body::empty()),
                RepresentationChoice::Identity => (None, Some(metadata.len()), Body::empty()),
                RepresentationChoice::NotAcceptable | RepresentationChoice::Brotli => {
                    unreachable!()
                }
            },
            ByteRange::Satisfiable { start, end, .. } => {
                let len = end + 1 - start;
                if send_payload {
                    file.seek(std::io::SeekFrom::Start(start)).await?;
                    Self::encode(choice, file.take(len), len)
                } else {
                    (None, Some(len), Body::empty())
                }
            }
            ByteRange::Unsatisfiable { .. } => (None, Some(0), Body::empty()),
        };

        Ok(Some(Self {
            data,
            len,
            range,
            encoding,
            e_tag: Some(e_tag),
            cache_control: Some(PRIVATE_REVALIDATE_CACHE_CONTROL),
            mime: MimeGuess::from_path(path)
                .first()
                .map(|m| m.essence_str().into()),
            digest: None,
            status: StatusCode::OK,
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
        let (digest, len) = if let Some((hash, len)) = file.hash() {
            (Some(("blake3", hash.as_bytes().to_vec())), len)
        } else {
            (None, contents.size().await?)
        };

        let qualities = RepresentationQualities::from_request(req);
        let mut range = requested_range(req, len, None);
        let choice = qualities.select_for_range(&mut range, true, false);
        if choice == RepresentationChoice::NotAcceptable {
            return Ok(Some(Self::not_acceptable()));
        }

        let (encoding, len, data) = match range {
            ByteRange::Full => Self::encode(choice, contents.reader().await?.take(len), len),
            ByteRange::Satisfiable { start, end, .. } => {
                let len = end + 1 - start;
                Self::encode(choice, contents.slice(start, len).await?, len)
            }
            ByteRange::Unsatisfiable { .. } => (None, Some(0), Body::empty()),
        };

        Ok(Some(Self {
            data: if req.method == Method::HEAD {
                Body::empty()
            } else {
                data
            },
            len,
            range,
            encoding,
            e_tag: None,
            cache_control: None,
            mime: MimeGuess::from_path(path)
                .first()
                .map(|m| m.essence_str().into()),
            digest,
            status: StatusCode::OK,
        }))
    }

    fn into_response(self, req: &RequestParts) -> Result<Response, Error> {
        let not_modified = self
            .e_tag
            .as_deref()
            .is_some_and(|e_tag| if_none_match(req, e_tag));
        let mut builder = Response::builder();
        if let Some(mime) = self.mime {
            builder = builder.header(CONTENT_TYPE, &*mime);
        }
        if let Some(e_tag) = &self.e_tag {
            builder = builder.header(ETAG, &**e_tag);
        }
        if let Some(cache_control) = self.cache_control {
            builder = builder.header(CACHE_CONTROL, cache_control);
        }
        builder = builder.header(VARY, "Accept-Encoding");
        if self.status != StatusCode::OK {
            return builder
                .status(self.status)
                .body(Body::empty())
                .with_kind(ErrorKind::Network);
        }

        builder = builder.header(ACCEPT_RANGES, "bytes");
        if self.encoding.is_none()
            && let Some((algorithm, digest)) = self.digest
        {
            builder = builder.header(
                "Repr-Digest",
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

        if not_modified {
            return builder
                .status(StatusCode::NOT_MODIFIED)
                .body(Body::empty())
                .with_kind(ErrorKind::Network);
        }

        builder = match self.range {
            ByteRange::Full => builder,
            ByteRange::Satisfiable { start, end, size } => builder
                .header(CONTENT_RANGE, format!("bytes {start}-{end}/{size}"))
                .status(StatusCode::PARTIAL_CONTENT),
            ByteRange::Unsatisfiable { size } => {
                return builder
                    .header(CONTENT_RANGE, format!("bytes */{size}"))
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .body(Body::empty())
                    .with_kind(ErrorKind::Network);
            }
        };
        if let Some(len) = self.len {
            builder = builder.header(CONTENT_LENGTH, len);
        }
        if let Some(encoding) = self.encoding {
            builder = builder.header(CONTENT_ENCODING, encoding);
        }

        builder.body(self.data).with_kind(ErrorKind::Network)
    }
}

lazy_static::lazy_static! {
    static ref INSTANCE_NONCE: u64 = rand::random();
}

fn embedded_e_tag(path: &Path, representation: &'static [u8]) -> String {
    e_tag(
        path,
        format!(
            "{}:{:p}:{}",
            *INSTANCE_NONCE,
            representation.as_ptr(),
            representation.len(),
        ),
    )
}

fn e_tag(path: &Path, modified: impl AsRef<[u8]>) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(format!("{:?}", path).as_bytes());
    hasher.update(modified.as_ref());
    let res = hasher.finalize();
    format!(
        "\"{}\"",
        base32::encode(base32::Alphabet::Rfc4648 { padding: false }, res.as_slice()).to_lowercase()
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::body::to_bytes;
    use http::header::HeaderName;
    use include_dir::{DirEntry, File, Metadata};

    use super::*;

    const METADATA: Metadata = Metadata::new(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_secs(1),
    );
    static TEST_UI_DIR: Dir<'static> = Dir::new(
        "",
        &[
            DirEntry::File(
                File::new("index.html", b"<html>StartOS</html>").with_metadata(METADATA),
            ),
            DirEntry::File(
                File::new("main-ABCDEFGH.js", b"console.log('StartOS')").with_metadata(METADATA),
            ),
            DirEntry::File(
                File::new("main-ABCDEFGH.js.gz", b"compressed javascript").with_metadata(METADATA),
            ),
            DirEntry::File(File::new("styles-ABCD_ef-.css", b"body {}").with_metadata(METADATA)),
            DirEntry::File(File::new("empty.txt", b"").with_metadata(METADATA)),
            DirEntry::File(
                File::new("ngsw-worker.js", b"self.addEventListener()").with_metadata(METADATA),
            ),
            DirEntry::File(File::new("assets/logo.svg", b"<svg></svg>").with_metadata(METADATA)),
            DirEntry::File(
                File::new(
                    "manifest.webmanifest",
                    br#"{"name":"StartOS","short_name":"StartOS"}"#,
                )
                .with_metadata(METADATA),
            ),
        ],
    );

    fn request(method: Method, uri: &str, headers: &[(HeaderName, &str)]) -> Request {
        let mut request = Request::builder().method(method).uri(uri);
        for (name, value) in headers {
            request = request.header(name, *value);
        }
        request.body(Body::empty()).unwrap()
    }

    fn ui_response(uri: &str, headers: &[(HeaderName, &str)]) -> Response {
        serve_ui_from_dir(request(Method::GET, uri, headers), &TEST_UI_DIR).unwrap()
    }

    async fn path_response(
        method: Method,
        path: &Path,
        headers: &[(HeaderName, &str)],
    ) -> Response {
        let request_parts = request(method, "/", headers).into_parts().0;
        FileData::from_installed_s9pk(&request_parts, path)
            .await
            .unwrap()
            .unwrap()
            .into_response(&request_parts)
            .unwrap()
    }

    fn header(response: &Response, name: http::header::HeaderName) -> &str {
        response
            .headers()
            .get(name)
            .map_or("", |header| header.to_str().unwrap())
    }

    #[tokio::test]
    async fn refresher_honors_identity_rejection() {
        use tower_service::Service;

        for (accept_encoding, expected) in [
            ("identity", StatusCode::OK),
            ("identity;q=0", StatusCode::NOT_ACCEPTABLE),
            ("*;q=0", StatusCode::NOT_ACCEPTABLE),
        ] {
            let request = request(Method::GET, "/", &[(ACCEPT_ENCODING, accept_encoding)]);
            let response = refresher().call(request).await.unwrap();
            assert_eq!(response.status(), expected, "{accept_encoding}");
            assert_eq!(header(&response, VARY), "Accept-Encoding");
        }
    }

    #[test]
    fn content_hashed_paths_are_top_level_bundles() {
        for path in [
            "main-ABCDEFGH.js",
            "polyfills-Ab_0-cDe.js",
            "chunk-C-f2EvjP.js",
            "styles-XYUDF62Z.css",
        ] {
            assert!(is_content_hashed(Path::new(path)), "{path}");
        }
        for path in [
            "index.html",
            "main.js",
            "chunk-C-f2EvjP.js.map",
            "favicon-96x96.png",
            "assets/font-ABCDEFGH.css",
        ] {
            assert!(!is_content_hashed(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn stable_ui_files_revalidate_and_hashed_bundles_are_immutable() {
        for path in ["/", "/ngsw-worker.js", "/assets/logo.svg"] {
            let response = ui_response(path, &[]);
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert_eq!(
                header(&response, CACHE_CONTROL),
                REVALIDATE_CACHE_CONTROL,
                "{path}",
            );
            assert!(response.headers().contains_key(ETAG), "{path}");
        }

        for path in ["/main-ABCDEFGH.js", "/styles-ABCD_ef-.css"] {
            let response = ui_response(path, &[]);
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert_eq!(
                header(&response, CACHE_CONTROL),
                IMMUTABLE_UI_CACHE_CONTROL,
                "{path}",
            );
            assert!(response.headers().contains_key(ETAG), "{path}");
        }

        let response = ui_response("/", &[]);
        let e_tag = header(&response, ETAG).to_owned();
        for validator in [
            format!("W/{e_tag}"),
            format!("\"old\", {e_tag}"),
            "*".into(),
        ] {
            let response = ui_response("/", &[(IF_NONE_MATCH, &validator)]);
            assert_eq!(response.status(), StatusCode::NOT_MODIFIED, "{validator}");
            assert_eq!(header(&response, CACHE_CONTROL), REVALIDATE_CACHE_CONTROL);
        }
    }

    #[test]
    fn embedded_etags_follow_bytes_and_content_encoding() {
        assert_ne!(
            e_tag(Path::new("index.html"), b"first"),
            e_tag(Path::new("index.html"), b"second"),
        );

        let identity = ui_response("/main-ABCDEFGH.js", &[]);
        let identity_e_tag = header(&identity, ETAG).to_owned();
        assert_eq!(header(&identity, VARY), "Accept-Encoding");

        let gzip = ui_response("/main-ABCDEFGH.js", &[(ACCEPT_ENCODING, "gzip")]);
        assert_eq!(header(&gzip, CONTENT_ENCODING), "gzip");
        assert_ne!(header(&gzip, ETAG), identity_e_tag);

        let rejected = ui_response("/main-ABCDEFGH.js", &[(ACCEPT_ENCODING, "gzip;q=0")]);
        assert!(!rejected.headers().contains_key(CONTENT_ENCODING));
        assert_eq!(header(&rejected, ETAG), identity_e_tag);
    }

    #[tokio::test]
    async fn installed_s9pks_revalidate_and_resume_with_strong_etags() {
        let path = std::env::temp_dir().join(format!(
            "start-core-static-server-{}.s9pk",
            rand::random::<u64>()
        ));
        let mut contents = vec![0; 256];
        contents[..10].copy_from_slice(b"0123456789");
        tokio::fs::write(&path, &contents).await.unwrap();

        let full = path_response(Method::GET, &path, &[]).await;
        assert_eq!(
            header(&full, CACHE_CONTROL),
            PRIVATE_REVALIDATE_CACHE_CONTROL,
        );
        let e_tag = header(&full, ETAG).to_owned();
        assert!(!e_tag.starts_with("W/"));

        let gzip = path_response(Method::GET, &path, &[(ACCEPT_ENCODING, "gzip")]).await;
        let gzip_e_tag = header(&gzip, ETAG).to_owned();
        assert_eq!(header(&gzip, CONTENT_ENCODING), "gzip");
        assert!(gzip_e_tag.starts_with("W/"));
        assert_ne!(gzip_e_tag, e_tag);

        let ranged = path_response(
            Method::GET,
            &path,
            &[(RANGE, "bytes=2-5"), (IF_RANGE, &e_tag)],
        )
        .await;
        assert_eq!(ranged.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(header(&ranged, CONTENT_RANGE), "bytes 2-5/256");
        assert_eq!(
            to_bytes(ranged.into_body(), usize::MAX).await.unwrap(),
            "2345",
        );

        let revalidated = path_response(Method::GET, &path, &[(IF_NONE_MATCH, &e_tag)]).await;
        assert_eq!(revalidated.status(), StatusCode::NOT_MODIFIED);
        let gzip_revalidated = path_response(
            Method::GET,
            &path,
            &[(ACCEPT_ENCODING, "gzip"), (IF_NONE_MATCH, &gzip_e_tag)],
        )
        .await;
        assert_eq!(gzip_revalidated.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(header(&gzip_revalidated, ETAG), gzip_e_tag);
        assert!(!gzip_revalidated.headers().contains_key(CONTENT_ENCODING));
        assert!(
            to_bytes(gzip_revalidated.into_body(), usize::MAX)
                .await
                .unwrap()
                .is_empty()
        );

        let gzip_head = path_response(Method::HEAD, &path, &[(ACCEPT_ENCODING, "gzip")]).await;
        assert_eq!(gzip_head.status(), StatusCode::OK);
        assert_eq!(header(&gzip_head, ETAG), gzip_e_tag);
        assert_eq!(header(&gzip_head, CONTENT_ENCODING), "gzip");
        assert!(!gzip_head.headers().contains_key(CONTENT_LENGTH));
        assert!(
            to_bytes(gzip_head.into_body(), usize::MAX)
                .await
                .unwrap()
                .is_empty()
        );

        drop(full);
        drop(gzip);
        drop(revalidated);
        contents[200] = 1;
        tokio::fs::write(&path, &contents).await.unwrap();
        let changed = path_response(Method::GET, &path, &[]).await;
        assert_ne!(header(&changed, ETAG), e_tag);

        tokio::fs::remove_file(path).await.unwrap();
    }

    #[test]
    fn representation_digest_is_omitted_for_gzip() {
        fn response(encoding: Option<&'static str>) -> Response {
            let request_parts = request(Method::GET, "/", &[]).into_parts().0;
            FileData {
                data: Body::empty(),
                len: Some(3),
                range: ByteRange::Full,
                encoding,
                e_tag: None,
                cache_control: None,
                mime: None,
                digest: Some(("blake3", vec![1, 2, 3])),
                status: StatusCode::OK,
            }
            .into_response(&request_parts)
            .unwrap()
        }

        assert!(response(None).headers().contains_key("Repr-Digest"));
        assert!(!response(Some("gzip")).headers().contains_key("Repr-Digest"));
    }

    #[test]
    fn encoding_negotiation_combines_field_lines_and_prefers_brotli_on_ties() {
        let request_parts = request(
            Method::GET,
            "/",
            &[
                (ACCEPT_ENCODING, "gzip;q=0"),
                (ACCEPT_ENCODING, "gzip;q=1, identity;q=0"),
            ],
        )
        .into_parts()
        .0;
        assert_eq!(
            RepresentationQualities::from_request(&request_parts).select(true, false),
            RepresentationChoice::Gzip,
        );

        let request_parts = request(Method::GET, "/", &[(ACCEPT_ENCODING, "identity, gzip, br")])
            .into_parts()
            .0;
        assert_eq!(
            RepresentationQualities::from_request(&request_parts).select(true, true),
            RepresentationChoice::Brotli,
        );
    }

    #[test]
    fn encoding_negotiation_honors_identity_rejection() {
        let response = ui_response(
            "/main-ABCDEFGH.js",
            &[(ACCEPT_ENCODING, "gzip;q=1, identity;q=0")],
        );
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(header(&response, CONTENT_ENCODING), "gzip");

        let lower_quality = ui_response("/main-ABCDEFGH.js", &[(ACCEPT_ENCODING, "gzip;q=0.5")]);
        assert!(!lower_quality.headers().contains_key(CONTENT_ENCODING));

        for (path, accept_encoding) in [
            ("/index.html", "identity;q=0"),
            ("/main-ABCDEFGH.js", "*;q=0"),
        ] {
            let response = ui_response(path, &[(ACCEPT_ENCODING, accept_encoding)]);
            assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE, "{path}");
            assert_eq!(header(&response, VARY), "Accept-Encoding", "{path}");
        }

        let ranged = ui_response(
            "/main-ABCDEFGH.js",
            &[
                (RANGE, "bytes=0-4"),
                (ACCEPT_ENCODING, "gzip;q=1, identity;q=0"),
            ],
        );
        assert_eq!(ranged.status(), StatusCode::OK);
        assert_eq!(header(&ranged, CONTENT_ENCODING), "gzip");
        assert!(!ranged.headers().contains_key(CONTENT_RANGE));
    }

    #[test]
    fn byte_ranges_cover_suffixes_and_boundaries() {
        let size = 20;
        assert_eq!(
            parse_range(&HeaderValue::from_static("bytes=-5"), size),
            ByteRange::Satisfiable {
                start: 15,
                end: 19,
                size,
            },
        );
        assert_eq!(
            parse_range(&HeaderValue::from_static("bytes=-50"), size),
            ByteRange::Satisfiable {
                start: 0,
                end: 19,
                size,
            },
        );
        for range in ["bytes=10-", "bytes= 10-"] {
            assert_eq!(
                parse_range(&HeaderValue::from_static(range), size),
                ByteRange::Satisfiable {
                    start: 10,
                    end: 19,
                    size,
                },
            );
        }
        assert_eq!(
            parse_range(&HeaderValue::from_static("bytes=20-"), size),
            ByteRange::Unsatisfiable { size },
        );
        assert_eq!(
            parse_range(&HeaderValue::from_static("bytes=0-"), 0),
            ByteRange::Unsatisfiable { size: 0 },
        );
        assert_eq!(
            parse_range(&HeaderValue::from_static("bytes=-1"), 0),
            ByteRange::Full,
        );
        assert_eq!(
            parse_range(
                &HeaderValue::from_static("bytes=-18446744073709551616"),
                size,
            ),
            ByteRange::Satisfiable {
                start: 0,
                end: 19,
                size,
            },
        );
        assert_eq!(
            parse_range(
                &HeaderValue::from_static("bytes=5-18446744073709551616"),
                size,
            ),
            ByteRange::Satisfiable {
                start: 5,
                end: 19,
                size,
            },
        );
        assert_eq!(
            parse_range(
                &HeaderValue::from_static("bytes=18446744073709551616-"),
                size,
            ),
            ByteRange::Unsatisfiable { size },
        );
        for malformed in [
            "bytes=-",
            "bytes=garbage-",
            "bytes=0-garbage",
            "bytes=+1-+2",
            "bytes=+1-2",
            "bytes=1-+2",
            "bytes=--5",
            "bytes=1--2",
            "bytes=10-5",
            "bytes=10 -15",
            "bytes=10- 15",
            "bytes= 10 - 15",
            "bytes=18446744073709551617-18446744073709551616",
        ] {
            assert_eq!(
                parse_range(&HeaderValue::from_str(malformed).unwrap(), 0),
                ByteRange::Full,
                "{malformed}",
            );
        }
        assert_eq!(
            parse_range(&HeaderValue::from_static("items=0-5"), size),
            ByteRange::Full,
        );
    }

    #[tokio::test]
    async fn range_responses_use_identity_bytes_and_valid_status_headers() {
        let full = ui_response("/", &[]);
        let e_tag = header(&full, ETAG).to_owned();

        let suffix = ui_response("/", &[(RANGE, "bytes=-5"), (ACCEPT_ENCODING, "gzip")]);
        assert_eq!(suffix.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(header(&suffix, CONTENT_RANGE), "bytes 15-19/20");
        assert!(!suffix.headers().contains_key(CONTENT_ENCODING));
        assert_eq!(
            to_bytes(suffix.into_body(), usize::MAX).await.unwrap(),
            "html>",
        );

        let case_variant = ui_response("/", &[(RANGE, "Bytes=0-4")]);
        assert_eq!(case_variant.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(header(&case_variant, CONTENT_RANGE), "bytes 0-4/20");

        let unsatisfiable = ui_response("/", &[(RANGE, "bytes=20-")]);
        assert_eq!(unsatisfiable.status(), StatusCode::RANGE_NOT_SATISFIABLE,);
        assert_eq!(header(&unsatisfiable, CONTENT_RANGE), "bytes */20");

        let repeated = ui_response("/", &[(RANGE, "bytes=20-"), (RANGE, "bytes=0-4")]);
        assert_eq!(repeated.status(), StatusCode::OK);
        assert!(!repeated.headers().contains_key(CONTENT_RANGE));

        let not_modified = ui_response("/", &[(RANGE, "bytes=0-4"), (IF_NONE_MATCH, &e_tag)]);
        assert_eq!(not_modified.status(), StatusCode::NOT_MODIFIED);
        assert!(!not_modified.headers().contains_key(CONTENT_RANGE));
    }

    #[tokio::test]
    async fn ranges_require_get_and_a_current_if_range_validator() {
        let full = ui_response("/", &[]);
        let e_tag = header(&full, ETAG).to_owned();

        let matched = ui_response("/", &[(RANGE, "bytes=0-4"), (IF_RANGE, &e_tag)]);
        assert_eq!(matched.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(header(&matched, CONTENT_RANGE), "bytes 0-4/20");

        for validator in ["\"stale\"".to_owned(), format!("W/{e_tag}")] {
            let response = ui_response("/", &[(RANGE, "bytes=0-4"), (IF_RANGE, &validator)]);
            assert_eq!(response.status(), StatusCode::OK);
            assert!(!response.headers().contains_key(CONTENT_RANGE));
            assert_eq!(
                to_bytes(response.into_body(), usize::MAX).await.unwrap(),
                "<html>StartOS</html>",
            );
        }

        let head = serve_ui_from_dir(
            request(Method::HEAD, "/", &[(RANGE, "bytes=0-4")]),
            &TEST_UI_DIR,
        )
        .unwrap();
        assert_eq!(head.status(), StatusCode::OK);
        assert_eq!(header(&head, CONTENT_LENGTH), "20");
        assert!(!head.headers().contains_key(CONTENT_RANGE));
        assert!(
            to_bytes(head.into_body(), usize::MAX)
                .await
                .unwrap()
                .is_empty()
        );

        let post = serve_ui_from_dir(
            request(Method::POST, "/", &[(RANGE, "bytes=0-4")]),
            &TEST_UI_DIR,
        )
        .unwrap();
        assert_eq!(post.status(), StatusCode::METHOD_NOT_ALLOWED);

        for malformed in ["bytes=-", "bytes=garbage-", "bytes=0-garbage"] {
            let response = ui_response("/empty.txt", &[(RANGE, malformed)]);
            assert_eq!(response.status(), StatusCode::OK, "{malformed}");
            assert!(!response.headers().contains_key(CONTENT_RANGE));
        }
    }

    #[tokio::test]
    async fn spa_fallback_uses_index_for_routes_and_not_for_asset_paths() {
        let index = ui_response("/", &[]);
        let index_e_tag = header(&index, ETAG).to_owned();
        let index_body = to_bytes(index.into_body(), usize::MAX).await.unwrap();

        for path in [
            "/settings/general",
            "/settings/general/",
            "/route.name/child",
            "/settings/general?tab=a.b",
        ] {
            let response = ui_response(path, &[]);
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert_eq!(header(&response, CONTENT_TYPE), "text/html", "{path}");
            assert_eq!(header(&response, ETAG), index_e_tag, "{path}");
            assert_eq!(
                header(&response, CACHE_CONTROL),
                REVALIDATE_CACHE_CONTROL,
                "{path}",
            );
            assert_eq!(
                to_bytes(response.into_body(), usize::MAX).await.unwrap(),
                index_body,
                "{path}",
            );
        }

        for path in [
            "/missing.js",
            "/missing.js?route=general",
            "/assets/missing/icon.svg",
            "/.hidden",
            "/route.",
        ] {
            assert_eq!(
                ui_response(path, &[]).status(),
                StatusCode::NOT_FOUND,
                "{path}"
            );
        }
        assert_eq!(
            ui_response("/main-ABCDEFGH.js?v=1.2", &[]).status(),
            StatusCode::OK,
        );
    }

    #[tokio::test]
    async fn generated_webmanifest_revalidates_its_body() {
        fn response(hostname: &str, if_none_match: Option<&str>) -> Response {
            let headers = if_none_match
                .map(|e_tag| vec![(IF_NONE_MATCH, e_tag)])
                .unwrap_or_default();
            let request_parts = request(Method::GET, "/manifest.webmanifest", &headers)
                .into_parts()
                .0;
            let hostname =
                ServerHostname::new_from_input(InternedString::intern(hostname)).unwrap();
            webmanifest_send(&request_parts, &TEST_UI_DIR, &hostname).unwrap()
        }

        let alpha = response("alpha", None);
        assert_eq!(alpha.status(), StatusCode::OK);
        assert_eq!(header(&alpha, CONTENT_TYPE), "application/manifest+json");
        assert_eq!(header(&alpha, CACHE_CONTROL), REVALIDATE_CACHE_CONTROL);
        assert_eq!(header(&alpha, VARY), "Accept-Encoding");
        let alpha_e_tag = header(&alpha, ETAG).to_owned();
        let alpha_body: serde_json::Value =
            serde_json::from_slice(&to_bytes(alpha.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(alpha_body["name"], "alpha");
        assert_eq!(alpha_body["short_name"], "alpha");

        let revalidated = response("alpha", Some(&alpha_e_tag));
        assert_eq!(revalidated.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(
            header(&revalidated, CACHE_CONTROL),
            REVALIDATE_CACHE_CONTROL,
        );

        let weak = response("alpha", Some(&format!("W/{alpha_e_tag}")));
        assert_eq!(weak.status(), StatusCode::NOT_MODIFIED);

        let beta = response("beta", None);
        assert_ne!(header(&beta, ETAG), alpha_e_tag);

        let request_parts = request(
            Method::GET,
            "/manifest.webmanifest",
            &[(ACCEPT_ENCODING, "identity;q=0")],
        )
        .into_parts()
        .0;
        let rejected = webmanifest_send(
            &request_parts,
            &TEST_UI_DIR,
            &ServerHostname::new_from_input(InternedString::intern("alpha")).unwrap(),
        )
        .unwrap();
        assert_eq!(rejected.status(), StatusCode::NOT_ACCEPTABLE);
        assert_eq!(header(&rejected, VARY), "Accept-Encoding");
    }
}
