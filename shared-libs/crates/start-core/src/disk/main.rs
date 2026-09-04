use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use color_eyre::eyre::eyre;
use imbl_value::InternedString;
use rust_i18n::t;
use tokio::process::Command;
use tracing::instrument;

use super::fsck::ext4::e2fsck_preen_strict;
use super::fsck::{RepairStrategy, RequiresReboot, detect_filesystem};
use super::util::pvscan;
use crate::disk::mount::filesystem::block_dev::BlockDev;
use crate::disk::mount::filesystem::{FileSystem, ReadOnly, ReadWrite};
use crate::disk::mount::util::unmount;
use crate::progress::FullProgressTracker;
use crate::util::Invoke;
use crate::{Error, ErrorKind, ResultExt};

pub const PASSWORD_PATH: &'static str = "/run/startos/password";
pub const DEFAULT_PASSWORD: &'static str = "password";
pub const MAIN_FS_SIZE: FsSize = FsSize::Gigabytes(8);

#[derive(Debug, Clone, Copy)]
pub(crate) enum ImportMode {
    ReadOnly,
    ReadWrite(RepairStrategy),
}
impl ImportMode {
    fn is_read_only(self) -> bool {
        matches!(self, Self::ReadOnly)
    }
}

#[instrument(skip_all)]
pub async fn create<I, P>(
    disks: &I,
    pvscan: &BTreeMap<PathBuf, Option<InternedString>>,
    datadir: impl AsRef<Path>,
    password: Option<&str>,
) -> Result<InternedString, Error>
where
    for<'a> &'a I: IntoIterator<Item = &'a P>,
    P: AsRef<Path>,
{
    let guid = create_pool(disks, pvscan, password.is_some()).await?;
    create_all_fs(&guid, &datadir, password).await?;
    export(&guid, datadir).await?;
    Ok(guid)
}

#[instrument(skip_all)]
pub async fn create_pool<I, P>(
    disks: &I,
    pvscan: &BTreeMap<PathBuf, Option<InternedString>>,
    encrypted: bool,
) -> Result<InternedString, Error>
where
    for<'a> &'a I: IntoIterator<Item = &'a P>,
    P: AsRef<Path>,
{
    Command::new("dmsetup")
        .arg("remove_all") // TODO: find a higher finesse way to do this for portability reasons
        .invoke(crate::ErrorKind::DiskManagement)
        .await?;
    for disk in disks {
        if pvscan.contains_key(disk.as_ref()) {
            Command::new("pvremove")
                .arg("-yff")
                .arg(disk.as_ref())
                .invoke(crate::ErrorKind::DiskManagement)
                .await?;
        }
        tokio::fs::write(disk.as_ref(), &[0; 2048]).await?; // wipe partition table
        Command::new("pvcreate")
            .arg("-yff")
            .arg(disk.as_ref())
            .invoke(crate::ErrorKind::DiskManagement)
            .await?;
    }
    let mut guid = format!(
        "STARTOS_{}",
        base32::encode(
            base32::Alphabet::Rfc4648 { padding: false },
            &rand::random::<[u8; 20]>(),
        )
    );
    if !encrypted {
        guid += "_UNENC";
    }
    let mut cmd = Command::new("vgcreate");
    cmd.arg("-y").arg(&guid);
    for disk in disks {
        cmd.arg(disk.as_ref());
    }
    cmd.invoke(crate::ErrorKind::DiskManagement).await?;
    Ok(guid.into())
}

#[derive(Debug, Clone, Copy)]
pub enum FsSize {
    Gigabytes(usize),
    FreePercentage(usize),
}

