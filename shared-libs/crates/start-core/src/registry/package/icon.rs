use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::extract::Request;
use axum::response::Response;
use http::header::{CACHE_CONTROL, CONTENT_TYPE};
use imbl_value::InternedString;
use new_mime_guess::MimeGuess;
use serde::{Deserialize, Serialize};

use crate::middleware::auth::signature::{AUTH_SIG_HEADER, verify_request_signature};
use crate::net::static_server::{bad_request, not_found, server_error};
use crate::prelude::*;
use crate::registry::context::RegistryContext;
use crate::registry::package::index::{PackageIndex, PackageInfo, Reader};
use crate::util::DataUrl;

pub(crate) const ICON_PATH: &str = "icons";
const ICON_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";
const BLAKE3_DIR: &str = "blake3";
const HASH_LEN: usize = 52;

/// Inline for StartOS before 0.4.0.3 and from registries before 1.2.0.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, VisitVariants)]
#[serde(untagged)]
pub enum PackageIcon {
    Inline(DataUrl<'static>),
    /// A path under the registry's `/icons/`, led by the scheme that names the file.
    Stored(InternedString),
}

rpc_toolkit::reflect_ts!(PackageIcon);
rpc_toolkit::ts_export!(PackageIcon, namespaces = [""]);

pub fn icons_dir(datadir: &Path) -> PathBuf {
    datadir.join("icons")
}

/// Unpadded lowercase base32 blake3 of the bytes, then an extension carrying the MIME type.
fn is_blake3_file_name(name: &str) -> bool {
    name.split_once('.').is_some_and(|(hash, ext)| {
        hash.len() == HASH_LEN
            && hash.bytes().all(|b| matches!(b, b'a'..=b'z' | b'2'..=b'7'))
            && !ext.is_empty()
            && ext.bytes().all(|b| b.is_ascii_alphanumeric())
    })
}

fn is_stored_path(path: &str) -> bool {
    path.split_once('/')
        .is_some_and(|(scheme, name)| scheme == BLAKE3_DIR && is_blake3_file_name(name))
}

pub(crate) fn is_icon_url(registry: &url::Url, target: &url::Url) -> bool {
    let mut prefix = registry.clone();
    let Ok(mut path) = prefix.path_segments_mut() else {
        return false;
    };
    path.pop_if_empty().push(ICON_PATH).push("");
    drop(path);
    target
        .as_str()
        .strip_prefix(prefix.as_str())
        .is_some_and(is_stored_path)
}

fn icon_mime(path: &str) -> String {
    MimeGuess::from_path(path)
        .first_or_octet_stream()
        .essence_str()
        .to_owned()
}

/// Writes the icon into the store unless it is already there.
pub fn store_icon(dir: &Path, icon: &DataUrl<'_>) -> Result<InternedString, Error> {
    let hash = base32::encode(
        base32::Alphabet::Rfc4648 { padding: false },
        blake3::hash(&icon.data).as_bytes(),
    )
    .to_lowercase();
    let ext = new_mime_guess::get_mime_extensions_str(&icon.mime)
        .and_then(|exts| exts.first())
        .copied()
        .unwrap_or("bin");
    let name = format!("{hash}.{ext}");
    let blake3_dir = &dir.join(BLAKE3_DIR);
    let file = &blake3_dir.join(&name);
    if !file.exists() {
        std::fs::create_dir_all(blake3_dir).with_ctx(|_| {
            (
                ErrorKind::Filesystem,
                lazy_format!("mkdir -p {blake3_dir:?}"),
            )
        })?;
        let tmp = &blake3_dir.join(format!(".{name}.{:016x}", rand::random::<u64>()));
        std::fs::write(tmp, &icon.data)
            .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("write {tmp:?}")))?;
        std::fs::rename(tmp, file).with_ctx(|_| {
            (
                ErrorKind::Filesystem,
                lazy_format!("mv {tmp:?} -> {file:?}"),
            )
        })?;
    }
    Ok(InternedString::from_display(&lazy_format!(
        "{BLAKE3_DIR}/{name}"
    )))
}

pub async fn store_icon_async(dir: &Path, icon: DataUrl<'static>) -> Result<PackageIcon, Error> {
    let dir = dir.to_owned();
    tokio::task::spawn_blocking(move || store_icon(&dir, &icon))
        .await
        .with_kind(ErrorKind::Unknown)?
        .map(PackageIcon::Stored)
}

/// Reads a stored icon back as its data URL.
pub async fn load_icon(dir: &Path, path: &str) -> Result<String, Error> {
    if !is_stored_path(path) {
        return Err(Error::new(
            eyre!("{}", t!("registry.package.icon.invalid-name", name = path)),
            ErrorKind::InvalidRequest,
        ));
    }
    let file = dir.join(path);
    let data = tokio::fs::read(&file)
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("read {file:?}")))?;
    Ok(DataUrl::from_vec(&icon_mime(path), data).to_string())
}

