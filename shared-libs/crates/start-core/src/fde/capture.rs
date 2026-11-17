//! Fresh-authorized configuration capture; unpublished candidates are not active protection.

#[cfg(test)]
#[path = "capture_tests.rs"]
mod tests;

use std::ffi::{CString, OsString};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use futures::StreamExt;
use nix::mount::{MsFlags, mount};
use nix::sys::memfd::{MFdFlags, memfd_create};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::process::Command;

use super::generation::capture_configuration;
use super::initramfs::{BootMode, load_configuration};
use super::memory::ProcessSecretMemory;
use super::namespace::invalid_source;
use super::private_config::ProjectedConfig;
use crate::context::RpcContext;
use crate::disk::mount::filesystem::bind::Bind;
use crate::disk::mount::filesystem::private_tmpfs::PrivateTmpfs;
use crate::disk::mount::filesystem::{ReadOnly, ReadWrite};
use crate::disk::mount::guard::MountGuard;
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
use crate::util::Invoke;
use crate::util::io::{TmpDir, open_file, read_secret_file, write_file_private_atomic};
use crate::util::squashfs::Squashfs;

const REQUESTS: &str = "/run/startos-fde-capture";
const PUBLIC: &str = "/media/startos/current";
const IMAGES: &str = "/media/startos/images";
const CONFIGURATION: &str = "/media/startos/config/overlay";
const CAPTURE_FILE: &str = "configuration.squashfs";
const KEYS_FILE: &str = "host-keys.json";

