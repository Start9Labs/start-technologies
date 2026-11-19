//! Fresh-authorized configuration capture; unpublished candidates are not active protection.

#[cfg(test)]
#[path = "capture_tests.rs"]
mod tests;

use std::ffi::{CString, OsString};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use nix::sys::memfd::{MFdFlags, memfd_create};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncSeekExt;
use tokio::process::Command;
use zeroize::Zeroizing;

use super::generation::capture_configuration;
use super::initramfs::{BootMode, load_configuration};
use super::memory::ProcessSecretMemory;
use super::namespace::invalid_source;
use super::private_config::ProjectedConfig;
use crate::context::RpcContext;
use crate::disk::mount::filesystem::bind::{Bind, IsolatedBind};
use crate::disk::mount::filesystem::loop_dev::SquashfsImage;
use crate::disk::mount::filesystem::private_tmpfs::PrivateTmpfs;
use crate::disk::mount::filesystem::{ReadOnly, ReadWrite};
use crate::disk::mount::guard::{GenericMountGuard, MountGuard};
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
use crate::util::Invoke;
use crate::util::io::{TmpDir, read_secret_file, write_file_private_atomic};
use crate::util::squashfs::Squashfs;

const REQUESTS: &str = "/run/startos-fde-capture";
const PUBLIC: &str = "/media/startos/current";
const IMAGES: &str = "/media/startos/images";
const CONFIGURATION: &str = "/media/startos/config/overlay";
const CAPTURE_FILE: &str = "configuration.squashfs";
const KEYS_FILE: &str = "host-keys.json";
const PROFILES_FILE: &str = "network-profiles.json";
const PROFILE_CONTENTS: &str = "network-profiles";

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
    network_profiles: PathBuf,
    source: ConfigurationSource,
    #[serde(default = "default_generation_store")]
    generation_store: PathBuf,
    frozen_filesystems: Vec<PathBuf>,
    readonly_filesystems: Vec<PathBuf>,
}

fn default_generation_store() -> PathBuf {
    super::boot::generation_store(Path::new(CONFIGURATION).parent().unwrap())
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
            crate::net::wifi::SAVED_PROFILE_DIRECTORY.into(),
            default_generation_store(),
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
        network_profiles: PathBuf,
        generation_store: PathBuf,
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
                return Err(Error::new(
                    eyre!(t!(
                        "fde.capture.unsupported-filesystem",
                        path = path.display()
                    )),
                    ErrorKind::InvalidRequest,
                ));
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
            network_profiles,
            source,
            generation_store,
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

    async fn enter(&self) -> Result<(), Error> {
        for path in &self.readonly_filesystems {
            remount(path, true).await?;
        }
        for path in &self.frozen_filesystems {
            frozen(path, true)?;
        }
        Ok(())
    }

    async fn release(&self) -> Result<(), Error> {
        let mut first = None;
        for path in self.frozen_filesystems.iter().rev() {
            if let Err(error) = frozen(path, false) {
                first.get_or_insert(error);
            }
        }
        for path in self.readonly_filesystems.iter().rev() {
            if let Err(error) = remount(path, false).await {
                first.get_or_insert(error);
            }
        }
        first.map_or(Ok(()), Err)
    }
}

pub(super) async fn check_eligibility() -> Result<(), Error> {
    ReadEpoch::current().await?;
    IsolatedBind::new(PUBLIC).check_readonly()
}

/// Preserves native mount options while changing filesystem writability.
async fn remount(path: &Path, readonly: bool) -> Result<(), Error> {
    let flags = invoke_mount_tool(
        Command::new("/usr/bin/findmnt")
            .args([
                "--kernel",
                "--tab-file",
                "/proc/self/mountinfo",
                "--first-only",
                "--direction=backward",
                "--noheadings",
                "--vfs-all",
                "--output",
                "VFS-OPTIONS",
                "--mountpoint",
            ])
            .arg(path),
    )
    .await?;
    let flags = std::str::from_utf8(&flags)
        .with_kind(ErrorKind::Filesystem)?
        .trim_end();
    let mode = if readonly { "ro" } else { "rw" };
    invoke_mount_tool(
        Command::new("/usr/bin/mount")
            .args([
                "--internal-only",
                "--no-mtab",
                "--options-source=mtab",
                "--options-mode=ignore",
                "--options",
            ])
            .arg(format!("{flags},remount,{mode}"))
            .arg("--target")
            .arg(path),
    )
    .await?;
    Ok(())
}

