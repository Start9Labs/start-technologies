//! Called by the authenticated FDE initramfs, never selected by public disk metadata.

use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::boot::Descriptor;
use super::credentials::Credentials;
use super::memory::InitramfsMemory;
use super::root::RootComposition;
use crate::disk::mount::filesystem::ReadWrite;
use crate::disk::mount::filesystem::private_tmpfs::PrivateTmpfs;
use crate::disk::mount::guard::MountGuard;
use crate::prelude::*;
use crate::util::io::{TmpDir, read_file_to_string, write_file_private_atomic};

pub const DESCRIPTOR_FILE: &str = "/etc/startos/fde-boot.json";
pub const RUNTIME: &str = "/run/startos-fde";
pub const HANDOFF_FILE: &str = "handoff.json";

/// Private, unswappable boot-lifetime state, removed after authoritative data-drive handoff.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BootHandoff {
    pub rootfs: PathBuf,
    pub overlay: PathBuf,
    pub working_upper: PathBuf,
    pub credentials: Credentials,
}

async fn prepare_mounted(
    volume: &Path,
    root: &Path,
    runtime: &Path,
) -> Result<(RootComposition, BootHandoff), Error> {
    let images = volume.join("images");
    let config = volume.join("config");
    let memory = InitramfsMemory::protect(runtime)?;
    let descriptor: Descriptor = serde_json::from_str(&read_file_to_string(DESCRIPTOR_FILE).await?)
        .with_kind(ErrorKind::Deserialization)?;
    let mounted = descriptor
        .mount(&images, &config, runtime)
        .await?
        .ok_or_else(|| {
            Error::new(
                eyre!(t!("fde.boot.invalid-descriptor")),
                ErrorKind::InvalidSignature,
            )
        })?;
    let mounted = RootComposition::mount(mounted, volume, &images, &config, root, runtime).await?;
    match mounted.verified().release_bootstrap(&memory, runtime).await {
        Ok(credentials) => {
            let handoff = BootHandoff {
                rootfs: mounted.verified().rootfs().to_owned(),
                overlay: mounted.verified().overlay().to_owned(),
                working_upper: mounted.working_upper().to_owned(),
                credentials,
            };
            Ok((mounted, handoff))
        }
        Err(error) => {
            mounted.unmount().await.log_err();
            Err(error)
        }
    }
}

/// Composes the running and working roots before publishing bootstrap identities.
/// The FDE initramfs must terminate on error without entering an interactive shell.
pub async fn prepare(volume: &Path, root: &Path) -> Result<(), Error> {
    let runtime = Path::new(RUNTIME);
    std::fs::DirBuilder::new().mode(0o700).create(runtime)?;
    let ram = MountGuard::mount(&PrivateTmpfs::new(), runtime, ReadWrite).await?;
    let workspace = match MountGuard::mount(&PrivateTmpfs::new(), TmpDir::BASE, ReadWrite).await {
        Ok(workspace) => workspace,
        Err(error) => {
            ram.unmount(false).await.log_err();
            return Err(error);
        }
    };
    let prepared = prepare_mounted(volume, root, runtime).await;
    let workspace_result = workspace.unmount(false).await;
    let result = match (prepared, workspace_result) {
        (Ok((mounted, handoff)), Ok(())) => {
            let published = async {
                let bytes = Zeroizing::new(
                    serde_json::to_vec(&handoff).with_kind(ErrorKind::Serialization)?,
                );
                write_file_private_atomic(runtime.join(HANDOFF_FILE), &*bytes).await
            }
            .await;
            match published {
                Ok(()) => {
                    mounted.persist();
                    ram.persist();
                    return Ok(());
                }
                Err(error) => {
                    mounted.unmount().await.log_err();
                    Err(error)
                }
            }
        }
        (Ok((mounted, _)), Err(error)) => {
            mounted.unmount().await.log_err();
            Err(error)
        }
        (Err(error), _) => Err(error),
    };
    ram.unmount(false).await.log_err();
    result
}
