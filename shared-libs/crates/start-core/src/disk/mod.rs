use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use itertools::Itertools;
use lazy_format::lazy_format;
use rpc_toolkit::{CallRemoteHandler, Context, Empty, HandlerExt, ParentHandler, from_fn_async};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::context::{CliContext, RpcContext};
use crate::disk::util::{DiskInfo, get_mount_source};
use crate::prelude::*;
use crate::util::Invoke;
use crate::util::serde::{HandlerExtSerde, WithIoFormat, display_serializable};
use crate::{Error, ErrorKind};

pub mod fsck;
pub mod main;
pub mod mount;
pub mod util;

pub const BOOT_RW_PATH: &str = "/media/boot-rw";
pub const REPAIR_DISK_PATH: &str = "/media/startos/config/repair-disk";
pub const BACKUP_DIR_NAME: &str = "StartOSBackupsV2";
pub const LEGACY_BACKUP_DIR_NAME: &str = "StartOSBackups";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OsPartitionInfo {
    pub bios: Option<PathBuf>,
    pub boot: PathBuf,
    pub root: PathBuf,
    #[serde(default)]
    pub extra_boot: BTreeMap<String, PathBuf>,
    #[serde(skip)]
    pub data: Option<PathBuf>,
}
impl OsPartitionInfo {
    pub fn contains(&self, logicalname: impl AsRef<Path>) -> bool {
        let p = logicalname.as_ref();
        self.bios.as_deref() == Some(p)
            || p == &*self.boot
            || p == &*self.root
            || self.extra_boot.values().any(|v| v == p)
    }

    /// Build partition info by resolving the OS root device, parsing /etc/fstab
    /// for the boot partition(s), and discovering the BIOS boot partition
    /// (which is never mounted).
    pub async fn from_fstab() -> Result<Self, Error> {
        let fstab = tokio::fs::read_to_string("/etc/fstab")
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, "/etc/fstab"))?;

        let mut boot = None;
        let mut extra_boot = BTreeMap::new();

        for line in fstab.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split_whitespace();
            let Some(source) = fields.next() else {
                continue;
            };
            let Some(target) = fields.next() else {
                continue;
            };

            // The installed OS root comes from its live bind mount.
            if target != "/boot" && !target.starts_with("/boot/") {
                continue;
            }

            let dev = match resolve_fstab_source(source).await {
                Ok(d) => d,
                Err(FstabSourceError::Ignored(e)) => {
                    tracing::warn!("Failed to resolve fstab source {source}: {e}");
                    continue;
                }
                Err(FstabSourceError::Ambiguous(e)) => return Err(e),
            };

            match target {
                "/boot" => boot = Some(dev),
                t if t.starts_with("/boot/") => {
                    if let Some(name) = t.strip_prefix("/boot/") {
                        extra_boot.insert(name.to_string(), dev);
                    }
                }
                _ => {}
            }
        }

        let root = os_root_device().await.unwrap_or_default();

        let boot = boot.unwrap_or_default();
        let bios = if !boot.as_os_str().is_empty() {
            find_bios_boot_partition(&boot).await.ok().flatten()
        } else {
            None
        };

        Ok(Self {
            bios,
            boot,
            root,
            extra_boot,
            data: None,
        })
    }
}

const OS_ROOT_MOUNT: &str = "/media/startos/root";

// The live installer has no installed-root mount.
async fn os_root_device() -> Option<PathBuf> {
    get_mount_source(OS_ROOT_MOUNT).await.ok().flatten()
}

const BIOS_BOOT_TYPE_GUID: &str = "21686148-6449-6E6F-744E-656564454649";

/// Find the BIOS boot partition on the same disk as `known_part`.
async fn find_bios_boot_partition(known_part: &Path) -> Result<Option<PathBuf>, Error> {
    let output = Command::new("lsblk")
        .args(["-n", "-l", "-o", "NAME,PKNAME,PARTTYPE"])
        .arg(known_part)
        .invoke(ErrorKind::DiskManagement)
        .await?;
    let text = String::from_utf8(output)?;

    let parent_disk = text.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let _name = fields.next()?;
        let pkname = fields.next()?;
        (!pkname.is_empty()).then(|| pkname.to_string())
    });

    let Some(parent_disk) = parent_disk else {
        return Ok(None);
    };

    let output = Command::new("lsblk")
        .args(["-n", "-l", "-o", "NAME,PARTTYPE"])
        .arg(format!("/dev/{parent_disk}"))
        .invoke(ErrorKind::DiskManagement)
        .await?;
    let text = String::from_utf8(output)?;

    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(name) = fields.next() else { continue };
        let Some(parttype) = fields.next() else {
            continue;
        };
        if parttype.eq_ignore_ascii_case(BIOS_BOOT_TYPE_GUID) {
            return Ok(Some(PathBuf::from(format!("/dev/{name}"))));
        }
    }

    Ok(None)
}

enum FstabSourceError {
    Ignored(Error),
    Ambiguous(Error),
}

