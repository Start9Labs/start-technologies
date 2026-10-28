use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use axum::extract::ws::Message;
use imbl_value::InternedString;
use serde::{Deserialize, Serialize};
use startos::util::net::WebSocket;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::continuations::{Guid, RpcContinuation};
use crate::invoke::Invoke;
use crate::prelude::*;
use crate::ServerContext;

#[derive(Debug, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub timestamp: String,
    pub message: String,
}
rpc_toolkit::reflect_ts!(LogEntry);

#[derive(Debug, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct LogsResponse {
    pub entries: Vec<LogEntry>,
}
rpc_toolkit::reflect_ts!(LogsResponse);

/// Parse a logread line into a LogEntry.
///
/// logread format: "Dow Mon DD HH:MM:SS YYYY facility.pri source: msg"
/// The timestamp is always the first 24 characters (fixed-width syslog format).
pub fn parse_logread_line(line: &str) -> Option<LogEntry> {
    let bytes = line.as_bytes();
    if bytes.len() < 25 || bytes[24] != b' ' {
        return None;
    }
    if !bytes[20..24].iter().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(LogEntry {
        timestamp: line[..24].to_string(),
        message: line[25..].to_string(),
    })
}

#[derive(Deserialize, Serialize, visit_rs::VisitFields)]
pub struct FollowLogsParams {
    #[visit(ts(skip))]
    #[serde(rename = "__Auth_signer")]
    signer: Option<InternedString>,
}
rpc_toolkit::reflect_ts!(FollowLogsParams);

#[derive(Serialize, Deserialize, visit_rs::VisitFields)]
pub struct FollowLogsRes {
    /// Streams each new [`LogEntry`] as JSON from `/ws/rpc/{guid}`.
    pub guid: Guid,
}
rpc_toolkit::reflect_ts!(FollowLogsRes);

pub async fn follow_logs(
    ctx: ServerContext,
    FollowLogsParams { signer }: FollowLogsParams,
) -> Result<FollowLogsRes, Error> {
    let guid = Guid::new();
    ctx.continuations
        .add(
            guid.clone(),
            RpcContinuation::ws_authed(ctx.clone(), signer, stream_logs, Duration::from_secs(30)),
        )
        .await;
    Ok(FollowLogsRes { guid })
}

async fn stream_logs(mut ws: WebSocket) {
    let mut child = match Command::new("logread")
        .arg("-f")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            tracing::error!("failed to spawn logread: {e}");
            return;
        }
    };

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout).lines();

    loop {
        tokio::select! {
            line = reader.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        let Some(entry) = parse_logread_line(&line) else {
                            continue;
                        };
                        let Ok(json) = serde_json::to_string(&entry) else {
                            continue;
                        };
                        if ws.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
            msg = ws.recv() => {
                match msg {
                    Some(Ok(_)) => continue,
                    _ => break,
                }
            }
        }
    }
}

/// The file logd's follower appends to; it renames a full file to `.old`.
pub(crate) const HISTORY_FILES: [&str; 2] = ["var/log/messages.old", "var/log/messages"];

/// Where images before 1.3.0 kept the same files. A backup made on one
/// restores them here.
pub(crate) const LEGACY_HISTORY_FILES: [&str; 2] = ["etc/log/messages.old", "etc/log/messages"];

/// The persistent log under a filesystem root, oldest line first.
///
/// Empty when neither file exists.
pub async fn read_history(root: &Path) -> Result<Vec<u8>, Error> {
    Ok(read_history_files(root).await?.concat())
}

async fn read_history_files(root: &Path) -> Result<[Vec<u8>; 2], Error> {
    let mut files = [Vec::new(), Vec::new()];
    for (file, path) in files.iter_mut().zip(HISTORY_FILES) {
        *file = read_or_empty(&root.join(path)).await?;
    }
    Ok(files)
}

