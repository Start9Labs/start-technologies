use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::Parser;
use color_eyre::eyre::eyre;
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use ts_rs::TS;

use crate::Error;
use crate::context::config::ServerConfig;
use crate::context::{CliContext, SetupContext};
use crate::disk::OsPartitionInfo;
use crate::disk::mount::filesystem::bind::Bind;
use crate::disk::mount::filesystem::block_dev::BlockDev;
use crate::disk::mount::filesystem::efivarfs::EfiVarFs;
use crate::disk::mount::filesystem::overlayfs::OverlayFs;
use crate::disk::mount::filesystem::{MountType, ReadWrite};
use crate::disk::mount::guard::{GenericMountGuard, MountGuard, TmpMountGuard};
use crate::disk::util::PartitionTable;
use crate::prelude::*;
use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
use crate::setup::SetupInfo;
use crate::util::Invoke;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::{TmpDir, delete_dir, delete_file, open_file, write_file_atomic};
use crate::util::serde::IoFormat;

mod gpt;
mod mbr;
mod quiesce;

/// Probe a squashfs image to determine its target architecture
async fn probe_squashfs_arch(squashfs_path: &Path) -> Result<InternedString, Error> {
    let output = String::from_utf8(
        Command::new("unsquashfs")
            .arg("-cat")
            .arg(squashfs_path)
            .arg("usr/lib/startos/PLATFORM.txt")
            .invoke(ErrorKind::ParseSysInfo)
            .await?,
    )?;
    Ok(crate::platform_to_arch(&output.trim()).into())
}

pub fn partition_for(disk: impl AsRef<Path>, idx: u32) -> PathBuf {
    let disk_path = disk.as_ref();
    let (root, leaf) = if let (Some(root), Some(leaf)) = (
        disk_path.parent(),
        disk_path.file_name().and_then(|s| s.to_str()),
    ) {
        (root, leaf)
    } else {
        return Default::default();
    };
    if leaf.ends_with(|c: char| c.is_ascii_digit()) {
        root.join(format!("{}p{}", leaf, idx))
    } else {
        root.join(format!("{}{}", leaf, idx))
    }
}

async fn partition(
    disk_path: &Path,
    capacity: u64,
    partition_table: Option<PartitionTable>,
    protect: Option<&Path>,
    use_efi: bool,
) -> Result<OsPartitionInfo, Error> {
    let partition_type = match (protect.is_none(), partition_table) {
        (true, _) | (_, None) => PartitionTable::Gpt,
        (_, Some(t)) => t,
    };
    match partition_type {
        PartitionTable::Gpt => gpt::partition(disk_path, capacity, protect, use_efi).await,
        PartitionTable::Mbr => mbr::partition(disk_path, capacity, protect).await,
    }
}

