use std::path::{Path, PathBuf};

use ed25519::PublicKeyBytes;
use ed25519::pkcs8::EncodePrivateKey;
use ed25519_dalek::{SigningKey, VerifyingKey};
use tokio::io::AsyncWriteExt;
use tracing::instrument;

use crate::context::CliContext;
use crate::context::config::local_config_path;
use crate::prelude::*;
use crate::util::io::create_file_mod;
use crate::util::serde::Pem;

pub const OS_ID_KEY_PATH: &str = "/run/startos/id.key.pem";

pub fn default_id_key_path() -> PathBuf {
    local_config_path()
        .as_deref()
        .unwrap_or_else(|| Path::new(crate::context::config::CONFIG_PATH))
        .parent()
        .unwrap_or(Path::new("/"))
        .join("id.key.pem")
}

/// Move a key file from its pre-1.1.0 name to `new`, if `new` doesn't exist
/// yet (`developer.key.pem` → `id.key.pem`, `build-key` → `build.key.pem`).
// TODO: remove this legacy-filename migration after a few releases.
pub fn migrate_legacy_key_file(new: &Path, legacy: &Path) {
    if matches!(new.try_exists(), Ok(false)) && matches!(legacy.try_exists(), Ok(true)) {
        match std::fs::rename(legacy, new) {
            Ok(()) => tracing::info!(
                "Renamed legacy key file {} -> {}",
                legacy.display(),
                new.display()
            ),
            Err(e) => tracing::warn!("Failed to rename legacy key file {}: {e}", legacy.display()),
        }
    }
}

pub async fn write_signing_key(
    secret: &ed25519_dalek::SigningKey,
    path: impl AsRef<Path>,
) -> Result<(), Error> {
    let keypair_bytes = ed25519::KeypairBytes {
        secret_key: secret.to_bytes(),
        public_key: Some(PublicKeyBytes(VerifyingKey::from(secret).to_bytes())),
    };
    let mut file = create_file_mod(path, 0o640).await?;
    file.write_all(
        keypair_bytes
            .to_pkcs8_pem(base64ct::LineEnding::default())
            .with_kind(crate::ErrorKind::Pem)?
            .as_bytes(),
    )
    .await?;
    file.sync_all().await?;
    Ok(())
}

/// Read counterpart to [`write_signing_key`]: load a PKCS#8 PEM ed25519 key
/// (e.g. a workspace `.startos/build.key.pem`) into a `SigningKey`.
pub fn load_signing_key(path: impl AsRef<Path>) -> Result<SigningKey, Error> {
    let path = path.as_ref();
    let pair = <ed25519::KeypairBytes as ed25519::pkcs8::DecodePrivateKey>::from_pkcs8_pem(
        &std::fs::read_to_string(path).with_ctx(|_| (ErrorKind::Filesystem, path.display()))?,
    )
    .with_kind(ErrorKind::Pem)?;
    let secret = ed25519_dalek::SecretKey::try_from(&pair.secret_key[..]).map_err(|_| {
        Error::new(
            eyre!("{}", t!("context.cli.pkcs8-key-incorrect-length")),
            ErrorKind::OpenSsl,
        )
    })?;
    Ok(secret.into())
}

#[instrument(skip_all)]
pub async fn init(ctx: CliContext) -> Result<(), Error> {
    if tokio::fs::metadata(OS_ID_KEY_PATH).await.is_ok() {
        println!("Identity key already exists at {}", OS_ID_KEY_PATH);
    } else if tokio::fs::metadata(&ctx.id_key_path).await.is_err() {
        tracing::info!("Generating new identity key...");
        let secret = SigningKey::generate(&mut crate::util::crypto::os_rng());
        tracing::info!("Writing key to {}", ctx.id_key_path.display());
        write_signing_key(&secret, &ctx.id_key_path).await?;
        println!(
            "New identity key generated at {}",
            ctx.id_key_path.display()
        );
    } else {
        println!(
            "Identity key already exists at {}",
            ctx.id_key_path.display()
        );
    }
    Ok(())
}

pub fn pubkey(ctx: CliContext) -> Result<Pem<ed25519_dalek::VerifyingKey>, Error> {
    Ok(Pem(ctx.id_key()?.verifying_key()))
}