#[instrument(skip_all)]
pub async fn create_fs<P: AsRef<Path>>(
    guid: &str,
    datadir: P,
    name: &str,
    size: FsSize,
    password: Option<&str>,
) -> Result<(), Error> {
    let mut cmd = Command::new("lvcreate");
    match size {
        FsSize::Gigabytes(a) => cmd.arg("-L").arg(format!("{}G", a)),
        FsSize::FreePercentage(a) => cmd.arg("-l").arg(format!("{}%FREE", a)),
    };
    cmd.arg("-y")
        .arg("-n")
        .arg(name)
        .arg(guid)
        .invoke(crate::ErrorKind::DiskManagement)
        .await?;
    let mut blockdev_path = Path::new("/dev").join(guid).join(name);
    if let Some(password) = password {
        if let Some(parent) = Path::new(PASSWORD_PATH).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(PASSWORD_PATH, password)
            .await
            .with_ctx(|_| (crate::ErrorKind::Filesystem, PASSWORD_PATH))?;
        Command::new("cryptsetup")
            .arg("-q")
            .arg("luksFormat")
            .arg(format!("--key-file={}", PASSWORD_PATH))
            .arg(format!("--keyfile-size={}", password.len()))
            .arg(&blockdev_path)
            .invoke(crate::ErrorKind::DiskManagement)
            .await?;
        Command::new("cryptsetup")
            .arg("-q")
            .arg("luksOpen")
            .arg("--allow-discards")
            .arg(format!("--key-file={}", PASSWORD_PATH))
            .arg(format!("--keyfile-size={}", password.len()))
            .arg(&blockdev_path)
            .arg(format!("{}_{}", guid, name))
            .invoke(crate::ErrorKind::DiskManagement)
            .await?;
        tokio::fs::remove_file(PASSWORD_PATH)
            .await
            .with_ctx(|_| (crate::ErrorKind::Filesystem, PASSWORD_PATH))?;
        blockdev_path = Path::new("/dev/mapper").join(format!("{}_{}", guid, name));
    }
    Command::new("mkfs.btrfs")
        .arg(&blockdev_path)
        .invoke(crate::ErrorKind::DiskManagement)
        .await?;
    BlockDev::new(&blockdev_path)
        .mount(datadir.as_ref().join(name), ReadWrite)
        .await?;
    Ok(())
}

#[instrument(skip_all)]
pub async fn create_all_fs<P: AsRef<Path>>(
    guid: &str,
    datadir: P,
    password: Option<&str>,
) -> Result<(), Error> {
    create_fs(guid, &datadir, "main", MAIN_FS_SIZE, password).await?;
    create_fs(
        guid,
        &datadir,
        "package-data",
        FsSize::FreePercentage(100),
        password,
    )
    .await?;
    Ok(())
}

async fn open_luks(
    blockdev_path: &Path,
    mapper_name: &str,
    password: &str,
    read_only: bool,
) -> Result<PathBuf, Error> {
    let mut cryptsetup = Command::new("cryptsetup");
    cryptsetup.arg("-q").arg("luksOpen");
    if read_only {
        cryptsetup.arg("--readonly");
    } else {
        cryptsetup.arg("--allow-discards");
    }
    let mut key = std::io::Cursor::new(password.as_bytes());
    cryptsetup
        .arg("--key-file=-")
        .arg(format!("--keyfile-size={}", password.len()))
        .arg(blockdev_path)
        .arg(mapper_name)
        .input(Some(&mut key))
        .invoke(crate::ErrorKind::DiskManagement)
        .await?;
    Ok(Path::new("/dev/mapper").join(mapper_name))
}

async fn close_luks(mapper_name: &str) -> Result<(), Error> {
    Command::new("cryptsetup")
        .arg("-q")
        .arg("luksClose")
        .arg(mapper_name)
        .invoke(crate::ErrorKind::DiskManagement)
        .await?;
    Ok(())
}

async fn set_block_read_only(blockdev_path: &Path, read_only: bool) -> Result<(), Error> {
    Command::new("blockdev")
        .arg(if read_only { "--setro" } else { "--setrw" })
        .arg(blockdev_path)
        .invoke(crate::ErrorKind::DiskManagement)
        .await?;
    Ok(())
}

#[instrument(skip_all)]
pub async fn unmount_fs<P: AsRef<Path>>(guid: &str, datadir: P, name: &str) -> Result<(), Error> {
    unmount(datadir.as_ref().join(name), false).await?;
    let mapper_name = format!("{guid}_{name}");
    if tokio::fs::metadata(Path::new("/dev/mapper").join(&mapper_name))
        .await
        .is_ok()
    {
        close_luks(&mapper_name).await?;
    }

    Ok(())
}

#[instrument(skip_all)]
pub async fn unmount_all_fs<P: AsRef<Path>>(guid: &str, datadir: P) -> Result<(), Error> {
    unmount_fs(guid, &datadir, "main").await?;
    unmount_fs(guid, &datadir, "package-data").await?;
    Command::new("dmsetup")
        .arg("remove_all") // TODO: find a higher finesse way to do this for portability reasons
        .invoke(crate::ErrorKind::DiskManagement)
        .await?;
    Ok(())
}

#[instrument(skip_all)]
pub async fn export<P: AsRef<Path>>(guid: &str, datadir: P) -> Result<(), Error> {
    Command::new("sync").invoke(ErrorKind::Filesystem).await?;
    unmount_all_fs(guid, datadir).await?;
    Command::new("vgchange")
        .arg("-an")
        .arg(guid)
        .invoke(crate::ErrorKind::DiskManagement)
        .await?;
    Command::new("vgexport")
        .arg(guid)
        .invoke(crate::ErrorKind::DiskManagement)
        .await?;
    Ok(())
}