pub async fn stored_icon(ctx: &RegistryContext, path: &str, mut request: Request) -> Response {
    if !is_stored_path(path) {
        return bad_request();
    }
    // Public icons remain available when signature verification fails.
    let key = if request.headers().contains_key(AUTH_SIG_HEADER) {
        verify_request_signature(ctx, &mut request).await.ok()
    } else {
        None
    };
    let cache_control = async {
        let db = ctx.db.peek().await;
        let reader = Reader::resolve(&db, key.as_ref())?;
        icon_cache_control(db.as_index().as_package(), path, reader.as_ref())
    }
    .await;
    let cache_control = match cache_control {
        Ok(Some(cache_control)) => cache_control,
        Ok(None) => return not_found(),
        Err(e) => return server_error(e),
    };
    match tokio::fs::read(icons_dir(&ctx.datadir).join(path)).await {
        Ok(data) => Response::builder()
            .header(CONTENT_TYPE, icon_mime(path))
            .header(CACHE_CONTROL, cache_control)
            .body(Body::from(data))
            .unwrap(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => not_found(),
        Err(e) => server_error(Error::new(e, ErrorKind::Filesystem)),
    }
}

fn package_icons(package: &Model<PackageInfo>) -> Result<BTreeSet<InternedString>, Error> {
    let mut paths = BTreeSet::new();
    for (_, version) in package.as_versions().as_entries()? {
        let dependencies = version
            .as_dependency_metadata()
            .as_entries()?
            .into_iter()
            .map(|(_, dependency)| dependency.as_icon().de())
            .collect::<Result<Vec<_>, _>>()?;
        for icon in std::iter::once(version.as_icon().de()?)
            .chain(dependencies)
            .flatten()
        {
            if let PackageIcon::Stored(path) = icon {
                paths.insert(path);
            }
        }
    }
    Ok(paths)
}

fn icon_cache_control(
    index: &Model<PackageIndex>,
    path: &str,
    reader: Option<&Reader>,
) -> Result<Option<&'static str>, Error> {
    let mut private_access = false;
    for (_, package) in index.as_packages().as_entries()? {
        if package_icons(package)?.contains(path) {
            if package.is_visible_to(None)? {
                return Ok(Some(ICON_CACHE_CONTROL));
            }
            private_access |= package.is_visible_to(reader)?;
        }
    }
    Ok(private_access.then_some("private, no-store"))
}

pub fn referenced_icons(index: &Model<PackageIndex>) -> Result<BTreeSet<InternedString>, Error> {
    let mut paths = BTreeSet::new();
    for (_, package) in index.as_packages().as_entries()? {
        paths.extend(package_icons(package)?);
    }
    Ok(paths)
}

