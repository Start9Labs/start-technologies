use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use imbl_value::Value;
use rpc_toolkit::{from_fn_async, Context, HandlerExt, ParentHandler};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::prelude::*;
use crate::{flash, ServerContext};

const EMMC_ROOTFS_MOUNT: &str = "/mnt/emmc_rootfs";
const EMMC_OVERLAY_MOUNT: &str = "/mnt/emmc_overlay";
const EMMC_MERGED_MOUNT: &str = "/mnt/emmc_merged";

/// Files excluded from conffiles backup — the wizard writes its own versions.
const CONFFILES_EXCLUDE: &[&str] = &["/etc/shadow"];

// ---------------------------------------------------------------------------
// Boot / device detection
// ---------------------------------------------------------------------------

/// Check if the system booted from removable media (setup mode).
pub async fn is_setup_mode() -> bool {
    match flash::boot_device().await {
        Ok(dev) => flash::mmc_device_type(&dev).await.as_deref() != Some("MMC"),
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// Overlayfs mount/unmount helpers
// ---------------------------------------------------------------------------

/// Find a partition named `name` on block device `dev_path` and return its node.
async fn find_partition_by_name(dev_path: &str, name: &str) -> Result<Option<String>, Error> {
    let sfdisk = flash::read_partition_table(dev_path).await?;
    Ok(sfdisk
        .partition_table
        .partitions
        .iter()
        .find(|p| p.name.as_deref() == Some(name))
        .map(|p| p.node.clone()))
}

/// Mount the three-layer SquashFS + ext4 overlay stack.
///
/// 1. squashfs rootfs → EMMC_ROOTFS_MOUNT (read-only base)
/// 2. ext4 rootfs_data → EMMC_OVERLAY_MOUNT (writable overlay)
/// 3. overlayfs → EMMC_MERGED_MOUNT (merged view)
///
/// On failure, any layers already mounted are cleaned up before returning.
async fn mount_emmc_overlayfs(rootfs_dev: &str, rootfs_data_dev: &str) -> Result<(), Error> {
    // 1. Mount squashfs (read-only)
    fs::create_dir_all(EMMC_ROOTFS_MOUNT).map_err(|e| {
        Error::new(
            eyre!("mkdir {EMMC_ROOTFS_MOUNT}: {e}"),
            ErrorKind::Filesystem,
        )
    })?;
    flash::run_cmd("mount", &["-t", "squashfs", rootfs_dev, EMMC_ROOTFS_MOUNT]).await?;

    // 2. Mount ext4 overlay (read-write)
    fs::create_dir_all(EMMC_OVERLAY_MOUNT).map_err(|e| {
        Error::new(
            eyre!("mkdir {EMMC_OVERLAY_MOUNT}: {e}"),
            ErrorKind::Filesystem,
        )
    })?;
    if let Err(e) = flash::run_cmd(
        "mount",
        &["-t", "ext4", rootfs_data_dev, EMMC_OVERLAY_MOUNT],
    )
    .await
    {
        let _ = flash::run_cmd("umount", &[EMMC_ROOTFS_MOUNT]).await;
        return Err(e);
    }

    // 3. Create upper/work dirs on the overlay
    let upper = format!("{EMMC_OVERLAY_MOUNT}/upper");
    let work = format!("{EMMC_OVERLAY_MOUNT}/work");
    if let Err(e) = fs::create_dir_all(&upper).and_then(|_| fs::create_dir_all(&work)) {
        let _ = flash::run_cmd("umount", &[EMMC_OVERLAY_MOUNT]).await;
        let _ = flash::run_cmd("umount", &[EMMC_ROOTFS_MOUNT]).await;
        return Err(Error::new(
            eyre!("mkdir overlay dirs: {e}"),
            ErrorKind::Filesystem,
        ));
    }

    // 4. Mount overlayfs
    fs::create_dir_all(EMMC_MERGED_MOUNT).map_err(|e| {
        Error::new(
            eyre!("mkdir {EMMC_MERGED_MOUNT}: {e}"),
            ErrorKind::Filesystem,
        )
    })?;
    let overlay_opts = format!("lowerdir={EMMC_ROOTFS_MOUNT},upperdir={upper},workdir={work}");
    if let Err(e) = flash::run_cmd(
        "mount",
        &[
            "-t",
            "overlay",
            "overlay",
            "-o",
            &overlay_opts,
            EMMC_MERGED_MOUNT,
        ],
    )
    .await
    {
        let _ = flash::run_cmd("umount", &[EMMC_OVERLAY_MOUNT]).await;
        let _ = flash::run_cmd("umount", &[EMMC_ROOTFS_MOUNT]).await;
        return Err(e);
    }

    Ok(())
}

/// Unmount the three-layer overlayfs stack in reverse order.
///
/// After unmounting overlayfs, the kernel's dentry/inode cache may still hold
/// references to the underlying mounts, keeping them temporarily busy. A sync
/// ensures all data is on disk, then lazy unmount is used as a fallback for the
/// underlying layers.
async fn umount_emmc_overlayfs() -> Result<(), Error> {
    flash::run_cmd("sync", &[]).await?;

    let mut errors = Vec::new();
    if let Err(e) = flash::run_cmd("umount", &[EMMC_MERGED_MOUNT]).await {
        errors.push(format!("{EMMC_MERGED_MOUNT}: {e}"));
    }

    // Drop cached dentries/inodes so the kernel releases refcounts on the
    // squashfs lowerdir that overlayfs was using.
    let _ = fs::write("/proc/sys/vm/drop_caches", b"3");

    for mount in [EMMC_OVERLAY_MOUNT, EMMC_ROOTFS_MOUNT] {
        if flash::run_cmd("umount", &[mount]).await.is_err() {
            if flash::run_cmd("umount", &["-l", mount]).await.is_err() && is_mounted(mount) {
                errors.push(format!("{mount}: still mounted after umount -l"));
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(Error::new(
            eyre!("umount errors: {}", errors.join("; ")),
            ErrorKind::Filesystem,
        ))
    }
}

/// Check whether a path is currently a mount point.
fn is_mounted(path: &str) -> bool {
    fs::read_to_string("/proc/mounts")
        .map(|m| m.lines().any(|l| l.split_whitespace().nth(1) == Some(path)))
        .unwrap_or(false)
}

/// Mark the overlay as FS_STATE_READY by creating a `.fs_state` symlink.
///
/// OpenWrt's `mount_root` / `fstools` checks for this symlink at the root of
/// the overlay partition. The symlink target is the state number: "2" means
/// FS_STATE_READY. Without this, `mount_root` on first boot sees
/// FS_STATE_UNKNOWN and wipes the overlay.
fn mark_overlay_ready(overlay_mount: &str) -> Result<(), Error> {
    let fs_state_path = format!("{overlay_mount}/.fs_state");
    // Remove if it already exists (e.g. from a previous flash attempt)
    let _ = fs::remove_file(&fs_state_path);
    std::os::unix::fs::symlink("2", &fs_state_path).map_err(|e| {
        Error::new(
            eyre!("create .fs_state symlink: {e}"),
            ErrorKind::Filesystem,
        )
    })?;
    Ok(())
}


// ---------------------------------------------------------------------------
// Disk state detection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskState {
    /// An eMMC device was found.
    pub emmc_found: bool,
    /// The eMMC has existing firmware (rootfs partition present).
    pub has_firmware: bool,
}

pub async fn detect_disk_state() -> Result<DiskState, Error> {
    let boot_dev = match flash::boot_device().await {
        Ok(dev) => dev,
        Err(_) => {
            return Ok(DiskState {
                emmc_found: false,
                has_firmware: false,
            })
        }
    };

    let emmc_dev = match flash::find_emmc(&boot_dev).await {
        Ok(dev) => dev,
        Err(_) => {
            return Ok(DiskState {
                emmc_found: false,
                has_firmware: false,
            })
        }
    };

    let emmc_path = format!("/dev/{emmc_dev}");

    let has_firmware = match flash::read_partition_table(&emmc_path).await {
        Ok(sfdisk) => sfdisk
            .partition_table
            .partitions
            .iter()
            .any(|p| p.name.as_deref() == Some("rootfs")),
        Err(_) => false,
    };

    Ok(DiskState {
        emmc_found: true,
        has_firmware,
    })
}

// ---------------------------------------------------------------------------
// Conffiles backup / restore
// ---------------------------------------------------------------------------

struct BackedUpFile {
    /// Absolute path on the target rootfs (e.g. "/etc/config/network").
    path: String,
    /// File mode bits.
    mode: u32,
    /// File contents.
    contents: Vec<u8>,
}

/// Build the list of conffiles to preserve from a mounted rootfs.
fn list_conffiles(rootfs_mount: &str) -> HashSet<String> {
    let mut files = HashSet::new();

    // /lib/upgrade/keep.d/*
    let keep_dir = format!("{rootfs_mount}/lib/upgrade/keep.d");
    if let Ok(entries) = fs::read_dir(&keep_dir) {
        for entry in entries.flatten() {
            if let Ok(content) = fs::read_to_string(entry.path()) {
                for line in content.lines() {
                    let line = line.trim();
                    if !line.is_empty() && !line.starts_with('#') {
                        files.insert(line.to_string());
                    }
                }
            }
        }
    }

    // /etc/sysupgrade.conf
    let sysupgrade_conf = format!("{rootfs_mount}/etc/sysupgrade.conf");
    if let Ok(content) = fs::read_to_string(&sysupgrade_conf) {
        for line in content.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with('#') {
                files.insert(line.to_string());
            }
        }
    }

    // opkg conffiles: /usr/lib/opkg/status
    let opkg_status = format!("{rootfs_mount}/usr/lib/opkg/status");
    if let Ok(content) = fs::read_to_string(&opkg_status) {
        let mut in_conffiles = false;
        for line in content.lines() {
            if line.starts_with("Conffiles:") {
                in_conffiles = true;
                continue;
            }
            if in_conffiles {
                if line.starts_with(' ') {
                    if let Some(path) = line.trim().split_whitespace().next() {
                        if !path.is_empty() {
                            files.insert(path.to_string());
                        }
                    }
                } else {
                    in_conffiles = false;
                }
            }
        }
    }

    // apk conffiles: /lib/apk/packages/*.conffiles_static
    let apk_dir = format!("{rootfs_mount}/lib/apk/packages");
    if let Ok(entries) = fs::read_dir(&apk_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().ends_with(".conffiles_static") {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    for line in content.lines() {
                        let line = line.trim();
                        if !line.is_empty() {
                            files.insert(line.to_string());
                        }
                    }
                }
            }
        }
    }

    // Remove excluded files
    for exclude in CONFFILES_EXCLUDE {
        files.remove(*exclude);
    }

    files
}