async fn invoke_mount_tool(command: &mut Command) -> Result<Vec<u8>, Error> {
    command
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .env("LC_ALL", "C")
        .env("LIBMOUNT_MTAB", "/proc/self/mountinfo")
        .timeout(Some(std::time::Duration::from_secs(10)))
        .invoke(ErrorKind::Filesystem)
        .await
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

/// Nonsecret paths and timestamps only; credential bytes use raw archive members.
fn append_json<T: Serialize>(
    archive: &mut tar::Builder<std::fs::File>,
    name: &str,
    value: &T,
) -> Result<(), Error> {
    let bytes = Zeroizing::new(serde_json::to_vec(value).with_kind(ErrorKind::Serialization)?);
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o600);
    header.set_cksum();
    archive.append_data(&mut header, name, bytes.as_slice())?;
    Ok(())
}

/// Profile bytes bypass JSON encoding and intermediate async file buffers.
fn append_profiles(
    archive: &mut tar::Builder<std::fs::File>,
    profiles: &crate::net::wifi::NetworkProfiles,
) -> Result<(), Error> {
    let metadata: std::collections::BTreeMap<_, _> = profiles
        .iter()
        .map(|(name, profile)| (name, profile.modified))
        .collect();
    append_json(archive, PROFILES_FILE, &metadata)?;
    for (name, profile) in profiles {
        let mut header = tar::Header::new_gnu();
        header.set_size(profile.contents.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        archive.append_data(
            &mut header,
            Path::new(PROFILE_CONTENTS).join(name),
            profile.contents.as_bytes(),
        )?;
    }
    Ok(())
}

/// Runs only in the operation unit's private mount namespace.
/// Its native stop cleanup owns restoration even after helper/requester death.
pub(crate) async fn run(path: &Path) -> Result<(), Error> {
    let memory = ProcessSecretMemory::protect()?;
    let epoch = request(path).await?;
    let scratch = MountGuard::mount(&PrivateTmpfs::new(), TmpDir::BASE, ReadWrite).await?;
    epoch.enter().await?;
    let result = async {
        let snapshot = epoch.snapshot().await?;
        configuration_backings(
            Path::new("/"),
            &epoch.data_root.join(crate::MAIN_FILESYSTEM),
            &epoch.generation_store,
            &snapshot,
        )
        .await?;
        let base = base_image_view(&epoch.base_root).await?;
        let data = data_view(&epoch.data_root).await?;
        let capture = capture_configuration(snapshot, base, data).await?;
        let profiles = crate::net::wifi::read_network_profiles(&epoch.network_profiles).await?;
        epoch.release().await?;
        let exported = async {
            let stdout = std::fs::File::from(
                nix::unistd::dup(std::io::stdout()).with_kind(ErrorKind::Filesystem)?,
            );
            let image = capture.image().to_owned();
            let keys: Vec<_> = capture
                .host_keys()
                .iter()
                .map(|path| path.as_os_str().to_owned())
                .collect();
            tokio::task::spawn_blocking(move || -> Result<(), Error> {
                let mut archive = tar::Builder::new(stdout);
                archive.append_file(CAPTURE_FILE, &mut std::fs::File::open(image)?)?;
                append_json(&mut archive, KEYS_FILE, &keys)?;
                append_profiles(&mut archive, &profiles)?;
                archive.into_inner()?;
                Ok(())
            })
            .await
            .with_kind(ErrorKind::Unknown)??;
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
    killed?;
    epoch.release().await
}

pub(crate) async fn release(path: &Path) -> Result<(), Error> {
    request(path).await?.release().await?;
    crate::util::io::delete_file(path).await
}

/// Host submounts are outside the rootfs image commitment.
async fn base_image_view(source: &Path) -> Result<MountGuard, Error> {
    capture_view(&IsolatedBind::new(source)).await
}

async fn configuration_backings(
    root: &Path,
    data: &Path,
    store: &Path,
    working: &Squashfs<crate::s9pk::merkle_archive::source::DynFileSource>,
) -> Result<std::collections::BTreeSet<Guid>, Error> {
    let data = capture_view(&IsolatedBind::new(data))
        .await?
        .with_strict_unmount();
    let mut mounts = super::root::VerifiedMounts::new(data);
    let result = async {
        let view = capture_view(&IsolatedBind::new(root))
            .await?
            .with_strict_unmount();
        let path = view.path().to_owned();
        mounts.push(view);
        let snapshot = Squashfs::from_directory(path).await?.into_dyn();
        let data = mounts.lower().path().to_owned();
        let mut retained = super::private_config::referenced_backings(&snapshot, &data).await?;
        retained.extend(super::private_config::referenced_backings(working, &data).await?);
        let generations = capture_view(&IsolatedBind::new(store))
            .await?
            .with_strict_unmount();
        let path = generations.path().to_owned();
        mounts.push(generations);
        for image in crate::os_overlay::published_overlays(&path).await? {
            let view = capture_view(&SquashfsImage::new(&image))
                .await?
                .with_strict_unmount();
            let result = async {
                let snapshot = Squashfs::from_directory(view.path()).await?.into_dyn();
                super::private_config::referenced_backings(&snapshot, &data).await
            }
            .await;
            retained.extend(crate::util::future::finish_with_cleanup(
                result,
                view.unmount(false).await,
            )?);
        }
        Ok(retained)
    }
    .await;
    crate::util::future::finish_with_cleanup(result, mounts.unmount().await)
}

async fn data_view(source: &Path) -> Result<MountGuard, Error> {
    capture_view(&Bind::new(source).recursive(true)).await
}

async fn capture_view(
    filesystem: &impl crate::disk::mount::filesystem::FileSystem,
) -> Result<MountGuard, Error> {
    let target = TmpDir::new().await?;
    let path = target.to_path_buf();
    target.leak();
    MountGuard::mount(filesystem, path, ReadOnly).await
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

async fn receive(
    mut archive: tokio::fs::File,
) -> Result<
    (
        tokio::fs::File,
        Vec<PathBuf>,
        crate::net::wifi::NetworkProfiles,
    ),
    Error,
> {
    archive.rewind().await?;
    let archive = archive.into_std().await;
    let output = anonymous_file()?.into_std().await;
    let (output, keys, profiles) =
        tokio::task::spawn_blocking(move || receive_archive(archive, output))
            .await
            .with_kind(ErrorKind::Unknown)??;
    Ok((tokio::fs::File::from_std(output), keys, profiles))
}

fn receive_archive(
    source: std::fs::File,
    mut output: std::fs::File,
) -> Result<
    (
        std::fs::File,
        Vec<PathBuf>,
        crate::net::wifi::NetworkProfiles,
    ),
    Error,
> {
    use std::io::{Read, Seek};

    let raw = source.try_clone()?;
    let mut archive = tar::Archive::new(source);
    archive.set_ignore_zeros(true);
    let mut snapshot = false;
    let mut keys = None;
    let mut metadata: Option<std::collections::BTreeMap<String, (i64, i64)>> = None;
    let mut profiles = crate::net::wifi::NetworkProfiles::new();
    for entry in archive.entries_with_seek()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            return Err(invalid_source());
        }
        let path = entry.path()?.into_owned();
        match path.as_path() {
            path if path == Path::new(CAPTURE_FILE) && !snapshot => {
                std::io::copy(&mut entry, &mut output)?;
                output.rewind()?;
                snapshot = true;
            }
            path if path == Path::new(KEYS_FILE) && keys.is_none() => {
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes)?;
                let paths: Vec<OsString> =
                    serde_json::from_slice(&bytes).with_kind(ErrorKind::Deserialization)?;
                keys = Some(paths.into_iter().map(PathBuf::from).collect());
            }
            path if path == Path::new(PROFILES_FILE) && metadata.is_none() => {
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes)?;
                metadata =
                    Some(serde_json::from_slice(&bytes).with_kind(ErrorKind::Deserialization)?);
            }
            path => {
                let name = path
                    .strip_prefix(PROFILE_CONTENTS)
                    .ok()
                    .and_then(Path::to_str)
                    .ok_or_else(invalid_source)?;
                if !crate::net::wifi::is_network_profile_name(std::ffi::OsStr::new(name))
                    || profiles.contains_key(name)
                {
                    return Err(invalid_source());
                }
                let modified = metadata
                    .as_mut()
                    .and_then(|values| values.remove(name))
                    .ok_or_else(invalid_source)?;
                let bytes =
                    crate::util::io::read_secret_at(&raw, entry.raw_file_position(), entry.size())?;
                let contents = Zeroizing::new(
                    std::str::from_utf8(&bytes)
                        .with_kind(ErrorKind::Deserialization)?
                        .to_owned(),
                );
                profiles.insert(
                    name.to_owned(),
                    crate::net::wifi::NetworkProfile { contents, modified },
                );
            }
        }
    }
    if !snapshot || !metadata.is_some_and(|values| values.is_empty()) {
        return Err(invalid_source());
    }
    Ok((output, keys.ok_or_else(invalid_source)?, profiles))
}