fn record_cleanup_error(first_error: &mut Option<Error>, result: Result<(), Error>) {
    if let Err(error) = result {
        if first_error.is_none() {
            *first_error = Some(error);
        } else {
            tracing::error!(?error, "Additional disk cleanup error");
        }
    }
}

#[instrument(skip_all)]
pub(crate) async fn deactivate<P: AsRef<Path>>(guid: &str, datadir: P) -> Result<(), Error> {
    let mut first_error = None;
    for name in ["package-data", "main"] {
        record_cleanup_error(
            &mut first_error,
            unmount(datadir.as_ref().join(name), false).await,
        );
        let mapper_name = format!("{guid}_{name}");
        if tokio::fs::metadata(Path::new("/dev/mapper").join(&mapper_name))
            .await
            .is_ok()
        {
            record_cleanup_error(&mut first_error, close_luks(&mapper_name).await);
        }
    }
    record_cleanup_error(
        &mut first_error,
        Command::new("vgchange")
            .arg("-an")
            .arg(guid)
            .invoke(crate::ErrorKind::DiskManagement)
            .await
            .map(|_| ()),
    );
    record_cleanup_error(
        &mut first_error,
        Command::new("vgexport")
            .arg(guid)
            .invoke(crate::ErrorKind::DiskManagement)
            .await
            .map(|_| ()),
    );
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[instrument(skip_all)]
pub(crate) async fn import<P: AsRef<Path>>(
    guid: &str,
    datadir: P,
    mode: ImportMode,
    password: Option<&str>,
    progress: Option<&FullProgressTracker>,
) -> Result<RequiresReboot, Error> {
    let scan = pvscan().await?;
    if scan
        .values()
        .filter_map(|a| a.as_ref())
        .filter(|a| a.starts_with("STARTOS_") || a.starts_with("EMBASSY_"))
        .next()
        .is_none()
    {
        return Err(Error::new(
            eyre!("{}", t!("disk.main.disk-not-found")),
            crate::ErrorKind::DiskNotAvailable,
        ));
    }
    if !scan
        .values()
        .filter_map(|a| a.as_ref())
        .any(|id| id == guid)
    {
        return Err(Error::new(
            eyre!("{}", t!("disk.main.incorrect-disk")),
            crate::ErrorKind::IncorrectDisk,
        ));
    }
    Command::new("dmsetup")
        .arg("remove_all") // TODO: find a higher finesse way to do this for portability reasons
        .invoke(crate::ErrorKind::DiskManagement)
        .await?;
    match Command::new("vgimport")
        .arg(guid)
        .invoke(crate::ErrorKind::DiskManagement)
        .await
    {
        Ok(_) => Ok(()),
        Err(e)
            if format!("{}", e.source)
                .lines()
                .any(|l| l.trim() == format!("Volume group \"{}\" is not exported", guid)) =>
        {
            Ok(())
        }
        Err(e) => Err(e),
    }?;
    Command::new("vgchange")
        .arg("-ay")
        .arg(guid)
        .invoke(crate::ErrorKind::DiskManagement)
        .await?;
    mount_all_fs(guid, datadir, mode, password, progress).await
}

#[instrument(skip_all)]
pub(crate) async fn mount_fs<P: AsRef<Path>>(
    guid: &str,
    datadir: P,
    name: &str,
    mode: ImportMode,
    password: Option<&str>,
    progress: Option<&FullProgressTracker>,
) -> Result<RequiresReboot, Error> {
    let orig_path = Path::new("/dev").join(guid).join(name);
    let mapper_name = format!("{guid}_{name}");
    let encrypted = !guid.ends_with("_UNENC");
    let password = password.unwrap_or(DEFAULT_PASSWORD);
    if mode.is_read_only() {
        set_block_read_only(&orig_path, true).await?;
    }
    let mut blockdev_path = if encrypted {
        open_luks(&orig_path, &mapper_name, password, mode.is_read_only()).await?
    } else {
        orig_path.clone()
    };

    if matches!(mode, ImportMode::ReadWrite(_))
        && detect_filesystem(&blockdev_path).await? == "ext2"
    {
        let mut convert_phase =
            progress.map(|p| p.add_phase(t!("disk.main.converting-to-btrfs").into(), Some(50)));
        if let Some(ref mut phase) = convert_phase {
            phase.start();
        }
        tracing::info!("{}", t!("disk.main.running-e2fsck", name = name));
        // e2fsck exit codes: 0 = no errors, 1 = errors corrected, 2 = corrected + reboot needed
        // Only codes >= 4 indicate actual failure, so we can't use .invoke() which treats any
        // non-zero exit as an error.
        let e2fsck_output = Command::new("e2fsck")
            .arg("-fy")
            .arg(&blockdev_path)
            .kill_on_drop(true)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await
            .with_kind(ErrorKind::DiskManagement)?;
        let e2fsck_exit = e2fsck_output.status.code().unwrap_or(4);
        if e2fsck_exit >= 4 {
            let msg = std::str::from_utf8(if e2fsck_output.stderr.is_empty() {
                &e2fsck_output.stdout
            } else {
                &e2fsck_output.stderr
            })
            .unwrap_or("e2fsck failed");
            return Err(Error::new(eyre!("{msg}"), ErrorKind::DiskManagement));
        }
        tracing::info!("{}", t!("disk.main.converting-ext4-to-btrfs", name = name));
        Command::new("btrfs-convert")
            .arg(&blockdev_path)
            .capture(false)
            .invoke(ErrorKind::DiskManagement)
            .await?;
        let tmp_mount = datadir.as_ref().join(format!("{name}.convert-tmp"));
        tokio::fs::create_dir_all(&tmp_mount).await?;
        BlockDev::new(&blockdev_path)
            .mount(&tmp_mount, ReadWrite)
            .await?;
        tracing::info!("{}", t!("disk.main.clearing-duplicate-files"));
        Command::new("btrfs")
            .args(["subvolume", "delete"])
            .arg(tmp_mount.join("ext2_saved"))
            .invoke(ErrorKind::DiskManagement)
            .await?;
        tracing::info!("{}", t!("disk.main.optimizing-filesystem"));
        Command::new("btrfs")
            .args(["filesystem", "defragment", "-r"])
            .arg(&tmp_mount)
            .invoke(ErrorKind::DiskManagement)
            .await?;
        unmount(&tmp_mount, false).await?;
        tokio::fs::remove_dir(&tmp_mount).await?;
        if let Some(ref mut phase) = convert_phase {
            phase.complete();
        }
    }

    let mut reboot = match mode {
        ImportMode::ReadOnly => RequiresReboot(false),
        ImportMode::ReadWrite(repair) => {
            let reboot = repair.fsck(&blockdev_path).await?;
            if !guid.ends_with("_UNENC") {
                let luks_folder = Path::new("/media/startos/config/luks");
                tokio::fs::create_dir_all(luks_folder).await?;
                let tmp_luks_bak = luks_folder.join(format!(".{mapper_name}.luks.bak.tmp"));
                if tokio::fs::metadata(&tmp_luks_bak).await.is_ok() {
                    tokio::fs::remove_file(&tmp_luks_bak).await?;
                }
                let luks_bak = luks_folder.join(format!("{mapper_name}.luks.bak"));
                Command::new("cryptsetup")
                    .arg("-q")
                    .arg("luksHeaderBackup")
                    .arg("--header-backup-file")
                    .arg(&tmp_luks_bak)
                    .arg(&orig_path)
                    .invoke(crate::ErrorKind::DiskManagement)
                    .await?;
                tokio::fs::rename(&tmp_luks_bak, &luks_bak).await?;
            }
            reboot
        }
    };

    let mountpoint = datadir.as_ref().join(name);
    match mode {
        ImportMode::ReadOnly => {
            let initial_error = match BlockDev::new(&blockdev_path)
                .mount(&mountpoint, ReadOnly)
                .await
            {
                Ok(()) => return Ok(reboot),
                Err(error) => error,
            };
            let fs_type = match detect_filesystem(&blockdev_path).await.as_deref() {
                Ok(fs_type @ ("ext2" | "btrfs")) => fs_type.to_owned(),
                _ => return Err(initial_error),
            };

            if encrypted {
                close_luks(&mapper_name).await?;
            }
            set_block_read_only(&orig_path, false).await?;
            if encrypted {
                blockdev_path = match open_luks(&orig_path, &mapper_name, password, false).await {
                    Ok(blockdev_path) => blockdev_path,
                    Err(error) => {
                        set_block_read_only(&orig_path, true).await.log_err();
                        return Err(error);
                    }
                };
            }

            let recovery_result = match fs_type.as_str() {
                "ext2" => e2fsck_preen_strict(&blockdev_path).await,
                "btrfs" => {
                    async {
                        BlockDev::new(&blockdev_path)
                            .mount(&mountpoint, ReadOnly)
                            .await?;
                        unmount(&mountpoint, false).await?;
                        Ok(RequiresReboot(false))
                    }
                    .await
                }
                _ => unreachable!(),
            };
            let close_result = if encrypted {
                close_luks(&mapper_name).await
            } else {
                Ok(())
            };
            let protect_result = set_block_read_only(&orig_path, true).await;
            let recovery_reboot = match recovery_result {
                Ok(reboot) => reboot,
                Err(error) => {
                    close_result.log_err();
                    protect_result.log_err();
                    return Err(error);
                }
            };
            close_result?;
            protect_result?;
            reboot |= recovery_reboot;

            if encrypted {
                blockdev_path = open_luks(&orig_path, &mapper_name, password, true).await?;
            }
            BlockDev::new(&blockdev_path)
                .mount(&mountpoint, ReadOnly)
                .await?;
        }
        ImportMode::ReadWrite(_) => {
            BlockDev::new(&blockdev_path)
                .mount(&mountpoint, ReadWrite)
                .await?
        }
    }

    Ok(reboot)
}

#[instrument(skip_all)]
pub(crate) async fn mount_all_fs<P: AsRef<Path>>(
    guid: &str,
    datadir: P,
    mode: ImportMode,
    password: Option<&str>,
    progress: Option<&FullProgressTracker>,
) -> Result<RequiresReboot, Error> {
    let mut reboot = RequiresReboot(false);
    reboot |= mount_fs(guid, &datadir, "main", mode, password, progress).await?;
    reboot |= mount_fs(guid, &datadir, "package-data", mode, password, progress).await?;
    Ok(reboot)
}

/// Probes `package-data` on an inactive or exported volume group.
#[instrument(skip_all)]
pub async fn probe_package_data_fs(guid: &str) -> Result<Option<String>, Error> {
    let mapper_name = format!("{guid}_package-data");
    let lv_path = Path::new("/dev").join(guid).join("package-data");
    let blockdev_path = if !guid.ends_with("_UNENC") {
        Path::new("/dev/mapper").join(&mapper_name)
    } else {
        lv_path.clone()
    };
    if tokio::fs::metadata(&blockdev_path).await.is_ok() {
        return detect_filesystem(&blockdev_path).await.map(Some);
    }
    let was_active = tokio::fs::metadata(&lv_path).await.is_ok();

    let imported = match Command::new("vgimport")
        .arg(guid)
        .invoke(ErrorKind::DiskManagement)
        .await
    {
        Ok(_) => true,
        Err(e)
            if format!("{}", e.source)
                .lines()
                .any(|l| l.trim() == format!("Volume group \"{}\" is not exported", guid)) =>
        {
            false
        }
        Err(e) => {
            tracing::warn!(
                "{}",
                t!("disk.main.could-not-import-vg", guid = guid, error = e)
            );
            return Ok(None);
        }
    };
    if let Err(e) = Command::new("vgchange")
        .arg("-ay")
        .arg(guid)
        .invoke(ErrorKind::DiskManagement)
        .await
    {
        if !was_active {
            Command::new("vgchange")
                .arg("-an")
                .arg(guid)
                .invoke(ErrorKind::DiskManagement)
                .await
                .log_err();
        }
        if imported {
            Command::new("vgexport")
                .arg(guid)
                .invoke(ErrorKind::DiskManagement)
                .await
                .log_err();
        }
        tracing::warn!(
            "{}",
            t!("disk.main.could-not-activate-vg", guid = guid, error = e)
        );
        return Ok(None);
    }

    let mut opened_luks = false;
    let result = async {
        if tokio::fs::metadata(&lv_path).await.is_err() {
            return Ok(None);
        }

        let blockdev_path = if !guid.ends_with("_UNENC") {
            let blockdev_path = open_luks(&lv_path, &mapper_name, DEFAULT_PASSWORD, true).await?;
            opened_luks = true;
            blockdev_path
        } else {
            lv_path
        };

        detect_filesystem(&blockdev_path).await.map(Some)
    }
    .await;

    if opened_luks {
        close_luks(&mapper_name).await.log_err();
    }
    if !was_active {
        Command::new("vgchange")
            .arg("-an")
            .arg(guid)
            .invoke(ErrorKind::DiskManagement)
            .await
            .log_err();
    }
    if imported {
        Command::new("vgexport")
            .arg(guid)
            .invoke(ErrorKind::DiskManagement)
            .await
            .log_err();
    }

    result
}
