//! Authenticated initramfs preparation and its private boot-lifetime handoff.

use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::boot::Descriptor;
use super::credentials::Credentials;
use super::memory::InitramfsMemory;
use super::root::RootComposition;
use super::tpm::MeasurementProfile;
use crate::disk::mount::filesystem::ReadWrite;
use crate::disk::mount::filesystem::private_tmpfs::PrivateTmpfs;
use crate::disk::mount::guard::MountGuard;
use crate::prelude::*;
use crate::util::io::{TmpDir, read_file_to_string, read_secret_file, write_file_private_atomic};

pub const DESCRIPTOR_FILE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../projects/start-os/build/lib/scripts/fde-descriptor-file"
))
.trim_ascii();
pub const RUNTIME: &str = "/run/startos-fde";
pub const HANDOFF_FILE: &str = "handoff.json";
pub(crate) const CONFIGURATION_FILE: &str = "configuration.json";
const BOOT_MODE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../projects/start-os/build/lib/scripts/fde-boot-mode"
));

/// Selects the workflow; protected execution still requires the attested handoff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BootMode {
    Ordinary,
    Protected,
}

impl BootMode {
    pub async fn current() -> Result<Self, Error> {
        #[cfg(target_os = "linux")]
        {
            Self::from_command_line(&read_file_to_string("/proc/cmdline").await?)
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(Self::Ordinary)
        }
    }

    fn from_command_line(line: &str) -> Result<Self, Error> {
        let parameters: Vec<_> = line
            .split_ascii_whitespace()
            .filter_map(|part| part.strip_prefix("BOOT="))
            .collect();
        if !parameters.contains(&BOOT_MODE.trim()) {
            return Ok(Self::Ordinary);
        }
        if parameters.len() != 1 {
            return Err(invalid_handoff());
        }
        Ok(Self::Protected)
    }
}

pub(crate) fn invalid_handoff() -> Error {
    Error::new(
        eyre!(t!("fde.boot.invalid-descriptor")),
        ErrorKind::InvalidSignature,
    )
}

async fn runtime_file(name: &str) -> Result<tokio::fs::File, Error> {
    let file = crate::os_overlay::regular_file(&Path::new(RUNTIME).join(name)).await?;
    let metadata = file.metadata().await?;
    if metadata.uid() != 0 || metadata.mode() & 0o077 != 0 {
        return Err(invalid_handoff());
    }
    Ok(file)
}

pub(crate) async fn load_handoff() -> Result<BootHandoff, Error> {
    let memory = InitramfsMemory::protect(Path::new(RUNTIME))?;
    let bytes = read_secret_file(runtime_file(HANDOFF_FILE).await?).await?;
    let handoff = serde_json::from_slice(&bytes).with_kind(ErrorKind::Deserialization)?;
    memory.check(Path::new(RUNTIME))?;
    Ok(handoff)
}

pub(crate) async fn load_configuration() -> Result<BootConfiguration, Error> {
    super::memory::require_tmpfs(Path::new(RUNTIME))?;
    let bytes = read_secret_file(runtime_file(CONFIGURATION_FILE).await?).await?;
    Ok(serde_json::from_slice(&bytes).with_kind(ErrorKind::Deserialization)?)
}

pub(crate) async fn finish_handoff() -> Result<(), Error> {
    crate::util::io::delete_file(Path::new(RUNTIME).join(HANDOFF_FILE)).await
}

/// Private credentials removed after authoritative data-drive handoff.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BootHandoff {
    pub credentials: Credentials,
}

/// Attested boot inputs retained in private RAM across daemon restarts.
/// These references do not establish durable EFI publication or active protection.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BootConfiguration {
    pub rootfs_image: PathBuf,
    pub rootfs: PathBuf,
    pub overlay: PathBuf,
    pub working_upper: PathBuf,
    pub rootfs_sha256: [u8; 32],
    pub descriptor: Descriptor,
    pub profile: MeasurementProfile,
}

async fn prepare_mounted(
    volume: &Path,
    root: &Path,
    runtime: &Path,
) -> Result<(RootComposition, BootHandoff, BootConfiguration), Error> {
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
            let configuration = BootConfiguration {
                rootfs_image: mounted.verified().rootfs_image().to_owned(),
                rootfs: mounted.verified().rootfs().to_owned(),
                overlay: mounted.verified().overlay().to_owned(),
                working_upper: mounted.working_upper().to_owned(),
                rootfs_sha256: mounted.verified().generation().manifest().rootfs_sha256,
                descriptor,
                profile: mounted.verified().generation().profile().clone(),
            };
            Ok((mounted, BootHandoff { credentials }, configuration))
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
        (Ok((mounted, handoff, configuration)), Ok(())) => {
            let published = async {
                write_file_private_atomic(
                    runtime.join(CONFIGURATION_FILE),
                    serde_json::to_vec(&configuration).with_kind(ErrorKind::Serialization)?,
                )
                .await?;
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
        (Ok((mounted, _, _)), Err(error)) => {
            mounted.unmount().await.log_err();
            Err(error)
        }
        (Err(error), _) => Err(error),
    };
    ram.unmount(false).await.log_err();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_mode_requires_the_authenticated_boot_parameter() {
        for line in ["", "root=LABEL=startos ro", "BOOT=local root=LABEL=startos"] {
            assert_eq!(
                BootMode::from_command_line(line).unwrap(),
                BootMode::Ordinary
            );
        }
        assert_eq!(
            BootMode::from_command_line(&format!(
                "root=LABEL=startos BOOT={} ro",
                BOOT_MODE.trim()
            ))
            .unwrap(),
            BootMode::Protected
        );
    }

    #[test]
    fn conflicting_boot_parameters_cannot_fall_back_to_ordinary_boot() {
        for line in [
            format!("BOOT=local BOOT={}", BOOT_MODE.trim()),
            format!("BOOT={} BOOT=local", BOOT_MODE.trim()),
            format!("BOOT={0} BOOT={0}", BOOT_MODE.trim()),
        ] {
            assert_eq!(
                BootMode::from_command_line(&line).unwrap_err().kind,
                ErrorKind::InvalidSignature
            );
        }
    }
}
