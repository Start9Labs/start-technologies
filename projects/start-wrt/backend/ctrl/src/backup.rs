use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{header, Response};
use clap::Parser;
use imbl_value::imbl::OrdMap;
use imbl_value::json;
use itertools::Itertools;
use openssl::symm::{decrypt_aead, encrypt_aead, Cipher};
use rpc_toolkit::{
    from_fn_async, CallRemote, Context, Empty, HandlerArgs, HandlerExt, ParentHandler,
};
use serde::{Deserialize, Serialize};
use startos::util::crypto::{unwrap_backup_key, wrap_backup_key};
use tokio::process::Command;
use zeroize::Zeroizing;

use crate::continuations::{self, Guid, RpcContinuation};
use crate::invoke::Invoke;
use crate::prelude::*;
use crate::{CliContext, ServerContext};

const MAGIC: &[u8] = b"STARTWRT-BACKUP";
const FORMAT_VERSION: u8 = 1;
const MAX_HEADER_LEN: usize = 4096;
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
const BASE32: base32::Alphabet = base32::Alphabet::Rfc4648 { padding: false };

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EncryptedHeader {
    password_hash: String,
    wrapped_key: String,
}

/// A backup without the magic prefix is a plain gzip archive.
pub fn is_encrypted(backup: &[u8]) -> bool {
    backup.starts_with(MAGIC)
}

fn invalid_archive() -> Error {
    Error::new(eyre!("Invalid backup archive"), ErrorKind::InvalidRequest)
}

/// Encrypts with a fresh key, wrapped under the password beside its Argon2id hash.
fn encrypt(archive: &[u8], password: &str) -> Result<Vec<u8>, Error> {
    let key = Zeroizing::new(rand::random::<[u8; 32]>());
    let header = serde_json::to_vec(&EncryptedHeader {
        password_hash: argon2::hash_encoded(
            password.as_bytes(),
            &rand::random::<[u8; 16]>(),
            &argon2::Config::rfc9106_low_mem(),
        )
        .map_err(|e| Error::new(e, ErrorKind::PasswordHashGeneration))?,
        wrapped_key: wrap_backup_key(&Zeroizing::new(base32::encode(BASE32, &*key)), password)?,
    })?;
    let mut backup =
        Vec::with_capacity(MAGIC.len() + 5 + header.len() + NONCE_LEN + archive.len() + TAG_LEN);
    backup.extend_from_slice(MAGIC);
    backup.push(FORMAT_VERSION);
    backup.extend_from_slice(&(header.len() as u32).to_be_bytes());
    backup.extend_from_slice(&header);
    let nonce: [u8; NONCE_LEN] = rand::random();
    let mut tag = [0; TAG_LEN];
    let ciphertext = encrypt_aead(
        Cipher::aes_256_gcm(),
        &*key,
        Some(&nonce),
        &backup,
        archive,
        &mut tag,
    )?;
    backup.extend_from_slice(&nonce);
    backup.extend_from_slice(&ciphertext);
    backup.extend_from_slice(&tag);
    Ok(backup)
}

/// Rejects a wrong password before touching the key.
fn decrypt(backup: &[u8], password: &str) -> Result<Zeroizing<Vec<u8>>, Error> {
    let rest = backup.strip_prefix(MAGIC).ok_or_else(invalid_archive)?;
    let (&version, rest) = rest.split_first().ok_or_else(invalid_archive)?;
    if version != FORMAT_VERSION {
        return Err(Error::new(
            eyre!("This backup needs a newer version of StartWRT"),
            ErrorKind::InvalidRequest,
        ));
    }
    let (len, rest) = rest.split_first_chunk::<4>().ok_or_else(invalid_archive)?;
    let len = u32::from_be_bytes(*len) as usize;
    if len > MAX_HEADER_LEN || rest.len() < len + NONCE_LEN + TAG_LEN {
        return Err(invalid_archive());
    }
    let (header, rest) = rest.split_at(len);
    let aad = &backup[..backup.len() - rest.len()];
    let header: EncryptedHeader = serde_json::from_slice(header).map_err(|_| invalid_archive())?;
    if !argon2::verify_encoded(&header.password_hash, password.as_bytes())
        .map_err(|_| invalid_archive())?
    {
        return Err(Error::new(
            eyre!("Enter the admin password in effect when this backup was created"),
            ErrorKind::IncorrectPassword,
        ));
    }
    let (key, _) =
        unwrap_backup_key(&header.wrapped_key, password).map_err(|_| invalid_archive())?;
    let key =
        Zeroizing::new(base32::decode(BASE32, &Zeroizing::new(key)).ok_or_else(invalid_archive)?);
    let (nonce, rest) = rest.split_at(NONCE_LEN);
    let (ciphertext, tag) = rest.split_at(rest.len() - TAG_LEN);
    decrypt_aead(
        Cipher::aes_256_gcm(),
        &key,
        Some(nonce),
        aad,
        ciphertext,
        tag,
    )
    .map(Zeroizing::new)
    .map_err(|_| invalid_archive())
}