nix::ioctl_readwrite!(freeze_filesystem, b'X', 119, libc::c_int);
nix::ioctl_readwrite!(thaw_filesystem, b'X', 120, libc::c_int);

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum ConfigurationSource {
    Legacy { path: PathBuf },
    Working { lower: PathBuf, upper: PathBuf },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReadEpoch {
    base_root: PathBuf,
    data_root: PathBuf,
    source: ConfigurationSource,
    frozen_filesystems: Vec<PathBuf>,
    readonly_filesystems: Vec<PathBuf>,
}

impl ReadEpoch {
    async fn current() -> Result<Self, Error> {
        let mut readonly = vec!["/".into(), CONFIGURATION.into()];
        let source = match BootMode::current().await? {
            BootMode::Ordinary => ConfigurationSource::Legacy {
                path: CONFIGURATION.into(),
            },
            BootMode::Protected => {
                readonly.push(super::initramfs::RUNTIME.into());
                let configuration = load_configuration().await?;
                if !crate::util::io::same_file(&configuration.rootfs, Path::new(PUBLIC)).await? {
                    return Err(invalid_source());
                }
                ConfigurationSource::Working {
                    lower: configuration.overlay,
                    upper: configuration.working_upper,
                }
            }
        };
        Self::new(
            source,
            PUBLIC.into(),
            crate::DATA_DIR.into(),
            &[
                "/media/startos/root".into(),
                crate::MAIN_DATA.into(),
                crate::PACKAGE_DATA.into(),
            ],
            &readonly,
        )
        .await
    }

    async fn new(
        source: ConfigurationSource,
        base_root: PathBuf,
        data_root: PathBuf,
        filesystems: &[PathBuf],
        readonly_candidates: &[PathBuf],
    ) -> Result<Self, Error> {
        let mut frozen_filesystems = Vec::new();
        let mut devices = std::collections::BTreeSet::new();
        for path in filesystems {
            if nix::sys::statfs::statfs(path)
                .with_kind(ErrorKind::Filesystem)?
                .filesystem_type()
                != nix::sys::statfs::BTRFS_SUPER_MAGIC
            {
                return Err(invalid_source());
            }
            if devices.insert(crate::util::io::file_identity(path).await?.0) {
                frozen_filesystems.push(path.to_owned());
            }
        }
        let mut readonly_filesystems = Vec::new();
        for path in readonly_candidates {
            let status = nix::sys::statfs::statfs(path).with_kind(ErrorKind::Filesystem)?;
            if matches!(
                status.filesystem_type(),
                nix::sys::statfs::OVERLAYFS_SUPER_MAGIC | nix::sys::statfs::TMPFS_MAGIC
            ) && !status
                .flags()
                .contains(nix::sys::statvfs::FsFlags::ST_RDONLY)
            {
                readonly_filesystems.push(path.to_owned());
            }
        }
        Ok(Self {
            base_root,
            data_root,
            source,
            frozen_filesystems,
            readonly_filesystems,
        })
    }

    async fn snapshot(
        &self,
    ) -> Result<
        crate::util::squashfs::Squashfs<crate::s9pk::merkle_archive::source::DynFileSource>,
        Error,
    > {
        Ok(match &self.source {
            ConfigurationSource::Legacy { path } => {
                Squashfs::from_directory(path).await?.into_dyn()
            }
            ConfigurationSource::Working { lower, upper } => {
                Squashfs::from_working_overlay(lower, upper).await?
            }
        })
    }

    fn enter(&self) -> Result<(), Error> {
        for path in &self.readonly_filesystems {
            remount(path, true)?;
        }
        for path in &self.frozen_filesystems {
            frozen(path, true)?;
        }
        Ok(())
    }

    fn release(&self) -> Result<(), Error> {
        let mut first = None;
        for result in self
            .frozen_filesystems
            .iter()
            .rev()
            .map(|path| frozen(path, false))
            .chain(
                self.readonly_filesystems
                    .iter()
                    .rev()
                    .map(|path| remount(path, false)),
            )
        {
            if let Err(error) = result {
                first.get_or_insert(error);
            }
        }
        first.map_or(Ok(()), Err)
    }
}

fn remount(path: &Path, readonly: bool) -> Result<(), Error> {
    let flags = MsFlags::MS_REMOUNT
        | if readonly {
            MsFlags::MS_RDONLY
        } else {
            MsFlags::empty()
        };
    mount(None::<&str>, path, None::<&str>, flags, None::<&str>).with_kind(ErrorKind::Filesystem)
}

fn frozen(path: &Path, freeze: bool) -> Result<(), Error> {
    let file = std::fs::File::open(path)?;
    let mut ignored = 0;
    let result = unsafe {
        if freeze {
            freeze_filesystem(file.as_raw_fd(), &mut ignored)
        } else {
            thaw_filesystem(file.as_raw_fd(), &mut ignored)
        }
    };
    match result {
        Ok(_) => Ok(()),
        Err(nix::errno::Errno::EINVAL) if !freeze => Ok(()),
        Err(error) => Err(Error::new(error, ErrorKind::Filesystem)),
    }
}

fn anonymous_file() -> Result<tokio::fs::File, Error> {
    let fd = memfd_create(
        CString::new("startos-private-capture").unwrap().as_c_str(),
        MFdFlags::MFD_CLOEXEC | MFdFlags::MFD_ALLOW_SEALING,
    )
    .with_kind(ErrorKind::Filesystem)?;
    Ok(tokio::fs::File::from_std(std::fs::File::from(fd)))
}

async fn request(path: &Path) -> Result<ReadEpoch, Error> {
    let file = crate::os_overlay::regular_file(path).await?;
    let metadata = file.metadata().await?;
    if metadata.uid() != 0 || metadata.mode() & 0o077 != 0 {
        return Err(invalid_source());
    }
    Ok(serde_json::from_slice(&read_secret_file(file).await?)
        .with_kind(ErrorKind::Deserialization)?)
}

/// Runs only in the operation unit's private mount namespace.
/// Its native stop cleanup owns restoration even after helper/requester death.
pub(crate) async fn run(path: &Path) -> Result<(), Error> {
    let memory = ProcessSecretMemory::protect()?;
    let epoch = request(path).await?;
    let scratch = MountGuard::mount(&PrivateTmpfs::new(), TmpDir::BASE, ReadWrite).await?;
    epoch.enter()?;
    let result = async {
        let base = disposable_view(&epoch.base_root).await?;
        let data = disposable_view(&epoch.data_root).await?;
        let capture = capture_configuration(epoch.snapshot().await?, base, data).await?;
        epoch.release()?;
        let exported = async {
            let mut archive = tokio_tar::Builder::new(tokio::fs::File::from_std(
                nix::unistd::dup(std::io::stdout())
                    .with_kind(ErrorKind::Filesystem)?
                    .into(),
            ));
            archive
                .append_file(CAPTURE_FILE, &mut open_file(capture.image()).await?)
                .await?;
            let keys: Vec<_> = capture
                .host_keys()
                .iter()
                .map(|path| path.as_os_str().to_owned())
                .collect();
            let bytes = serde_json::to_vec(&keys).with_kind(ErrorKind::Serialization)?;
            let mut header = tokio_tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o600);
            header.set_cksum();
            archive
                .append_data(&mut header, KEYS_FILE, bytes.as_slice())
                .await?;
            archive.into_inner().await?;
            memory.check()?;
            Ok::<_, Error>(())
        }
        .await;
        let cleanup = capture.unmount().await;
        exported?;
        cleanup
    }
    .await;
    let cleanup = scratch.unmount(false).await;
    result?;
    cleanup
}

