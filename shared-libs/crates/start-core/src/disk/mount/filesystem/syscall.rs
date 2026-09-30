//! Minimal wrappers around the Linux mount API needed for idmapped binds.
//!
//! The mount layer otherwise uses `mount(8)` (see the `FileSystem` impls).
//! The one thing no higher-level interface exposes is `open_tree_attr(2)`
//! (Linux 6.15+), which atomically clones a mount and applies an idmap —
//! required for the nested-idmap case (idmapping a clone whose source
//! filesystem is already idmapped, i.e. the in-LXC-userns inner bind). That,
//! plus the userns-fd-from-idmap helper it needs, is all this module
//! provides. Everything goes through `libc::syscall` because neither `nix`
//! nor `rustix` exposes `open_tree_attr`.

use std::ffi::{CString, OsString};
use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::time::Duration;

use libc::{
    AT_EMPTY_PATH, AT_FDCWD, AT_RECURSIVE, MOUNT_ATTR_IDMAP, MOUNT_ATTR_RDONLY, OPEN_TREE_CLOEXEC,
    OPEN_TREE_CLONE,
};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::prelude::*;

// linux/mount.h. Defined locally because libc 0.2 only exports
// `MOVE_MOUNT_F_EMPTY_PATH` for gnu-linux, not for the musl targets we also
// cross-build against.
const MOVE_MOUNT_F_EMPTY_PATH: libc::c_uint = 0x00000004;

// open_tree_attr(2) — added in Linux 6.15. libc 0.2 only defines
// SYS_open_tree_attr for m68k, but the number is unified at 467 across
// x86_64 / aarch64 / riscv64 / armv7 / i686, covering every arch start-os
// targets.
const SYS_OPEN_TREE_ATTR: libc::c_long = 467;

#[repr(C)]
#[derive(Default)]
struct MountAttr {
    attr_set: u64,
    attr_clr: u64,
    propagation: u64,
    userns_fd: u64,
}

fn cstr_path(p: &Path) -> Result<CString, Error> {
    CString::new(p.as_os_str().as_bytes())
        .with_ctx(|_| (ErrorKind::Filesystem, format!("path contains NUL: {p:?}")))
}

fn cstr_str(s: &str) -> Result<CString, Error> {
    CString::new(s).with_ctx(|_| (ErrorKind::Filesystem, format!("string contains NUL: {s:?}")))
}

fn fd_from_raw(raw: i64, what: &'static str) -> Result<OwnedFd, Error> {
    if raw < 0 {
        Err(Error::new(
            io::Error::last_os_error(),
            ErrorKind::Filesystem,
        ))
        .with_ctx(|_| (ErrorKind::Filesystem, what))
    } else {
        Ok(unsafe { OwnedFd::from_raw_fd(raw as RawFd) })
    }
}

fn check(ret: i64, what: &'static str) -> Result<(), Error> {
    if ret < 0 {
        Err(Error::new(
            io::Error::last_os_error(),
            ErrorKind::Filesystem,
        ))
        .with_ctx(|_| (ErrorKind::Filesystem, what))
    } else {
        Ok(())
    }
}

/// `open_tree(AT_FDCWD, path, OPEN_TREE_CLONE | OPEN_TREE_CLOEXEC | [AT_RECURSIVE])`
/// — a detached clone of the mount at `path` (its submounts too when
/// `recursive`).
pub fn open_tree_clone(path: &Path, recursive: bool) -> Result<OwnedFd, Error> {
    let p = cstr_path(path)?;
    let mut flags = OPEN_TREE_CLOEXEC | OPEN_TREE_CLONE;
    if recursive {
        flags |= AT_RECURSIVE as libc::c_uint;
    }
    let r = unsafe { libc::syscall(libc::SYS_open_tree, AT_FDCWD, p.as_ptr(), flags) };
    fd_from_raw(r, "open_tree")
}

/// `open_tree_attr(...)` with `MOUNT_ATTR_IDMAP` set and `attr->userns_fd =
/// userns` (Linux 6.15+).
///
/// The atomic equivalent of `open_tree(OPEN_TREE_CLONE) +
/// mount_setattr(MOUNT_ATTR_IDMAP)` — the cloned fd never exists in a
/// non-idmapped state. Required for the recursive-idmap case (idmapping a
/// clone whose source filesystem is already idmapped); the kernel refuses
/// the two-step pattern there.
pub fn open_tree_attr_idmap(
    path: &Path,
    recursive: bool,
    userns: BorrowedFd,
) -> Result<OwnedFd, Error> {
    let p = cstr_path(path)?;
    let mut flags = OPEN_TREE_CLOEXEC | OPEN_TREE_CLONE;
    if recursive {
        flags |= AT_RECURSIVE as libc::c_uint;
    }
    let attr = MountAttr {
        attr_set: MOUNT_ATTR_IDMAP,
        attr_clr: 0,
        propagation: 0,
        userns_fd: userns.as_raw_fd() as u64,
    };
    let r = unsafe {
        libc::syscall(
            SYS_OPEN_TREE_ATTR,
            AT_FDCWD,
            p.as_ptr(),
            flags,
            &attr as *const _ as *const libc::c_void,
            std::mem::size_of::<MountAttr>(),
        )
    };
    fd_from_raw(r, "open_tree_attr IDMAP")
}