#[derive(Serialize, Deserialize, visit_rs::VisitFields)]
pub struct BackupCreateParams {
    pub password: String,
}
rpc_toolkit::reflect_ts!(BackupCreateParams);

#[derive(Debug, Serialize, Deserialize, visit_rs::VisitFields)]
pub struct BackupCreateRes {
    pub guid: Guid,
    pub filename: String,
}
rpc_toolkit::reflect_ts!(BackupCreateRes);

#[derive(Serialize, Deserialize, visit_rs::VisitFields)]
pub struct BackupRestoreParams {
    pub password: Option<String>,
}
rpc_toolkit::reflect_ts!(BackupRestoreParams);

#[derive(Debug, Serialize, Deserialize, visit_rs::VisitFields)]
pub struct BackupRestoreRes {
    pub upload: Guid,
}
rpc_toolkit::reflect_ts!(BackupRestoreRes);

async fn encrypted_backup(password: String) -> Result<Vec<u8>, Error> {
    crate::auth::check_admin_password(&password).await?;
    let archive = Zeroizing::new(
        Command::new("sysupgrade")
            .args(["--create-backup", "-"])
            .invoke(ErrorKind::Filesystem.into())
            .await?,
    );
    tokio::task::spawn_blocking(move || encrypt(&archive, &password)).await?
}

async fn decrypted_archive(
    upload: Vec<u8>,
    password: Option<String>,
) -> Result<Zeroizing<Vec<u8>>, Error> {
    if !is_encrypted(&upload) {
        return Ok(Zeroizing::new(upload));
    }
    tokio::task::spawn_blocking(move || decrypt(&upload, password.as_deref().unwrap_or(""))).await?
}

#[instrument(skip_all)]
async fn create(
    ctx: ServerContext,
    BackupCreateParams { password }: BackupCreateParams,
) -> Result<BackupCreateRes, Error> {
    let hostname = match Command::new("uci")
        .args(["get", "system.@system[0].hostname"])
        .invoke(ErrorKind::Filesystem.into())
        .await
    {
        Ok(output) => String::from_utf8_lossy(&output).trim().to_string(),
        _ => "startwrt".to_string(),
    };

    let hostname: String = hostname
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect();
    let hostname = if hostname.is_empty() {
        "startwrt".to_string()
    } else {
        hostname
    };

    let date = chrono::Utc::now().format("%Y-%m-%d");
    let filename = format!("backup-{hostname}-{date}.swbk");

    let body = match encrypted_backup(password).await {
        Ok(body) => body,
        Err(e) => {
            crate::activity::log(
                "backup",
                "downloaded",
                false,
                "Failed to create config backup",
                Some(&e.to_string()),
            );
            return Err(e);
        }
    };

    crate::activity::log(
        "backup",
        "downloaded",
        true,
        "Downloaded config backup",
        None,
    );

    let fname = filename.clone();
    let guid = Guid::new();
    ctx.continuations
        .add(
            guid.clone(),
            RpcContinuation::rest(
                move |_req| async move {
                    Ok(Response::builder()
                        .header(header::CONTENT_TYPE, "application/octet-stream")
                        .header(
                            header::CONTENT_DISPOSITION,
                            format!("attachment; filename=\"{fname}\""),
                        )
                        .body(Body::from(body))
                        .unwrap())
                },
                continuations::DEFAULT_TTL,
            ),
        )
        .await;

    Ok(BackupCreateRes { guid, filename })
}

#[instrument(skip_all)]
async fn cli_download(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        ..
    }: HandlerArgs<CliContext>,
) -> Result<(), Error> {
    let password = rpassword::prompt_password("Admin Password: ")?;
    let res: BackupCreateRes = imbl_value::from_value(
        ctx.call_remote(
            &parent_method.into_iter().chain(method).join("."),
            OrdMap::new(),
            json!({ "password": password }),
            Empty {},
        )
        .await?,
    )
    .map_err(|e| {
        Error::new(
            eyre!("Failed to parse response: {e}"),
            ErrorKind::Deserialization,
        )
    })?;

    let (bytes, _) = ctx.rest_download(res.guid.as_ref()).await?;

    let download_dir = dirs::download_dir().unwrap_or_else(|| PathBuf::from("."));
    let path = download_dir.join(&res.filename);
    tokio::fs::write(&path, &bytes).await.map_err(|e| {
        crate::Error::new(
            eyre!("Failed to write {}: {e}", path.display()),
            ErrorKind::Filesystem,
        )
    })?;

    println!("{}", path.display());
    Ok(())
}

