//! Brackets SSH sessions in the activity log: an `auth.ssh` entry when a
//! dropbear session authenticates and another when it ends.

use std::collections::HashMap;
use std::net::IpAddr;
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

const AUTHORIZED_KEYS: &str = "/etc/dropbear/authorized_keys";
/// Open sessions tracked at once; the oldest is forgotten first.
const MAX_SESSIONS: usize = 64;
const RESTART_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Opened {
        pid: u32,
        user: String,
        method: Method,
        peer: IpAddr,
    },
    Closed {
        pid: u32,
        user: String,
        peer: IpAddr,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Method {
    Key {
        algorithm: String,
        /// `SHA256:` and unpadded base64, as dropbear prints it.
        fingerprint: String,
    },
    Password,
}

/// Follows the system log for the daemon's lifetime.
pub async fn run() {
    let mut sessions = Sessions::default();
    loop {
        if let Err(e) = follow(&mut sessions).await {
            tracing::warn!("SSH session tracking stopped: {e}");
        }
        tokio::time::sleep(RESTART_DELAY).await;
    }
}

async fn follow(sessions: &mut Sessions) -> std::io::Result<()> {
    let mut child = Command::new("logread")
        .arg("-f")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("logread has no stdout"))?;
    let mut lines = BufReader::new(stdout).lines();
    while let Some(line) = lines.next_line().await? {
        let Some(event) =
            crate::logs::parse_logread_line(&line).and_then(|entry| parse_event(&entry.message))
        else {
            continue;
        };
        let summary = describe(sessions.observe(event, Instant::now())).await;
        crate::activity::log("auth", "ssh", true, &summary, None);
    }
    Ok(())
}

/// What an entry reports.
#[derive(Debug, PartialEq, Eq)]
enum Report {
    Opened {
        user: String,
        method: Method,
        peer: IpAddr,
    },
    Closed {
        user: String,
        peer: IpAddr,
        /// Unknown for a session opened before the daemon started.
        lasted: Option<Duration>,
    },
}

/// Open sessions by dropbear process.
#[derive(Default)]
struct Sessions(HashMap<u32, Instant>);

impl Sessions {
    fn observe(&mut self, event: Event, now: Instant) -> Report {
        match event {
            Event::Opened {
                pid,
                user,
                method,
                peer,
            } => {
                if self.0.len() >= MAX_SESSIONS && !self.0.contains_key(&pid) {
                    if let Some(oldest) = self.0.iter().min_by_key(|(_, opened)| **opened) {
                        let oldest = *oldest.0;
                        self.0.remove(&oldest);
                    }
                }
                self.0.insert(pid, now);
                Report::Opened { user, method, peer }
            }
            Event::Closed { pid, user, peer } => Report::Closed {
                user,
                peer,
                lasted: self.0.remove(&pid).map(|opened| now - opened),
            },
        }
    }
}

async fn describe(report: Report) -> String {
    match report {
        Report::Opened { user, method, peer } => {
            let credential = match method {
                Method::Password => "a password".to_string(),
                Method::Key {
                    algorithm,
                    fingerprint,
                } => {
                    let keys = tokio::fs::read_to_string(AUTHORIZED_KEYS)
                        .await
                        .unwrap_or_default();
                    match key_name(&keys, &fingerprint) {
                        Some(name) => format!("key '{name}'"),
                        None => format!("{algorithm} key {fingerprint}"),
                    }
                }
            };
            format!(
                "SSH session opened for {user} from {} with {credential}",
                crate::peer::describe(peer).await
            )
        }
        Report::Closed { user, peer, lasted } => {
            let mut summary = format!(
                "SSH session closed for {user} from {}",
                crate::peer::describe(peer).await
            );
            if let Some(lasted) = lasted {
                summary.push_str(" after ");
                summary.push_str(&format_duration(lasted));
            }
            summary
        }
    }
}

/// The comment on the authorized key with this fingerprint.
fn key_name(authorized_keys: &str, fingerprint: &str) -> Option<String> {
    authorized_keys
        .lines()
        .filter_map(|line| line.trim().parse::<openssh_keys::PublicKey>().ok())
        .find(|key| format!("SHA256:{}", key.fingerprint()) == fingerprint)
        .and_then(|key| key.comment)
        .filter(|comment| !comment.is_empty())
}

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    match (h, m) {
        (0, 0) => format!("{s}s"),
        (0, _) => format!("{m}m {s}s"),
        _ => format!("{h}h {m}m"),
    }
}

/// Reads a dropbear session's authentication or exit from a log message.
fn parse_event(message: &str) -> Option<Event> {
    let (_, rest) = message.split_once("dropbear[")?;
    let (pid, text) = rest.split_once("]: ")?;
    let pid = pid.parse().ok()?;

    if let Some(rest) = text.strip_prefix("Pubkey auth succeeded for '") {
        if text.ends_with(", extra auth required") {
            return None;
        }
        let (user, rest) = rest.split_once("' with ")?;
        let (algorithm, rest) = rest.split_once(" key ")?;
        let (fingerprint, addr) = rest.split_once(" from ")?;
        return Some(Event::Opened {
            pid,
            user: user.to_string(),
            method: Method::Key {
                algorithm: algorithm.to_string(),
                fingerprint: fingerprint.to_string(),
            },
            peer: parse_peer(addr)?,
        });
    }
    if let Some(rest) = text.strip_prefix("Password auth succeeded for '") {
        if text.ends_with(", extra auth required") {
            return None;
        }
        let (user, addr) = rest.split_once("' from ")?;
        return Some(Event::Opened {
            pid,
            user: user.to_string(),
            method: Method::Password,
            peer: parse_peer(addr)?,
        });
    }
    if let Some(rest) = text.strip_prefix("Exit (") {
        let (user, rest) = rest.split_once(") from <")?;
        let (addr, _) = rest.split_once(">")?;
        return Some(Event::Closed {
            pid,
            user: user.to_string(),
            peer: parse_peer(addr)?,
        });
    }
    None
}

