mod redact;

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{header, Response};
use flate2::write::GzEncoder;
use flate2::Compression;
use imbl_value::imbl::OrdMap;
use imbl_value::json;
use itertools::Itertools;
use rpc_toolkit::{
    from_fn_async, from_fn_async_local, CallRemote, Context, Empty, HandlerArgs, HandlerExt,
    ParentHandler,
};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::continuations::{self, Guid, RpcContinuation};
use crate::invoke::Invoke;
use crate::prelude::*;
use crate::{CliContext, ServerContext};

#[derive(Debug, Serialize, Deserialize, visit_rs::VisitFields)]
pub struct DiagnosticsCreateRes {
    pub guid: Guid,
    pub filename: String,
}
rpc_toolkit::reflect_ts!(DiagnosticsCreateRes);

#[instrument(skip_all)]
async fn create(ctx: ServerContext) -> Result<DiagnosticsCreateRes, Error> {
    let date = chrono::Utc::now().format("%Y-%m-%d");
    let filename = format!("diagnostics-startwrt-{date}.log.gz");

    let body = match report(ctx.clone()).await {
        Ok(body) => body,
        Err(e) => {
            tracing::warn!("failed to build diagnostics: {e}");
            crate::activity::log(
                "diagnostics",
                "downloaded",
                false,
                "Failed to read diagnostics log",
                Some(&e.to_string()),
            );
            return Err(e);
        }
    };

    crate::activity::log(
        "diagnostics",
        "downloaded",
        true,
        "Downloaded support diagnostics",
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
        )
        .await;

    Ok(DiagnosticsCreateRes { guid, filename })
}

async fn report(ctx: ServerContext) -> Result<Vec<u8>, Error> {
    let header = header().await;

    let history = crate::logs::read_history(Path::new("/")).await?;
    let (log_title, log) = if history.is_empty() {
        (
            "System log (current boot)",
            Command::new("logread")
                .invoke(ErrorKind::Filesystem.into())
                .await?,
        )
    } else {
        ("System log", history)
    };

    let activity = crate::activity::list(
        ctx.clone(),
        crate::activity::ActivityListParams {
            offset: None,
            limit: Some(crate::activity::MAX_ENTRIES),
        },
    )
    .await?;

    let devices = crate::devices::list(ctx).await.unwrap_or_else(|e| {
        tracing::warn!("diagnostics: device list unavailable: {e}");
        Vec::new()
    });
    let cached = crate::device_names::load_all();
    let mut known = redact::Known::read(Path::new("/etc/config")).await;
    known.add_macs(
        devices
            .iter()
            .filter_map(|d| d.mac.as_deref())
            .chain(cached.keys().map(String::as_str)),
    );
    let redactor = redact::Redactor::new(known, rand::random());

    tokio::task::spawn_blocking(move || {
        let mut text = header.into_bytes();
        text.extend_from_slice(redact::NOTICE.as_bytes());

        text.extend(format!("\n== {log_title} ==\n").into_bytes());
        text.extend(redactor.log(&log));

        text.extend_from_slice(b"\n== Activity ==\n");
        for entry in activity.entries.iter().rev() {
            let status = if entry.success { "OK" } else { "FAIL" };
            let error = entry
                .error
                .as_deref()
                .map(|e| format!(": {e}"))
                .unwrap_or_default();
            let line = format!(
                "{} {}.{} {status} {}{error}",
                entry.timestamp, entry.category, entry.action, entry.summary
            );
            text.extend(redactor.line(&line, true).into_bytes());
            text.push(b'\n');
        }

        text.extend_from_slice(b"\n== Devices ==\n");
        for device in &devices {
            if let Some(row) = device_row(&redactor, device, &cached) {
                text.extend(row.into_bytes());
            }
        }

        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        gz.write_all(&text)?;
        gz.finish()
    })
    .await
    .map_err(|e| Error::new(eyre!("{e}"), ErrorKind::Unknown))?
    .map_err(Error::from)
}

