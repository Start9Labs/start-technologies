use std::path::{Path, PathBuf};

use tokio::process::Command;

use super::initramfs::{RUNTIME, invalid_handoff};
use crate::prelude::*;
use crate::util::Invoke;
use crate::util::io::{delete_file, write_file_atomic};

const TARGET: &str = "startos-fde.target";
const LOCK: &str = "services-locked";
const SSH_MARKER: &str = "etc/ssh/sshd_not_to_be_run";

async fn directory(root: &Path, parts: &[&str]) -> Result<PathBuf, Error> {
    let mut path = root.to_owned();
    for part in parts {
        path.push(part);
        match tokio::fs::create_dir(&path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        if !tokio::fs::symlink_metadata(&path).await?.is_dir() {
            return Err(invalid_handoff());
        }
    }
    Ok(path)
}

fn profile_links(root: &Path) -> [(PathBuf, PathBuf); 2] {
    [
        (
            root.join("etc/systemd/system/default.target"),
            TARGET.into(),
        ),
        (root.join(SSH_MARKER), Path::new(RUNTIME).join(LOCK)),
    ]
}

fn saved_entry(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap().to_os_string();
    name.push(".startos-fde-original");
    path.with_file_name(name)
}

async fn install_link(path: &Path, target: &Path) -> Result<(), Error> {
    let saved = saved_entry(path);
    match tokio::fs::symlink_metadata(&saved).await {
        Ok(_) => return Err(invalid_handoff()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    match tokio::fs::rename(path, saved).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    tokio::fs::symlink(target, path).await?;
    Ok(())
}

async fn restore_link(path: &Path, target: &Path) -> Result<(), Error> {
    match tokio::fs::read_link(path).await {
        Ok(existing) if existing != target => return Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => return Ok(()),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    delete_file(path).await?;
    match tokio::fs::rename(saved_entry(path), path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

async fn restore(root: &Path) -> Result<(), Error> {
    for (path, target) in profile_links(root) {
        restore_link(&path, &target).await?;
    }
    Ok(())
}

/// Runs before PID1 in the private running upper; saved configuration stays unchanged.
pub(super) async fn prepare(root: &Path, runtime: &Path) -> Result<(), Error> {
    let units = directory(root, &["etc", "systemd", "system"]).await?;
    write_file_atomic(runtime.join(LOCK), []).await?;
    write_file_atomic(units.join(TARGET), include_bytes!("bootstrap.target")).await?;
    for unit in ["emergency.service", "rescue.service", "debug-shell.service"] {
        let dropin = directory(&units, &[&format!("{unit}.d")]).await?;
        write_file_atomic(
            dropin.join("99-startos-fde.conf"),
            format!("[Unit]\nConditionPathExists=!{RUNTIME}/{LOCK}\n"),
        )
        .await?;
    }
    directory(root, &["etc", "ssh"]).await?;
    for (path, target) in profile_links(root) {
        install_link(&path, &target).await?;
    }
    Ok(())
}

/// The caller has validated retained keys and completed ordinary data initialization.
pub(crate) async fn release() -> Result<(), Error> {
    restore(Path::new("/")).await?;
    delete_file(Path::new(RUNTIME).join(LOCK)).await?;
    Command::new("systemctl")
        .arg("daemon-reload")
        .invoke(ErrorKind::Filesystem)
        .await?;
    Command::new("systemctl")
        .args(["start", "default.target"])
        .invoke(ErrorKind::Filesystem)
        .await?;
    Command::new("systemctl")
        .args(["stop", TARGET])
        .invoke(ErrorKind::Filesystem)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn normal_target_and_existing_ssh_disable_intent_survive_release() {
        for original in [None, Some("graphical.target"), Some("multi-user.target")] {
            let root = tempfile::tempdir().unwrap();
            let runtime = tempfile::tempdir().unwrap();
            let units = directory(root.path(), &["etc", "systemd", "system"])
                .await
                .unwrap();
            if let Some(target) = original {
                tokio::fs::symlink(target, units.join("default.target"))
                    .await
                    .unwrap();
            }
            directory(root.path(), &["etc", "ssh"]).await.unwrap();
            write_file_atomic(root.path().join(SSH_MARKER), b"administrator intent")
                .await
                .unwrap();
            prepare(root.path(), runtime.path()).await.unwrap();
            assert_eq!(
                tokio::fs::read_link(units.join("default.target"))
                    .await
                    .unwrap(),
                Path::new(TARGET)
            );
            assert!(runtime.path().join(LOCK).is_file());
            restore(root.path()).await.unwrap();
            restore(root.path()).await.unwrap();
            assert_eq!(
                tokio::fs::read_link(units.join("default.target"))
                    .await
                    .ok()
                    .as_deref(),
                original.map(Path::new)
            );
            assert_eq!(
                tokio::fs::read(root.path().join(SSH_MARKER)).await.unwrap(),
                b"administrator intent"
            );
        }
    }

    #[tokio::test]
    async fn profile_materializes_private_gate_and_native_interactive_unit_conditions() {
        let root = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        prepare(root.path(), runtime.path()).await.unwrap();
        assert_eq!(
            tokio::fs::read_link(root.path().join(SSH_MARKER))
                .await
                .unwrap(),
            Path::new(RUNTIME).join(LOCK)
        );
        for unit in ["emergency.service", "rescue.service", "debug-shell.service"] {
            assert!(
                root.path()
                    .join(format!("etc/systemd/system/{unit}.d/99-startos-fde.conf"))
                    .is_file()
            );
        }
    }

    #[tokio::test]
    async fn interrupted_restoration_preserves_a_dangling_administrator_marker() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("marker");
        tokio::fs::symlink("administrator-runtime-marker", &path)
            .await
            .unwrap();
        install_link(&path, Path::new("gate")).await.unwrap();
        delete_file(&path).await.unwrap();
        restore_link(&path, Path::new("gate")).await.unwrap();
        restore_link(&path, Path::new("gate")).await.unwrap();
        assert_eq!(
            tokio::fs::read_link(path).await.unwrap(),
            Path::new("administrator-runtime-marker")
        );
    }

    #[tokio::test]
    async fn restoration_keeps_an_authoritative_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("entry");
        write_file_atomic(&path, b"old intent").await.unwrap();
        install_link(&path, Path::new("gate")).await.unwrap();
        write_file_atomic(&path, b"new intent").await.unwrap();
        restore_link(&path, Path::new("gate")).await.unwrap();
        assert_eq!(tokio::fs::read(path).await.unwrap(), b"new intent");
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires root and native PID1 in an exclusive managed StartOS guest; controller unit must IgnoreOnIsolate; never run on the build host"]
    async fn real_systemd_profile_isolates_ssh_then_restores_normal_target() {
        use std::panic::AssertUnwindSafe;

        use futures::FutureExt;
        assert_eq!(unsafe { libc::geteuid() }, 0);
        let runtime = Path::new(RUNTIME);
        tokio::fs::create_dir(runtime).await.unwrap();
        let default_before = Command::new("systemctl")
            .arg("get-default")
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
        let result = AssertUnwindSafe(async {
            prepare(Path::new("/"), runtime).await.unwrap();
            Command::new("systemctl")
                .arg("daemon-reload")
                .invoke(ErrorKind::Filesystem)
                .await
                .unwrap();
            Command::new("systemctl")
                .args(["isolate", TARGET])
                .invoke(ErrorKind::Filesystem)
                .await
                .unwrap();
            let dependencies = Command::new("systemctl")
                .args([
                    "list-dependencies",
                    "--plain",
                    "--all",
                    "--no-pager",
                    TARGET,
                ])
                .invoke(ErrorKind::Filesystem)
                .await
                .unwrap();
            let dependencies = String::from_utf8(dependencies).unwrap();
            for forbidden in [
                "multi-user.target",
                "graphical.target",
                "sockets.target",
                "timers.target",
                "lxc.service",
                "lxc-net.service",
                "ssh.service",
                "ssh.socket",
            ] {
                assert!(
                    !dependencies
                        .lines()
                        .any(|line| line.split_whitespace().any(|word| word == forbidden)),
                    "unexpected bootstrap dependency: {forbidden}"
                );
            }
            for unit in ["ssh.service", "ssh.socket"] {
                Command::new("systemctl")
                    .args(["start", unit])
                    .invoke(ErrorKind::Filesystem)
                    .await
                    .unwrap();
                assert!(
                    Command::new("systemctl")
                        .args(["is-active", "--quiet", unit])
                        .invoke(ErrorKind::Filesystem)
                        .await
                        .is_err()
                );
            }
            let listeners = Command::new("ss")
                .args(["-H", "-ltn", "sport = :22"])
                .invoke(ErrorKind::Filesystem)
                .await
                .unwrap();
            assert!(listeners.is_empty());
            release().await.unwrap();
            assert!(
                Command::new("systemctl")
                    .args(["is-active", "--quiet", TARGET])
                    .invoke(ErrorKind::Filesystem)
                    .await
                    .is_err()
            );
            let default_after = Command::new("systemctl")
                .arg("get-default")
                .invoke(ErrorKind::Filesystem)
                .await
                .unwrap();
            assert_eq!(default_before, default_after);
            let default_name = String::from_utf8(default_after).unwrap();
            Command::new("systemctl")
                .args(["is-active", "--quiet", default_name.trim()])
                .invoke(ErrorKind::Filesystem)
                .await
                .unwrap();
            Command::new("systemctl")
                .args(["start", "ssh.service"])
                .invoke(ErrorKind::Filesystem)
                .await
                .unwrap();
            Command::new("systemctl")
                .args(["is-active", "--quiet", "ssh.service"])
                .invoke(ErrorKind::Filesystem)
                .await
                .unwrap();
            let listeners = Command::new("ss")
                .args(["-H", "-ltn", "sport = :22"])
                .invoke(ErrorKind::Filesystem)
                .await
                .unwrap();
            assert!(!listeners.is_empty());
        })
        .catch_unwind()
        .await;
        let cleanup = release().await;
        delete_file(Path::new("/etc/systemd/system").join(TARGET))
            .await
            .unwrap();
        for unit in ["emergency.service", "rescue.service", "debug-shell.service"] {
            delete_file(format!("/etc/systemd/system/{unit}.d/99-startos-fde.conf"))
                .await
                .unwrap();
        }
        crate::util::io::delete_dir(runtime).await.unwrap();
        Command::new("systemctl")
            .arg("daemon-reload")
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
        cleanup.unwrap();
    }

    #[tokio::test]
    async fn profile_directories_cannot_escape_through_signed_configuration_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        tokio::fs::symlink(outside.path(), root.path().join("etc"))
            .await
            .unwrap();
        assert_eq!(
            prepare(root.path(), runtime.path()).await.unwrap_err().kind,
            ErrorKind::InvalidSignature
        );
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    }
}
