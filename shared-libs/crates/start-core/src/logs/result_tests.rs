use std::time::Duration;

use super::*;

pub(super) const ENTRY: &str = r#"{"__REALTIME_TIMESTAMP":"1000000","MESSAGE":"last entry","__CURSOR":"cursor-a","_BOOT_ID":"boot-a"}"#;

pub(super) fn child(script: &str) -> Child {
    Command::new("/bin/sh")
        .args(["-c", script])
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

#[tokio::test]
async fn log_source_drains_entries_then_reports_the_process_failure() {
    let mut logs =
        LogStream::follow(child(&format!("printf '%s\\n' '{ENTRY}'; exit 7")), vec![]).unwrap();
    let entry = logs.try_next().await.unwrap().unwrap();
    let (cursor, entry) = entry.log_entry().unwrap();
    assert_eq!(cursor, "cursor-a");
    assert_eq!(entry.message, "last entry");
    let error = logs.try_next().await.unwrap_err();
    assert_eq!(error.kind, ErrorKind::Journald);
    assert!(error.to_string().contains("7"), "{error}");
    assert!(logs.try_next().await.unwrap().is_none());
}

#[tokio::test]
async fn log_source_stdout_eof_waits_for_the_process_result() {
    use tokio::io::AsyncWriteExt;

    let mut process = child("exec 1>&-; printf 'closed\\n' >&2; read release; exit 9");
    let mut release = process.stdin.take().unwrap();
    let mut ready = BufReader::new(process.stderr.take().unwrap()).lines();
    assert_eq!(ready.next_line().await.unwrap().as_deref(), Some("closed"));
    let mut logs = LogStream::follow(process, vec![]).unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(50), logs.try_next())
            .await
            .is_err()
    );
    release.write_all(b"release\n").await.unwrap();
    let error = tokio::time::timeout(Duration::from_secs(5), logs.try_next())
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Journald);
    assert!(error.to_string().contains("9"), "{error}");
}

#[tokio::test]
async fn log_source_success_preserves_snapshot_then_live_order_and_cursor() {
    let initial: JournalctlEntry = serde_json::from_str(ENTRY).unwrap();
    let mut logs = LogStream::follow(
        child(&format!(
            "printf '%s\\n' '{}'",
            ENTRY.replace("last entry", "live entry")
        )),
        vec![initial],
    )
    .unwrap();
    assert_eq!(
        logs.try_next().await.unwrap().unwrap().message,
        "last entry"
    );
    let (cursor, entry) = logs.try_next().await.unwrap().unwrap().log_entry().unwrap();
    assert_eq!(cursor, "cursor-a");
    assert_eq!(entry.message, "live entry");
    assert!(logs.try_next().await.unwrap().is_none());
}

#[tokio::test]
async fn log_source_disposal_stops_the_owned_child() {
    let process = child(&format!("printf '%s\\n' '{ENTRY}'; exec sleep 300"));
    let pid = nix::unistd::Pid::from_raw(process.id().unwrap() as i32);
    let mut logs = LogStream::follow(process, vec![]).unwrap();
    assert!(logs.try_next().await.unwrap().is_some());
    drop(logs);
    tokio::time::timeout(Duration::from_secs(5), async {
        while nix::sys::signal::kill(pid, None).is_ok() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[test]
fn boot_selector_rejects_wide_and_fractional_numbers_without_wrapping() {
    for literal in [
        "2147483648",
        "-2147483649",
        "4294967295",
        "18446744073709551615",
        "1.5",
        "-0.5",
        "2147483648.0",
        "1e100",
    ] {
        assert!(
            serde_json::from_str::<BootIdentifier>(literal).is_err(),
            "accepted {literal}"
        );
    }
}

#[test]
fn boot_selector_preserves_integer_boundaries_integral_floats_and_string_ids() {
    for (literal, expected) in [
        ("-2147483648", i32::MIN),
        ("2147483647", i32::MAX),
        ("1.0", 1),
        ("-1.0", -1),
        ("0", 0),
    ] {
        assert!(
            matches!(serde_json::from_str::<BootIdentifier>(literal).unwrap(), BootIdentifier::Index(value) if value == expected)
        );
    }
    for id in ["boot-id", "2147483648", ""] {
        assert!(
            matches!(serde_json::from_value::<BootIdentifier>(serde_json::json!(id)).unwrap(), BootIdentifier::Id(value) if value == id)
        );
    }
}
