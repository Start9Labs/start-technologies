use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};

/// Mirrors browser CORS requests before RPC body preparation.
pub fn layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::mirror_request())
        .allow_methods(AllowMethods::mirror_request())
        .allow_headers(AllowHeaders::mirror_request())
        .allow_credentials(true)
}

#[cfg(test)]
mod tests;
