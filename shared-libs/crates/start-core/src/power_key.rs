//! The case power button, while a backup is running.
//!
//! systemd-logind powers the server off the moment the power key is pressed,
//! which cuts a running backup off mid-write and can corrupt the service being
//! written. For as long as a backup is underway startd therefore holds a logind
//! `block` inhibitor on `handle-power-key`, and reads the key itself so the
//! press still means something: it records a deferred shutdown, which the web
//! UI surfaces and which StartOS carries out once the backup finishes.
//!
//! The inhibitor names `handle-power-key` and not `shutdown` on purpose —
//! blocking `shutdown` would also block the power-off StartOS itself asks
//! systemd for at the end of a graceful teardown.
//!
//! Failure gives the button back to logind rather than taking it away: the
//! inhibitor is only ever held while the key is also being read, so a server
//! whose key StartOS cannot see keeps powering off exactly as it does today.

use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use futures::FutureExt;
use futures::future::select_all;
use nix::sys::stat;
use patch_db::TypedDbWatch;
use patch_db::json_ptr::JsonPointer;
use tokio::io::unix::AsyncFd;
use zbus::proxy;
use zbus::zvariant::OwnedFd;

use crate::context::RpcContext;
use crate::db::model::public::{PowerAction, ServerStatus};
use crate::prelude::*;
use crate::shutdown::defer_until_backup_complete;
use crate::sound::BEP;

const EV_KEY: u16 = 0x01;
const KEY_POWER: usize = 116;
const KEY_PRESSED: i32 = 1;

const EVENT_SIZE: usize = std::mem::size_of::<libc::input_event>();
/// Offset of `struct input_event`'s trailing `type`, `code` and `value`. The
/// leading timestamp's width varies by architecture; those three fields are
/// always the last 8 bytes.
const EVENT_TAIL: usize = EVENT_SIZE - 8;

const STATUS_INFO_PTR: &str = "/public/serverInfo/statusInfo";
const POWER_SWITCH_TAG_DIR: &str = "/run/udev/tags/power-switch";

#[proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait Login1Manager {
    /// The returned file descriptor *is* the lock — it is released when dropped.
    fn inhibit(&self, what: &str, who: &str, why: &str, mode: &str) -> Result<OwnedFd, Error>;
}

pub async fn watch_power_key(ctx: RpcContext) {
    let devices = match power_key_devices().await {
        Ok(devices) => devices,
        Err(e) => {
            tracing::error!("could not enumerate input devices: {e}");
            tracing::debug!("{e:?}");
            return;
        }
    };
    if devices.is_empty() {
        tracing::info!(
            "no power-switch input device, so the power button stays systemd-logind's during a backup"
        );
        return;
    }
    // Whichever half stops, both do: an inhibitor outliving the reader would be
    // a power button that does nothing at all during a backup.
    tokio::select! {
        _ = inhibit_while_backing_up(ctx.clone()) => {}
        _ = read_power_key(devices, ctx) => {}
    }
    tracing::warn!("no longer handling the power button; systemd-logind has it back");
}

/// Holds a logind `handle-power-key` lock for exactly as long as a backup is
/// running, so that a press during one reaches [`read_power_key`] instead of
/// powering the server off.
async fn inhibit_while_backing_up(ctx: RpcContext) {
    let manager = match logind().await {
        Ok(manager) => manager,
        Err(e) => {
            tracing::warn!("cannot reach systemd-logind: {e}");
            tracing::debug!("{e:?}");
            return;
        }
    };
    let mut watch = ctx
        .db
        .watch(STATUS_INFO_PTR.parse::<JsonPointer>().unwrap())
        .await
        .typed::<ServerStatus>();
    if let Err(e) = inhibit_across_backups(&manager, &mut watch).await {
        tracing::error!("stopped inhibiting the power button during backups: {e}");
        tracing::debug!("{e:?}");
    }
}

async fn inhibit_across_backups(
    manager: &Login1ManagerProxy<'_>,
    watch: &mut TypedDbWatch<ServerStatus>,
) -> Result<(), Error> {
    loop {
        watch
            .wait_for(|status: &ServerStatus| status.backup_progress.is_some())
            .await?;
        let lock = manager
            .inhibit(
                "handle-power-key",
                "StartOS",
                "A backup is running",
                "block",
            )
            .await?;
        watch
            .wait_for(|status: &ServerStatus| status.backup_progress.is_none())
            .await?;
        drop(lock);
    }
}

/// Returns as soon as any one device stops being readable: there is no telling
/// which of them the firmware reports presses on, so a partial failure has to
/// count as a failure.
async fn read_power_key(devices: Vec<PathBuf>, ctx: RpcContext) {
    select_all(
        devices
            .into_iter()
            .map(|path| read_device(path, ctx.clone()).boxed()),
    )
    .await;
}