async fn get_block_device_size(path: impl AsRef<Path>) -> Result<u64, Error> {
    let path = path.as_ref();
    let device_name = path.file_name().and_then(|s| s.to_str()).ok_or_else(|| {
        Error::new(
            eyre!("Invalid block device path: {}", path.display()),
            ErrorKind::BlockDevice,
        )
    })?;
    let size_path = Path::new("/sys/block").join(device_name).join("size");
    let sectors: u64 = tokio::fs::read_to_string(&size_path)
        .await
        .with_ctx(|_| {
            (
                ErrorKind::BlockDevice,
                format!("reading {}", size_path.display()),
            )
        })?
        .trim()
        .parse()
        .map_err(|e| {
            Error::new(
                eyre!("Failed to parse block device size: {}", e),
                ErrorKind::BlockDevice,
            )
        })?;
    Ok(sectors * 512)
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct InstallOsParams {
    #[arg(help = "help.arg.os-drive-path")]
    os_drive: Option<PathBuf>,
    #[command(flatten)]
    data_drive: Option<DataDrive>,
}

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
struct DataDrive {
    #[arg(long = "data-drive", help = "help.arg.data-drive-path")]
    logicalname: PathBuf,
    #[arg(long, help = "help.arg.wipe-drive")]
    wipe: bool,
}

pub struct InstallOsResult {
    pub part_info: OsPartitionInfo,
    pub rootfs: TmpMountGuard,
    pub mok_enrolled: bool,
}

pub async fn install_os_to(
    squashfs_path: impl AsRef<Path>,
    disk_path: impl AsRef<Path>,
    capacity: u64,
    partition_table: Option<PartitionTable>,
    protect: Option<impl AsRef<Path>>,
    arch: &str,
    use_efi: bool,
) -> Result<InstallOsResult, Error> {
    let squashfs_path = squashfs_path.as_ref();
    let disk_path = disk_path.as_ref();
    let protect = protect.as_ref().map(|p| p.as_ref());

    let part_info = partition(disk_path, capacity, partition_table, protect, use_efi).await?;

    if let Some(efi) = part_info.extra_boot.get("efi") {
        Command::new("mkfs.vfat")
            .arg(efi)
            .invoke(crate::ErrorKind::DiskManagement)
            .await?;
        Command::new("fatlabel")
            .arg(efi)
            .arg("efi")
            .invoke(crate::ErrorKind::DiskManagement)
            .await?;
    }

    Command::new("mkfs.vfat")
        .arg(&part_info.boot)
        .invoke(crate::ErrorKind::DiskManagement)
        .await?;
    Command::new("fatlabel")
        .arg(&part_info.boot)
        .arg("boot")
        .invoke(crate::ErrorKind::DiskManagement)
        .await?;

    if protect.is_some() {
        if let Ok(guard) =
            TmpMountGuard::mount(&BlockDev::new(part_info.root.clone()), MountType::ReadWrite).await
        {
            if let Err(e) = async {
                // cp -r ${guard}/config /tmp/config
                delete_file(guard.path().join("config/upgrade")).await?;
                delete_file(guard.path().join("config/overlay/etc/hostname")).await?;
                delete_file(guard.path().join("config/disk.guid")).await?;
                delete_dir(guard.path().join("config/overlay/lib")).await?;
                delete_dir(guard.path().join("config/overlay/usr/lib")).await?;
                Command::new("cp")
                    .arg("-r")
                    .arg(guard.path().join("config"))
                    .arg("/tmp/config.bak")
                    .invoke(crate::ErrorKind::Filesystem)
                    .await?;
                Ok::<_, Error>(())
            }
            .await
            {
                tracing::error!("Error recovering previous config: {e}");
                tracing::debug!("{e:?}");
            }
            guard.unmount().await?;
        }
    }

    Command::new("mkfs.btrfs")
        .arg("-f")
        .arg(&part_info.root)
        .invoke(crate::ErrorKind::DiskManagement)
        .await?;
    Command::new("btrfs")
        .arg("property")
        .arg("set")
        .arg(&part_info.root)
        .arg("label")
        .arg("rootfs")
        .invoke(crate::ErrorKind::DiskManagement)
        .await?;
    let rootfs = TmpMountGuard::mount(&BlockDev::new(&part_info.root), ReadWrite).await?;

    let config_path = rootfs.path().join("config");

    if tokio::fs::metadata("/tmp/config.bak").await.is_ok() {
        crate::util::io::delete_dir(&config_path).await?;
        Command::new("cp")
            .arg("-r")
            .arg("/tmp/config.bak")
            .arg(&config_path)
            .invoke(crate::ErrorKind::Filesystem)
            .await?;
    } else {
        tokio::fs::create_dir_all(&config_path).await?;
    }

    let images_path = rootfs.path().join("images");
    tokio::fs::create_dir_all(&images_path).await?;
    let image_path = images_path
        .join(hex::encode(
            &MultiCursorFile::from(open_file(squashfs_path).await?)
                .blake3_mmap()
                .await?
                .as_bytes()[..16],
        ))
        .with_extension("rootfs");
    tokio::fs::copy(squashfs_path, &image_path).await?;
    // TODO: check hash of fs
    let unsquash_target = TmpDir::new().await?;
    let bootfs = MountGuard::mount(
        &BlockDev::new(&part_info.boot),
        unsquash_target.join("boot"),
        ReadWrite,
    )
    .await?;
    Command::new("unsquashfs")
        .arg("-n")
        .arg("-f")
        .arg("-d")
        .arg(&*unsquash_target)
        .arg(squashfs_path)
        .arg("boot")
        .invoke(crate::ErrorKind::Filesystem)
        .await?;
    bootfs.unmount(true).await?;
    unsquash_target.delete().await?;
    Command::new("ln")
        .arg("-rsf")
        .arg(&image_path)
        .arg(config_path.join("current.rootfs"))
        .invoke(ErrorKind::DiskManagement)
        .await?;

    tokio::fs::write(
        rootfs.path().join("config/config.yaml"),
        IoFormat::Yaml.to_vec(&ServerConfig::default())?,
    )
    .await?;

    let lower = TmpMountGuard::mount(&BlockDev::new(&image_path), MountType::ReadOnly).await?;
    let work = config_path.join("work");
    let upper = config_path.join("overlay");
    let overlay = TmpMountGuard::mount(
        &OverlayFs::new(vec![lower.path()], &upper, &work),
        ReadWrite,
    )
    .await?;

    let boot = MountGuard::mount(
        &BlockDev::new(&part_info.boot),
        overlay.path().join("boot"),
        ReadWrite,
    )
    .await?;
    let efi = if let Some(efi) = part_info.extra_boot.get("efi") {
        Some(
            MountGuard::mount(
                &BlockDev::new(efi),
                overlay.path().join("boot/efi"),
                ReadWrite,
            )
            .await?,
        )
    } else {
        None
    };
    let start_os_fs = MountGuard::mount(
        &Bind::new(rootfs.path()),
        overlay.path().join("media/startos/root"),
        MountType::ReadOnly,
    )
    .await?;
    let dev = MountGuard::mount(&Bind::new("/dev"), overlay.path().join("dev"), ReadWrite).await?;
    let proc =
        MountGuard::mount(&Bind::new("/proc"), overlay.path().join("proc"), ReadWrite).await?;
    let sys = MountGuard::mount(&Bind::new("/sys"), overlay.path().join("sys"), ReadWrite).await?;
    let efivarfs = if tokio::fs::metadata("/sys/firmware/efi").await.is_ok() {
        Some(
            MountGuard::mount(
                &EfiVarFs,
                overlay.path().join("sys/firmware/efi/efivars"),
                ReadWrite,
            )
            .await?,
        )
    } else {
        None
    };

    tokio::fs::write(
        overlay.path().join("etc/fstab"),
        format!(
            include_str!("fstab.template"),
            boot = part_info.boot.display(),
            efi = part_info
                .extra_boot
                .get("efi")
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "# N/A".to_owned()),
            root = part_info.root.display(),
        ),
    )
    .await?;

    Command::new("chroot")
        .arg(overlay.path())
        .arg("systemd-machine-id-setup")
        .invoke(crate::ErrorKind::Systemd)
        .await?;

    Command::new("chroot")
        .arg(overlay.path())
        .arg("ssh-keygen")
        .arg("-A")
        .invoke(crate::ErrorKind::OpenSsh)
        .await?;

    // Secure Boot: generate MOK key, sign unsigned modules, enroll MOK
    let mut mok_enrolled = false;
    if use_efi && crate::util::mok::is_secure_boot_enabled().await {
        let new_key = crate::util::mok::ensure_dkms_key(overlay.path()).await?;
        tracing::info!(
            "DKMS MOK key: {}",
            if new_key {
                "generated"
            } else {
                "already exists"
            }
        );

        crate::util::mok::sign_unsigned_modules(overlay.path()).await?;

        let mok_pub = overlay
            .path()
            .join(crate::util::mok::DKMS_MOK_PUB.trim_start_matches('/'));
        match crate::util::mok::enroll_mok(&mok_pub).await {
            Ok(enrolled) => mok_enrolled = enrolled,
            Err(e) => tracing::warn!("MOK enrollment failed: {e}"),
        }
    }

    let mut install = Command::new("chroot");
    install.arg(overlay.path()).arg("grub-install");
    if !use_efi {
        match arch {
            "x86_64" => install.arg("--target=i386-pc"),
            _ => &mut install,
        };
    } else {
        match arch {
            "x86_64" => install.arg("--target=x86_64-efi"),
            "aarch64" => install.arg("--target=arm64-efi"),
            "riscv64" => install.arg("--target=riscv64-efi"),
            _ => &mut install,
        };
    }
    install
        .arg(disk_path)
        .invoke(crate::ErrorKind::Grub)
        .await?;

    Command::new("chroot")
        .arg(overlay.path())
        .arg("update-grub")
        .invoke(crate::ErrorKind::Grub)
        .await?;
    dev.unmount(false).await?;
    if let Some(efivarfs) = efivarfs {
        efivarfs.unmount(false).await?;
    }
    sys.unmount(false).await?;
    proc.unmount(false).await?;
    start_os_fs.unmount(false).await?;
    if let Some(efi) = efi {
        efi.unmount(false).await?;
    }
    boot.unmount(false).await?;

    overlay.unmount().await?;
    tokio::fs::remove_dir_all(&work).await?;
    lower.unmount().await?;

    Ok(InstallOsResult {
        part_info,
        rootfs,
        mok_enrolled,
    })
}