async fn read_or_empty(path: &Path) -> Result<Vec<u8>, Error> {
    match tokio::fs::read(path).await {
        Ok(bytes) => Ok(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}

/// The persistent log as it stood before a backup was restored over it.
pub struct HistorySnapshot([Vec<u8>; 2]);

pub async fn snapshot_history(root: &Path) -> Result<HistorySnapshot, Error> {
    read_history_files(root).await.map(HistorySnapshot)
}

/// Keeps the router's own log after a restore, preceded by whatever older
/// history the backup carried.
///
/// Takes the backup's `tar -t` listing.
pub async fn merge_restored_history(
    root: &Path,
    before: HistorySnapshot,
    archive_listing: &str,
) -> Result<(), Error> {
    let listed = |file: &str| {
        archive_listing
            .lines()
            .any(|entry| entry.trim_start_matches("./").trim_start_matches('/') == file)
    };
    let mut restored = [Vec::new(), Vec::new()];
    let mut any_restored = false;
    for ((file, path), legacy) in restored
        .iter_mut()
        .zip(HISTORY_FILES)
        .zip(LEGACY_HISTORY_FILES)
    {
        let Some(path) = [path, legacy].into_iter().find(|path| listed(path)) else {
            continue;
        };
        *file = read_or_empty(&root.join(path)).await?;
        any_restored = true;
    }
    if !any_restored {
        return Ok(());
    }
    let restored = restored.concat();
    let [old, current] = before.0;
    let own = [old.as_slice(), current.as_slice()].concat();
    let rotated = [restored_prefix(&restored, &own), old.as_slice()].concat();
    for (path, contents) in HISTORY_FILES.into_iter().zip([rotated, current]) {
        write_history_file(&root.join(path), &contents).await?;
    }
    for path in LEGACY_HISTORY_FILES {
        match tokio::fs::remove_file(root.join(path)).await {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
    }
    let _ = tokio::fs::remove_dir(root.join("etc/log")).await;
    Ok(())
}

/// Lines of a restored log that precede everything in the router's own.
///
/// The router's first line marks where the overlap begins. A restored log
/// whose last line the router still holds adds nothing.
fn restored_prefix<'a>(restored: &'a [u8], current: &[u8]) -> &'a [u8] {
    let current = lines(current);
    let Some((_, first)) = current.first() else {
        return restored;
    };
    let restored_lines = lines(restored);
    if let Some((start, _)) = restored_lines.iter().find(|(_, line)| line == first) {
        return &restored[..*start];
    }
    match restored_lines.last() {
        Some((_, last)) if current.iter().any(|(_, line)| line == last) => &[],
        _ => restored,
    }
}

/// Non-empty lines without their newline, each with its byte offset.
fn lines(log: &[u8]) -> Vec<(usize, &[u8])> {
    let mut offset = 0;
    log.split_inclusive(|b| *b == b'\n')
        .map(|line| {
            let start = offset;
            offset += line.len();
            (start, line.strip_suffix(b"\n").unwrap_or(line))
        })
        .filter(|(_, line)| !line.is_empty())
        .collect()
}

async fn write_history_file(path: &Path, contents: &[u8]) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;

    if contents.is_empty() {
        return match tokio::fs::remove_file(path).await {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        };
    }
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    tokio::fs::write(&tmp, contents).await?;
    tokio::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}

