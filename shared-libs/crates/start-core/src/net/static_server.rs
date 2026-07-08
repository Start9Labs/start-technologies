use std::cmp::min;
use std::future::Future;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::UNIX_EPOCH;

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
    CONTENT_RANGE, CONTENT_TYPE, ETAG, RANGE,
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
use crate::middleware::auth::session::ValidSessionToken;
use crate::middleware::cors::Cors;
use crate::middleware::db::SyncDb;
use crate::prelude::*;
use crate::rpc_continuations::{Guid, RpcContinuations};
use crate::s9pk::S9pk;
use crate::s9pk::merkle_archive::source::FileSource;
use crate::s9pk::merkle_archive::source::http::HttpSource;
use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
use crate::sign::commitment::merkle_archive::MerkleArchiveCommitment;
use crate::util::io::open_file;
use crate::util::net::SyncBody;
use crate::util::serde::BASE64;
use crate::{PackageId, main_api};

const NOT_FOUND: &[u8] = b"Not Found";
const METHOD_NOT_ALLOWED: &[u8] = b"Method Not Allowed";
const NOT_AUTHORIZED: &[u8] = b"Not Authorized";
const INTERNAL_SERVER_ERROR: &[u8] = b"Internal Server Error";

const PROXY_STRIP_HEADERS: &[&str] = &["cookie", "host", "origin", "referer", "user-agent"];

pub const EMPTY_DIR: Dir<'_> = Dir::new("", &[]);