pub async fn install_os(ctx: SetupContext, params: InstallOsParams) -> Result<SetupInfo, Error> {
    let fut = ctx.install_os_future.mutate(|slot| {
        if let Some(existing) = slot.as_ref() {
            if existing.peek().is_none() {
                return existing.clone();
            }
        }
        // Own the task via NonDetachingJoinHandle inside the Shared so it survives
        // dropped awaiters but is aborted when the last reference goes away.
        let ctx = ctx.clone();
        let handle: NonDetachingJoinHandle<Result<SetupInfo, Arc<Error>>> =
            tokio::spawn(async move { install_os_inner(ctx, params).await.map_err(Arc::new) })
                .into();
        let new_fut = async move {
            match handle.await {
                Ok(res) => res,
                Err(join_err) => Err(Arc::new(Error::new(
                    eyre!("install_os task did not complete: {join_err}"),
                    ErrorKind::Unknown,
                ))),
            }
        }
        .boxed()
        .shared();
        *slot = Some(new_fut.clone());
        new_fut
    });
    fut.await.map_err(|e| e.clone_output())
}

async fn install_os_inner(
    ctx: SetupContext,
    InstallOsParams {
        os_drive,
        data_drive,
    }: InstallOsParams,
) -> Result<SetupInfo, Error> {
    let disks = crate::disk::util::list(&Default::default()).await?;

    // With an os_drive we install StartOS onto it; without one we're already
    // booted from the installed OS, so we load the running setup.json and just
    // provision the data drive into it. `data_part` is the data partition the
    // installer carved on the OS drive (install path only).
    let (mut setup_info, data_part) = if let Some(os_drive) = &os_drive {
        // Drop any rootfs/config mounts a prior install left pinned, so a retry
        // doesn't fight itself for the target partition.
        let prior = ctx.install_rootfs.mutate(|s| s.take());
        if let Some((rootfs, config)) = prior {
            if let Err(e) = config.unmount(false).await {
                tracing::warn!("failed to unmount stale install config bind: {e}");
            }
            if let Err(e) = rootfs.unmount().await {
                tracing::warn!("failed to unmount stale install rootfs: {e}");
            }
        }

        let disk = disks
            .iter()
            .find(|d| &d.logicalname == os_drive)
            .ok_or_else(|| {
                Error::new(
                    eyre!("Unknown disk {}", os_drive.display()),
                    crate::ErrorKind::DiskManagement,
                )
            })?;

        let protect: Option<PathBuf> = data_drive.as_ref().and_then(|dd| {
            if dd.wipe {
                return None;
            }
            if disk.guid.as_ref().map_or(false, |g| {
                g.starts_with("EMBASSY_") || g.starts_with("STARTOS_")
            }) && disk.logicalname == dd.logicalname
            {
                return Some(disk.logicalname.clone());
            }
            disk.partitions
                .iter()
                .find(|p| {
                    p.guid.as_ref().map_or(false, |g| {
                        g.starts_with("EMBASSY_") || g.starts_with("STARTOS_")
                    })
                })
                .map(|p| p.logicalname.clone())
        });

        let use_efi = tokio::fs::metadata("/sys/firmware/efi").await.is_ok();

        let InstallOsResult {
            part_info,
            rootfs,
            mok_enrolled,
        } = install_os_to(
            "/run/live/medium/live/filesystem.squashfs",
            &disk.logicalname,
            disk.capacity,
            disk.partition_table,
            protect.as_ref(),
            crate::ARCH,
            use_efi,
        )
        .await?;

        let config = MountGuard::mount(
            &Bind::new(rootfs.path().join("config")),
            "/media/startos/config",
            ReadWrite,
        )
        .await?;

        let mut info = SetupInfo::default();
        info.mok_enrolled = mok_enrolled;
        info.os_drive = Some(os_drive.clone());
        ctx.install_rootfs.replace(Some((rootfs, config)));

        (info, part_info.data)
    } else {
        let info = IoFormat::Json.from_slice(
            tokio::fs::read_to_string("/media/startos/config/setup.json")
                .await
                .with_ctx(|_| (ErrorKind::Filesystem, "setup.json"))?
                .as_bytes(),
        )?;
        (info, None)
    };

    if let Some(data_drive) = data_drive {
        let mut logicalname = &*data_drive.logicalname;
        if Some(logicalname) == os_drive.as_deref() {
            logicalname = data_part.as_deref().ok_or_else(|| {
                Error::new(
                    eyre!("not enough room on OS drive for data"),
                    ErrorKind::InvalidRequest,
                )
            })?;
        }
        if let Some(guid) = (!data_drive.wipe)
            .then(|| disks.iter())
            .into_iter()
            .flatten()
            .find_map(|d| {
                d.guid
                    .as_ref()
                    .filter(|_| &d.logicalname == logicalname)
                    .cloned()
                    .or_else(|| {
                        d.partitions.iter().find_map(|p| {
                            p.guid
                                .as_ref()
                                .filter(|_| &p.logicalname == logicalname)
                                .cloned()
                        })
                    })
            })
        {
            setup_info.guid = Some(guid);
            setup_info.attach = true;
        } else {
            let guid = crate::setup::setup_data_drive(&ctx, logicalname).await?;
            setup_info.guid = Some(guid);
        }
    }

    write_file_atomic(
        "/media/startos/config/setup.json",
        IoFormat::JsonPretty.to_vec(&setup_info)?,
    )
    .await?;

    Ok(setup_info)
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct CliInstallOsParams {
    #[arg(help = "help.arg.squashfs-image-path")]
    squashfs: PathBuf,
    #[arg(help = "help.arg.target-disk")]
    disk: PathBuf,
    #[arg(long, help = "help.arg.use-efi-boot")]
    efi: Option<bool>,
}

pub async fn cli_install_os(
    _ctx: CliContext,
    CliInstallOsParams {
        squashfs,
        disk,
        efi,
    }: CliInstallOsParams,
) -> Result<OsPartitionInfo, Error> {
    let capacity = get_block_device_size(&disk).await?;
    let partition_table = crate::disk::util::get_partition_table(&disk).await?;

    let arch = probe_squashfs_arch(&squashfs).await?;

    let use_efi = efi.unwrap_or_else(|| !matches!(partition_table, Some(PartitionTable::Mbr)));

    let InstallOsResult {
        part_info,
        rootfs,
        mok_enrolled: _,
    } = install_os_to(
        &squashfs,
        &disk,
        capacity,
        partition_table,
        None::<&str>,
        &*arch,
        use_efi,
    )
    .await?;

    rootfs.unmount().await?;

    Ok(part_info)
}