#[instrument(skip_all)]
async fn restore(
    ctx: ServerContext,
    BackupRestoreParams { password }: BackupRestoreParams,
) -> Result<BackupRestoreRes, Error> {
    let guid = Guid::new();
    ctx.continuations
        .add(
            guid.clone(),
            RpcContinuation::rest(
                move |req| async move {
                    let body_bytes =
                        axum::body::to_bytes(req.into_body(), continuations::MAX_UPLOAD_BYTES)
                            .await
                            .map_err(|e| {
                                Error::new(eyre!("Failed to read upload: {e}"), ErrorKind::Network)
                            })?;

                    let archive = match decrypted_archive(body_bytes.into(), password).await {
                        Ok(archive) => archive,
                        Err(e) => {
                            crate::activity::log(
                                "backup",
                                "restored",
                                false,
                                "Failed to restore config backup",
                                Some(&e.to_string()),
                            );
                            return Err(e.into());
                        }
                    };

                    let tmp_path = "/tmp/backup-restore.tar.gz";
                    startos::util::io::write_file_private_atomic(tmp_path, &*archive)
                        .await
                        .map_err(Error::from)?;
                    drop(archive);

                    let Ok(listing) = Command::new("tar")
                        .args(["-tzf", tmp_path])
                        .invoke(ErrorKind::Filesystem.into())
                        .await
                    else {
                        let _ = tokio::fs::remove_file(tmp_path).await;
                        crate::activity::log(
                            "backup",
                            "restored",
                            false,
                            "Failed to restore config backup",
                            Some("Invalid backup archive"),
                        );
                        return Err(Error::new(
                            eyre!("Invalid backup archive"),
                            ErrorKind::InvalidRequest,
                        )
                        .into());
                    };

                    let log_history = crate::logs::snapshot_history(Path::new("/")).await;

                    let apply = Command::new("sysupgrade")
                        .args(["--restore-backup", tmp_path])
                        .invoke(ErrorKind::Filesystem.into())
                        .await;

                    let _ = tokio::fs::remove_file(tmp_path).await;

                    if let Err(e) = apply {
                        crate::activity::log(
                            "backup",
                            "restored",
                            false,
                            "Failed to restore config backup",
                            Some(&e.to_string()),
                        );
                        return Err(e.into());
                    }

                    let listing = String::from_utf8_lossy(&listing);
                    if let Err(e) =
                        crate::migrate::forget_unless_restored(Path::new("/"), &listing).await
                    {
                        tracing::error!("failed to reset migration record after restore: {e}");
                    }
                    match log_history {
                        Ok(before) => {
                            if let Err(e) = crate::logs::merge_restored_history(
                                Path::new("/"),
                                before,
                                &listing,
                            )
                            .await
                            {
                                tracing::error!("failed to merge system log after restore: {e}");
                            }
                        }
                        Err(e) => tracing::error!("failed to read system log before restore: {e}"),
                    }

                    crate::activity::log(
                        "backup",
                        "restored",
                        true,
                        "Restored config backup (rebooting)",
                        None,
                    );

                    tokio::spawn(async {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        if let Err(e) = Command::new("reboot")
                            .invoke(ErrorKind::Filesystem.into())
                            .await
                        {
                            tracing::error!("failed to reboot after restore: {e}");
                        }
                    });

                    Ok(Response::builder()
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(r#"{"success":true}"#))
                        .unwrap())
                },
                continuations::DEFAULT_TTL,
            ),
        )
        .await;
    Ok(BackupRestoreRes { upload: guid })
}

#[derive(Debug, Deserialize, Serialize, Parser, visit_rs::VisitFields)]
struct RestoreParams {
    /// Path to the backup file
    file: PathBuf,
}
rpc_toolkit::reflect_ts!(RestoreParams);

#[instrument(skip_all)]
async fn cli_upload(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        params: RestoreParams { file },
        ..
    }: HandlerArgs<CliContext, RestoreParams>,
) -> Result<(), Error> {
    let data = tokio::fs::read(&file).await.map_err(|e| {
        crate::Error::new(
            eyre!("Failed to read {}: {e}", file.display()),
            ErrorKind::Filesystem,
        )
    })?;
    let password = if is_encrypted(&data) {
        Some(rpassword::prompt_password(
            "Admin password when this backup was created: ",
        )?)
    } else {
        None
    };

    let res: BackupRestoreRes = imbl_value::from_value(
        ctx.call_remote(
            &parent_method.into_iter().chain(method).join("."),
            OrdMap::new(),
            json!({ "password": password }),
            Empty {},
        )
        .await?,
    )
    .map_err(|e| {
        Error::new(
            eyre!("Failed to parse response: {e}"),
            ErrorKind::Deserialization,
        )
    })?;

    ctx.rest_upload(res.upload.as_ref(), data).await?;

    println!("Backup restored. Device is rebooting.");
    Ok(())
}

