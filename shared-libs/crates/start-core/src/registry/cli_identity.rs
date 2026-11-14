use std::path::{Path, PathBuf};

use tokio::io::AsyncWriteExt;
use url::Url;

use crate::prelude::*;
use crate::sign::AnyVerifyingKey;
use crate::sign::fingerprint::fingerprint;
use crate::util::io::{AtomicFile, maybe_read_file_to_string};

fn target(url: &Url, address: &str) -> Result<Url, Error> {
    let mut target = url.clone();
    target
        .set_host(Some(address))
        .with_kind(ErrorKind::ParseUrl)?;
    target
        .set_username("")
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    target
        .set_password(None)
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    target.set_query(None);
    target.set_fragment(None);
    Ok(target)
}

fn identity_file(directory: &Path, target: &Url) -> PathBuf {
    directory.join(format!(
        "{}.pem",
        blake3::hash(target.as_str().as_bytes()).to_hex()
    ))
}

pub(super) async fn observe(
    directory: &Path,
    url: &Url,
    address: &str,
    peer: &AnyVerifyingKey,
) -> Vec<String> {
    let target = match target(url, address) {
        Ok(target) => target,
        Err(error) => {
            return vec![
                t!(
                    "registry.identity.read-failed",
                    target = url.origin().ascii_serialization(),
                    details = error
                )
                .to_string(),
            ];
        }
    };
    let path = identity_file(directory, &target);
    let mut warnings = Vec::new();
    match maybe_read_file_to_string(&path).await.and_then(|contents| {
        contents
            .map(|key| key.parse::<AnyVerifyingKey>())
            .transpose()
    }) {
        Ok(Some(previous)) if previous == *peer => return warnings,
        Ok(Some(previous)) => warnings.push(
            t!(
                "registry.identity.changed",
                target = target,
                previous = fingerprint(&previous),
                current = fingerprint(peer)
            )
            .to_string(),
        ),
        Ok(_) => (),
        Err(error) => warnings.push(
            t!(
                "registry.identity.read-failed",
                target = target,
                details = error
            )
            .to_string(),
        ),
    }
    let stored = async {
        tokio::fs::create_dir_all(directory)
            .await
            .with_kind(ErrorKind::Filesystem)?;
        let temporary = path.with_extension(format!("{}.tmp", crate::util::new_guid()));
        let mut file = AtomicFile::new(&path, Some(&temporary)).await?;
        file.write_all(peer.to_string().as_bytes())
            .await
            .with_kind(ErrorKind::Filesystem)?;
        file.save().await
    }
    .await;
    if let Err(error) = stored {
        warnings.push(
            t!(
                "registry.identity.write-failed",
                target = target,
                details = error
            )
            .to_string(),
        );
    }
    warnings
}

#[cfg(test)]
mod tests;