/// Thaws before PID1 waits for killed descendants blocked on frozen filesystems.
pub(crate) async fn stop(path: &Path, unit: &str) -> Result<(), Error> {
    let epoch = request(path).await?;
    let killed = if std::env::var_os("MAINPID").is_some() {
        Command::new("/usr/bin/systemctl")
            .args(["kill", "--kill-whom=main", "--signal=KILL", unit])
            .timeout(Some(std::time::Duration::from_secs(10)))
            .invoke(ErrorKind::Filesystem)
            .await
    } else {
        Ok(Vec::new())
    };
    let restored = epoch.release();
    killed?;
    restored
}

pub(crate) async fn release(path: &Path) -> Result<(), Error> {
    request(path).await?.release()?;
    crate::util::io::delete_file(path).await
}

async fn disposable_view(source: &Path) -> Result<MountGuard, Error> {
    let target = TmpDir::new().await?;
    let path = target.to_path_buf();
    target.leak();
    MountGuard::mount(&Bind::new(source).recursive(true), path, ReadOnly).await
}

#[derive(Deserialize)]
struct LoopDevices {
    loopdevices: Vec<LoopBacking>,
}
#[derive(Deserialize)]
struct LoopBacking {
    #[serde(rename = "maj:min")]
    device: String,
    #[serde(rename = "back-maj:min")]
    backing_device: String,
    #[serde(rename = "back-ino")]
    inode: u64,
}

fn device_name(device: u64) -> String {
    format!(
        "{}:{}",
        nix::sys::stat::major(device),
        nix::sys::stat::minor(device)
    )
}

async fn running_image() -> Result<PathBuf, Error> {
    if BootMode::current().await? == BootMode::Protected {
        let configuration = load_configuration().await?;
        let image = configuration.descriptor.rootfs_path(Path::new(IMAGES))?;
        if crate::os_overlay::digest_file(&image).await? != configuration.rootfs_sha256 {
            return Err(invalid_source());
        }
        return Ok(image);
    }
    loop_backing_image(Path::new(PUBLIC), Path::new(IMAGES)).await
}

async fn loop_backing_image(base: &Path, images: &Path) -> Result<PathBuf, Error> {
    let device = device_name(crate::util::io::file_identity(base).await?.0);
    let bytes = Command::new("/usr/sbin/losetup")
        .args([
            "--list",
            "--json",
            "--output",
            "MAJ:MIN,BACK-MAJ:MIN,BACK-INO",
        ])
        .invoke(ErrorKind::Filesystem)
        .await?;
    let loops: LoopDevices =
        serde_json::from_slice(&bytes).with_kind(ErrorKind::Deserialization)?;
    let backing = loops
        .loopdevices
        .iter()
        .find(|item| item.device == device)
        .ok_or_else(invalid_source)?;
    let mut entries = tokio::fs::read_dir(images).await?;
    while let Some(entry) = entries.next_entry().await? {
        let metadata = tokio::fs::symlink_metadata(entry.path()).await?;
        if metadata.is_file()
            && metadata.ino() == backing.inode
            && device_name(metadata.dev()) == backing.backing_device
        {
            return Ok(entry.path());
        }
    }
    Err(invalid_source())
}

async fn receive(mut archive: tokio::fs::File) -> Result<(tokio::fs::File, Vec<PathBuf>), Error> {
    archive.rewind().await?;
    let mut archive = tokio_tar::Archive::new(archive);
    let mut entries = archive.entries()?;
    let mut snapshot = None;
    let mut keys = None;
    while let Some(entry) = entries.next().await {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            return Err(invalid_source());
        }
        match entry.path()?.as_ref() {
            path if path == Path::new(CAPTURE_FILE) && snapshot.is_none() => {
                let mut output = anonymous_file()?;
                tokio::io::copy(&mut entry, &mut output).await?;
                output.rewind().await?;
                snapshot = Some(output);
            }
            path if path == Path::new(KEYS_FILE) && keys.is_none() => {
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes).await?;
                let paths: Vec<OsString> =
                    serde_json::from_slice(&bytes).with_kind(ErrorKind::Deserialization)?;
                keys = Some(paths.into_iter().map(PathBuf::from).collect());
            }
            _ => return Err(invalid_source()),
        }
    }
    Ok((
        snapshot.ok_or_else(invalid_source)?,
        keys.ok_or_else(invalid_source)?,
    ))
}

