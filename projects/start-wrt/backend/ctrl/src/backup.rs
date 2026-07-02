use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Response, header};
use clap::Parser;
use imbl_value::imbl::OrdMap;
use imbl_value::json;
use itertools::Itertools;
use rpc_toolkit::{from_fn_async, CallRemote, Context, Empty, HandlerArgs, HandlerExt, ParentHandler};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::continuations::{self, Guid, RpcContinuation};
use crate::invoke::Invoke;
use crate::prelude::*;
use crate::{CliContext, ServerContext};

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupCreateRes {
    pub guid: Guid,
    pub filename: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupRestoreRes {
    pub upload: Guid,
}

/// RPC handler: buffer backup, register download continuation, return guid + filename.
#[instrument(skip_all)]
async fn create(ctx: ServerContext) -> Result<BackupCreateRes, Error> {
    // Get hostname for the filename
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
    let filename = format!("backup-{hostname}-{date}.tar.gz");

    let output = match Command::new("sysupgrade")
        .args(["--create-backup", "-"])
        .invoke(ErrorKind::Filesystem.into())
        .await
    {
        Ok(output) => output,
        Err(e) => {
            crate::activity::log("backup", "downloaded", false, "Failed to create config backup", Some(&e.to_string()));
            return Err(e.into());
        }
    };

    crate::activity::log("backup", "downloaded", true, "Downloaded config backup", None);

    let body = output;
    let fname = filename.clone();
    let guid = Guid::new();
    ctx.continuations.add(
        guid.clone(),
        RpcContinuation::rest(
            move |_req| async move {
                Ok(Response::builder()
                    .header(header::CONTENT_TYPE, "application/gzip")
                    .header(
                        header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{fname}\""),
                    )
                    .body(Body::from(body))
                    .unwrap())
            },
            continuations::DEFAULT_TTL,
        ),
    );

    Ok(BackupCreateRes { guid, filename })
}

/// CLI handler: call backup.create via RPC, download the file, write to ~/Downloads.
#[instrument(skip_all)]
async fn cli_download(
    HandlerArgs {
        context: ctx,
        parent_method,
        method,
        ..
    }: HandlerArgs<CliContext>,
) -> Result<(), Error> {
    let res: BackupCreateRes = imbl_value::from_value(
        ctx.call_remote(
            &parent_method.into_iter().chain(method).join("."),
            OrdMap::new(),
            json!({}),
            Empty {},
        )
        .await?,
    )
    .map_err(|e| Error::new(eyre!("Failed to parse response: {e}"), ErrorKind::Deserialization))?;

    let (bytes, _) = ctx.rest_download(res.guid.as_ref()).await?;

    let download_dir = dirs::download_dir()
        .unwrap_or_else(|| PathBuf::from("."));
    let path = download_dir.join(&res.filename);
    tokio::fs::write(&path, &bytes).await.map_err(|e| {
        crate::Error::new(eyre!("Failed to write {}: {e}", path.display()), ErrorKind::Filesystem)
    })?;

    println!("{}", path.display());
    Ok(())
}

/// RPC handler: register upload continuation for restore, return guid.
#[instrument(skip_all)]
async fn restore(ctx: ServerContext) -> Result<BackupRestoreRes, Error> {
    let guid = Guid::new();
    ctx.continuations.add(
        guid.clone(),
        RpcContinuation::rest(
            move |req| async move {
                let body_bytes = axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024)
                    .await
                    .map_err(|e| Error::new(eyre!("Failed to read upload: {e}"), ErrorKind::Network))?;

                // Atomic file write
                let tmp_path = "/tmp/backup-restore.tar.gz";
                startos::util::io::write_file_atomic(tmp_path, &body_bytes)
                    .await
                    .map_err(Error::from)?;

                // Validate tar.gz
                if let Err(_) = Command::new("tar")
                    .args(["-tzf", tmp_path])
                    .capture(false)
                    .invoke(ErrorKind::Filesystem.into())
                    .await
                {
                    let _ = tokio::fs::remove_file(tmp_path).await;
                    crate::activity::log("backup", "restored", false, "Failed to restore config backup", Some("Invalid backup archive"));
                    return Err(Error::new(eyre!("Invalid backup archive"), ErrorKind::InvalidRequest));
                }

                // Apply the backup
                let apply = Command::new("sysupgrade")
                    .args(["--restore-backup", tmp_path])
                    .invoke(ErrorKind::Filesystem.into())
                    .await;

                let _ = tokio::fs::remove_file(tmp_path).await;

                if let Err(e) = apply {
                    crate::activity::log("backup", "restored", false, "Failed to restore config backup", Some(&e.to_string()));
                    return Err(e.into());
                }

                crate::activity::log("backup", "restored", true, "Restored config backup (rebooting)", None);

                // Spawn delayed reboot
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
    );
    Ok(BackupRestoreRes { upload: guid })
}

#[derive(Debug, Deserialize, Serialize, Parser)]
struct RestoreParams {
    /// Path to the backup file
    file: PathBuf,
}

/// CLI handler: read local file, call backup.restore via RPC, upload the file.
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
        crate::Error::new(eyre!("Failed to read {}: {e}", file.display()), ErrorKind::Filesystem)
    })?;

    let res: BackupRestoreRes = imbl_value::from_value(
        ctx.call_remote(
            &parent_method.into_iter().chain(method).join("."),
            OrdMap::new(),
            json!({}),
            Empty {},
        )
        .await?,
    )
    .map_err(|e| Error::new(eyre!("Failed to parse response: {e}"), ErrorKind::Deserialization))?;

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
                .no_display()
                .with_about("Download a config backup"),
        )
        .subcommand("restore", from_fn_async(restore).no_cli())
        .subcommand(
            "restore",
            from_fn_async(cli_upload)
                .no_display()
                .with_about("Restore config backup from file"),
        )
}
