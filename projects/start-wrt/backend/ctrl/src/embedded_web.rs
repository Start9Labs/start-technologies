use axum::body::Body;
use axum::http::{header, Request, Response, StatusCode};
use include_dir::{include_dir, Dir};
use startos::net::static_server::{is_ui_asset_immutable, is_ui_route};

static WEB_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../../web/dist/startwrt/browser");

const ETAG: &str = concat!("\"", env!("STARTWRT_GIT_HASH"), "\"");

pub async fn serve_embedded(req: Request<Body>) -> Response<Body> {
    let path = req.uri().path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    let file = WEB_DIR.get_file(path).or_else(|| {
        is_ui_route(path)
            .then(|| WEB_DIR.get_file("index.html"))
            .flatten()
    });

    let Some(file) = file else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("not found"))
            .unwrap();
    };

    let cache_control = if is_ui_asset_immutable(&WEB_DIR, file.path()) {
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
    fn declared_assets_are_immutable() {
        if !dist_embedded() {
            return;
        }
        let path = WEB_DIR
            .files()
            .find(|file| is_ui_asset_immutable(&WEB_DIR, file.path()))
            .expect("the UI build declares an immutable asset")
            .path();
        let res = get(&format!("/{}", path.display()), None);
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            header_str(&res, header::CACHE_CONTROL),
            "public, max-age=31536000, immutable"
        );
    }
}