/// Executable and request paths must be literal systemd arguments.
/// PID1 retains the shared stdin lease through stop-post, including requester death.
async fn capture_in_unit(
    epoch: &ReadEpoch,
    executable: &Path,
    unit: &str,
    images: &crate::util::FileLock,
) -> Result<
    (
        tokio::fs::File,
        Vec<PathBuf>,
        crate::net::wifi::NetworkProfiles,
    ),
    Error,
> {
    let input = Stdio::from(images.share()?);
    let archive = anonymous_file()?;
    let output = Stdio::from(archive.try_clone().await?.into_std().await);
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
        .stdin(input)
        .stdout(output)
        .stderr(Stdio::piped())
        .capture(false)
        .invoke(ErrorKind::Filesystem)
        .await;
    restore_after_unit(epoch, &path, unit).await;
    result?;
    receive(archive).await
}

/// The caller holds its database pause until restoration succeeds.
async fn restore_after_unit(epoch: &ReadEpoch, path: &Path, unit: &str) {
    let mut reported = false;
    loop {
        let restored = async {
            if !stopped_unit(unit).await? {
                Command::new("/usr/bin/systemctl")
                    .arg("stop")
                    .arg(unit)
                    .timeout(Some(std::time::Duration::from_secs(10)))
                    .invoke(ErrorKind::Filesystem)
                    .await?;
                if !stopped_unit(unit).await? {
                    return Err(invalid_source());
                }
            }
            epoch.release().await?;
            crate::util::io::delete_file(path).await
        }
        .await;
        match restored {
            Ok(()) => return,
            Err(error) => {
                if !reported {
                    tracing::error!("Configuration capture cleanup remains incomplete: {error}");
                    reported = true;
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
}

async fn stopped_unit(unit: &str) -> Result<bool, Error> {
    let load = Command::new("/usr/bin/systemctl")
        .args(["show", "--value", "--property=LoadState", unit])
        .timeout(Some(std::time::Duration::from_secs(10)))
        .invoke(ErrorKind::Filesystem)
        .await?;
    if load == b"not-found\n" {
        return Ok(true);
    }
    let active = Command::new("/usr/bin/systemctl")
        .args(["show", "--value", "--property=ActiveState", unit])
        .timeout(Some(std::time::Duration::from_secs(10)))
        .invoke(ErrorKind::Filesystem)
        .await?;
    Ok(active == b"inactive\n" || active == b"failed\n")
}

/// Caller owns password/keyslot serialization and fresh whole-configuration authorization.
/// The image-store reservation excludes native update/pruning through manifest preparation.
/// The operation request contains paths, never signing or unlock secrets.
/// Unpublished preparation does not admit the base as a public builder source.
/// Publishes nothing; awaits candidate and fresh-backing disposal under the reservation.
pub(crate) async fn prepare(ctx: &RpcContext) -> Result<(), Error> {
    let memory = ProcessSecretMemory::protect()?;
    let images = crate::os_overlay::reserve_image_store(false).await?;
    let rootfs = running_image().await?;
    let epoch = ReadEpoch::current().await?;
    let store = super::boot::create_generation_store(Path::new("/media/startos/config"))?;
    crate::os_overlay::recover_unpublished(&store).await?;
    let pause = ctx.db.pause_mutations().await;
    let mut credentials =
        super::credentials::Credentials::capture(&ctx.db.peek().await, ctx.disk_guid.clone())?;
    let unit = format!("startos-fde-capture-{}", Guid::new());
    let captured = capture_in_unit(&epoch, Path::new("/usr/bin/startbox"), &unit, &images).await;
    drop(pause);
    let (image, keys, profiles) = captured?;
    credentials.network_profiles = profiles;
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
    let sealed = super::credentials::SealedCredentials::seal(
        &credentials,
        &enrollment.policy_key,
        Path::new("/run"),
    )
    .await?;
    drop(credentials);
    let mut projected =
        ProjectedConfig::stage(snapshot, Path::new(crate::MAIN_DATA), &keys).await?;
    let prepared = async {
        projected.insert_bootstrap(sealed)?;
        projected
            .prepare_generation(&rootfs, &store, &enrollment.manifest_key)
            .await
    }
    .await;
    let prepared = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            return crate::util::future::finish_with_cleanup(Err(error), projected.discard().await);
        }
    };
    let result = async {
        memory.check()?;
        let descriptor = super::boot::Descriptor::for_generation(
            &prepared,
            super::tpm::public_key(&enrollment.policy_key)?,
        )?;
        super::public_image::stage(ctx, &prepared, &descriptor, &images).await?;
        memory.check()
    }
    .await;
    crate::util::future::finish_with_cleanup(result, projected.discard_generation(prepared).await)
}
