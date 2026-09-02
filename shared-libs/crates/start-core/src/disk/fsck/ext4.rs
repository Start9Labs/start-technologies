use std::ffi::OsStr;
use std::path::Path;

use color_eyre::eyre::eyre;
use futures::FutureExt;
use futures::future::BoxFuture;
use rust_i18n::t;
use tokio::process::Command;
use tracing::instrument;

use crate::Error;
use crate::disk::fsck::RequiresReboot;

#[instrument(skip_all)]
pub async fn e2fsck_preen(
    logicalname: impl AsRef<Path> + std::fmt::Debug,
) -> Result<RequiresReboot, Error> {
    e2fsck_runner(Command::new("e2fsck").arg("-p"), logicalname, false).await
}

pub async fn e2fsck_preen_strict(
    logicalname: impl AsRef<Path> + std::fmt::Debug,
) -> Result<RequiresReboot, Error> {
    e2fsck_runner(Command::new("e2fsck").arg("-p"), logicalname, true).await
}

fn backup_existing_undo_file<'a>(path: &'a Path) -> BoxFuture<'a, Result<(), Error>> {
    async move {
        if tokio::fs::metadata(path).await.is_ok() {
            let bak = path.with_extension(format!(
                "{}.bak",
                path.extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
            ));
            backup_existing_undo_file(&bak).await?;
            tokio::fs::rename(path, &bak).await?;
        }
        Ok(())
    }
    .boxed()
}

#[instrument(skip_all)]
pub async fn e2fsck_aggressive(
    logicalname: impl AsRef<Path> + std::fmt::Debug,
) -> Result<RequiresReboot, Error> {
    let undo_path = Path::new("/media/startos/config")
        .join(
            logicalname
                .as_ref()
                .file_name()
                .unwrap_or(OsStr::new("unknown")),
        )
        .with_extension("e2undo");
    backup_existing_undo_file(&undo_path).await?;
    e2fsck_runner(
        Command::new("e2fsck").arg("-y").arg("-z").arg(undo_path),
        logicalname,
        false,
    )
    .await
}

async fn e2fsck_runner(
    e2fsck_cmd: &mut Command,
    logicalname: impl AsRef<Path> + std::fmt::Debug,
    fail_on_uncorrected: bool,
) -> Result<RequiresReboot, Error> {
    let e2fsck_out = e2fsck_cmd.arg(logicalname.as_ref()).output().await?;
    let e2fsck_stderr = String::from_utf8(e2fsck_out.stderr)?;
    let code = e2fsck_out.status.code().ok_or_else(|| {
        Error::new(
            eyre!("{}", t!("disk.fsck.process-terminated-by-signal")),
            crate::ErrorKind::DiskManagement,
        )
    })?;
    if code & 4 != 0 {
        tracing::error!(
            "{}",
            t!(
                "disk.fsck.errors-not-corrected",
                device = logicalname.as_ref().display(),
                stderr = e2fsck_stderr
            ),
        );
    } else if code & 1 != 0 {
        tracing::warn!(
            "{}",
            t!(
                "disk.fsck.errors-corrected",
                device = logicalname.as_ref().display(),
                stderr = e2fsck_stderr
            ),
        );
    }
    if fail_on_uncorrected && code & 4 != 0 {
        return Err(Error::new(
            eyre!("{}", t!("disk.fsck.e2fsck-error", stderr = e2fsck_stderr)),
            crate::ErrorKind::DiskManagement,
        ));
    }
    if code < 8 {
        if code & 2 != 0 {
            tracing::warn!("{}", t!("disk.fsck.reboot-required"));
            Ok(RequiresReboot(true))
        } else {
            Ok(RequiresReboot(false))
        }
    } else {
        Err(Error::new(
            eyre!("{}", t!("disk.fsck.e2fsck-error", stderr = e2fsck_stderr)),
            crate::ErrorKind::DiskManagement,
        ))
    }
}