/// Recursively enumerate all regular files under `dir`, returning their paths
/// relative to `rootfs_mount` (e.g. "/etc/config/startwrt").
fn enumerate_dir_files(dir: &Path, rootfs_mount: &str) -> Vec<String> {
    let mut result = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return result;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            result.extend(enumerate_dir_files(&path, rootfs_mount));
        } else if path.is_file() {
            if let Ok(suffix) = path.strip_prefix(rootfs_mount) {
                result.push(format!("/{}", suffix.display()));
            }
        }
    }
    result
}

/// Back up conffiles from a mounted rootfs.
fn backup_conffiles(rootfs_mount: &str) -> Vec<BackedUpFile> {
    let conffiles = list_conffiles(rootfs_mount);
    let mut backed_up = Vec::new();

    // Expand directory entries and collect all file paths to back up.
    let mut all_paths: HashSet<String> = HashSet::new();
    for path in &conffiles {
        let full_path = format!("{rootfs_mount}{path}");
        let p = Path::new(&full_path);
        if p.is_dir() {
            for file_path in enumerate_dir_files(p, rootfs_mount) {
                if !CONFFILES_EXCLUDE.contains(&file_path.as_str()) {
                    all_paths.insert(file_path);
                }
            }
        } else {
            all_paths.insert(path.clone());
        }
    }

    for path in all_paths {
        let full_path = format!("{rootfs_mount}{path}");
        let p = Path::new(&full_path);
        if !p.exists() || !p.is_file() {
            continue;
        }
        let Ok(metadata) = fs::metadata(&full_path) else {
            continue;
        };
        let Ok(contents) = fs::read(&full_path) else {
            continue;
        };
        backed_up.push(BackedUpFile {
            path,
            mode: metadata.mode(),
            contents,
        });
    }

    backed_up
}

