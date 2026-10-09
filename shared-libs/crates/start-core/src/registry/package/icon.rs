use axum::body::Body;
use axum::response::Response;
use http::header::{CACHE_CONTROL, CONTENT_TYPE};

use crate::PackageId;
use crate::net::static_server::{bad_request, not_found, server_error};
use crate::prelude::*;
use crate::registry::context::RegistryContext;
use crate::util::VersionString;

/// Serves the icon of a package version, or of one of its dependencies as that version packs it.
pub async fn icon(
    ctx: &RegistryContext,
    id: &str,
    version: &str,
    dependency: Option<&str>,
) -> Response {
    let (Ok(id), Ok(version), Ok(dependency)) = (
        id.parse::<PackageId>(),
        version.parse::<VersionString>(),
        dependency.map(str::parse::<PackageId>).transpose(),
    ) else {
        return bad_request();
    };
    let peek = ctx.db.peek().await;
    let icon = match peek
        .as_index()
        .as_package()
        .as_packages()
        .as_idx(&id)
        .and_then(|p| p.as_versions().as_idx(&version))
    {
        None => Ok(None),
        Some(info) => match &dependency {
            None => info.as_icon().de(),
            Some(dep) => info
                .as_dependency_metadata()
                .as_idx(dep)
                .map_or(Ok(None), |d| d.as_icon().de()),
        },
    };
    match icon {
        Ok(Some(icon)) => Response::builder()
            .header(CONTENT_TYPE, &*icon.mime)
            .header(CACHE_CONTROL, "public, max-age=3600")
            .body(Body::from(icon.data.into_owned()))
            .unwrap(),
        Ok(None) => not_found(),
        Err(e) => server_error(e),
    }
}
