use axum::body::Body;
use axum::http::{header, Request, Response, StatusCode};
use include_dir::{include_dir, Dir};

static WEB_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../../web/dist/startwrt/browser");

// One ETag for the whole bundle: include_dir embeds no per-file metadata, and
// the dist only ever changes as a unit — with the firmware build.
const ETAG: &str = concat!("\"", env!("STARTWRT_GIT_HASH"), "\"");

/// Angular emits content-hashed top-level bundle files (`main-NUVV5TLQ.js`,
/// `chunk-C-f2EvjP.js`, `styles-XYUDF62Z.css`): a `-` plus 8 chars of
/// `[A-Za-z0-9_-]` before the extension. Their names change with their
/// content, so browsers may cache them forever. Everything else (index.html,
/// assets/) keeps a stable name across builds and must be revalidated.
fn is_content_hashed(path: &str) -> bool {
    if path.contains('/') {
        return false;
    }
    let Some(stem) = path
        .strip_suffix(".js")
        .or_else(|| path.strip_suffix(".css"))
    else {
        return false;
    };
    let bytes = stem.as_bytes();
    bytes.len() > 9
        && bytes[bytes.len() - 9] == b'-'
        && bytes[bytes.len() - 8..]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_')
}

pub async fn serve_embedded(req: Request<Body>) -> Response<Body> {
    let path = req.uri().path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    let file = WEB_DIR.get_file(path).or_else(|| {
        // SPA fallback: unknown extensionless paths are Angular routes and get
        // index.html. Asset-like paths (anything with an extension, e.g. a
        // hashed chunk from a previous firmware requested by a stale browser)
        // must 404 instead — a 200 HTML body there breaks module loading and
        // can get cached as the chunk.
        let last_segment = path.rsplit('/').next().unwrap_or(path);
        (!last_segment.contains('.'))
            .then(|| WEB_DIR.get_file("index.html"))
            .flatten()
    });

    let Some(file) = file else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("not found"))
            .unwrap();
    };

    // Content-hashed bundles are immutable; everything else is cached but
    // revalidated on every load (a cheap 304 below), so a firmware update is
    // picked up immediately. Browsers were previously left to heuristics here,
    // which let them serve a stale pre-update UI indefinitely.
    let cache_control = if is_content_hashed(&file.path().to_string_lossy()) {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };

    if req
        .headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|h| h.to_str().ok())
        == Some(ETAG)
    {
        return Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(header::ETAG, ETAG)
            .header(header::CACHE_CONTROL, cache_control)
            .body(Body::empty())
            .unwrap();
    }

    let mime = mime_guess::from_path(file.path())
        .first_raw()
        .unwrap_or("application/octet-stream");
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::ETAG, ETAG)
        .header(header::CACHE_CONTROL, cache_control)
        .body(Body::from(file.contents()))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hashed_detection() {
        assert!(is_content_hashed("main-NUVV5TLQ.js"));
        assert!(is_content_hashed("chunk-C-f2EvjP.js"));
        assert!(is_content_hashed("chunk-Bu_0_vFc.js"));
        assert!(is_content_hashed("styles-XYUDF62Z.css"));

        assert!(!is_content_hashed("index.html"));
        assert!(!is_content_hashed("favicon-96x96.png"));
        assert!(!is_content_hashed("assets/fonts/font-AbCdEf12.css"));
        assert!(!is_content_hashed("main.js"));
        assert!(!is_content_hashed("chunk-C-f2EvjP.js.map"));
    }

    fn get(path: &str, if_none_match: Option<&str>) -> Response<Body> {
        let mut req = Request::builder().uri(path);
        if let Some(etag) = if_none_match {
            req = req.header(header::IF_NONE_MATCH, etag);
        }
        futures::executor::block_on(serve_embedded(req.body(Body::empty()).unwrap()))
    }

    fn header_str<'a>(res: &'a Response<Body>, name: header::HeaderName) -> &'a str {
        res.headers().get(name).map_or("", |h| h.to_str().unwrap())
    }

    // The assertions below need a real embedded dist; `cargo test` without a
    // prior web build embeds the empty placeholder dir, so skip there.
    fn dist_embedded() -> bool {
        WEB_DIR.get_file("index.html").is_some()
    }

    #[test]
    fn index_revalidates_and_matches_etag() {
        if !dist_embedded() {
            return;
        }
        let res = get("/", None);
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(header_str(&res, header::CACHE_CONTROL), "no-cache");
        assert_eq!(header_str(&res, header::ETAG), ETAG);

        let res = get("/", Some(ETAG));
        assert_eq!(res.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(header_str(&res, header::ETAG), ETAG);

        // A stale browser (previous build's ETag) gets the full document.
        let res = get("/", Some("\"someoldbuild\""));
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[test]
    fn spa_routes_fall_back_to_index_but_stale_assets_404() {
        if !dist_embedded() {
            return;
        }
        let res = get("/settings/general", None);
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(header_str(&res, header::CONTENT_TYPE), "text/html");
        assert_eq!(header_str(&res, header::CACHE_CONTROL), "no-cache");

        // A hashed chunk from an older firmware must not resolve to index.html.
        let res = get("/chunk-OLDBUILD.js", None);
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn hashed_bundles_are_immutable() {
        if !dist_embedded() {
            return;
        }
        let main = WEB_DIR
            .files()
            .map(|f| f.path().to_string_lossy().into_owned())
            .find(|p| is_content_hashed(p))
            .expect("built dist contains hashed bundles");
        let res = get(&format!("/{main}"), None);
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            header_str(&res, header::CACHE_CONTROL),
            "public, max-age=31536000, immutable"
        );
    }
}