/// Deletes stored icons and interrupted writes that the index no longer refers to.
pub async fn sweep_icons(dir: &Path, referenced: &BTreeSet<InternedString>) -> Result<(), Error> {
    let blake3_dir = &dir.join(BLAKE3_DIR);
    let mut entries = match tokio::fs::read_dir(blake3_dir).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(e).with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("ls {blake3_dir:?}")));
        }
    };
    while let Some(entry) = entries
        .next_entry()
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("ls {blake3_dir:?}")))?
    {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let stale = if is_blake3_file_name(name) {
            !referenced.contains(&*format!("{BLAKE3_DIR}/{name}"))
        } else {
            name.strip_prefix('.')
                .and_then(|n| n.rsplit_once('.'))
                .is_some_and(|(n, _)| is_blake3_file_name(n))
        };
        if stale {
            let path = entry.path();
            tokio::fs::remove_file(&path)
                .await
                .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("rm {path:?}")))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_references_share_visibility_and_sweep_ownership() {
        let reader = Reader {
            signer: crate::rpc_continuations::Guid::new(),
            admin: false,
        };
        let public_version =
            imbl_value::json!({"icon": "blake3/shared.png", "dependencyMetadata": {}});
        let private_version = imbl_value::json!({
            "icon": "blake3/secret.png", "dependencyMetadata": {
                "dependency": {"icon": "blake3/dependency.png"},
                "shared": {"icon": "blake3/shared.png"}
            }
        });
        let index: Model<PackageIndex> = imbl_value::json!({"categories": {}, "packages": {
            "public": {"authorized": {}, "versions": {"1.0.0:0": public_version}},
            "secret": {"private": true, "readers": [reader.signer], "authorized": {},
                "versions": {"1.0.0:0": private_version}}
        }})
        .into();
        assert_eq!(
            referenced_icons(&index).unwrap(),
            BTreeSet::from([
                "blake3/shared.png".into(),
                "blake3/secret.png".into(),
                "blake3/dependency.png".into()
            ])
        );
        for path in ["blake3/secret.png", "blake3/dependency.png"] {
            assert_eq!(icon_cache_control(&index, path, None).unwrap(), None);
            assert_eq!(
                icon_cache_control(&index, path, Some(&reader)).unwrap(),
                Some("private, no-store")
            );
        }
        assert_eq!(
            icon_cache_control(&index, "blake3/shared.png", None).unwrap(),
            Some(ICON_CACHE_CONTROL)
        );
        assert_eq!(
            icon_cache_control(&index, "blake3/missing.png", Some(&reader)).unwrap(),
            None
        );
    }

    fn png(data: &[u8]) -> DataUrl<'static> {
        DataUrl::from_vec("image/png", data.to_vec())
    }

    #[tokio::test]
    async fn identical_icons_share_one_file() {
        let dir = tempfile::tempdir().unwrap();
        let a = store_icon(dir.path(), &png(b"icon")).unwrap();
        let b = store_icon(dir.path(), &png(b"icon")).unwrap();
        let c = store_icon(dir.path(), &png(b"other")).unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.starts_with("blake3/") && a.ends_with(".png"));
        assert_eq!(
            std::fs::read_dir(dir.path().join(BLAKE3_DIR))
                .unwrap()
                .count(),
            2
        );
        assert_eq!(
            load_icon(dir.path(), &a).await.unwrap(),
            png(b"icon").to_string()
        );
    }

    #[test]
    fn the_name_is_the_hash_of_the_stored_bytes_and_carries_the_mime_type() {
        let dir = tempfile::tempdir().unwrap();
        let svg = DataUrl::from_vec("image/svg+xml", b"<svg/>".to_vec());
        let path = store_icon(dir.path(), &svg).unwrap();
        let contents = std::fs::read(dir.path().join(&*path)).unwrap();
        let hash = base32::encode(
            base32::Alphabet::Rfc4648 { padding: false },
            blake3::hash(&contents).as_bytes(),
        )
        .to_lowercase();
        assert_eq!(&*path, format!("blake3/{hash}.svg"));
        assert_eq!(icon_mime(&path), "image/svg+xml");
    }

    #[tokio::test]
    async fn paths_outside_the_store_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = store_icon(dir.path(), &png(b"icon")).unwrap();
        assert!(is_stored_path(&path));
        let (_, name) = path.split_once('/').unwrap();
        for bad in [
            "../registry.db".to_owned(),
            name.to_owned(),
            format!("blur/{name}"),
            format!("blake3/../{name}"),
            format!("blake3/{}", name.replace('.', "")),
            format!("blake3/{}", name.to_uppercase()),
        ] {
            assert!(!is_stored_path(&bad), "{bad}");
            assert!(load_icon(dir.path(), &bad).await.is_err(), "{bad}");
        }
    }

    #[test]
    fn data_urls_are_inline_and_paths_are_stored() {
        let inline: PackageIcon =
            serde_json::from_value(serde_json::json!("data:image/png;base64,aWNvbg==")).unwrap();
        assert_eq!(inline, PackageIcon::Inline(png(b"icon")));
        let stored: PackageIcon =
            serde_json::from_value(serde_json::json!("blake3/x.png")).unwrap();
        assert_eq!(stored, PackageIcon::Stored("blake3/x.png".into()));
        assert_eq!(
            serde_json::to_value(&stored).unwrap(),
            serde_json::json!("blake3/x.png")
        );
    }

    #[tokio::test]
    async fn sweep_keeps_only_referenced_icons() {
        let dir = tempfile::tempdir().unwrap();
        let kept = store_icon(dir.path(), &png(b"kept")).unwrap();
        let dropped = store_icon(dir.path(), &png(b"dropped")).unwrap();
        let blake3_dir = dir.path().join(BLAKE3_DIR);
        let (_, kept_name) = kept.split_once('/').unwrap();
        let interrupted = blake3_dir.join(format!(".{kept_name}.0123456789abcdef"));
        let unrelated = blake3_dir.join("README");
        std::fs::write(&interrupted, b"partial").unwrap();
        std::fs::write(&unrelated, b"keep me").unwrap();

        sweep_icons(dir.path(), &[kept.clone()].into_iter().collect())
            .await
            .unwrap();

        assert!(dir.path().join(&*kept).exists());
        assert!(!dir.path().join(&*dropped).exists());
        assert!(!interrupted.exists());
        assert!(unrelated.exists());
    }

    #[tokio::test]
    async fn sweep_tolerates_a_missing_store() {
        let dir = tempfile::tempdir().unwrap();
        sweep_icons(&dir.path().join("icons"), &BTreeSet::new())
            .await
            .unwrap();
    }
}