/// Restore backed-up conffiles to a mounted rootfs.
async fn restore_conffiles(rootfs_mount: &str, files: &[BackedUpFile]) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::AsyncWriteExt;

    for file in files {
        let full_path = format!("{rootfs_mount}{}", file.path);
        if let Some(parent) = Path::new(&full_path).parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                Error::new(
                    eyre!("mkdir {}: {e}", parent.display()),
                    ErrorKind::Filesystem,
                )
            })?;
        }

        let mut f = startos::util::io::AtomicFile::new(&full_path, None::<&Path>)
            .await
            .map_err(Error::from)?;
        f.set_permissions(std::fs::Permissions::from_mode(file.mode))
            .await
            .map_err(|e| Error::new(eyre!("chmod {full_path}: {e}"), ErrorKind::Filesystem))?;
        f.write_all(&file.contents)
            .await
            .map_err(|e| Error::new(eyre!("write {full_path}: {e}"), ErrorKind::Filesystem))?;
        f.save().await.map_err(Error::from)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Post-flash configuration
// ---------------------------------------------------------------------------

/// Write the admin password hash to /etc/shadow on a mounted rootfs.
async fn write_admin_password(rootfs_mount: &str, password: &str) -> Result<(), Error> {
    let shadow_path = format!("{rootfs_mount}/etc/shadow");
    let shadow = tokio::fs::read_to_string(&shadow_path)
        .await
        .map_err(|e| Error::new(eyre!("read {shadow_path}: {e}"), ErrorKind::Filesystem))?;

    let new_hash = pwhash::sha512_crypt::hash(password).map_err(|e| {
        Error::new(
            eyre!("hash password: {e}"),
            ErrorKind::PasswordHashGeneration,
        )
    })?;

    let mut found = false;
    let new_content: String = shadow
        .lines()
        .map(|line| {
            if line.starts_with("root:") {
                found = true;
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    let mut owned: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
                    owned[1] = new_hash.clone();
                    return owned.join(":");
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");

    if !found {
        return Err(Error::new(
            eyre!("root user not found in /etc/shadow"),
            ErrorKind::NotFound,
        ));
    }

    let new_content = if new_content.ends_with('\n') {
        new_content
    } else {
        format!("{new_content}\n")
    };

    // Atomic write
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::AsyncWriteExt;
    let mut f = startos::util::io::AtomicFile::new(&shadow_path, None::<&Path>)
        .await
        .map_err(Error::from)?;
    f.set_permissions(std::fs::Permissions::from_mode(0o600))
        .await
        .map_err(|e| Error::new(eyre!("chmod {shadow_path}: {e}"), ErrorKind::Filesystem))?;
    f.write_all(new_content.as_bytes())
        .await
        .map_err(|e| Error::new(eyre!("write {shadow_path}: {e}"), ErrorKind::Filesystem))?;
    f.save().await.map_err(Error::from)?;

    Ok(())
}

/// Write the chosen timezone into the eMMC `/etc/config/system` on the mounted
/// merged overlay (`merged_mount`, e.g. `/mnt/emmc_merged`). Resolves the POSIX
/// TZ string from the running (microSD) system's LuCI zoneinfo table — the same
/// authoritative source `system.set-timezone` uses — and stores both the IANA
/// `zonename` (verbatim) and the POSIX `timezone`, matching LuCI's writer. An
/// unknown zone is left at the shipped UTC default rather than treated as an
/// error. Records the outcome directly in the eMMC's activity DB so the
/// success / UTC-fallback shows up once the device boots from eMMC.
async fn write_timezone(merged_mount: &str, iana: &str) -> Result<(), Error> {
    let config_dir = format!("{merged_mount}/etc/config");
    let activity_db = format!("{merged_mount}/etc/startwrt/activity.db");

    let posix = match crate::system::resolve_posix_tz(iana).await? {
        Some(p) => p,
        None => {
            tracing::warn!("setup timezone '{iana}' not in zoneinfo table; leaving UTC default");
            crate::activity::log_to(
                &activity_db,
                "system",
                "timezone-updated",
                false,
                &format!("Timezone '{iana}' not recognized; kept UTC"),
                Some("not found in zoneinfo table"),
            );
            return Ok(());
        }
    };

    // `uci -c <dir>` operates on the alternate config tree; commit writes the
    // package file back into the merged overlay (upper = eMMC rootfs_data).
    let result = async {
        flash::run_cmd(
            "uci",
            &["-c", &config_dir, "set", &format!("system.@system[0].zonename={iana}")],
        )
        .await?;
        flash::run_cmd(
            "uci",
            &["-c", &config_dir, "set", &format!("system.@system[0].timezone={posix}")],
        )
        .await?;
        flash::run_cmd("uci", &["-c", &config_dir, "commit", "system"]).await
    }
    .await;

    match result {
        Ok(()) => {
            crate::activity::log_to(
                &activity_db,
                "system",
                "timezone-updated",
                true,
                &format!("Set timezone to {iana}"),
                None,
            );
            Ok(())
        }
        Err(e) => {
            crate::activity::log_to(
                &activity_db,
                "system",
                "timezone-updated",
                false,
                "Failed to apply setup timezone",
                Some(&e.to_string()),
            );
            Err(e)
        }
    }
}

// ---------------------------------------------------------------------------
// Flash orchestration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FlashMode {
    Update,
    FreshStart,
}

/// Streaming event sent to the frontend during flash.
#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "phase",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SetupEvent {
    Copying {
        copied: u64,
        total: u64,
        step: u32,
        total_steps: u32,
    },
    Status {
        message: String,
        step: u32,
        total_steps: u32,
    },
    Complete,
    Error {
        message: String,
    },
}

/// Run the full setup flash sequence (blocking).
///
/// Sends progress events through `tx`. On error, sends an Error event
/// before returning.
pub async fn run_setup_flash(
    mode: FlashMode,
    admin_password: &str,
    timezone: Option<&str>,
    tx: &mpsc::Sender<SetupEvent>,
) {
    if let Err(e) = run_setup_flash_inner(mode, admin_password, timezone, tx).await {
        let _ = tx
            .send(SetupEvent::Error {
                message: e.to_string(),
            })
            .await;
    }
}

async fn run_setup_flash_inner(
    mode: FlashMode,
    admin_password: &str,
    timezone: Option<&str>,
    tx: &mpsc::Sender<SetupEvent>,
) -> Result<(), Error> {
    let total_steps: u32 = 3;

    // ── Step 1: Preparing ──────────────────────────────────────────────
    let _ = tx
        .send(SetupEvent::Status {
            message: "Validating password...".into(),
            step: 1,
            total_steps,
        })
        .await;

    if admin_password.len() < 12 {
        return Err(Error::new(
            eyre!("password must be at least 12 characters"),
            ErrorKind::InvalidRequest,
        ));
    }

    // Update: backup conffiles before flash (mount old eMMC squashfs + overlay)
    let conffiles_backup = if mode == FlashMode::Update {
        let _ = tx
            .send(SetupEvent::Status {
                message: "Backing up configuration...".into(),
                step: 1,
                total_steps,
            })
            .await;

        let boot_dev = flash::boot_device().await?;
        let emmc_dev = flash::find_emmc(&boot_dev).await?;
        let emmc_path = format!("/dev/{emmc_dev}");

        // Find eMMC rootfs and rootfs_data
        let rootfs_dev = find_partition_by_name(&emmc_path, "rootfs")
            .await?
            .ok_or_else(|| {
                Error::new(
                    eyre!("rootfs partition not found on eMMC"),
                    ErrorKind::NotFound,
                )
            })?;
        let rootfs_data_dev = find_partition_by_name(&emmc_path, "rootfs_data")
            .await?
            .ok_or_else(|| {
                Error::new(
                    eyre!("rootfs_data partition not found on eMMC"),
                    ErrorKind::NotFound,
                )
            })?;

        // Mount squashfs + overlay to get merged view of old config
        mount_emmc_overlayfs(&rootfs_dev, &rootfs_data_dev).await?;
        let backup = backup_conffiles(EMMC_MERGED_MOUNT);
        if let Err(e) = umount_emmc_overlayfs().await {
            eprintln!("WARNING: first umount_emmc_overlayfs failed: {e}");
        }

        Some(backup)
    } else {
        None
    };

    // ── Step 2: Flashing eMMC ──────────────────────────────────────────
    let on_progress = |event: flash::FlashEvent| {
        let setup_event = match event {
            flash::FlashEvent::Copying { copied, total } => SetupEvent::Copying {
                copied,
                total,
                step: 2,
                total_steps,
            },
            flash::FlashEvent::Status { message } => SetupEvent::Status {
                message,
                step: 2,
                total_steps,
            },
        };
        let _ = tx.try_send(setup_event);
    };
    let result = flash::run_flash_unattended(&on_progress).await?;

    // ── Step 3: Applying settings ──────────────────────────────────────
    let _ = tx
        .send(SetupEvent::Status {
            message: "Configuring system...".into(),
            step: 3,
            total_steps,
        })
        .await;

    // Mount SquashFS + ext4 overlay to get a writable merged view
    mount_emmc_overlayfs(&result.rootfs_dev, &result.rootfs_data_dev).await?;

    // Restore conffiles (Update only)
    if let Some(ref files) = conffiles_backup {
        let _ = tx
            .send(SetupEvent::Status {
                message: "Restoring configuration...".into(),
                step: 3,
                total_steps,
            })
            .await;
        restore_conffiles(EMMC_MERGED_MOUNT, files).await?;
    }

    // Write admin password
    let _ = tx
        .send(SetupEvent::Status {
            message: "Setting admin password...".into(),
            step: 3,
            total_steps,
        })
        .await;
    write_admin_password(EMMC_MERGED_MOUNT, admin_password).await?;

    // Carry the setup timezone into the fresh eMMC config. FreshStart only:
    // Update restores the device's existing /etc/config/system via conffiles,
    // so its timezone is already preserved. The live `system.set-timezone`
    // call made during the wizard writes to the throwaway microSD overlay, so
    // this is the only place a fresh install's timezone reaches eMMC.
    if mode == FlashMode::FreshStart {
        if let Some(tz) = timezone {
            let _ = tx
                .send(SetupEvent::Status {
                    message: "Applying timezone...".into(),
                    step: 3,
                    total_steps,
                })
                .await;
            if let Err(e) = write_timezone(EMMC_MERGED_MOUNT, tz).await {
                // Non-fatal: the device boots on the shipped UTC default and
                // the user can correct it later in Settings.
                tracing::warn!("failed to carry setup timezone '{tz}' to eMMC: {e}");
            }
        }
    }

    // WiFi config is intentionally not written here. The post-reboot daemon
    // resolves the WiFi password from EEPROM (tag 0x2F) via
    // restore_wifi_if_needed() and brings up the AP from there.

    // Mark overlay as FS_STATE_READY so mount_root doesn't wipe it on first boot
    mark_overlay_ready(EMMC_OVERLAY_MOUNT)?;

    // Unmount the overlayfs stack (non-fatal — mounts are cleaned up on reboot)
    if let Err(e) = umount_emmc_overlayfs().await {
        eprintln!("WARNING: umount_emmc_overlayfs failed (non-fatal, cleaned up on reboot): {e}");
    }

    let _ = tx.send(SetupEvent::Complete).await;

    Ok(())
}

// ---------------------------------------------------------------------------
// RPC endpoints
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatusRes {
    pub setup_mode: bool,
    pub disk: DiskState,
}

#[instrument(skip_all)]
async fn setup_status_impl(_ctx: ServerContext) -> Result<SetupStatusRes, Error> {
    let disk = detect_disk_state().await?;
    Ok(SetupStatusRes {
        setup_mode: is_setup_mode().await,
        disk,
    })
}

pub fn setup<C: Context>() -> ParentHandler<C> {
    ParentHandler::new().subcommand(
        "status",
        from_fn_async(setup_status_impl)
            .with_metadata("no_auth", Value::Bool(true))
            .no_cli(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;
    use tokio::sync::mpsc;

    // ── SetupEvent serialization ─────────────────────────────────────

    #[test]
    fn setup_event_copying_json() {
        let event = SetupEvent::Copying {
            copied: 100,
            total: 1000,
            step: 2,
            total_steps: 3,
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["phase"], "copying");
        assert_eq!(json["copied"], 100);
        assert_eq!(json["total"], 1000);
        assert_eq!(json["step"], 2);
        assert_eq!(json["totalSteps"], 3);
    }

    #[test]
    fn setup_event_status_json() {
        let event = SetupEvent::Status {
            message: "Flashing...".into(),
            step: 1,
            total_steps: 3,
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["phase"], "status");
        assert_eq!(json["message"], "Flashing...");
        assert_eq!(json["step"], 1);
        assert_eq!(json["totalSteps"], 3);
    }

    #[test]
    fn setup_event_complete_json() {
        let event = SetupEvent::Complete;
        let parsed: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(parsed, serde_json::json!({"phase": "complete"}));
    }

    #[test]
    fn setup_event_error_json() {
        let event = SetupEvent::Error {
            message: "disk full".into(),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["phase"], "error");
        assert_eq!(json["message"], "disk full");
    }

    // ── FlashMode deserialization ────────────────────────────────────

    #[test]
    fn flash_mode_from_kebab_case() {
        let update: FlashMode = serde_json::from_str("\"update\"").unwrap();
        assert_eq!(update, FlashMode::Update);
        let fresh: FlashMode = serde_json::from_str("\"fresh-start\"").unwrap();
        assert_eq!(fresh, FlashMode::FreshStart);
    }

    #[test]
    fn flash_mode_invalid() {
        let result = serde_json::from_str::<FlashMode>("\"invalid\"");
        assert!(result.is_err());
    }

    // ── list_conffiles ───────────────────────────────────────────────

    #[test]
    fn list_conffiles_keep_d() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        let keep_d = root.join("lib/upgrade/keep.d");
        fs::create_dir_all(&keep_d).unwrap();
        fs::write(keep_d.join("base"), "/etc/config/network\n/etc/hosts\n").unwrap();

        let files = list_conffiles(root.to_str().unwrap());
        assert!(files.contains("/etc/config/network"));
        assert!(files.contains("/etc/hosts"));
    }

    #[test]
    fn list_conffiles_sysupgrade_conf() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("etc")).unwrap();
        fs::write(root.join("etc/sysupgrade.conf"), "/etc/foo.conf\n").unwrap();

        let files = list_conffiles(root.to_str().unwrap());
        assert!(files.contains("/etc/foo.conf"));
    }

    #[test]
    fn list_conffiles_opkg_status() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("usr/lib/opkg")).unwrap();
        let status_content = "\
Package: base-files
Status: install ok installed
Conffiles:
 /etc/profile abc123
 /etc/shells def456
