use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Response, header};
use imbl_value::imbl::OrdMap;
use imbl_value::json;
use itertools::Itertools;
use rpc_toolkit::{from_fn_async, CallRemote, Context, Empty, HandlerArgs, HandlerExt, ParentHandler};
use serde::{Deserialize, Serialize};

use crate::continuations::{self, Guid, RpcContinuation};
use crate::prelude::*;
use crate::{CliContext, ServerContext};

#[derive(Debug, Serialize, Deserialize)]
pub struct DiagnosticsCreateRes {
    pub guid: Guid,
    pub filename: String,
}

/// RPC handler: read syslog, register download continuation, return guid + filename.
#[instrument(skip_all)]
async fn create(ctx: ServerContext) -> Result<DiagnosticsCreateRes, Error> {
    let date = chrono::Utc::now().format("%Y-%m-%d");
    let filename = format!("diagnostics-startwrt-{date}.log");

    let output = match tokio::process::Command::new("logread").output().await {
        Ok(o) if o.status.success() => o,
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            tracing::warn!("logread failed: {stderr}");
            crate::activity::log(
                "diagnostics",
                "downloaded",
                false,
                "Failed to read diagnostics log",
                Some(&format!("logread exited with {}: {stderr}", o.status)),
            );
            return Err(Error::new(eyre!("logread failed: {stderr}"), ErrorKind::Filesystem));
        }
        Err(e) => {
            tracing::warn!("failed to run logread: {e}");
            crate::activity::log(
                "diagnostics",
                "downloaded",
                false,
                "Failed to read diagnostics log",
                Some(&e.to_string()),
            );
            return Err(Error::new(eyre!("Failed to run logread: {e}"), ErrorKind::Filesystem));
        }
    };
    let body = output.stdout;

    crate::activity::log(
        "diagnostics",
        "downloaded",
        true,
        "Downloaded support diagnostics",
        None,
    );

    let fname = filename.clone();
    let guid = Guid::new();
    ctx.continuations.add(
        guid.clone(),
        RpcContinuation::rest(
            move |_req| async move {
                Ok(Response::builder()
                    .header(header::CONTENT_TYPE, "text/plain")
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

    Ok(DiagnosticsCreateRes { guid, filename })
}

/// CLI handler: call diagnostics.create via RPC, download the file, write to ~/Downloads.
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
    .map_err(|e| Error::new(eyre!("Failed to parse response: {e}"), ErrorKind::Deserialization))?;

    let (bytes, _) = ctx.rest_download(res.guid.as_ref()).await?;

    let download_dir = dirs::download_dir()
        .unwrap_or_else(|| PathBuf::from("."));
    let path = download_dir.join(&res.filename);
    tokio::fs::write(&path, &bytes).await.map_err(|e| {
        Error::new(eyre!("Failed to write {}: {e}", path.display()), ErrorKind::Filesystem)
    })?;

    println!("{}", path.display());
    Ok(())
}

pub fn diagnostics<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand("create", from_fn_async(create).no_cli())
        .subcommand(
            "create",
            from_fn_async(cli_download)
                .no_display()
                .with_about("Download support diagnostics"),
        )
}