/// `move_mount(detached, "", AT_FDCWD, to, MOVE_MOUNT_F_EMPTY_PATH)` — attach
/// a detached mount fd at a path.
fn move_mount(detached: BorrowedFd, to: &Path) -> Result<(), Error> {
    let empty = cstr_str("")?;
    let t = cstr_path(to)?;
    let r = unsafe {
        libc::syscall(
            libc::SYS_move_mount,
            detached.as_raw_fd(),
            empty.as_ptr(),
            AT_FDCWD,
            t.as_ptr(),
            MOVE_MOUNT_F_EMPTY_PATH,
        )
    };
    check(r, "move_mount")
}

/// `mount_setattr(fd, "", AT_EMPTY_PATH, attr, sizeof(attr))` toggling
/// `MOUNT_ATTR_RDONLY`.
fn mount_setattr_ro(fd: BorrowedFd, ro: bool) -> Result<(), Error> {
    let attr = MountAttr {
        attr_set: if ro { MOUNT_ATTR_RDONLY } else { 0 },
        attr_clr: if ro { 0 } else { MOUNT_ATTR_RDONLY },
        propagation: 0,
        userns_fd: 0,
    };
    let empty = cstr_str("")?;
    let r = unsafe {
        libc::syscall(
            libc::SYS_mount_setattr,
            fd.as_raw_fd(),
            empty.as_ptr(),
            AT_EMPTY_PATH,
            &attr as *const _ as *const libc::c_void,
            std::mem::size_of::<MountAttr>(),
        )
    };
    check(r, "mount_setattr RDONLY")
}

/// A detached mount fd (from `open_tree`/`open_tree_attr`). Not visible at any
/// path until [`DetachedMount::attach`]; dropping the fd discards it.
#[derive(Debug)]
pub struct DetachedMount {
    fd: OwnedFd,
}
impl DetachedMount {
    pub fn from_fd(fd: OwnedFd) -> Self {
        Self { fd }
    }
    /// Toggle `MOUNT_ATTR_RDONLY` on the detached mount.
    pub fn set_readonly(&self, ro: bool) -> Result<(), Error> {
        mount_setattr_ro(self.fd.as_fd(), ro)
    }
    /// `move_mount` the detached mount onto `mountpoint`, consuming self.
    pub fn attach(self, mountpoint: &Path) -> Result<(), Error> {
        move_mount(self.fd.as_fd(), mountpoint)
    }
}