/// `host:port`, the host unbracketed even when IPv6.
fn parse_peer(addr: &str) -> Option<IpAddr> {
    let (host, _port) = addr.trim().rsplit_once(':')?;
    host.parse::<IpAddr>().ok().map(|ip| ip.to_canonical())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl laptop";

    fn key_fingerprint() -> String {
        format!(
            "SHA256:{}",
            KEY.parse::<openssh_keys::PublicKey>()
                .unwrap()
                .fingerprint()
        )
    }

    #[test]
    fn reads_dropbear_session_lines() {
        let fp = key_fingerprint();
        assert_eq!(
            parse_event(&format!(
                "authpriv.notice dropbear[4242]: Pubkey auth succeeded for 'root' with ssh-ed25519 key {fp} from 192.168.0.23:51234"
            )),
            Some(Event::Opened {
                pid: 4242,
                user: "root".into(),
                method: Method::Key {
                    algorithm: "ssh-ed25519".into(),
                    fingerprint: fp.clone(),
                },
                peer: "192.168.0.23".parse().unwrap(),
            })
        );
        assert_eq!(
            parse_event(
                "authpriv.notice dropbear[7]: Password auth succeeded for 'root' from fd00::5:40000"
            ),
            Some(Event::Opened {
                pid: 7,
                user: "root".into(),
                method: Method::Password,
                peer: "fd00::5".parse().unwrap(),
            })
        );
        assert_eq!(
            parse_event(
                "authpriv.info dropbear[4242]: Exit (root) from <::ffff:192.168.0.23:51234>: Disconnect received"
            ),
            Some(Event::Closed {
                pid: 4242,
                user: "root".into(),
                peer: "192.168.0.23".parse().unwrap(),
            })
        );
        for ignored in [
            "authpriv.info dropbear[9]: Child connection from 192.168.0.23:51234",
            "authpriv.info dropbear[9]: Exit before auth from <192.168.0.23:51234>: Exited normally",
            "authpriv.warn dropbear[9]: Bad password attempt for 'root' from 192.168.0.23:51234",
            "authpriv.notice dropbear[9]: Password auth succeeded for 'root' from 192.168.0.23:51234, extra auth required",
            "daemon.info startwrt-ctrld[1]: startwrt-activity [auth.ssh] OK SSH session opened for root",
        ] {
            assert_eq!(parse_event(ignored), None, "{ignored}");
        }
    }

    #[test]
    fn a_session_closes_with_how_long_it_lasted() {
        let mut sessions = Sessions::default();
        let peer: IpAddr = "192.168.0.23".parse().unwrap();
        let start = Instant::now();
        let opened = sessions.observe(
            Event::Opened {
                pid: 1,
                user: "root".into(),
                method: Method::Password,
                peer,
            },
            start,
        );
        assert!(matches!(opened, Report::Opened { .. }));
        let closed = |sessions: &mut Sessions, pid| {
            sessions.observe(
                Event::Closed {
                    pid,
                    user: "root".into(),
                    peer,
                },
                start + Duration::from_secs(725),
            )
        };
        assert_eq!(
            closed(&mut sessions, 1),
            Report::Closed {
                user: "root".into(),
                peer,
                lasted: Some(Duration::from_secs(725)),
            }
        );
        assert_eq!(
            closed(&mut sessions, 1),
            Report::Closed {
                user: "root".into(),
                peer,
                lasted: None,
            },
            "opened before the daemon started"
        );
    }

    #[test]
    fn tracking_stays_bounded() {
        let mut sessions = Sessions::default();
        let start = Instant::now();
        for pid in 0..=MAX_SESSIONS as u32 {
            sessions.observe(
                Event::Opened {
                    pid,
                    user: "root".into(),
                    method: Method::Password,
                    peer: "192.168.0.23".parse().unwrap(),
                },
                start + Duration::from_secs(pid.into()),
            );
        }
        assert_eq!(sessions.0.len(), MAX_SESSIONS);
        assert!(!sessions.0.contains_key(&0));
    }

    #[test]
    fn keys_are_named_by_their_comment() {
        let fp = key_fingerprint();
        let keys = format!("# managed\n\nnot a key\n{KEY}\n");
        assert_eq!(key_name(&keys, &fp).as_deref(), Some("laptop"));
        assert_eq!(key_name(&keys, "SHA256:other"), None);
        let uncommented = KEY.rsplit_once(' ').unwrap().0;
        assert_eq!(key_name(uncommented, &fp), None);
    }

    #[test]
    fn durations_read_at_a_glance() {
        assert_eq!(format_duration(Duration::from_secs(45)), "45s");
        assert_eq!(format_duration(Duration::from_secs(725)), "12m 5s");
        assert_eq!(format_duration(Duration::from_secs(7500)), "2h 5m");
    }
}
