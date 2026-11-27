use std::io::{Cursor, Read, Write};
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
const MAX_BACKUP_BYTES: u64 = 256 << 20;

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
    let config = argon2::Config::rfc9106_low_mem();
    let prefix = format!(
        "${}$v={}$m={},t={},p={}$",
        config.variant, config.version, config.mem_cost, config.time_cost, config.lanes,
    );
    if !header.password_hash.starts_with(&prefix) {
        return Err(invalid_archive());
    }
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

fn is_activity_db_entry(path: &str) -> bool {
    let p = path.trim_start_matches('/').trim_start_matches("./");
    let p = p.trim_start_matches('/');
    let db = |b| matches!(p.strip_prefix(b), Some("" | "-journal" | "-wal" | "-shm"));
    db(crate::activity::ACTIVITY_DB.trim_start_matches('/'))
        || db(crate::migrate::LEGACY_ACTIVITY_DB)
}

fn copy_gzip_bounded(archive: &[u8], out: &mut impl Write) -> Result<(), Error> {
    let mut dec = flate2::read::MultiGzDecoder::new(archive).take(MAX_BACKUP_BYTES + 1);
    match std::io::copy(&mut dec, out) {
        Ok(n) if n > MAX_BACKUP_BYTES => Err(Error::new(
            eyre!("Backup archive exceeds 256 MiB limit"),
            ErrorKind::InvalidRequest,
        )),
        Ok(n) if n > 0 => Ok(()),
        _ => Err(invalid_archive()),
    }
}

fn strip_activity_db(archive: Zeroizing<Vec<u8>>) -> Result<Zeroizing<Vec<u8>>, Error> {
    let mut raw = Zeroizing::new(Vec::new());
    copy_gzip_bounded(&archive, &mut *raw)?;
    strip_tar(archive, &raw).ok_or_else(invalid_archive)
}

fn strip_tar(archive: Zeroizing<Vec<u8>>, raw: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    let (mut kept, mut cur) = (Vec::new(), 0usize);
    let mut tar = tar::Archive::new(Cursor::new(raw));
    for entry in tar.entries_with_seek().ok()? {
        let entry = entry.ok()?;
        let ty = entry.header().entry_type();
        let mut pos = usize::try_from(entry.raw_file_position()).ok()?;
        let mut size = entry.size();
        if ty.is_gnu_sparse() {
            let mut ext = entry.header().as_gnu()?.is_extended();
            while ext {
                ext = raw.get(pos..pos.checked_add(512)?)?[504] == 1;
                pos += 512;
            }
            size = entry.header().entry_size().ok()?;
        }
        let padded = usize::try_from(size.div_ceil(512).checked_mul(512)?).ok()?;
        let end = pos.checked_add(padded)?;
        (end >= cur && end <= raw.len()).then_some(())?;
        let path = entry.path().ok()?;
        if ty.is_pax_global_extensions() || !is_activity_db_entry(&path.to_string_lossy()) {
            kept.push(&raw[cur..end]);
        }
        cur = end;
    }
    if kept.iter().map(|s| s.len()).sum::<usize>() == cur {
        return Some(archive);
    }
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    kept.iter().try_for_each(|s| enc.write_all(s)).ok()?;
    enc.write_all(&[0u8; 1024]).ok()?;
    Some(Zeroizing::new(enc.finish().ok()?))
}

async fn remove_temp_file(path: &Path) {
    if let Err(e) = tokio::fs::remove_file(path).await {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!("failed to remove temporary backup {}: {e}", path.display());
        }
    }
}

async fn prepare_restore_archive(tmp: &Path, archive: Zeroizing<Vec<u8>>) -> Result<String, Error> {
    let f = move || copy_gzip_bounded(&archive, &mut std::io::sink()).map(|_| archive);
    let archive = tokio::task::spawn_blocking(f).await??;
    startos::util::io::write_file_private_atomic(tmp, &*archive).await?;
    let mut tar = Command::new("tar");
    tar.arg("-tzf").arg(tmp);
    if let Ok(out) = tar.invoke(ErrorKind::Filesystem.into()).await {
        return Ok(String::from_utf8_lossy(&out).into_owned());
    }
    remove_temp_file(tmp).await;
    Err(invalid_archive())
}