Architecture: riscv64

Package: dropbear
Status: install ok installed
Conffiles:
 /etc/dropbear/authorized_keys xyz789
";
        fs::write(root.join("usr/lib/opkg/status"), status_content).unwrap();

        let files = list_conffiles(root.to_str().unwrap());
        assert!(files.contains("/etc/profile"));
        assert!(files.contains("/etc/shells"));
        assert!(files.contains("/etc/dropbear/authorized_keys"));
    }

    #[test]
    fn list_conffiles_apk() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("lib/apk/packages")).unwrap();
        fs::write(
            root.join("lib/apk/packages/base-files.conffiles_static"),
            "/etc/apk.conf\n/etc/apk2.conf\n",
        )
        .unwrap();

        let files = list_conffiles(root.to_str().unwrap());
        assert!(files.contains("/etc/apk.conf"));
        assert!(files.contains("/etc/apk2.conf"));
    }

    #[test]
    fn list_conffiles_excludes() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        let keep_d = root.join("lib/upgrade/keep.d");
        fs::create_dir_all(&keep_d).unwrap();
        fs::write(
            keep_d.join("base"),
            "/etc/shadow\n/etc/config/wireless\n/etc/config/network\n",
        )
        .unwrap();

        let files = list_conffiles(root.to_str().unwrap());
        assert!(!files.contains("/etc/shadow"), "shadow should be excluded");
        assert!(
            files.contains("/etc/config/wireless"),
            "wireless should be preserved (Update path keeps user VLAN/profile config)"
        );
        assert!(files.contains("/etc/config/network"));
    }

    #[test]
    fn list_conffiles_comments_and_blanks() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        let keep_d = root.join("lib/upgrade/keep.d");
        fs::create_dir_all(&keep_d).unwrap();
        fs::write(
            keep_d.join("base"),
            "# this is a comment\n\n/etc/real\n  \n# another comment\n",
        )
        .unwrap();

        let files = list_conffiles(root.to_str().unwrap());
        assert!(files.contains("/etc/real"));
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn list_conffiles_empty_rootfs() {
        let dir = TempDir::new().unwrap();
        let files = list_conffiles(dir.path().to_str().unwrap());
        assert!(files.is_empty());
    }

    // ── backup / restore round-trip ──────────────────────────────────

    #[tokio::test]
    async fn backup_restore_round_trip() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create conffiles listing
        let keep_d = root.join("lib/upgrade/keep.d");
        fs::create_dir_all(&keep_d).unwrap();
        fs::write(keep_d.join("base"), "/etc/config/network\n/etc/hosts\n").unwrap();

        // Create actual files with content and specific modes
        fs::create_dir_all(root.join("etc/config")).unwrap();
        fs::write(root.join("etc/config/network"), "config interface 'lan'\n").unwrap();
        fs::set_permissions(
            root.join("etc/config/network"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();

        fs::write(root.join("etc/hosts"), "127.0.0.1 localhost\n").unwrap();
        fs::set_permissions(root.join("etc/hosts"), fs::Permissions::from_mode(0o644)).unwrap();

        // Backup
        let rootfs = root.to_str().unwrap();
        let backup = backup_conffiles(rootfs);
        assert_eq!(backup.len(), 2);

        // Wipe originals
        fs::remove_file(root.join("etc/config/network")).unwrap();
        fs::remove_file(root.join("etc/hosts")).unwrap();

        // Restore to a fresh location
        let restore_dir = TempDir::new().unwrap();
        let restore_root = restore_dir.path();
        restore_conffiles(restore_root.to_str().unwrap(), &backup)
            .await
            .unwrap();

        // Verify content
        let network = fs::read_to_string(restore_root.join("etc/config/network")).unwrap();
        assert_eq!(network, "config interface 'lan'\n");

        let hosts = fs::read_to_string(restore_root.join("etc/hosts")).unwrap();
        assert_eq!(hosts, "127.0.0.1 localhost\n");

        // Verify mode bits (mask to permission bits only)
        let network_mode = fs::metadata(restore_root.join("etc/config/network"))
            .unwrap()
            .mode()
            & 0o7777;
        assert_eq!(network_mode, 0o644);
    }

    #[test]
    fn backup_skips_missing_files() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // List a file that doesn't exist on disk
        let keep_d = root.join("lib/upgrade/keep.d");
        fs::create_dir_all(&keep_d).unwrap();
        fs::write(keep_d.join("base"), "/etc/nonexistent\n").unwrap();

        let backup = backup_conffiles(root.to_str().unwrap());
        assert!(backup.is_empty());
    }

    #[test]
    fn backup_expands_directory_entries() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // keep.d lists a directory (trailing slash)
        let keep_d = root.join("lib/upgrade/keep.d");
        fs::create_dir_all(&keep_d).unwrap();
        fs::write(keep_d.join("base"), "/etc/config/\n").unwrap();

        // Create files under /etc/config/, including a nested subdir
        fs::create_dir_all(root.join("etc/config/subdir")).unwrap();
        fs::write(root.join("etc/config/startwrt"), "startwrt data\n").unwrap();
        fs::write(root.join("etc/config/network"), "network data\n").unwrap();
        fs::write(root.join("etc/config/subdir/nested"), "nested data\n").unwrap();
        fs::write(root.join("etc/config/wireless"), "wireless data\n").unwrap();

        let rootfs = root.to_str().unwrap();
        let backup = backup_conffiles(rootfs);

        let paths: HashSet<&str> = backup.iter().map(|f| f.path.as_str()).collect();
        assert!(
            paths.contains("/etc/config/startwrt"),
            "should include startwrt"
        );
        assert!(
            paths.contains("/etc/config/network"),
            "should include network"
        );
        assert!(
            paths.contains("/etc/config/subdir/nested"),
            "should include nested files"
        );
        assert!(
            paths.contains("/etc/config/wireless"),
            "wireless should be preserved on Update"
        );
        assert_eq!(backup.len(), 4);

        // Verify content
        let startwrt = backup
            .iter()
            .find(|f| f.path == "/etc/config/startwrt")
            .unwrap();
        assert_eq!(startwrt.contents, b"startwrt data\n");
    }

    #[tokio::test]
    async fn restore_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let files = vec![BackedUpFile {
            path: "/etc/config/deep/nested/file.conf".to_string(),
            mode: 0o644,
            contents: b"content".to_vec(),
        }];

        restore_conffiles(root.to_str().unwrap(), &files)
            .await
            .unwrap();

        let restored = fs::read_to_string(root.join("etc/config/deep/nested/file.conf")).unwrap();
        assert_eq!(restored, "content");
    }

    // ── write_admin_password ─────────────────────────────────────────

    #[tokio::test]
    async fn write_password_replaces_root_hash() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("etc")).unwrap();
        fs::write(
            root.join("etc/shadow"),
            "root:*:0:0:99999:7:::\nnobody:!:0:0:99999:7:::\n",
        )
        .unwrap();

        write_admin_password(root.to_str().unwrap(), "mysecurepassword")
            .await
            .unwrap();

        let shadow = fs::read_to_string(root.join("etc/shadow")).unwrap();
        let root_line = shadow.lines().find(|l| l.starts_with("root:")).unwrap();
        let hash = root_line.split(':').nth(1).unwrap();
        assert!(
            hash.starts_with("$6$"),
            "hash should be sha512_crypt, got: {hash}"
        );
        assert_ne!(hash, "*", "hash should have been replaced");
    }

    #[tokio::test]
    async fn write_password_preserves_other_fields() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("etc")).unwrap();
        fs::write(
            root.join("etc/shadow"),
            "root:*:19000:0:99999:7:::\nnobody:!:19000:0:99999:7:::\n",
        )
        .unwrap();

        write_admin_password(root.to_str().unwrap(), "mysecurepassword")
            .await
            .unwrap();

        let shadow = fs::read_to_string(root.join("etc/shadow")).unwrap();

        // Check root's other fields are preserved
        let root_line = shadow.lines().find(|l| l.starts_with("root:")).unwrap();
        let parts: Vec<&str> = root_line.split(':').collect();
        assert_eq!(parts[0], "root");
        // parts[1] is the new hash — skip
        assert_eq!(parts[2], "19000");
        assert_eq!(parts[3], "0");
        assert_eq!(parts[4], "99999");
        assert_eq!(parts[5], "7");

        // Check nobody line is completely unchanged
        let nobody_line = shadow.lines().find(|l| l.starts_with("nobody:")).unwrap();
        assert_eq!(nobody_line, "nobody:!:19000:0:99999:7:::");
    }

    #[tokio::test]
    async fn write_password_no_root_entry() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("etc")).unwrap();
        fs::write(root.join("etc/shadow"), "nobody:!:0:0:99999:7:::\n").unwrap();

        let result = write_admin_password(root.to_str().unwrap(), "mysecurepassword").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("root"), "error should mention root: {err}");
    }

    #[tokio::test]
    async fn write_password_trailing_newline() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("etc")).unwrap();
        // Input without trailing newline
        fs::write(root.join("etc/shadow"), "root:*:0:0:99999:7:::").unwrap();

        write_admin_password(root.to_str().unwrap(), "mysecurepassword")
            .await
            .unwrap();

        let shadow = fs::read_to_string(root.join("etc/shadow")).unwrap();
        assert!(shadow.ends_with('\n'), "output should end with newline");
    }

    // ── run_setup_flash event protocol ───────────────────────────────

    #[tokio::test]
    async fn flash_rejects_short_password() {
        let (tx, mut rx) = mpsc::channel(16);
        run_setup_flash(FlashMode::FreshStart, "short", None, &tx).await;
        drop(tx);

        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }

        // First event: Status with step 1 ("Validating password...")
        assert!(
            events.len() >= 2,
            "expected at least 2 events, got {}",
            events.len()
        );
        match &events[0] {
            SetupEvent::Status { step, message, .. } => {
                assert_eq!(*step, 1);
                assert!(
                    message.contains("Validating"),
                    "expected validating message, got: {message}"
                );
            }
            other => panic!("expected Status event, got: {other:?}"),
        }

        // Last event: Error about password length
        match events.last().unwrap() {
            SetupEvent::Error { message } => {
                assert!(
                    message.contains("12"),
                    "error should mention 12 chars: {message}"
                );
            }
            other => panic!("expected Error event, got: {other:?}"),
        }
    }
}