pub async fn get_logs(_ctx: ServerContext) -> Result<LogsResponse, Error> {
    let output = Command::new("logread")
        .invoke(ErrorKind::Filesystem.into())
        .await?;

    let stdout = String::from_utf8_lossy(&output);
    let entries = stdout.lines().filter_map(parse_logread_line).collect();

    Ok(LogsResponse { entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_typical_line() {
        let line =
            "Thu Mar  3 12:34:56 2026 daemon.info dnsmasq[1234]: query[A] example.com from 192.168.1.100";
        let entry = parse_logread_line(line).unwrap();
        assert_eq!(entry.timestamp, "Thu Mar  3 12:34:56 2026");
        assert_eq!(
            entry.message,
            "daemon.info dnsmasq[1234]: query[A] example.com from 192.168.1.100"
        );
    }

    #[test]
    fn test_parse_two_digit_day() {
        let line = "Mon Mar 13 08:00:00 2026 kern.info kernel: [12345.678] something happened";
        let entry = parse_logread_line(line).unwrap();
        assert_eq!(entry.timestamp, "Mon Mar 13 08:00:00 2026");
        assert_eq!(
            entry.message,
            "kern.info kernel: [12345.678] something happened"
        );
    }

    #[test]
    fn test_parse_empty_line() {
        assert!(parse_logread_line("").is_none());
    }

    #[test]
    fn test_parse_short_line() {
        assert!(parse_logread_line("too short").is_none());
    }

    #[test]
    fn test_parse_no_year() {
        assert!(parse_logread_line("Thu Mar  3 12:34:56 abcd rest of message").is_none());
    }

    #[test]
    fn restored_prefix_keeps_only_older_lines() {
        let backup = b"a\nb\nc\n";
        assert_eq!(restored_prefix(backup, b"b\nc\nd\n"), b"a\n");
        assert_eq!(restored_prefix(backup, b"x\nb\nc\nd\n"), b"");
        assert_eq!(restored_prefix(backup, b"y\nz\n"), backup);
        assert_eq!(restored_prefix(backup, b""), backup);
        assert_eq!(restored_prefix(b"", b"y\n"), b"");
    }

    fn history_root(old: Option<&str>, current: Option<&str>) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("var/log")).unwrap();
        for (path, text) in HISTORY_FILES.into_iter().zip([old, current]) {
            if let Some(text) = text {
                std::fs::write(dir.path().join(path), text).unwrap();
            }
        }
        dir
    }

    fn history_file(dir: &tempfile::TempDir, path: &str) -> Option<String> {
        std::fs::read_to_string(dir.path().join(path)).ok()
    }

    const LISTING: &str = "etc/config/system\nvar/log/messages.old\nvar/log/messages\n";

    #[tokio::test]
    async fn restore_onto_another_router_puts_the_backup_first() {
        let dir = history_root(None, Some("boot\n"));
        let before = snapshot_history(dir.path()).await.unwrap();
        std::fs::write(dir.path().join(HISTORY_FILES[0]), "a\n").unwrap();
        std::fs::write(dir.path().join(HISTORY_FILES[1]), "b\n").unwrap();
        merge_restored_history(dir.path(), before, LISTING)
            .await
            .unwrap();
        assert_eq!(
            history_file(&dir, HISTORY_FILES[0]).as_deref(),
            Some("a\nb\n")
        );
        assert_eq!(
            history_file(&dir, HISTORY_FILES[1]).as_deref(),
            Some("boot\n")
        );
        assert_eq!(read_history(dir.path()).await.unwrap(), b"a\nb\nboot\n");
    }

    #[tokio::test]
    async fn restore_of_an_unrotated_backup_keeps_the_routers_rotated_log() {
        let dir = history_root(Some("b\nc\n"), Some("d\n"));
        let before = snapshot_history(dir.path()).await.unwrap();
        std::fs::write(dir.path().join(HISTORY_FILES[1]), "a\n").unwrap();
        merge_restored_history(dir.path(), before, "var/log/messages\n")
            .await
            .unwrap();
        assert_eq!(read_history(dir.path()).await.unwrap(), b"a\nb\nc\nd\n");
        assert_eq!(history_file(&dir, HISTORY_FILES[1]).as_deref(), Some("d\n"));
    }

    #[tokio::test]
    async fn restore_of_a_pre_1_3_backup_merges_from_its_old_location() {
        let dir = history_root(None, Some("boot\n"));
        let before = snapshot_history(dir.path()).await.unwrap();
        std::fs::create_dir_all(dir.path().join("etc/log")).unwrap();
        std::fs::write(dir.path().join(LEGACY_HISTORY_FILES[0]), "a\n").unwrap();
        std::fs::write(dir.path().join(LEGACY_HISTORY_FILES[1]), "b\n").unwrap();
        merge_restored_history(
            dir.path(),
            before,
            "etc/config/system\netc/log/messages.old\netc/log/messages\n",
        )
        .await
        .unwrap();
        assert_eq!(read_history(dir.path()).await.unwrap(), b"a\nb\nboot\n");
        assert_eq!(
            history_file(&dir, HISTORY_FILES[1]).as_deref(),
            Some("boot\n")
        );
        assert!(!dir.path().join("etc/log").exists());
    }

    #[tokio::test]
    async fn restore_onto_the_same_router_keeps_its_log() {
        let dir = history_root(Some("b\nc\n"), Some("d\ne\n"));
        let before = snapshot_history(dir.path()).await.unwrap();
        std::fs::write(dir.path().join(HISTORY_FILES[0]), "a\nb\n").unwrap();
        std::fs::write(dir.path().join(HISTORY_FILES[1]), "c\n").unwrap();
        merge_restored_history(dir.path(), before, LISTING)
            .await
            .unwrap();
        assert_eq!(read_history(dir.path()).await.unwrap(), b"a\nb\nc\nd\ne\n");
        assert_eq!(
            history_file(&dir, HISTORY_FILES[1]).as_deref(),
            Some("d\ne\n")
        );
    }

    #[tokio::test]
    async fn restore_without_logs_leaves_the_log_alone() {
        let dir = history_root(None, Some("b\n"));
        let before = snapshot_history(dir.path()).await.unwrap();
        merge_restored_history(dir.path(), before, "etc/config/system\n")
            .await
            .unwrap();
        assert_eq!(read_history(dir.path()).await.unwrap(), b"b\n");
        assert_eq!(history_file(&dir, HISTORY_FILES[0]), None);
    }
}