async fn apply_restore_preserving_activity<Fut: std::future::Future<Output = Result<(), Error>>>(
    root: &Path,
    tmp_path: &Path,
    apply: impl FnOnce() -> Fut,
) -> Result<(), Error> {
    let quiesce = crate::activity::quiesce();
    let before = match crate::activity::snapshot(root).await {
        Ok(b) => b,
        Err(e) => {
            remove_temp_file(tmp_path).await;
            return Err(e);
        }
    };
    let applied = apply().await;
    remove_temp_file(tmp_path).await;
    let restored = before.restore(root).await;
    drop(quiesce);
    if let Err(e) = restored {
        tracing::error!("failed to preserve activity log across restore: {e}");
    }
    applied
}

async fn encrypted_backup(password: String) -> Result<Vec<u8>, Error> {
    crate::auth::check_admin_password(&password).await?;
    let archive = Zeroizing::new(
        Command::new("sysupgrade")
            .args(["--create-backup", "-"])
            .invoke(ErrorKind::Filesystem.into())
            .await?,
    );
    tokio::task::spawn_blocking(move || encrypt(&strip_activity_db(archive)?, &password)).await?
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

                    let (root, tmp) = (Path::new("/"), Path::new("/tmp/backup-restore.tar.gz"));
                    let (listing, log_history) = async {
                        let archive = decrypted_archive(body_bytes.into(), password).await?;
                        let listing = prepare_restore_archive(tmp, archive).await?;
                        let log_history = crate::logs::snapshot_history(root).await;
                        let run = || async {
                            let mut cmd = Command::new("sysupgrade");
                            cmd.arg("--restore-backup").arg(tmp);
                            cmd.invoke(ErrorKind::Filesystem.into()).await?;
                            Ok(())
                        };
                        apply_restore_preserving_activity(root, tmp, run).await?;
                        Ok::<_, Error>((listing, log_history))
                    }
                    .await
                    .inspect_err(|e| {
                        crate::activity::log(
                            "backup",
                            "restored",
                            false,
                            "Failed to restore config backup",
                            Some(&e.to_string()),
                        );
                    })?;

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

                    crate::boot::record_reboot(crate::boot::RunState::Restoring);
                    tokio::spawn(async {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        if let Err(e) = Command::new("reboot")
                            .invoke(ErrorKind::Filesystem.into())
                            .await
                        {
                            crate::boot::record_running();
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
    use flate2::read::MultiGzDecoder;
    use flate2::write::GzEncoder;
    use flate2::Compression;

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
    fn altered_password_hash_parameters_are_invalid() {
        let backup = encrypt(ARCHIVE, PASSWORD).unwrap();
        let end = header_end(&backup);
        let header: EncryptedHeader =
            serde_json::from_slice(&backup[MAGIC.len() + 5..end]).unwrap();
        for (parameter, replacement) in [
            ("m=65536", "m=4294967295"),
            ("t=3", "t=4294967295"),
            ("p=1", "p=4294967295"),
            ("v=19", "v=16"),
            ("argon2id", "argon2i"),
        ] {
            let altered = serde_json::to_vec(&EncryptedHeader {
                password_hash: header.password_hash.replace(parameter, replacement),
                wrapped_key: header.wrapped_key.clone(),
            })
            .unwrap();
            let mut tampered = backup[..MAGIC.len() + 1].to_vec();
            tampered.extend_from_slice(&(altered.len() as u32).to_be_bytes());
            tampered.extend_from_slice(&altered);
            tampered.extend_from_slice(&backup[end..]);
            for password in [PASSWORD, "wrong-password"] {
                assert_eq!(kind(&tampered, password), ErrorKind::InvalidRequest);
            }
        }
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

    fn build_tar_gz(entries: &[(&str, u32, Option<&str>, &[u8])]) -> Vec<u8> {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = tar::Builder::new(encoder);
        builder.preserve_absolute(true);
        for &(path, mode, link_target, data) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_mode(mode);
            if let Some(target) = link_target {
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_size(0);
                builder.append_link(&mut header, path, target).unwrap();
            } else {
                header.set_entry_type(tar::EntryType::Regular);
                header.set_size(data.len() as u64);
                builder.append_data(&mut header, path, data).unwrap();
            }
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn is_activity_db_entry_matches_current_legacy_and_sidecars() {
        for path in [
            "var/lib/startwrt/activity.db",
            "./var/lib/startwrt/activity.db",
            "/var/lib/startwrt/activity.db",
            "/./var/lib/startwrt/activity.db",
            "var/lib/startwrt/activity.db-journal",
            "var/lib/startwrt/activity.db-wal",
            "var/lib/startwrt/activity.db-shm",
            "etc/startwrt/activity.db",
            "./etc/startwrt/activity.db",
            "/etc/startwrt/activity.db",
            "etc/startwrt/activity.db-journal",
            "etc/startwrt/activity.db-wal",
            "etc/startwrt/activity.db-shm",
        ] {
            assert!(is_activity_db_entry(path), "should match {path}");
        }
        for path in [
            "var/lib/startwrt/run-state",
            "var/lib/startwrt/activity.db.bak",
            "var/log/messages",
            "etc/startwrt/activity.db.bak",
            "etc/startwrt/auth.db",
            "etc/config/system",
        ] {
            assert!(!is_activity_db_entry(path), "should not match {path}");
        }
    }

    #[test]
    fn encrypt_backup_omits_activity_db_and_preserves_other_entries() {
        let long_path = format!("etc/startwrt/certs/{}.pem", "a".repeat(100));
        let archive = build_tar_gz(&[
            (
                "etc/config/network",
                0o644,
                None,
                b"config interface 'lan'\n",
            ),
            ("etc/shadow", 0o600, None, b"root:hash:0:0:99999:7:::\n"),
            ("etc/localtime", 0o777, Some("/tmp/localtime"), b""),
            (&long_path, 0o644, None, b"LONG-CERT"),
            ("var/log/messages", 0o600, None, b"syslog line\n"),
            ("var/lib/startwrt/activity.db", 0o644, None, b"sqlite-db"),
            ("./var/lib/startwrt/activity.db-journal", 0o644, None, b"j"),
            ("/var/lib/startwrt/activity.db-wal", 0o644, None, b"w"),
            ("var/lib/startwrt/activity.db-shm", 0o644, None, b"s"),
            ("etc/startwrt/activity.db", 0o644, None, b"legacy-db"),
            ("etc/startwrt/activity.db-journal", 0o644, None, b"lj"),
            ("/etc/startwrt/activity.db-wal", 0o644, None, b"lw"),
            ("./etc/startwrt/activity.db-shm", 0o644, None, b"ls"),
        ]);

        let stripped = strip_activity_db(Zeroizing::new(archive)).unwrap();
        let encrypted = encrypt(&stripped, PASSWORD).unwrap();
        let decrypted = decrypt(&encrypted, PASSWORD).unwrap();

        let mut tar = tar::Archive::new(MultiGzDecoder::new(decrypted.as_slice()));
        let mut seen = Vec::new();
        for entry in tar.entries().unwrap() {
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap().to_string_lossy().into_owned();
            let mode = entry.header().mode().unwrap();
            let link = entry
                .link_name()
                .unwrap()
                .map(|p| p.to_string_lossy().into_owned());
            let mut data = Vec::new();
            entry.read_to_end(&mut data).unwrap();
            seen.push((path, mode, link, data));
        }

        let paths: Vec<&str> = seen.iter().map(|(p, _, _, _)| p.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "etc/config/network",
                "etc/shadow",
                "etc/localtime",
                long_path.as_str(),
                "var/log/messages",
            ]
        );
        assert_eq!(seen[1].1, 0o600);
        assert_eq!(seen[2].2.as_deref(), Some("/tmp/localtime"));
        assert_eq!(seen[3].3, b"LONG-CERT");
        assert_eq!(seen[4].3, b"syslog line\n");
    }

    #[test]
    fn strip_activity_db_returns_unchanged_when_absent() {
        let archive = build_tar_gz(&[("etc/config/system", 0o644, None, b"config system\n")]);
        assert_eq!(
            &*strip_activity_db(Zeroizing::new(archive.clone())).unwrap(),
            &archive
        );
    }

    fn gzip_bytes(raw: &[u8]) -> Vec<u8> {
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        enc.write_all(raw).unwrap();
        enc.finish().unwrap()
    }

    fn append_raw_member(out: &mut Vec<u8>, name: &str, ty: tar::EntryType, data: &[u8]) {
        let mut header = tar::Header::new_ustar();
        header.as_old_mut().name[..name.len()].copy_from_slice(name.as_bytes());
        header.set_mode(0o644);
        header.set_entry_type(ty);
        header.set_size(data.len() as u64);
        header.set_cksum();
        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(data);
        let rem = data.len() % 512;
        if rem != 0 {
            out.resize(out.len() + (512 - rem), 0);
        }
    }

    #[test]
    fn strip_activity_db_preserves_gnu_sparse_entries() {
        let mut raw = Vec::new();
        append_raw_member(
            &mut raw,
            "etc/config/before",
            tar::EntryType::Regular,
            b"before",
        );

        let mut header = tar::Header::new_gnu();
        header.set_path("etc/sparse.bin").unwrap();
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::GNUSparse);
        header.set_size(5 * 512);
        {
            let gnu = header.as_gnu_mut().unwrap();
            for i in 0..4 {
                gnu.sparse[i].set_offset((i as u64) * 1024);
                gnu.sparse[i].set_length(512);
            }
            gnu.set_is_extended(true);
            gnu.set_real_size(4608);
        }
        header.set_cksum();
        raw.extend_from_slice(header.as_bytes());

        let mut ext = tar::GnuExtSparseHeader::new();
        ext.sparse_mut()[0].set_offset(4096);
        ext.sparse_mut()[0].set_length(512);
        ext.set_is_extended(false);
        raw.extend_from_slice(ext.as_bytes());
        for chunk in 0u8..5 {
            raw.extend_from_slice(&[b'A' + chunk; 512]);
        }

        append_raw_member(
            &mut raw,
            "var/lib/startwrt/activity.db",
            tar::EntryType::Regular,
            b"stale-db",
        );
        append_raw_member(
            &mut raw,
            "etc/config/after",
            tar::EntryType::Regular,
            b"after",
        );
        raw.resize(raw.len() + 1024, 0);

        let stripped = strip_activity_db(Zeroizing::new(gzip_bytes(&raw))).unwrap();
        let mut tar = tar::Archive::new(MultiGzDecoder::new(stripped.as_slice()));
        let mut seen = Vec::new();
        for entry in tar.entries().unwrap() {
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap().to_string_lossy().into_owned();
            let mut data = Vec::new();
            entry.read_to_end(&mut data).unwrap();
            seen.push((path, data));
        }

        assert_eq!(seen.len(), 3);
        assert_eq!(
            seen[0],
            ("etc/config/before".to_string(), b"before".to_vec())
        );
        assert_eq!(seen[1].0, "etc/sparse.bin");
        assert_eq!(seen[1].1.len(), 4608);
        assert_eq!(&seen[1].1[0..512], &[b'A'; 512]);
        assert_eq!(&seen[1].1[512..1024], &[0u8; 512]);
        assert_eq!(&seen[1].1[4096..4608], &[b'E'; 512]);
        assert_eq!(seen[2], ("etc/config/after".to_string(), b"after".to_vec()));
    }

    #[tokio::test]
    async fn prepare_restore_archive_validates_and_cleans_up_invalid_archive() {
        let tmp_dir = tempfile::tempdir().unwrap();

        let archive = build_tar_gz(&[(
            "etc/config/system",
            0o644,
            None,
            b"config system 'restored'\n",
        )]);
        let ok_path = tmp_dir.path().join("valid-restore.tar.gz");
        let listing = prepare_restore_archive(&ok_path, Zeroizing::new(archive))
            .await
            .unwrap();
        assert!(listing.contains("etc/config/system"));
        assert!(ok_path.exists());

        let mut enc = GzEncoder::new(Vec::new(), Compression::fast());
        let chunk = vec![0u8; 1 << 20];
        for _ in 0..(MAX_BACKUP_BYTES >> 20) {
            enc.write_all(&chunk).unwrap();
        }
        enc.write_all(&[0u8]).unwrap();
        let oversized = enc.finish().unwrap();

        let err = strip_activity_db(Zeroizing::new(oversized.clone())).unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidRequest);
        assert!(err
            .to_string()
            .contains("Backup archive exceeds 256 MiB limit"));

        for (name, bytes, expected_msg) in [
            (
                "not-gzip.tar.gz",
                b"not a tar.gz".to_vec(),
                "Invalid backup archive",
            ),
            ("empty.tar.gz", gzip_bytes(b""), "Invalid backup archive"),
            (
                "non-tar.tar.gz",
                gzip_bytes(&[b'x'; 1024]),
                "Invalid backup archive",
            ),
            (
                "oversized.tar.gz",
                oversized,
                "Backup archive exceeds 256 MiB limit",
            ),
        ] {
            let bad_path = tmp_dir.path().join(name);
            let err = prepare_restore_archive(&bad_path, Zeroizing::new(bytes))
                .await
                .unwrap_err();
            assert_eq!(err.kind, ErrorKind::InvalidRequest, "case {name}");
            assert!(
                err.to_string().contains(expected_msg),
                "case {name}: unexpected error {err}"
            );
            assert!(!bad_path.exists(), "case {name} left temp file behind");
        }
    }

    #[tokio::test]
    async fn apply_restore_preserving_activity_handles_snapshot_apply_and_restore_failures() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let rel = crate::activity::ACTIVITY_DB.trim_start_matches('/');

        // (a) snapshot fails (DB path is a directory): apply never runs, tmp_path removed, Err returned.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(rel)).unwrap();
        let tmp = root.join("restore-a.tar.gz");
        std::fs::write(&tmp, b"archive").unwrap();
        let called = AtomicBool::new(false);
        let err = apply_restore_preserving_activity(root, &tmp, || {
            called.store(true, Ordering::SeqCst);
            async { Ok(()) }
        })
        .await
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::Filesystem);
        assert!(!called.load(Ordering::SeqCst));
        assert!(!tmp.exists());
        crate::activity::list(ServerContext::default(), Default::default())
            .await
            .unwrap();

        // (b) apply fails after overwriting the DB: original DB is restored, tmp_path removed, apply Err returned.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let db_path = root.join(rel);
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        std::fs::write(&db_path, b"original-db").unwrap();
        let tmp = root.join("restore-b.tar.gz");
        std::fs::write(&tmp, b"archive").unwrap();
        let err = apply_restore_preserving_activity(root, &tmp, || async {
            std::fs::write(&db_path, b"overwritten-db").unwrap();
            Err(Error::new(
                eyre!("sysupgrade failed"),
                ErrorKind::Filesystem,
            ))
        })
        .await
        .unwrap_err();
        assert!(err.to_string().contains("sysupgrade failed"));
        assert!(!tmp.exists());
        assert_eq!(std::fs::read(&db_path).unwrap(), b"original-db");
        crate::activity::list(ServerContext::default(), Default::default())
            .await
            .unwrap();

        // (c) apply succeeds but restore fails (DB replaced by non-empty dir): logs error, returns Ok(()), and releases quiesce.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let db_path = root.join(rel);
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        std::fs::write(&db_path, b"original-db").unwrap();
        let tmp = root.join("restore-c.tar.gz");
        std::fs::write(&tmp, b"archive").unwrap();
        apply_restore_preserving_activity(root, &tmp, || async {
            assert!(
                crate::activity::list(ServerContext::default(), Default::default())
                    .await
                    .is_err()
            );
            std::fs::remove_file(&db_path).unwrap();
            std::fs::create_dir_all(db_path.join("blocker")).unwrap();
            std::fs::write(db_path.join("blocker/file"), b"x").unwrap();
            Ok(())
        })
        .await
        .unwrap();
        assert!(db_path.is_dir());
        assert!(!tmp.exists());
        crate::activity::list(ServerContext::default(), Default::default())
            .await
            .unwrap();
    }
}
