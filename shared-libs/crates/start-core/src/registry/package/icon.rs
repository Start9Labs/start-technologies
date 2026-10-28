use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::response::Response;
use http::header::{CACHE_CONTROL, CONTENT_TYPE};
use imbl_value::InternedString;
use new_mime_guess::MimeGuess;
use serde::{Deserialize, Serialize};

use crate::net::static_server::{bad_request, not_found, server_error};
use crate::prelude::*;
use crate::registry::context::RegistryContext;
use crate::registry::package::index::PackageIndex;
use crate::util::DataUrl;

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

/// Serves a stored icon by its path under `/icons/`.
pub async fn stored_icon(ctx: &RegistryContext, path: &str) -> Response {
    if !is_stored_path(path) {
        return bad_request();
    }
    match tokio::fs::read(icons_dir(&ctx.datadir).join(path)).await {
        Ok(data) => Response::builder()
            .header(CONTENT_TYPE, icon_mime(path))
            .header(CACHE_CONTROL, ICON_CACHE_CONTROL)
            .body(Body::from(data))
            .unwrap(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => not_found(),
        Err(e) => server_error(Error::new(e, ErrorKind::Filesystem)),
    }
}

/// Every stored icon path the package index refers to.
pub fn referenced_icons(index: &PackageIndex) -> BTreeSet<InternedString> {
    index
        .packages
        .values()
        .flat_map(|p| p.versions.values())
        .flat_map(|v| {
            v.icon
                .iter()
                .chain(v.dependency_metadata.values().flat_map(|d| d.icon.iter()))
        })
        .filter_map(|icon| match icon {
            PackageIcon::Stored(path) => Some(path.clone()),
            PackageIcon::Inline(_) => None,
        })
        .collect()
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
