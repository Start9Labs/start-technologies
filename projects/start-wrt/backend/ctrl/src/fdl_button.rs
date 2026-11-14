//! Factory reset from the FDL button.
//!
//! The kernel's `gpio-keys` driver reports the button as an input device. A
//! press held for [`HOLD`] blinks the status LED. The release wipes the overlay
//! and reboots, keeping the BootROM's download strap low across the reset.

use std::fs::{self, File};
use std::io::Read;
use std::os::fd::{AsFd, AsRawFd};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use nix::poll::{poll, PollFd, PollFlags, PollTimeout};

use crate::prelude::*;

const HOLD: Duration = Duration::from_secs(10);

const INPUT_DEVICE_NAME: &str = "gpio-keys";
const STATUS_LED: &str = "/sys/class/leds/sys-led";

const EV_KEY: u16 = 0x01;
const KEY_RESTART: u16 = 0x198;
const KEY_STATE_BYTES: usize = 0x300 / 8;
const EVENT_SIZE: usize = std::mem::size_of::<libc::input_event>();
const EVENT_TYPE_OFFSET: usize = EVENT_SIZE - 8;

nix::ioctl_read_buf!(eviocgkey, b'E', 0x18, u8);

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Wait(Option<Duration>),
    Blink,
    Wipe,
}

#[derive(Default)]
struct Button {
    held_since: Option<Duration>,
    armed: bool,
}

impl Button {
    fn key(&mut self, pressed: bool, now: Duration) {
        if !pressed {
            self.held_since = None;
        } else if self.held_since.is_none() {
            self.held_since = Some(now);
        }
    }

    fn step(&mut self, now: Duration) -> Action {
        match (self.armed, self.held_since) {
            (true, None) => Action::Wipe,
            (true, Some(_)) | (false, None) => Action::Wait(None),
            (false, Some(since)) if now >= since + HOLD => {
                self.armed = true;
                Action::Blink
            }
            (false, Some(since)) => Action::Wait(Some(since + HOLD - now)),
        }
    }
}

fn find_device() -> Result<Option<PathBuf>, Error> {
    for entry in fs::read_dir("/sys/class/input").with_kind(ErrorKind::Filesystem)? {
        let entry = entry.with_kind(ErrorKind::Filesystem)?;
        let event = entry.file_name();
        let Some(event) = event.to_str().filter(|n| n.starts_with("event")) else {
            continue;
        };
        let name = fs::read_to_string(entry.path().join("device/name")).unwrap_or_default();
        if name.trim() == INPUT_DEVICE_NAME {
            return Ok(Some(PathBuf::from("/dev/input").join(event)));
        }
    }
    Ok(None)
}

fn key_is_down(device: &File) -> Result<bool, Error> {
    let mut state = [0u8; KEY_STATE_BYTES];
    // SAFETY: `state` is a live buffer whose length the ioctl receives.
    unsafe { eviocgkey(device.as_raw_fd(), &mut state) }.with_kind(ErrorKind::Filesystem)?;
    Ok(state[usize::from(KEY_RESTART) / 8] & (1 << (KEY_RESTART % 8)) != 0)
}

fn parse_key_event(event: &[u8; EVENT_SIZE]) -> Option<bool> {
    let field = |offset: usize| u16::from_ne_bytes([event[offset], event[offset + 1]]);
    let value = i32::from_ne_bytes([
        event[EVENT_TYPE_OFFSET + 4],
        event[EVENT_TYPE_OFFSET + 5],
        event[EVENT_TYPE_OFFSET + 6],
        event[EVENT_TYPE_OFFSET + 7],
    ]);
    (field(EVENT_TYPE_OFFSET) == EV_KEY && field(EVENT_TYPE_OFFSET + 2) == KEY_RESTART)
        .then_some(value != 0)
}

fn blink_status_led() {
    for (attr, value) in [
        ("trigger", "timer"),
        ("delay_on", "100"),
        ("delay_off", "100"),
    ] {
        if let Err(e) = fs::write(format!("{STATUS_LED}/{attr}"), value) {
            tracing::warn!("failed to set status LED {attr}: {e}");
        }
    }
}