fn parse_blkid_devices(output: &str) -> Result<Option<PathBuf>, Vec<PathBuf>> {
    let devices = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect::<Vec<_>>();

    match devices.len() {
        0 => Ok(None),
        1 => Ok(devices.into_iter().next()),
        _ => Err(devices),
    }
}

/// Resolve an fstab device spec (e.g. /dev/sda1, PARTUUID=..., UUID=...) to a
/// canonical device path.
async fn resolve_fstab_source(source: &str) -> Result<PathBuf, FstabSourceError> {
    if source.starts_with('/') {
        return Ok(tokio::fs::canonicalize(source)
            .await
            .unwrap_or_else(|_| PathBuf::from(source)));
    }
    // Only TAG=value specs (PARTUUID=, UUID=, LABEL=) are resolvable via blkid;
    // pseudo sources (overlay, tmpfs, none, ...) are not block devices.
    if !source.contains('=') {
        return Err(FstabSourceError::Ignored(Error::new(
            eyre!("not a block device spec"),
            ErrorKind::DiskManagement,
        )));
    }
    let output = Command::new("blkid")
        .args(["-o", "device", "-t", source])
        .invoke(ErrorKind::DiskManagement)
        .await
        .map_err(FstabSourceError::Ignored)?;
    let output = String::from_utf8(output)
        .map_err(Error::from)
        .map_err(FstabSourceError::Ignored)?;

    match parse_blkid_devices(&output) {
        Ok(Some(device)) => Ok(device),
        Ok(None) => Err(FstabSourceError::Ignored(Error::new(
            eyre!("no matching block device"),
            ErrorKind::DiskManagement,
        ))),
        Err(devices) => Err(FstabSourceError::Ambiguous(Error::new(
            eyre!(
                "fstab source {source} matches multiple devices: {}",
                devices.iter().map(|path| path.display()).format(", ")
            ),
            ErrorKind::DiskManagement,
        ))),
    }
}

pub fn disk<C: Context>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand(
            "list",
            from_fn_async(list)
                .with_display_serializable()
                .with_custom_display_fn(|handle, result| display_disk_info(handle.params, result))
                .with_about("about.list-disk-info")
                .with_call_remote::<CliContext>(),
        )
        .subcommand("repair", from_fn_async(|_: C| repair()).no_cli())
        .subcommand(
            "repair",
            CallRemoteHandler::<CliContext, _, _>::new(
                from_fn_async(|_: RpcContext| repair())
                    .no_display()
                    .with_about("about.repair-disk-corruption"),
            ),
        )
}

fn display_disk_info(params: WithIoFormat<Empty>, args: Vec<DiskInfo>) -> Result<(), Error> {
    use prettytable::*;

    if let Some(format) = params.format {
        return display_serializable(format, args);
    }

    let mut table = Table::new();
    table.add_row(row![bc =>
        "LOGICALNAME",
        "LABEL",
        "CAPACITY",
        "USED",
        "STARTOS VERSION"
    ]);
    for disk in args {
        let row = row![
            disk.logicalname.display(),
            "N/A",
            &format!("{:.2} GiB", disk.capacity as f64 / 1024.0 / 1024.0 / 1024.0),
            "N/A",
            "N/A",
        ];
        table.add_row(row);
        for part in disk.partitions {
            let row = row![
                part.logicalname.display(),
                if let Some(label) = part.label.as_ref() {
                    label
                } else {
                    "N/A"
                },
                part.capacity,
                &if let Some(used) = part
                    .used
                    .map(|u| format!("{:.2} GiB", u as f64 / 1024.0 / 1024.0 / 1024.0))
                {
                    used
                } else {
                    "N/A".to_owned()
                },
                &if part.start_os.is_empty() {
                    "N/A".to_owned()
                } else if part.start_os.len() == 1 {
                    part.start_os
                        .first_key_value()
                        .map(|(_, info)| info.version.to_string())
                        .unwrap()
                } else {
                    part.start_os
                        .iter()
                        .map(|(id, info)| lazy_format!("{} ({})", info.version, id))
                        .join(", ")
                },
            ];
            table.add_row(row);
        }
    }
    table.print_tty(false)?;
    Ok(())
}

// #[command(display(display_disk_info))]
pub async fn list(ctx: RpcContext, _: Empty) -> Result<Vec<DiskInfo>, Error> {
    crate::disk::util::list(&ctx.os_partitions, None).await
}

pub async fn repair() -> Result<(), Error> {
    tokio::fs::write(REPAIR_DISK_PATH, b"").await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::parse_blkid_devices;

    #[test]
    fn parse_blkid_devices_with_no_matches() {
        assert_eq!(parse_blkid_devices(""), Ok(None));
    }

    #[test]
    fn parse_blkid_devices_with_one_match() {
        assert_eq!(
            parse_blkid_devices("/dev/sda1\n"),
            Ok(Some(PathBuf::from("/dev/sda1")))
        );
    }

    #[test]
    fn parse_blkid_devices_with_multiple_matches() {
        assert_eq!(
            parse_blkid_devices("/dev/sda1\n/dev/sdb1\n"),
            Err(vec![PathBuf::from("/dev/sda1"), PathBuf::from("/dev/sdb1")])
        );
    }
}
