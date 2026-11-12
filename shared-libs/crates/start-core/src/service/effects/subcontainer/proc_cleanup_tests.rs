use super::*;

#[test]
fn image_proc_directories_cannot_supply_process_metadata() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("proc/1/ns")).unwrap();
    std::os::unix::fs::symlink("/proc/1/ns/pid", dir.path().join("proc/1/ns/pid")).unwrap();
    let root = File::open(dir.path()).unwrap();
    assert!(proc_mount(&root).unwrap().is_none());
    kill_init(&root).unwrap();
}

#[test]
fn proc_symlinks_are_rejected_without_touching_host_procfs() {
    let dir = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink("/proc", dir.path().join("proc")).unwrap();
    let root = File::open(dir.path()).unwrap();
    assert!(kill_init(&root).is_err());
    assert!(std::fs::metadata("/proc/self/status").is_ok());
}

#[test]
fn container_side_proc_lookup_retains_its_alias_support() {
    let dir = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink("/proc", dir.path().join("proc")).unwrap();
    assert!(container_proc_mount(dir.path()).unwrap().is_some());
    let root = File::open(dir.path()).unwrap();
    assert!(proc_mount(&root).is_err());
}

async fn namespace_init_cleanup(depth: usize) {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("root");
    let outside = dir.path().join("outside");
    let moved = dir.path().join("moved");
    std::fs::create_dir_all(original.join("proc")).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink("/proc", outside.join("proc")).unwrap();
    let root = File::open(&original).unwrap();
    let mut command = tokio::process::Command::new("unshare");
    command.args(["--pid", "--fork", "--kill-child"]);
    for _ in 1..depth {
        command.args(["unshare", "--pid", "--fork", "--kill-child"]);
    }
    command
        .args([
            "sh",
            "-c",
            "mount -t proc proc \"$1/proc\" && printf 'ready\\n' && exec sleep 300",
            "_",
        ])
        .arg(&original)
        .stdout(Stdio::piped())
        .kill_on_drop(true);
    let mut init = command.spawn().unwrap();
    let mut stdout = BufReader::new(init.stdout.take().unwrap()).lines();
    let ready = tokio::time::timeout(std::time::Duration::from_secs(5), stdout.next_line())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ready.as_deref(), Some("ready"));
    assert!(proc_mount(&root).unwrap().is_some());
    assert!(init.try_wait().unwrap().is_none());

    let mut unrelated = tokio::process::Command::new("sleep")
        .arg("300")
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    std::fs::rename(&original, &moved).unwrap();
    std::os::unix::fs::symlink(&outside, &original).unwrap();
    tokio::task::spawn_blocking(move || kill_init(&root))
        .await
        .unwrap()
        .unwrap();
    let status = tokio::time::timeout(std::time::Duration::from_secs(5), init.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(!status.success());
    assert!(unrelated.try_wait().unwrap().is_none());
    assert!(!moved.join("proc/1").exists());
    assert!(std::fs::metadata(outside.join("proc/self/status")).is_ok());
    unrelated.kill().await.unwrap();
}

#[tokio::test]
#[ignore = "requires root in a private mount/PID test environment"]
async fn proc_cleanup_targets_only_the_pinned_namespace_init() {
    namespace_init_cleanup(1).await;
}

#[tokio::test]
#[ignore = "requires root in a private mount/PID test environment"]
async fn proc_cleanup_supports_deeper_pid_namespace_nesting() {
    namespace_init_cleanup(2).await;
}