pub fn backup<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand("create", from_fn_async(create).no_cli())
        .subcommand(
            "create",
            from_fn_async(cli_download)
                .no_ts()
                .no_display()
                .with_about("Download a config backup"),
        )
        .subcommand("restore", from_fn_async(restore).no_cli())
        .subcommand(
            "restore",
            from_fn_async(cli_upload)
                .no_ts()
                .no_display()
                .with_about("Restore config backup from file"),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARCHIVE: &[u8] = b"\x1f\x8b\x08\x00 router settings and private keys";
    const PASSWORD: &str = "admin-password";

    fn header_end(backup: &[u8]) -> usize {
        let len = u32::from_be_bytes(backup[MAGIC.len() + 1..][..4].try_into().unwrap());
        MAGIC.len() + 5 + len as usize
    }

    fn kind(backup: &[u8], password: &str) -> ErrorKind {
        decrypt(backup, password).unwrap_err().kind
    }

    #[test]
    fn roundtrip_hides_the_archive() {
        let backup = encrypt(ARCHIVE, PASSWORD).unwrap();
        assert!(is_encrypted(&backup));
        assert!(!is_encrypted(ARCHIVE));
        assert!(!backup
            .windows(ARCHIVE.len() - 4)
            .any(|w| w == &ARCHIVE[4..]));
        assert_eq!(&*decrypt(&backup, PASSWORD).unwrap(), ARCHIVE);
        assert!(encrypt(ARCHIVE, PASSWORD).unwrap() != backup);
    }

    #[test]
    fn wrong_password_is_incorrect() {
        let backup = encrypt(ARCHIVE, PASSWORD).unwrap();
        for password in ["wrong-password", ""] {
            assert_eq!(kind(&backup, password), ErrorKind::IncorrectPassword);
        }
    }

    #[test]
    fn tampering_and_truncation_are_invalid() {
        let backup = encrypt(ARCHIVE, PASSWORD).unwrap();
        let end = header_end(&backup);
        let wrapped = backup.windows(3).position(|w| w == b"v1:").unwrap() + 10;
        for index in [wrapped, end, end + NONCE_LEN, backup.len() - 1] {
            let mut tampered = backup.clone();
            tampered[index] ^= 1;
            assert_eq!(kind(&tampered, PASSWORD), ErrorKind::InvalidRequest);
        }
        for len in [
            0,
            MAGIC.len(),
            MAGIC.len() + 1,
            MAGIC.len() + 5,
            end + NONCE_LEN + TAG_LEN - 1,
            backup.len() - 1,
        ] {
            assert_eq!(kind(&backup[..len], PASSWORD), ErrorKind::InvalidRequest);
        }
        let mut oversized = backup.clone();
        oversized[MAGIC.len() + 1..][..4].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(kind(&oversized, PASSWORD), ErrorKind::InvalidRequest);
    }

    #[test]
    fn newer_format_is_named() {
        let mut backup = encrypt(ARCHIVE, PASSWORD).unwrap();
        backup[MAGIC.len()] = FORMAT_VERSION + 1;
        let error = decrypt(&backup, PASSWORD).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidRequest);
        assert!(error.to_string().contains("newer version"));
    }

    #[tokio::test]
    async fn legacy_upload_passes_through() {
        let archive = decrypted_archive(ARCHIVE.to_vec(), None).await.unwrap();
        assert_eq!(&*archive, ARCHIVE);
        let backup = encrypt(ARCHIVE, PASSWORD).unwrap();
        let error = decrypted_archive(backup, None).await.unwrap_err();
        assert_eq!(error.kind, ErrorKind::IncorrectPassword);
    }
}
