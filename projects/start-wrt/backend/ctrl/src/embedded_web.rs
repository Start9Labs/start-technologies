use axum::body::Body;
use axum::http::{Request, Response, StatusCode, header};
use include_dir::{Dir, include_dir};

static WEB_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../../web/dist/startwrt/browser");

pub async fn serve_embedded(req: Request<Body>) -> Response<Body> {
    let path = req.uri().path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    let file = WEB_DIR.get_file(path).or_else(|| WEB_DIR.get_file("index.html"));

    match file {
        Some(file) => {
            let mime = mime_guess::from_path(file.path())
                .first_raw()
                .unwrap_or("application/octet-stream");
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime)
                .body(Body::from(file.contents()))
                .unwrap()
        }
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("not found"))
            .unwrap(),
    }
}
