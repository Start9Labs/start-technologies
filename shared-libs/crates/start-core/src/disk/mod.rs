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

            // `/` is an overlayfs the initramfs sets up, so its fstab source
            // (`overlay`) names no block device — root comes from the live OS
            // mount below. Only /boot* entries are real block-device mounts.
            if target != "/boot" && !target.starts_with("/boot/") {
                continue;
            }

            let dev = match resolve_fstab_source(source).await {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!("Failed to resolve fstab source {source}: {e}");
                    continue;
                }
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

/// The initramfs bind-mounts the installed OS root partition here on every
/// StartOS boot. It exists only on a running installed system — not in the live
/// installer — so it names the OS root exactly when there is one.
const OS_ROOT_MOUNT: &str = "/media/startos/root";

/// Resolve the installed OS root block device from its live mount.
///
/// It can't come from the fstab `/` entry (`/` is an overlayfs the initramfs
/// stacks over the real partition) nor from "whatever the system booted from":
/// during os_install we're booted off the installer USB, which is not the OS
/// root this struct describes. The initramfs bind-mounts the real OS partition
/// at `/media/startos/root`, so that mount is the source of truth — and its
/// absence in the installer correctly yields no OS root.
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

/// Resolve an fstab device spec (e.g. /dev/sda1, PARTUUID=..., UUID=...) to a
/// canonical device path.
async fn resolve_fstab_source(source: &str) -> Result<PathBuf, Error> {
    if source.starts_with('/') {
        return Ok(tokio::fs::canonicalize(source)
            .await
            .unwrap_or_else(|_| PathBuf::from(source)));
    }
    // Only TAG=value specs (PARTUUID=, UUID=, LABEL=) are resolvable via blkid;
    // pseudo sources (overlay, tmpfs, none, ...) are not block devices.
    if !source.contains('=') {
        return Err(Error::new(
            eyre!("not a block device spec"),
            ErrorKind::DiskManagement,
        ));
    }
    let output = Command::new("blkid")
        .args(["-o", "device", "-t", source])
        .invoke(ErrorKind::DiskManagement)
        .await?;
    Ok(PathBuf::from(String::from_utf8(output)?.trim()))
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
    crate::disk::util::list(&ctx.os_partitions).await
}

pub async fn repair() -> Result<(), Error> {
    tokio::fs::write(REPAIR_DISK_PATH, b"").await?;
    Ok(())
}