pub trait UiContext: Context + AsRef<RpcContinuations> + Clone + Sized {
    fn ui_dir() -> &'static Dir<'static>;
    fn api() -> ParentHandler<Self>;
    fn middleware(server: Server<Self>) -> HttpServer<Self>;
    fn extend_router(self, router: Router) -> Router {
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
            .middleware(
                Auth::new()
                    .with_local_auth()
                    .with_signature_auth()
                    .with_session_auth(),
            )
            .middleware(SyncDb::new())
    }
    fn extend_router(self, router: Router) -> Router {
        router
            .route("/proxy/{url}", {
                let ctx = self.clone();
                any(move |x::Path(url): x::Path<String>, request: Request| {
                    let ctx = ctx.clone();
                    async move {
                        proxy_request(ctx, request, url)
                            .await
                            .unwrap_or_else(server_error)
                    }
                })
            })
            .nest("/s9pk", s9pk_router(self.clone()))
            .route("/static/local-root-ca.crt", {
                let ctx = self.clone();
                get(move || {
                    let ctx = ctx.clone();
                    async move {
                        ctx.account.peek(|account| {
                            cert_send(&account.root_ca_cert, &account.hostname.hostname)
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
                            mobileconfig_send(&account.root_ca_cert, &account.hostname.hostname)
                        })
                    }
                }),
            )
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

fn serve_ui<C: UiContext>(req: Request) -> Result<Response, Error> {
    let (request_parts, _body) = req.into_parts();
    match &request_parts.method {
        &Method::GET | &Method::HEAD => {
            let uri_path = request_parts
                .uri
                .path()
                .strip_prefix('/')
                .unwrap_or(request_parts.uri.path());

            let file = C::ui_dir()
                .get_file(uri_path)
                .or_else(|| C::ui_dir().get_file("index.html"));

            if let Some(file) = file {
                FileData::from_embedded(&request_parts, file, C::ui_dir())?
                    .into_response(&request_parts)
            } else {
                Ok(not_found())
            }
        }
        _ => Ok(method_not_allowed()),
    }
}

pub fn ui_router<C: UiContext>(ctx: C) -> Router {
    ctx.clone()
        .extend_router(rpc_router(
            ctx.clone(),
            C::middleware(Server::new(move || ready(Ok(ctx.clone())), C::api())),
        ))
        .fallback(any(|request: Request| async move {
            serve_ui::<C>(request).unwrap_or_else(server_error)
        }))
}

pub fn refresher() -> Router {
    Router::new().fallback(get(|request: Request| async move {
        let res = include_bytes!("./refresher.html");
        FileData {
            data: Body::from(&res[..]),
            content_range: None,
            e_tag: None,
            encoding: None,
            len: Some(res.len() as u64),
            mime: Some("text/html".into()),
            digest: None,
        }
        .into_response(&request.into_parts().0)
        .unwrap_or_else(server_error)
    }))
}

async fn proxy_request(ctx: RpcContext, request: Request, url: String) -> Result<Response, Error> {
    if_authorized(&ctx, request, |mut request| async {
        for header in PROXY_STRIP_HEADERS {
            request.headers_mut().remove(*header);
        }
        *request.uri_mut() = url.parse()?;
        let request = request.map(|b| reqwest::Body::wrap_stream(SyncBody::from(b)));
        let response = ctx.client.execute(request.try_into()?).await?;
        Ok(Response::from(response).map(|b| Body::new(b)))
    })
    .await
}

fn s9pk_router(ctx: RpcContext) -> Router {
    Router::new()
        .route("/installed/{s9pk}", {
            let ctx = ctx.clone();
            any(
                |x::Path(s9pk): x::Path<String>, request: Request| async move {
                    if_authorized(&ctx, request, |request| async {
                        let id = s9pk
                            .strip_suffix(".s9pk")
                            .unwrap_or(&s9pk)
                            .parse::<PackageId>()?;
                        let (parts, _) = request.into_parts();
                        match FileData::from_path(
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
            any(
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
            any(
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
    request: Request,
    f: F,
) -> Result<Response, Error> {
    if let Err(e) =
        ValidSessionToken::from_header(request.headers().get(http::header::COOKIE), ctx).await
    {
        // TODO: other auth methods
        Ok(unauthorized(e, request.uri().path()))
    } else {
        f(request).await
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

fn parse_range(header: &HeaderValue, len: u64) -> Result<(u64, u64, u64), Error> {
    let r = header
        .to_str()
        .with_kind(ErrorKind::Network)?
        .trim()
        .strip_prefix("bytes=")
        .ok_or_else(|| Error::new(eyre!("invalid range units"), ErrorKind::InvalidRequest))?;

    if r.contains(",") {
        return Err(Error::new(
            eyre!("multi-range requests are unsupported"),
            ErrorKind::InvalidRequest,
        ));
    }
    if let Some((start, end)) = r.split_once("-").map(|(s, e)| (s.trim(), e.trim())) {
        Ok((
            if start.is_empty() {
                0u64
            } else {
                start.parse()?
            },
            if end.is_empty() {
                len - 1
            } else {
                min(end.parse()?, len - 1)
            },
            len,
        ))
    } else {
        Ok((len - r.trim().parse::<u64>()?, len - 1, len))
    }
}

struct FileData {
    data: Body,
    len: Option<u64>,
    content_range: Option<(u64, u64, u64)>,
    encoding: Option<&'static str>,
    e_tag: Option<String>,
    mime: Option<InternedString>,
    digest: Option<(&'static str, Vec<u8>)>,
}
impl FileData {
    fn from_embedded(
        req: &RequestParts,
        file: &'static include_dir::File<'static>,
        ui_dir: &'static Dir<'static>,
    ) -> Result<Self, Error> {
        let path = file.path();
        let (encoding, data, len, content_range) = if let Some(range) = req.headers.get(RANGE) {
            let data = file.contents();
            let (start, end, size) = parse_range(range, data.len() as u64)?;
            let encoding = req
                .headers
                .get_all(ACCEPT_ENCODING)
                .into_iter()
                .filter_map(|h| h.to_str().ok())
                .flat_map(|s| s.split(","))
                .filter_map(|s| s.split(";").next())
                .map(|s| s.trim())
                .any(|e| e == "gzip")
                .then_some("gzip");
            let data = if start > end {
                &[]
            } else {
                &data[(start as usize)..=(end as usize)]
            };
            let (len, data) = if encoding == Some("gzip") {
                (
                    None,
                    Body::from_stream(ReaderStream::new(GzipEncoder::new(Cursor::new(data)))),
                )
            } else {
                (Some(data.len() as u64), Body::from(data))
            };
            (encoding, data, len, Some((start, end, size)))
        } else {
            let (encoding, data) = req
                .headers
                .get_all(ACCEPT_ENCODING)
                .into_iter()
                .filter_map(|h| h.to_str().ok())
                .flat_map(|s| s.split(","))
                .filter_map(|s| s.split(";").next())
                .map(|s| s.trim())
                .fold((None, file.contents()), |acc, e| {
                    if let Some(file) = (e == "br")
                        .then_some(())
                        .and_then(|_| ui_dir.get_file(format!("{}.br", path.display())))
                    {
                        (Some("br"), file.contents())
                    } else if let Some(file) = (e == "gzip" && acc.0 != Some("br"))
                        .then_some(())
                        .and_then(|_| ui_dir.get_file(format!("{}.gz", path.display())))
                    {
                        (Some("gzip"), file.contents())
                    } else {
                        acc
                    }
                });
            (encoding, Body::from(data), Some(data.len() as u64), None)
        };

        Ok(Self {
            len,
            encoding,
            content_range,
            data: if req.method == Method::HEAD {
                Body::empty()
            } else {
                data
            },
            e_tag: file.metadata().map(|metadata| {
                e_tag(
                    path,
                    format!(
                        "{}",
                        metadata
                            .modified()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or_else(|e| e.duration().as_secs() as i64 * -1),
                    )
                    .as_bytes(),
                )
            }),
            mime: MimeGuess::from_path(path)
                .first()
                .map(|m| m.essence_str().into()),
            digest: None,
        })
    }

    fn encode<R: AsyncRead + Send + 'static>(
        encoding: &mut Option<&str>,
        data: R,
        len: u64,
    ) -> (Option<u64>, Body) {
        if *encoding == Some("gzip") {
            (
                None,
                Body::from_stream(ReaderStream::new(GzipEncoder::new(BufReader::new(data)))),
            )
        } else {
            *encoding = None;
            (Some(len), Body::from_stream(ReaderStream::new(data)))
        }
    }

    async fn from_path(req: &RequestParts, path: &Path) -> Result<Option<Self>, Error> {
        let mut encoding = req
            .headers
            .get_all(ACCEPT_ENCODING)
            .into_iter()
            .filter_map(|h| h.to_str().ok())
            .flat_map(|s| s.split(","))
            .filter_map(|s| s.split(";").next())
            .map(|s| s.trim())
            .any(|e| e == "gzip")
            .then_some("gzip");

        if tokio::fs::metadata(path).await.is_err() {
            return Ok(None);
        }

        let mut file = open_file(path).await?;

        let metadata = file
            .metadata()
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, path.display().to_string()))?;

        let content_range = req
            .headers
            .get(RANGE)
            .map(|r| parse_range(r, metadata.len()))
            .transpose()?;

        let e_tag = Some(e_tag(
            path,
            format!(
                "{}",
                metadata
                    .modified()?
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or_else(|e| e.duration().as_secs() as i64 * -1)
            )
            .as_bytes(),
        ));

        let (len, data) = if let Some((start, end, _)) = content_range {
            let len = end + 1 - start;
            file.seek(std::io::SeekFrom::Start(start)).await?;
            Self::encode(&mut encoding, file.take(len), len)
        } else {
            Self::encode(&mut encoding, file, metadata.len())
        };

        Ok(Some(Self {
            data: if req.method == Method::HEAD {
                Body::empty()
            } else {
                data
            },
            len,
            content_range,
            encoding,
            e_tag,
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
        let mut encoding = req
            .headers
            .get_all(ACCEPT_ENCODING)
            .into_iter()
            .filter_map(|h| h.to_str().ok())
            .flat_map(|s| s.split(","))
            .filter_map(|s| s.split(";").next())
            .map(|s| s.trim())
            .any(|e| e == "gzip")
            .then_some("gzip");

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

        let content_range = req
            .headers
            .get(RANGE)
            .map(|r| parse_range(r, len))
            .transpose()?;

        let (len, data) = if let Some((start, end, _)) = content_range {
            let len = end + 1 - start;
            Self::encode(&mut encoding, contents.slice(start, len).await?, len)
        } else {
            Self::encode(&mut encoding, contents.reader().await?.take(len), len)
        };

        Ok(Some(Self {
            data: if req.method == Method::HEAD {
                Body::empty()
            } else {
                data
            },
            len,
            content_range,
            encoding,
            e_tag: None,
            mime: MimeGuess::from_path(path)
                .first()
                .map(|m| m.essence_str().into()),
            digest,
        }))
    }

    fn into_response(self, req: &RequestParts) -> Result<Response, Error> {
        let mut builder = Response::builder();
        if let Some(mime) = self.mime {
            builder = builder.header(CONTENT_TYPE, &*mime);
        }
        if let Some(e_tag) = &self.e_tag {
            builder = builder
                .header(ETAG, &**e_tag)
                .header(CACHE_CONTROL, "public, max-age=21000000, immutable");
        }

        builder = builder.header(ACCEPT_RANGES, "bytes");
        if let Some((start, end, size)) = self.content_range {
            builder = builder
                .header(CONTENT_RANGE, format!("bytes {start}-{end}/{size}"))
                .status(StatusCode::PARTIAL_CONTENT);
        }

        if let Some((algorithm, digest)) = self.digest {
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

        if self.e_tag.is_some()
            && req
                .headers
                .get("if-none-match")
                .and_then(|h| h.to_str().ok())
                == self.e_tag.as_deref()
        {
            builder.status(StatusCode::NOT_MODIFIED).body(Body::empty())
        } else {
            if let Some(len) = self.len {
                builder = builder.header(CONTENT_LENGTH, len);
            }
            if let Some(encoding) = self.encoding {
                builder = builder.header(CONTENT_ENCODING, encoding);
            }

            builder.body(self.data)
        }
        .with_kind(ErrorKind::Network)
    }
}

lazy_static::lazy_static! {
    static ref INSTANCE_NONCE: u64 = rand::random();
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