async fn read_device(path: PathBuf, ctx: RpcContext) {
    let device = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(&path)
        .and_then(AsyncFd::new);
    let device = match device {
        Ok(device) => device,
        Err(e) => {
            tracing::warn!("could not read the power key from {}: {e}", path.display());
            return;
        }
    };
    // evdev only ever hands back whole events, so a batch never splits one.
    let mut buf = [0u8; EVENT_SIZE * 16];
    loop {
        let read = match device.readable().await {
            Ok(mut guard) => guard.try_io(|fd| {
                let mut file = fd.get_ref();
                file.read(&mut buf)
            }),
            Err(e) => {
                tracing::warn!("stopped reading {}: {e}", path.display());
                return;
            }
        };
        match read {
            Err(_would_block) => continue,
            Ok(Err(e)) => {
                tracing::warn!("stopped reading {}: {e}", path.display());
                return;
            }
            Ok(Ok(len)) => {
                for event in buf[..len].chunks_exact(EVENT_SIZE) {
                    if is_power_key_press(event) {
                        on_power_key(&ctx).await;
                    }
                }
            }
        }
    }
}

async fn on_power_key(ctx: &RpcContext) {
    match defer_until_backup_complete(ctx, PowerAction::Shutdown).await {
        Ok(true) => {
            tracing::info!("power key pressed during a backup; shutting down once it finishes");
            // The only feedback available to whoever is standing at the server,
            // whose press otherwise appears to have done nothing. Spawned so a
            // contended sound device cannot stall the reader.
            tokio::spawn(async { BEP.play().await.log_err() });
        }
        // No backup left to protect.
        Ok(false) => (),
        Err(e) => {
            tracing::error!("could not defer the shutdown for the running backup: {e}");
            tracing::debug!("{e:?}");
        }
    }
}

async fn logind() -> Result<Login1ManagerProxy<'static>, Error> {
    Ok(Login1ManagerProxy::new(&zbus::Connection::system().await?).await?)
}

/// The devices systemd-logind itself treats as power switches: udev tags them
/// `power-switch`, and names each one by device number in that tag's directory.
/// Reading logind's own device set rather than picking devices by capability is
/// what keeps a press meaning here exactly what it would have meant to logind.
async fn power_key_devices() -> Result<Vec<PathBuf>, Error> {
    let mut devices = Vec::new();
    let mut dir = tokio::fs::read_dir("/dev/input").await?;
    while let Some(entry) = dir.next_entry().await? {
        if !entry.file_name().as_encoded_bytes().starts_with(b"event") {
            continue;
        }
        let id = device_id(entry.metadata().await?.rdev());
        if Path::new(POWER_SWITCH_TAG_DIR).join(id).exists() {
            devices.push(entry.path());
        }
    }
    Ok(devices)
}

fn device_id(rdev: u64) -> String {
    format!("c{}:{}", stat::major(rdev), stat::minor(rdev))
}

fn is_power_key_press(event: &[u8]) -> bool {
    let tail = &event[EVENT_TAIL..];
    u16::from_ne_bytes([tail[0], tail[1]]) == EV_KEY
        && u16::from_ne_bytes([tail[2], tail[3]]) as usize == KEY_POWER
        && i32::from_ne_bytes([tail[4], tail[5], tail[6], tail[7]]) == KEY_PRESSED
}

#[cfg(test)]
mod test {
    use super::*;

    /// `/dev/input/event0` is char device 13:64, and udev names its tag entry
    /// after exactly that.
    #[test]
    fn names_a_device_the_way_udev_tags_it() {
        assert_eq!(device_id(stat::makedev(13, 64)), "c13:64");
        assert_eq!(device_id(stat::makedev(13, 71)), "c13:71");
    }

    #[test]
    fn recognizes_a_power_key_press() {
        let mut event = [0u8; EVENT_SIZE];
        event[EVENT_TAIL..EVENT_TAIL + 2].copy_from_slice(&EV_KEY.to_ne_bytes());
        event[EVENT_TAIL + 2..EVENT_TAIL + 4].copy_from_slice(&(KEY_POWER as u16).to_ne_bytes());
        event[EVENT_TAIL + 4..].copy_from_slice(&KEY_PRESSED.to_ne_bytes());
        assert!(is_power_key_press(&event));

        // A release, which must not power anything off.
        event[EVENT_TAIL + 4..].copy_from_slice(&0i32.to_ne_bytes());
        assert!(!is_power_key_press(&event));

        // Some other key.
        event[EVENT_TAIL + 2..EVENT_TAIL + 4].copy_from_slice(&30u16.to_ne_bytes());
        event[EVENT_TAIL + 4..].copy_from_slice(&KEY_PRESSED.to_ne_bytes());
        assert!(!is_power_key_press(&event));
    }
}
