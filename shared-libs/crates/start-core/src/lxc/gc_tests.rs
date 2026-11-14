use std::os::unix::fs::{PermissionsExt, symlink};

use super::*;

#[test]
fn gc_stops_untracked_containers_and_reports_failures() {
    const CHILD: &str = "STARTOS_LXC_GC_TEST_DIR";
    let test = format!(
        "{}::gc_stops_untracked_containers_and_reports_failures",
        module_path!().split_once("::").unwrap().1
    );
    let Some(directory) = std::env::var_os(CHILD) else {
        let directory = tempfile::tempdir().unwrap();
        let cli = directory.path().join("cli");
        std::fs::write(
            &cli,
            r#"#!/bin/sh
set -eu
case "${0##*/}" in
    lxc-ls)
        if [ "$#" = 2 ]; then
            [ "$1" = -1 ] && [ "$2" = --running ] || exit 2
            printf '%s\n' "$GC_TRACKED" "$GC_RUNNING" "$GC_FAILED"
        else
            [ "$#" = 1 ] && [ "$1" = -1 ] || exit 2
            printf '%s\n' "$GC_TRACKED" "$GC_RUNNING" "$GC_STOPPED" "$GC_FAILED"
        fi
        ;;
    lxc-stop)
        [ "$#" = 3 ] && [ "$1" = --kill ] && [ "$2" = --name ] || exit 2
        printf 'stop %s\n' "$3" >> "$STARTOS_LXC_GC_TEST_DIR/events"
        if [ "$3" = "$GC_FAILED" ] && [ -e "$STARTOS_LXC_GC_TEST_DIR/fail-stop" ]; then
            printf 'injected stop failure\n' >&2
            exit 1
        fi
        ;;
    lxc-destroy)
        [ "$#" = 3 ] && [ "$1" = --force ] && [ "$2" = --name ] || exit 2
        printf 'destroy %s\n' "$3" >> "$STARTOS_LXC_GC_TEST_DIR/events"
        if [ "$3" = "$GC_STOPPED" ] && [ -e "$STARTOS_LXC_GC_TEST_DIR/fail-destroy" ]; then
            printf 'injected destroy failure\n' >&2
            exit 1
        fi
        ;;
    *) exit 2 ;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
        for command in ["lxc-ls", "lxc-stop", "lxc-destroy"] {
            symlink(&cli, directory.path().join(command)).unwrap();
        }
        std::fs::write(directory.path().join("fail-stop"), "").unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &test, "--nocapture"])
            .env(CHILD, directory.path())
            .env("PATH", directory.path())
            .env("GC_TRACKED", &*new_guid())
            .env("GC_RUNNING", &*new_guid())
            .env("GC_STOPPED", &*new_guid())
            .env("GC_FAILED", &*new_guid())
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stdout.contains("running 1 test"), "{stdout}\n{stderr}");
        assert!(output.status.success(), "{stdout}\n{stderr}");
        return;
    };

    let directory = Path::new(&directory);
    let tracked = std::env::var("GC_TRACKED").unwrap();
    let running = std::env::var("GC_RUNNING").unwrap();
    let stopped = std::env::var("GC_STOPPED").unwrap();
    let failed = std::env::var("GC_FAILED").unwrap();
    for container in [&tracked, &running, &stopped, &failed] {
        assert!(!Path::new(LXC_CONTAINER_DIR).join(container).exists());
    }
    let tracked = Arc::new(ContainerId::try_from(tracked.as_str()).unwrap());
    let manager = LxcManager::new();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        manager
            .containers
            .lock()
            .await
            .push(Arc::downgrade(&tracked));
        let error = manager.gc().await.unwrap_err();
        assert!(error.to_string().contains("injected stop failure"));
        let events_path = directory.join("events");
        let events = std::fs::read_to_string(&events_path).unwrap();
        let mut expected = vec![
            format!("stop {running}"),
            format!("destroy {running}"),
            format!("destroy {stopped}"),
            format!("stop {failed}"),
        ];
        assert_events(&events, &expected);
        assert_stop_precedes_destroy(&events, &running);

        std::fs::remove_file(directory.join("fail-stop")).unwrap();
        std::fs::write(directory.join("fail-destroy"), "").unwrap();
        std::fs::remove_file(&events_path).unwrap();
        manager.gc().await.unwrap();
        let events = std::fs::read_to_string(&events_path).unwrap();
        expected.push(format!("destroy {failed}"));
        assert_events(&events, &expected);
        for container in [&running, &failed] {
            assert_stop_precedes_destroy(&events, container);
        }
    });
}

fn assert_stop_precedes_destroy(events: &str, container: &str) {
    let stop = events.find(&format!("stop {container}\n")).unwrap();
    let destroy = events.find(&format!("destroy {container}\n")).unwrap();
    assert!(stop < destroy);
}

fn assert_events(actual: &str, expected: &[String]) {
    let mut actual: Vec<_> = actual.lines().collect();
    let mut expected: Vec<_> = expected.iter().map(String::as_str).collect();
    actual.sort_unstable();
    expected.sort_unstable();
    assert_eq!(actual, expected);
}