/// Watches the button for the life of the system; a completed hold wipes the overlay and reboots.
pub async fn watch() -> Result<(), Error> {
    let started = Instant::now();
    let Some(path) = find_device()? else {
        tracing::warn!("no {INPUT_DEVICE_NAME} input device");
        return Ok(());
    };
    let mut device = File::open(path).with_kind(ErrorKind::Filesystem)?;
    let mut button = Button::default();
    button.key(key_is_down(&device)?, started.elapsed());

    loop {
        let timeout = match button.step(started.elapsed()) {
            Action::Wipe => return crate::system::wipe_overlay_and_reboot().await,
            Action::Blink => {
                blink_status_led();
                continue;
            }
            Action::Wait(None) => PollTimeout::NONE,
            Action::Wait(Some(wait)) => PollTimeout::try_from(wait.max(Duration::from_millis(1)))
                .unwrap_or(PollTimeout::MAX),
        };

        let mut fds = [PollFd::new(device.as_fd(), PollFlags::POLLIN)];
        if poll(&mut fds, timeout).with_kind(ErrorKind::Filesystem)? > 0 {
            let mut event = [0u8; EVENT_SIZE];
            device
                .read_exact(&mut event)
                .with_kind(ErrorKind::Filesystem)?;
            if let Some(pressed) = parse_key_event(&event) {
                button.key(pressed, started.elapsed());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    #[test]
    fn idle_button_waits_without_a_deadline() {
        let mut button = Button::default();
        assert_eq!(button.step(secs(0)), Action::Wait(None));
        assert_eq!(button.step(secs(86_400)), Action::Wait(None));
    }

    #[test]
    fn key_already_down_at_start_counts_as_a_press() {
        let mut button = Button::default();
        button.key(true, secs(0));
        assert_eq!(button.step(secs(0)), Action::Wait(Some(HOLD)));
        assert_eq!(button.step(secs(10)), Action::Blink);
    }

    #[test]
    fn full_hold_blinks_then_wipes_only_on_release() {
        let mut button = Button::default();
        button.key(true, secs(5));
        assert_eq!(button.step(secs(14)), Action::Wait(Some(secs(1))));
        assert_eq!(button.step(secs(15)), Action::Blink);
        assert_eq!(button.step(secs(500)), Action::Wait(None));
        button.key(false, secs(500));
        assert_eq!(button.step(secs(500)), Action::Wipe);
    }

    #[test]
    fn early_release_cancels() {
        let mut button = Button::default();
        button.key(true, secs(5));
        button.key(false, secs(14));
        assert_eq!(button.step(secs(14)), Action::Wait(None));
    }

    #[test]
    fn hold_long_after_boot_still_arms() {
        let mut button = Button::default();
        button.key(true, secs(86_400));
        assert_eq!(button.step(secs(86_400)), Action::Wait(Some(HOLD)));
        assert_eq!(button.step(secs(86_410)), Action::Blink);
    }

    #[test]
    fn repeat_events_do_not_restart_the_hold() {
        let mut button = Button::default();
        button.key(true, secs(1));
        button.key(true, secs(5));
        assert_eq!(button.step(secs(11)), Action::Blink);
    }

    fn event(kind: u16, code: u16, value: i32) -> [u8; EVENT_SIZE] {
        let mut bytes = [0u8; EVENT_SIZE];
        bytes[EVENT_TYPE_OFFSET..EVENT_TYPE_OFFSET + 2].copy_from_slice(&kind.to_ne_bytes());
        bytes[EVENT_TYPE_OFFSET + 2..EVENT_TYPE_OFFSET + 4].copy_from_slice(&code.to_ne_bytes());
        bytes[EVENT_TYPE_OFFSET + 4..EVENT_TYPE_OFFSET + 8].copy_from_slice(&value.to_ne_bytes());
        bytes
    }

    #[test]
    fn parses_only_the_reset_key() {
        assert_eq!(parse_key_event(&event(EV_KEY, KEY_RESTART, 1)), Some(true));
        assert_eq!(parse_key_event(&event(EV_KEY, KEY_RESTART, 0)), Some(false));
        assert_eq!(parse_key_event(&event(0, 0, 0)), None);
        assert_eq!(parse_key_event(&event(EV_KEY, 116, 1)), None);
    }
}