/// Ties a device's tokens to what it is, without its name or address.
fn device_row(
    redactor: &redact::Redactor,
    device: &crate::devices::Device,
    cached: &HashMap<String, crate::device_names::CachedName>,
) -> Option<String> {
    let mac = device.mac.as_deref()?;
    let tag = redactor.mac(mac)?;
    let kind = cached
        .get(&mac.to_uppercase())
        .and_then(|c| c.fingerprint.as_ref())
        .and_then(crate::device_ident::fingerprint_os)
        .or_else(|| crate::device_ident::oui_vendor(mac))
        .unwrap_or("unidentified");
    let connection = match device.connection.as_deref() {
        Some(c) if c.starts_with("VPN ") => "VPN",
        Some(c) => c,
        None => "none",
    };
    let names = [device.custom_name.as_deref(), device.hostname.as_deref()]
        .into_iter()
        .flatten()
        .filter(|name| !name.is_empty())
        .map(|name| redactor.name(name))
        .unique()
        .join(", ");
    Some(format!(
        "{tag} {kind}; connection {connection}; network {}; names {}\n",
        device.profile_network.as_deref().unwrap_or("none"),
        if names.is_empty() { "none" } else { &names },
    ))
}

async fn header() -> String {
    let mut header = format!(
        "StartWRT support diagnostics\n\
         Generated: {}\n\
         Version: {} ({})\n\
         Board: {} ({})\n\
         Kernel: {}\n\
         Uptime: {} s\n\
         Boot ID: {}\n",
        chrono::Utc::now().to_rfc3339(),
        env!("CARGO_PKG_VERSION"),
        env!("STARTWRT_GIT_HASH"),
        read_trimmed("/tmp/sysinfo/model").await,
        read_trimmed("/tmp/sysinfo/board_name").await,
        read_trimmed("/proc/sys/kernel/osrelease").await,
        read_trimmed("/proc/uptime")
            .await
            .split(['.', ' '])
            .next()
            .unwrap_or_default(),
        read_trimmed("/proc/sys/kernel/random/boot_id").await,
    );
    for (disk, wear) in storage_wear().await {
        header.push_str(&format!("Storage {disk}: {wear}\n"));
    }
    header
}

/// eMMC lifetime estimates; other storage reports none.
async fn storage_wear() -> Vec<(String, String)> {
    let mut wear = Vec::new();
    let Ok(mut disks) = tokio::fs::read_dir("/sys/block").await else {
        return wear;
    };
    while let Ok(Some(disk)) = disks.next_entry().await {
        let device = disk.path().join("device");
        let Ok(life_time) = tokio::fs::read_to_string(device.join("life_time")).await else {
            continue;
        };
        let name = disk.file_name().to_string_lossy().into_owned();
        if name.contains("boot") || name.contains("rpmb") {
            continue;
        }
        wear.push((
            format!(
                "{name} ({})",
                read_trimmed(&device.join("name").to_string_lossy()).await
            ),
            format!(
                "life_time {}, pre_eol_info {}",
                life_time.trim(),
                read_trimmed(&device.join("pre_eol_info").to_string_lossy()).await
            ),
        ));
    }
    wear.sort();
    wear
}

async fn read_trimmed(path: &str) -> String {
    tokio::fs::read_to_string(path)
        .await
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
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
    let res: DiagnosticsCreateRes = imbl_value::from_value(
        ctx.call_remote(
            &parent_method.into_iter().chain(method).join("."),
            OrdMap::new(),
            json!({}),
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
        Error::new(
            eyre!("Failed to write {}: {e}", path.display()),
            ErrorKind::Filesystem,
        )
    })?;

    println!("{}", path.display());
    Ok(())
}

pub fn diagnostics<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand("create", from_fn_async_local(create).no_cli())
        .subcommand(
            "create",
            from_fn_async(cli_download)
                .no_display()
                .with_about("Download support diagnostics"),
        )
}
