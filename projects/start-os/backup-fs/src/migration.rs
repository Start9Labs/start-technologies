use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use chacha20::Key;
use log::warn;

use crate::atomic_file::AtomicFile;
use crate::error::{BkfsError, BkfsErrorKind, BkfsResult};
use crate::vault::{self, EccParams};

/// Called while the exclusive store lock and authenticated pending flag are held.
pub(crate) fn reseal_store(dir: &Path, key: &Key) -> BkfsResult<()> {
    let segments = dir.join("segments");
    if directory_exists(&segments)? {
        let checkpoint = segments.join(crate::seglog::INDEX_CACHE_NAME);
        match fs::remove_file(checkpoint) {
            Ok(()) => File::open(&segments)?.sync_all()?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        crate::seglog::migrate_segments(&segments, key)?;
    }
    for name in ["contents", "dirents"] {
        let path = dir.join(name);
        if directory_exists(&path)? {
            reseal_tree(&path, key)?;
        }
    }
    Ok(())
}

/// Unreadable blobs stay in place and fail later reads, as before the upgrade.
pub(crate) fn reseal_blob(blob: &[u8], key: &Key) -> BkfsResult<Option<Vec<u8>>> {
    let opened = if vault::is_legacy(blob) {
        vault::open_legacy(blob, key).map(|plain| {
            let ecc = EccParams {
                data: blob[5],
                parity: blob[6],
            };
            Some(vault::seal(&plain, key, ecc))
        })
    } else {
        vault::open(blob, key).map(|_| None)
    };
    match opened {
        Err(e) if matches!(e.kind, BkfsErrorKind::BadChecksum) => {
            warn!("leaving unreadable blob unmigrated");
            Ok(None)
        }
        res => res,
    }
}

fn directory_exists(path: &Path) -> BkfsResult<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => Ok(true),
        Ok(_) => Err(BkfsError::unsupported(
            "unexpected non-directory in vault store",
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

fn reseal_tree(dir: &Path, key: &Key) -> BkfsResult<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            reseal_tree(&path, key)?;
        } else if kind.is_file() {
            if path.extension().is_some_and(|ext| ext == "tmp") {
                continue;
            }
            let blob = fs::read(&path)?;
            if let Some(sealed) = reseal_blob(&blob, key)? {
                let mut out = AtomicFile::create_buffered(path)?;
                out.write_all(&sealed)?;
                out.save()?;
                File::open(dir)?.sync_all()?;
            }
        } else {
            return Err(BkfsError::unsupported("unexpected non-file in vault store"));
        }
    }
    File::open(dir)?.sync_all()?;
    Ok(())
}