/// Build a userns whose uid_map/gid_map describes `idmap` and return an owned
/// fd to it, for `open_tree_attr(MOUNT_ATTR_IDMAP, ...)`.
///
/// Spawns the `unshare-userns` helper subcommand, which `unshare`s a userns
/// and pauses; the parent writes its `/proc/<pid>/uid_map` + `gid_map` (it
/// has CAP_SETUID in the parent ns), opens `/proc/<pid>/ns/user`, then closes
/// the helper's stdin to let it exit. The captured fd keeps the namespace
/// alive. Done as a subprocess rather than an in-process fork because forking
/// a multi-threaded tokio runtime is unsound.
pub async fn userns_fd_from_idmap(
    idmap: &[crate::disk::mount::filesystem::idmapped::IdMap],
) -> Result<OwnedFd, Error> {
    // /proc/<pid>/uid_map columns are "id-in-userns id-in-parent range".
    // For an idmapped mount the kernel runs the on-disk id through
    // map_id_down (matched against col1, output col2), so col1 is the
    // on-disk id (IdMap::from_id) and col2 is the id the mount user sees
    // (IdMap::to_id). Order: `from_id to_id range`.
    let uid_map = idmap
        .iter()
        .map(|i| format!("{} {} {}\n", i.from_id, i.to_id, i.range))
        .collect::<String>();
    let gid_map = uid_map.clone();

    let helper = which_self_exe()?;
    // The helper runs `unshare(CLONE_NEWUSER)`, which the kernel rejects
    // (EINVAL) on a multi-threaded process, so it must be a minimal
    // multi-call applet rather than a CLI subcommand: the CLI's main()
    // enables the logger, whose tracing-appender background thread makes
    // the process multi-threaded before any handler runs. The applet does
    // no such global init and stays single-threaded.
    //
    // The MultiExecutable dispatcher selects the applet by argv[0]'s
    // basename, falling through to argv[1] only when argv[0] isn't itself a
    // registered applet. current_exe is `start-container` inside the LXC (a
    // registered, default applet), which would shadow the selector and run
    // the container CLI. Override argv[0] with a non-applet sentinel so
    // dispatch falls through to `unshare-userns` regardless of current_exe's
    // name (busybox-style multi-call selection).
    let mut child = Command::new(&helper)
        .arg0("startos-unshare-userns-helper")
        .arg("unshare-userns")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_kind(ErrorKind::Filesystem)?;

    let pid = child.id().ok_or_else(|| {
        Error::new(
            eyre!("helper exited before reporting pid"),
            ErrorKind::Filesystem,
        )
    })?;

    {
        let stdout = child
            .stdout
            .as_mut()
            .ok_or_else(|| Error::new(eyre!("no stdout"), ErrorKind::Filesystem))?;
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        // bounded wait so a wedged helper doesn't hang us forever
        match tokio::time::timeout(Duration::from_secs(5), reader.read_line(&mut line)).await {
            Ok(Ok(_)) if line.trim() == "ready" => (),
            Ok(Ok(_)) => {
                let mut stderr = String::new();
                if let Some(mut e) = child.stderr.take() {
                    use tokio::io::AsyncReadExt;
                    let _ = e.read_to_string(&mut stderr).await;
                }
                let _ = child.kill().await;
                return Err(Error::new(
                    eyre!(
                        "unshare-userns helper failed (stdout: {:?}, stderr: {})",
                        line.trim(),
                        stderr.trim()
                    ),
                    ErrorKind::Filesystem,
                ));
            }
            Ok(Err(e)) => {
                let _ = child.kill().await;
                return Err(Error::new(e, ErrorKind::Filesystem));
            }
            Err(_) => {
                let _ = child.kill().await;
                return Err(Error::new(
                    eyre!("unshare-userns helper timed out"),
                    ErrorKind::Filesystem,
                ));
            }
        }
    }

    let uid_map_path = format!("/proc/{pid}/uid_map");
    let gid_map_path = format!("/proc/{pid}/gid_map");
    if let Err(e) = tokio::fs::write(&uid_map_path, &uid_map).await {
        let _ = child.kill().await;
        return Err(Error::new(e, ErrorKind::Filesystem))
            .with_ctx(|_| (ErrorKind::Filesystem, format!("write {uid_map_path}")));
    }
    if let Err(e) = tokio::fs::write(&gid_map_path, &gid_map).await {
        let _ = child.kill().await;
        return Err(Error::new(e, ErrorKind::Filesystem))
            .with_ctx(|_| (ErrorKind::Filesystem, format!("write {gid_map_path}")));
    }

    let ns_path = format!("/proc/{pid}/ns/user");
    let userns_fd = tokio::task::spawn_blocking({
        let ns_path = ns_path.clone();
        move || -> Result<OwnedFd, std::io::Error> {
            use std::os::fd::IntoRawFd;
            let f = std::fs::File::open(&ns_path)?;
            Ok(unsafe { OwnedFd::from_raw_fd(f.into_raw_fd()) })
        }
    })
    .await
    .with_kind(ErrorKind::Cancelled)?
    .map_err(|e| Error::new(e, ErrorKind::Filesystem))
    .with_ctx(|_| (ErrorKind::Filesystem, format!("open {ns_path}")))?;

    // signal helper to exit and reap it
    drop(child.stdin.take());
    let _ = child.wait().await;

    Ok(userns_fd)
}

fn which_self_exe() -> Result<OsString, Error> {
    // Tests run outside the multicall binary, so the `unshare-userns` applet
    // isn't reachable via current_exe; let them point at a stand-in helper.
    #[cfg(test)]
    if let Some(p) = std::env::var_os("STARTOS_TEST_USERNS_HELPER") {
        return Ok(p);
    }
    std::env::current_exe()
        .map(|p| p.into_os_string())
        .map_err(|e| Error::new(e, ErrorKind::Filesystem))
        .with_ctx(|_| (ErrorKind::Filesystem, "readlink /proc/self/exe"))
}

/// Body of the `unshare-userns` subcommand: `unshare(CLONE_NEWUSER)`, print
/// `ready\n`, then block until stdin closes.
pub fn unshare_userns_main() -> std::io::Result<()> {
    use std::io::{Read, Write};

    nix::sched::unshare(nix::sched::CloneFlags::CLONE_NEWUSER)
        .map_err(|e| std::io::Error::from_raw_os_error(e as i32))?;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    out.write_all(b"ready\n")?;
    out.flush()?;
    drop(out);

    let mut buf = [0u8; 1];
    let _ = std::io::stdin().read(&mut buf);
    Ok(())
}