/// Executable and request paths must be literal systemd arguments.
async fn capture_in_unit(
    epoch: &ReadEpoch,
    executable: &Path,
    unit: &str,
) -> Result<(tokio::fs::File, Vec<PathBuf>), Error> {
    tokio::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(REQUESTS)
        .await?;
    let path = Path::new(REQUESTS).join(format!("{}.json", Guid::new()));
    write_file_private_atomic(
        &path,
        serde_json::to_vec(&epoch).with_kind(ErrorKind::Serialization)?,
    )
    .await?;
    let archive = anonymous_file()?;
    let result = Command::new("/usr/bin/systemd-run")
        .args(["--quiet", "--wait", "--pipe", "--collect"])
        .arg(format!("--unit={unit}"))
        .arg("--property=KillMode=control-group")
        // PID-namespace init ignores SIGTERM without a handler.
        .arg("--property=KillSignal=SIGKILL")
        .arg("--property=TimeoutStopSec=30s")
        .arg("--property=RuntimeMaxSec=300s")
        .arg(format!(
            "--property=ExecStop={} start-fde-stop {} {unit}",
            executable.display(),
            path.display()
        ))
        .arg(format!(
            "--property=ExecStopPost={} start-fde-release {}",
            executable.display(),
            path.display()
        ))
        .args([
            "--",
            "/usr/bin/unshare",
            "--mount",
            "--propagation=slave",
            "--",
        ])
        .arg(executable)
        .arg("start-fde-capture")
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::from(archive.try_clone().await?.into_std().await))
        .stderr(Stdio::piped())
        .capture(false)
        .invoke(ErrorKind::Filesystem)
        .await;
    if result.is_err() {
        Command::new("/usr/bin/systemctl")
            .arg("stop")
            .arg(unit)
            .invoke(ErrorKind::Filesystem)
            .await
            .log_err();
        if stopped_unit(unit).await? && tokio::fs::try_exists(&path).await? {
            release(&path).await?;
        }
    }
    result?;
    receive(archive).await
}

async fn stopped_unit(unit: &str) -> Result<bool, Error> {
    let load = Command::new("/usr/bin/systemctl")
        .args(["show", "--value", "--property=LoadState", unit])
        .invoke(ErrorKind::Filesystem)
        .await?;
    if load == b"not-found\n" {
        return Ok(true);
    }
    let active = Command::new("/usr/bin/systemctl")
        .args(["show", "--value", "--property=ActiveState", unit])
        .invoke(ErrorKind::Filesystem)
        .await?;
    Ok(active == b"inactive\n" || active == b"failed\n")
}

/// Caller owns password/keyslot serialization and fresh whole-configuration authorization.
/// The image-store reservation excludes native update/pruning through manifest preparation.
/// The operation request contains paths, never signing or unlock secrets.
/// Unpublished preparation does not admit the base as a public builder source.
pub(crate) async fn prepare(ctx: &RpcContext) -> Result<String, Error> {
    let memory = ProcessSecretMemory::protect()?;
    let _images = crate::os_overlay::reserve_image_store(false).await?;
    let rootfs = running_image().await?;
    let epoch = ReadEpoch::current().await?;
    let pause = ctx.db.pause_mutations().await;
    let unit = format!("startos-fde-capture-{}", Guid::new());
    let captured = capture_in_unit(&epoch, Path::new("/usr/bin/startbox"), &unit).await;
    drop(pause);
    let (image, keys) = captured?;
    let snapshot = Squashfs::deserialize(MultiCursorFile::from(image))
        .await?
        .into_dyn();
    let enrollment = ctx
        .db
        .peek()
        .await
        .as_private()
        .as_fde_enrollment()
        .de()?
        .ok_or_else(invalid_source)?;
    let projected = ProjectedConfig::stage(snapshot, Path::new(crate::MAIN_DATA), &keys).await?;
    let store = super::boot::generation_store(Path::new("/media/startos/config"));
    tokio::fs::create_dir_all(&store).await?;
    let prepared = projected
        .prepare_generation(&rootfs, &store, &enrollment.manifest_key)
        .await?;
    memory.check()?;
    Ok(prepared.name().to_owned())
}
