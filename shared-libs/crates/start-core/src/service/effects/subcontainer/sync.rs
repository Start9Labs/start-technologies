use std::ffi::{OsStr, OsString, c_int};
use std::fs::File;
use std::io::{BufRead, BufReader, IsTerminal, Read};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Stdio};
use std::sync::Arc;

use nix::errno::Errno;
use nix::sched::CloneFlags;
use nix::unistd::Pid;
use signal_hook::consts::signal::*;
use termion::raw::IntoRawMode;
use tokio::sync::oneshot;

use crate::CAP_1_KiB;
use crate::service::effects::ContainerCliContext;
use crate::service::effects::prelude::*;
use crate::util::io::TermSize;

const FWD_SIGNALS: &[c_int] = &[
    SIGABRT, SIGALRM, SIGCONT, SIGHUP, SIGINT, SIGIO, SIGPIPE, SIGPROF, SIGQUIT, SIGTERM, SIGTRAP,
    SIGTSTP, SIGTTIN, SIGTTOU, SIGURG, SIGUSR1, SIGUSR2, SIGVTALRM,
];

pub fn kill_init(procfs: &Path, chroot: &Path) -> Result<(), Error> {
    if chroot.join("proc/1").exists() {
        let ns_id = procfs::process::Process::new_with_root(chroot.join("proc/1"))
            .with_ctx(|_| (ErrorKind::Filesystem, "open subcontainer procfs"))?
            .namespaces()
            .with_ctx(|_| (ErrorKind::Filesystem, "read subcontainer pid 1 ns"))?
            .0
            .get(OsStr::new("pid"))
            .or_not_found("pid namespace")?
            .identifier;
        for proc in procfs::process::all_processes_with_root(procfs)
            .with_ctx(|_| (ErrorKind::Filesystem, "open procfs"))?
        {
            let proc = proc.with_ctx(|_| (ErrorKind::Filesystem, "read single process details"))?;
            let pid = proc.pid();
            if proc
                .namespaces()
                .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("read pid {} ns", pid)))?
                .0
                .get(OsStr::new("pid"))
                .map_or(false, |ns| ns.identifier == ns_id)
            {
                let pids = proc.read::<_, NSPid>("status").with_ctx(|_| {
                    (
                        ErrorKind::Filesystem,
                        lazy_format!("read pid {} NSpid", pid),
                    )
                })?;
                if pids.0.len() == 2 && pids.0[1] == 1 {
                    match nix::sys::signal::kill(
                        Pid::from_raw(pid),
                        Some(nix::sys::signal::SIGKILL),
                    ) {
                        Err(Errno::ESRCH) => Ok(()),
                        a => a,
                    }
                    .with_ctx(|_| {
                        (
                            ErrorKind::Filesystem,
                            lazy_format!(
                                "kill pid {} (determined to be pid 1 in subcontainer)",
                                pid
                            ),
                        )
                    })?;
                }
            }
        }
        nix::mount::umount(&chroot.join("proc"))
            .with_ctx(|_| (ErrorKind::Filesystem, "unmounting subcontainer procfs"))?;
    }
    Ok(())
}

struct NSPid(Vec<i32>);
impl procfs::FromBufRead for NSPid {
    fn from_buf_read<R: std::io::BufRead>(r: R) -> procfs::ProcResult<Self> {
        for line in r.lines() {
            let line = line?;
            if let Some(row) = line.trim().strip_prefix("NSpid:") {
                return Ok(Self(
                    row.trim()
                        .split_ascii_whitespace()
                        .map(|pid| pid.parse::<i32>())
                        .collect::<Result<Vec<_>, _>>()?,
                ));
            }
        }
        Err(procfs::ProcError::Incomplete(None))
    }
}

fn open_file_read(path: impl AsRef<Path>) -> Result<File, Error> {
    File::open(&path).with_ctx(|_| {
        (
            ErrorKind::Filesystem,
            lazy_format!("open r {}", path.as_ref().display()),
        )
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
#[group(skip)]
pub struct ExecParams {
    #[arg(long, help = "help.arg.force-tty")]
    force_tty: bool,
    #[arg(long, help = "help.arg.force-stderr-tty")]
    force_stderr_tty: bool,
    #[arg(long, help = "help.arg.pty-size")]
    pty_size: Option<TermSize>,
    #[arg(short, long, help = "help.arg.env-variable")]
    env: Vec<String>,
    #[arg(long, help = "help.arg.env-file-path")]
    env_file: Option<PathBuf>,
    #[arg(short, long, help = "help.arg.workdir-path")]
    workdir: Option<PathBuf>,
    #[arg(short, long, help = "help.arg.user-name")]
    user: Option<String>,
    #[arg(help = "help.arg.chroot-path")]
    chroot: PathBuf,
    #[arg(trailing_var_arg = true, help = "help.arg.command-to-execute")]
    command: Vec<OsString>,
}
impl ExecParams {
    fn exec(&self, parent_death_signal: Option<c_int>) -> Result<(), Error> {
        let ExecParams {
            env,
            env_file,
            workdir,
            user,
            chroot,
            command,
            ..
        } = self;
        let Some(([command], args)) = command.split_at_checked(1) else {
            return Err(Error::new(
                eyre!("command cannot be empty"),
                ErrorKind::InvalidRequest,
            ));
        };

        let mut cmd = StdCommand::new(command);
        cmd.env_clear();
        cmd.env(
            "PATH",
            "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        );

        let mut needs_home = true;

        let mut update_env = |line: &str| {
            if let Some((k, v)) = line.split_once("=") {
                needs_home &= k != "HOME";
                cmd.env(k, v);
            } else if !line.is_empty() {
                cmd.env_remove(line);
            }
        };
        if let Some(f) = env_file {
            let mut lines = BufReader::new(
                File::open(&f).with_ctx(|_| (ErrorKind::Filesystem, format!("open r {f:?}")))?,
            )
            .lines();
            while let Some(line) = lines.next().transpose()? {
                update_env(&line);
            }
        }

        for line in std::fs::read_to_string("/etc/default/locale")
            .unwrap_or_default()
            .lines()
        {
            update_env(line);
        }

        for line in env {
            update_env(&line);
        }

        let ExecUser {
            uid,
            gid,
            home,
            groups,
        } = ExecUser::resolve(chroot, user.as_deref())?;
        if needs_home {
            cmd.env("HOME", home);
        }

        // Switch into the subcontainer rootfs via pivot_root rather than
        // chroot. The kernel's `current_chrooted()` check rejects
        // `unshare(CLONE_NEWUSER)` from a chrooted process, so a chrooted
        // service can't spawn a rootless OCI runtime (podman/docker), which
        // is the whole point of `manifest.userspaceFilesystems`. pivot_root runs
        // inside its own mount namespace and doesn't trip that check.
        nix::sched::unshare(CloneFlags::CLONE_NEWNS)
            .with_ctx(|_| (ErrorKind::Filesystem, "unshare mount ns"))?;
        nix::mount::mount(
            None::<&str>,
            "/",
            None::<&str>,
            nix::mount::MsFlags::MS_REC | nix::mount::MsFlags::MS_SLAVE,
            None::<&str>,
        )
        .with_ctx(|_| (ErrorKind::Filesystem, "make / private"))?;
        // pivot_root requires the new root to itself be a mount.
        nix::mount::mount(
            Some(chroot.as_path()),
            chroot.as_path(),
            None::<&str>,
            nix::mount::MsFlags::MS_BIND | nix::mount::MsFlags::MS_REC,
            None::<&str>,
        )
        .with_ctx(|_| {
            (
                ErrorKind::Filesystem,
                lazy_format!("bind {chroot:?} on itself"),
            )
        })?;
        let put_old = chroot.join(".put_old");
        std::fs::create_dir_all(&put_old)
            .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("mkdir {put_old:?}")))?;
        std::env::set_current_dir(chroot)
            .with_ctx(|_| (ErrorKind::Filesystem, lazy_format!("chdir {chroot:?}")))?;
        nix::unistd::pivot_root(".", ".put_old")
            .with_ctx(|_| (ErrorKind::Filesystem, "pivot_root"))?;
        std::env::set_current_dir("/").with_ctx(|_| (ErrorKind::Filesystem, "chdir /"))?;
        nix::mount::umount2("/.put_old", nix::mount::MntFlags::MNT_DETACH)
            .with_ctx(|_| (ErrorKind::Filesystem, "umount /.put_old"))?;
        std::fs::remove_dir("/.put_old").ok();
        if uid != 0 {
            std::os::unix::fs::chown("/proc/self/fd/0", Some(uid), Some(gid)).ok();
            std::os::unix::fs::chown("/proc/self/fd/1", Some(uid), Some(gid)).ok();
            std::os::unix::fs::chown("/proc/self/fd/2", Some(uid), Some(gid)).ok();
        }
        // Handle credential changes in pre_exec to control the order:
        // setgroups must happen before setgid/setuid (requires CAP_SETGID)
        unsafe {
            cmd.pre_exec(move || {
                // Create a new session so entrypoint scripts that do
                // kill(0, SIGTERM) don't cascade to other subcontainers.
                // EPERM means we're already a session leader (e.g. pty_process
                // called setsid() for us), which is fine.
                match nix::unistd::setsid() {
                    Ok(_) | Err(Errno::EPERM) => {}
                    Err(e) => {
                        return Err(std::io::Error::from_raw_os_error(e as i32));
                    }
                }
                nix::unistd::setgroups(&groups)
                    .map_err(|e| std::io::Error::from_raw_os_error(e as i32))?;
                nix::unistd::setgid(nix::unistd::Gid::from_raw(gid))
                    .map_err(|e| std::io::Error::from_raw_os_error(e as i32))?;
                nix::unistd::setuid(nix::unistd::Uid::from_raw(uid))
                    .map_err(|e| std::io::Error::from_raw_os_error(e as i32))?;
                // Restore dumpable flag cleared by setuid so that
                // /proc/self/fd/* is owned by the current uid and
                // /dev/stderr works for the target user.
                libc::prctl(libc::PR_SET_DUMPABLE, 1, 0, 0, 0);
                // Credential changes clear the parent-death signal.
                if let Some(signal) = parent_death_signal {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, signal) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
        cmd.args(args);

        if let Some(workdir) = workdir {
            cmd.current_dir(workdir);
        } else {
            cmd.current_dir("/");
        }
        Err(cmd.exec().into())
    }
}

struct ExecUser {
    uid: u32,
    gid: u32,
    home: String,
    groups: Vec<nix::unistd::Gid>,
}

impl ExecUser {
    fn resolve(chroot: &Path, spec: Option<&str>) -> Result<Self, Error> {
        let passwd = read_user_database(&chroot.join("etc/passwd"))?;
        let group = read_user_database(&chroot.join("etc/group"))?;
        let (user, group_spec) = match spec {
            Some(spec) => spec
                .split_once(':')
                .map_or((spec, None), |(u, g)| (u, Some(g))),
            None => ("0", None),
        };

        let uid_spec = user.parse::<u32>().ok();
        let entry = user_database_entries(&passwd)
            .find(|(name, uid, _)| uid_spec.map_or(*name == user, |u| u == *uid));
        let uid = uid_spec
            .or(entry.as_ref().map(|(_, uid, _)| *uid))
            .or((user == "root").then_some(0))
            .ok_or_else(|| {
                Error::new(
                    eyre!(
                        "{}",
                        t!(
                            "service.effects.subcontainer.sync.unknown-user",
                            user = user
                        )
                    ),
                    ErrorKind::InvalidRequest,
                )
            })?;

        let gid = match group_spec {
            Some(g) => g
                .parse()
                .ok()
                .or(user_database_entries(&group)
                    .find(|(name, ..)| *name == g)
                    .map(|(_, gid, _)| gid))
                .or((g == "root").then_some(0))
                .ok_or_else(|| {
                    Error::new(
                        eyre!(
                            "{}",
                            t!("service.effects.subcontainer.sync.unknown-group", group = g)
                        ),
                        ErrorKind::InvalidRequest,
                    )
                })?,
            None => entry
                .as_ref()
                .and_then(|(_, _, rest)| rest.first()?.parse().ok())
                .unwrap_or(0),
        };

        let home = entry
            .as_ref()
            .and_then(|(_, _, rest)| rest.get(2).copied())
            .unwrap_or("/")
            .to_owned();

        let groups = match &entry {
            Some((name, ..)) => user_database_entries(&group)
                .filter(|(_, _, rest)| {
                    rest.first()
                        .is_some_and(|members| members.split(',').any(|m| m == *name))
                })
                .map(|(_, gid, _)| nix::unistd::Gid::from_raw(gid))
                .collect(),
            None => Vec::new(),
        };

        Ok(Self {
            uid,
            gid,
            home,
            groups,
        })
    }
}

fn read_user_database(path: &Path) -> Result<String, Error> {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        res => res.with_ctx(|_| {
            (
                ErrorKind::Filesystem,
                lazy_format!("read {}", path.display()),
            )
        }),
    }
}

fn user_database_entries(db: &str) -> impl Iterator<Item = (&str, u32, Vec<&str>)> {
    db.lines().filter_map(|line| {
        let mut fields = line.trim().split(':');
        let name = fields.next()?;
        fields.next();
        let id = fields.next()?.parse().ok()?;
        Some((name, id, fields.collect()))
    })
}

#[cfg(test)]
mod user_database_tests {
    use super::*;

    #[test]
    fn missing_user_database_runs_as_root() {
        let dir = tempfile::tempdir().unwrap();
        for spec in [None, Some("root"), Some("0:0"), Some("root:root")] {
            let user = ExecUser::resolve(dir.path(), spec).unwrap();
            assert_eq!((user.uid, user.gid, user.home.as_str()), (0, 0, "/"));
            assert!(user.groups.is_empty());
        }
        assert!(ExecUser::resolve(dir.path(), Some("app")).is_err());
        assert!(ExecUser::resolve(dir.path(), Some("0:app")).is_err());
    }

    #[test]
    fn resolves_from_user_database() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("etc")).unwrap();
        std::fs::write(
            dir.path().join("etc/passwd"),
            "root:x:0:0:root:/admin:/bin/sh\napp:x:1000:1001:app:/home/app:/bin/sh\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("etc/group"), "staff:x:2000:app\n").unwrap();
        let user = ExecUser::resolve(dir.path(), None).unwrap();
        assert_eq!((user.uid, user.gid, user.home.as_str()), (0, 0, "/admin"));
        let user = ExecUser::resolve(dir.path(), Some("app:staff")).unwrap();
        assert_eq!(
            (user.uid, user.gid, user.home.as_str()),
            (1000, 2000, "/home/app")
        );
        assert_eq!(user.groups, vec![nix::unistd::Gid::from_raw(2000)]);
    }
}

pub fn launch(
    _: ContainerCliContext,
    ExecParams {
        force_tty,
        force_stderr_tty,
        pty_size,
        env,
        env_file,
        workdir,
        user,
        chroot,
        command,
    }: ExecParams,
) -> Result<(), Error> {
    use std::io::Write;

    kill_init(Path::new("/proc"), &chroot)?;
    let mut sig = signal_hook::iterator::Signals::new(FWD_SIGNALS)?;
    let (send_pid, recv_pid) = oneshot::channel();
    std::thread::spawn(move || {
        if let Ok(pid) = recv_pid.blocking_recv() {
            for sig in sig.forever() {
                match nix::sys::signal::kill(
                    Pid::from_raw(pid),
                    Some(nix::sys::signal::Signal::try_from(sig).unwrap()),
                ) {
                    Err(Errno::ESRCH) => Ok(()),
                    a => a,
                }
                .unwrap()
            }
        }
    });

    let mut stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let stderr_tty = force_stderr_tty || stderr.is_terminal();

    let tty = force_tty || (stdin.is_terminal() && stdout.is_terminal());

    let raw = if stdin.is_terminal() && stdout.is_terminal() {
        Some(termion::get_tty()?.into_raw_mode()?)
    } else {
        None
    };

    let (stdin_send, stdin_recv) = oneshot::channel::<Box<dyn Write + Send>>();
    std::thread::spawn(move || {
        if let Ok(mut cstdin) = stdin_recv.blocking_recv() {
            if tty {
                let mut buf = [0_u8; CAP_1_KiB];
                while let Ok(n) = stdin.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    cstdin.write_all(&buf[..n]).ok();
                    cstdin.flush().ok();
                }
            } else {
                std::io::copy(&mut stdin, &mut cstdin).unwrap();
            }
        }
    });
    let (stdout_send, stdout_recv) = oneshot::channel::<Box<dyn std::io::Read + Send>>();
    let stdout_thread = std::thread::spawn(move || {
        if let Ok(mut cstdout) = stdout_recv.blocking_recv() {
            if tty {
                let mut stdout = stdout.lock();
                let mut buf = [0_u8; CAP_1_KiB];
                while let Ok(n) = cstdout.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    stdout.write_all(&buf[..n]).ok();
                    stdout.flush().ok();
                }
            } else {
                std::io::copy(&mut cstdout, &mut stdout.lock()).unwrap();
            }
        }
    });
    let (stderr_send, stderr_recv) = oneshot::channel::<Box<dyn std::io::Read + Send>>();
    let stderr_thread = if !stderr_tty {
        Some(std::thread::spawn(move || {
            if let Ok(mut cstderr) = stderr_recv.blocking_recv() {
                std::io::copy(&mut cstderr, &mut stderr.lock()).unwrap();
            }
        }))
    } else {
        None
    };
    nix::sched::unshare(CloneFlags::CLONE_NEWPID)
        .with_ctx(|_| (ErrorKind::Filesystem, "unshare pid ns"))?;
    nix::sched::unshare(CloneFlags::CLONE_NEWCGROUP)
        .with_ctx(|_| (ErrorKind::Filesystem, "unshare cgroup ns"))?;
    nix::sched::unshare(CloneFlags::CLONE_NEWIPC)
        .with_ctx(|_| (ErrorKind::Filesystem, "unshare ipc ns"))?;

    if tty {
        use pty_process::blocking as pty_process;
        let (pty, pts) = pty_process::open().with_kind(ErrorKind::Filesystem)?;
        let mut cmd = pty_process::Command::new("/usr/bin/start-container");
        cmd = cmd.arg("subcontainer").arg("launch-init");
        for env in env {
            cmd = cmd.arg("-e").arg(env)
        }
        if let Some(env_file) = env_file {
            cmd = cmd.arg("--env-file").arg(env_file);
        }
        if let Some(workdir) = workdir {
            cmd = cmd.arg("--workdir").arg(workdir);
        }
        if let Some(user) = user {
            cmd = cmd.arg("--user").arg(user);
        }
        cmd = cmd.arg(&chroot).args(&command);
        if !stderr_tty {
            cmd = cmd.stderr(Stdio::piped());
        }
        let mut child = cmd
            .spawn(pts)
            .map_err(color_eyre::eyre::Report::msg)
            .with_ctx(|_| (ErrorKind::Filesystem, "spawning child process"))?;
        send_pid.send(child.id() as i32).unwrap_or_default();
        if let Some(pty_size) = pty_size.or_else(|| TermSize::get_current()) {
            let size = if let Some((x, y)) = pty_size.pixels {
                ::pty_process::Size::new_with_pixel(pty_size.rows, pty_size.cols, x, y)
            } else {
                ::pty_process::Size::new(pty_size.rows, pty_size.cols)
            };
            pty.resize(size).with_kind(ErrorKind::Filesystem)?;
        }
        let shared = ArcPty(Arc::new(pty));
        stdin_send
            .send(Box::new(shared.clone()))
            .unwrap_or_default();
        stdout_send
            .send(Box::new(shared.clone()))
            .unwrap_or_default();
        if let Some(stderr) = child.stderr.take() {
            stderr_send.send(Box::new(stderr)).unwrap_or_default();
        }
        let exit = child
            .wait()
            .with_ctx(|_| (ErrorKind::Filesystem, "waiting on child process"))?;
        stdout_thread.join().unwrap();
        stderr_thread.map(|t| t.join().unwrap());
        if let Some(code) = exit.code() {
            drop(raw);
            std::process::exit(code);
        } else if exit.success() || exit.signal() == Some(15) {
            Ok(())
        } else {
            Err(Error::new(
                color_eyre::eyre::Report::msg(exit),
                ErrorKind::Unknown,
            ))
        }
    } else {
        let mut cmd = StdCommand::new("/usr/bin/start-container");
        cmd.arg("subcontainer").arg("launch-init");
        for env in env {
            cmd.arg("-e").arg(env);
        }
        if let Some(env_file) = env_file {
            cmd.arg("--env-file").arg(env_file);
        }
        if let Some(workdir) = workdir {
            cmd.arg("--workdir").arg(workdir);
        }
        if let Some(user) = user {
            cmd.arg("--user").arg(user);
        }
        cmd.arg(&chroot);
        cmd.args(&command);
        let mut child = cmd
            .spawn()
            .map_err(color_eyre::eyre::Report::msg)
            .with_ctx(|_| (ErrorKind::Filesystem, "spawning child process"))?;
        send_pid.send(child.id() as i32).unwrap_or_default();
        let exit = child
            .wait()
            .with_ctx(|_| (ErrorKind::Filesystem, "waiting on child process"))?;
        if let Some(code) = exit.code() {
            nix::mount::umount(&chroot.join("proc"))
                .with_ctx(|_| (ErrorKind::Filesystem, "umount procfs"))?;
            std::process::exit(code);
        } else if exit.success() || exit.signal() == Some(15) {
            Ok(())
        } else {
            Err(Error::new(
                color_eyre::eyre::Report::msg(exit),
                ErrorKind::Unknown,
            ))
        }
    }
}

pub fn launch_init(_: ContainerCliContext, params: ExecParams) -> Result<(), Error> {
    nix::mount::mount(
        Some("proc"),
        &params.chroot.join("proc"),
        Some("proc"),
        nix::mount::MsFlags::empty(),
        None::<&str>,
    )
    .with_ctx(|_| (ErrorKind::Filesystem, "mount procfs"))?;
    if params.command.is_empty() {
        let mut signals = signal_hook::iterator::Signals::new(
            signal_hook::consts::TERM_SIGNALS
                .iter()
                .copied()
                .chain([SIGCHLD]),
        )?;
        // the mount above is what lets a caller join the namespace, so a child can
        // already have died — and its SIGCHLD been discarded — before this point
        reap_orphans();
        for signal in signals.forever() {
            if signal != SIGCHLD {
                break;
            }
            reap_orphans();
        }
        std::process::exit(0)
    } else {
        params.exec(None)
    }
}

fn reap_orphans() {
    use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};

    loop {
        match waitpid(None, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) | Err(Errno::ECHILD) => break,
            // nix reports EINVAL for a death by real-time signal, which it cannot
            // classify — the child is reaped by then, so keep draining
            Ok(_) | Err(Errno::EINVAL) => (),
            Err(_) => break,
        }
    }
}

#[derive(Clone)]
struct ArcPty(Arc<pty_process::blocking::Pty>);
impl std::io::Write for ArcPty {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        (&*self.0).write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        (&*self.0).flush()
    }
}
impl std::io::Read for ArcPty {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        (&*self.0).read(buf)
    }
}

pub fn exec(
    _: ContainerCliContext,
    ExecParams {
        force_tty,
        force_stderr_tty,
        pty_size,
        env,
        env_file,
        workdir,
        user,
        chroot,
        command,
    }: ExecParams,
) -> Result<(), Error> {
    use std::io::Write;

    let mut sig = signal_hook::iterator::Signals::new(FWD_SIGNALS)?;
    let (send_pid, recv_pid) = oneshot::channel();
    std::thread::spawn(move || {
        if let Ok(pid) = recv_pid.blocking_recv() {
            for sig in sig.forever() {
                match nix::sys::signal::kill(
                    Pid::from_raw(pid),
                    Some(nix::sys::signal::Signal::try_from(sig).unwrap()),
                ) {
                    Err(Errno::ESRCH) => Ok(()),
                    a => a,
                }
                .unwrap();
            }
        }
    });

    let mut stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let stderr_tty = force_stderr_tty || stderr.is_terminal();

    let tty = force_tty || (stdin.is_terminal() && stdout.is_terminal());

    let raw = if stdin.is_terminal() && stdout.is_terminal() {
        Some(termion::get_tty()?.into_raw_mode()?)
    } else {
        None
    };

    let (stdin_send, stdin_recv) = oneshot::channel::<Box<dyn Write + Send>>();
    std::thread::spawn(move || {
        if let Ok(mut cstdin) = stdin_recv.blocking_recv() {
            if tty {
                let mut buf = [0_u8; CAP_1_KiB];
                while let Ok(n) = stdin.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    cstdin.write_all(&buf[..n]).ok();
                    cstdin.flush().ok();
                }
            } else {
                std::io::copy(&mut stdin, &mut cstdin).unwrap();
            }
        }
    });
    let (stdout_send, stdout_recv) = oneshot::channel::<Box<dyn std::io::Read + Send>>();
    let stdout_thread = std::thread::spawn(move || {
        if let Ok(mut cstdout) = stdout_recv.blocking_recv() {
            if tty {
                let mut stdout = stdout.lock();
                let mut buf = [0_u8; CAP_1_KiB];
                while let Ok(n) = cstdout.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    stdout.write_all(&buf[..n]).ok();
                    stdout.flush().ok();
                }
            } else {
                std::io::copy(&mut cstdout, &mut stdout.lock()).unwrap();
            }
        }
    });
    let (stderr_send, stderr_recv) = oneshot::channel::<Box<dyn std::io::Read + Send>>();
    let stderr_thread = if !stderr_tty {
        Some(std::thread::spawn(move || {
            if let Ok(mut cstderr) = stderr_recv.blocking_recv() {
                std::io::copy(&mut cstderr, &mut stderr.lock()).unwrap();
            }
        }))
    } else {
        None
    };
    nix::sched::setns(
        open_file_read(chroot.join("proc/1/ns/pid"))?,
        CloneFlags::CLONE_NEWPID,
    )
    .with_ctx(|_| (ErrorKind::Filesystem, "set pid ns"))?;
    nix::sched::setns(
        open_file_read(chroot.join("proc/1/ns/cgroup"))?,
        CloneFlags::CLONE_NEWCGROUP,
    )
    .with_ctx(|_| (ErrorKind::Filesystem, "set cgroup ns"))?;
    nix::sched::setns(
        open_file_read(chroot.join("proc/1/ns/ipc"))?,
        CloneFlags::CLONE_NEWIPC,
    )
    .with_ctx(|_| (ErrorKind::Filesystem, "set ipc ns"))?;

    if tty {
        use pty_process::blocking as pty_process;
        let (pty, pts) = pty_process::open().with_kind(ErrorKind::Filesystem)?;
        let mut cmd = pty_process::Command::new("/usr/bin/start-container");
        cmd = cmd.arg("subcontainer").arg("exec-command");
        for env in env {
            cmd = cmd.arg("-e").arg(env);
        }
        if let Some(env_file) = env_file {
            cmd = cmd.arg("--env-file").arg(env_file);
        }
        if let Some(workdir) = workdir {
            cmd = cmd.arg("--workdir").arg(workdir);
        }
        if let Some(user) = user {
            cmd = cmd.arg("--user").arg(user);
        }
        cmd = cmd.arg(&chroot).args(&command);
        if !stderr_tty {
            cmd = cmd.stderr(Stdio::piped());
        }
        let mut child = cmd
            .spawn(pts)
            .map_err(color_eyre::eyre::Report::msg)
            .with_ctx(|_| (ErrorKind::Filesystem, "spawning child process"))?;
        send_pid.send(child.id() as i32).unwrap_or_default();
        if let Some(pty_size) = pty_size.or_else(|| TermSize::get_current()) {
            let size = if let Some((x, y)) = pty_size.pixels {
                ::pty_process::Size::new_with_pixel(pty_size.rows, pty_size.cols, x, y)
            } else {
                ::pty_process::Size::new(pty_size.rows, pty_size.cols)
            };
            pty.resize(size).with_kind(ErrorKind::Filesystem)?;
        }
        let shared = ArcPty(Arc::new(pty));
        stdin_send
            .send(Box::new(shared.clone()))
            .unwrap_or_default();
        stdout_send
            .send(Box::new(shared.clone()))
            .unwrap_or_default();
        if let Some(stderr) = child.stderr.take() {
            stderr_send.send(Box::new(stderr)).unwrap_or_default();
        }
        let exit = child
            .wait()
            .with_ctx(|_| (ErrorKind::Filesystem, "waiting on child process"))?;
        stdout_thread.join().unwrap();
        stderr_thread.map(|t| t.join().unwrap());
        if let Some(code) = exit.code() {
            drop(raw);
            std::process::exit(code);
        } else if exit.success() {
            Ok(())
        } else {
            Err(Error::new(
                color_eyre::eyre::Report::msg(exit),
                ErrorKind::Unknown,
            ))
        }
    } else {
        let mut cmd = StdCommand::new("/usr/bin/start-container");
        cmd.arg("subcontainer").arg("exec-command");
        for env in env {
            cmd.arg("-e").arg(env);
        }
        if let Some(env_file) = env_file {
            cmd.arg("--env-file").arg(env_file);
        }
        if let Some(workdir) = workdir {
            cmd.arg("--workdir").arg(workdir);
        }
        if let Some(user) = user {
            cmd.arg("--user").arg(user);
        }
        cmd.arg(&chroot);
        cmd.args(&command);
        let mut child = cmd
            .spawn()
            .map_err(color_eyre::eyre::Report::msg)
            .with_ctx(|_| (ErrorKind::Filesystem, "spawning child process"))?;
        send_pid.send(child.id() as i32).unwrap_or_default();
        let exit = child
            .wait()
            .with_ctx(|_| (ErrorKind::Filesystem, "waiting on child process"))?;
        if let Some(code) = exit.code() {
            std::process::exit(code);
        } else if exit.success() || exit.signal() == Some(15) {
            Ok(())
        } else {
            Err(Error::new(
                color_eyre::eyre::Report::msg(exit),
                ErrorKind::Unknown,
            ))
        }
    }
}

pub fn exec_command(_: ContainerCliContext, params: ExecParams) -> Result<(), Error> {
    params.exec(Some(SIGKILL))
}

/// Wrap a child process so that its stdout/stderr are always pipes, even when
/// the wrapper's own FDs are sockets (e.g. systemd journal).  This lets
/// descendants `open("/dev/stderr")` via `/proc/self/fd/2` without ENXIO.
pub fn pipe_wrap(
    _: ContainerCliContext,
    PipeWrapParams { command }: PipeWrapParams,
) -> Result<(), Error> {
    use std::os::fd::AsRawFd;

    let Some(([program], args)) = command.split_at_checked(1) else {
        return Err(Error::new(
            eyre!("pipe-wrap: command cannot be empty"),
            ErrorKind::InvalidRequest,
        ));
    };

    let mut cmd = StdCommand::new(program);
    cmd.args(args);
    cmd.env("STARTOS_ENVIRONMENT", crate::version::ENVIRONMENT.trim());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .with_ctx(|_| (ErrorKind::Filesystem, "pipe-wrap: spawning child process"))?;

    let child_stdout = child.stdout.take().unwrap();
    let child_stderr = child.stderr.take().unwrap();

    let orig_stdout_fd = std::io::stdout().as_raw_fd();
    let orig_stderr_fd = std::io::stderr().as_raw_fd();

    // Relay child stdout → original stdout (which may be a socket)
    std::thread::spawn(move || {
        let mut reader = child_stdout;
        let mut buf = [0u8; 8192];
        loop {
            match Read::read(&mut reader, &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let _ = nix::unistd::write(
                        unsafe { std::os::fd::BorrowedFd::borrow_raw(orig_stdout_fd) },
                        &buf[..n],
                    );
                }
            }
        }
    });

    // Relay child stderr → original stderr
    std::thread::spawn(move || {
        let mut reader = child_stderr;
        let mut buf = [0u8; 8192];
        loop {
            match Read::read(&mut reader, &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let _ = nix::unistd::write(
                        unsafe { std::os::fd::BorrowedFd::borrow_raw(orig_stderr_fd) },
                        &buf[..n],
                    );
                }
            }
        }
    });

    // Forward signals to the child
    let child_pid = child.id() as i32;
    let mut sig = signal_hook::iterator::Signals::new(FWD_SIGNALS)?;
    std::thread::spawn(move || {
        for sig in sig.forever() {
            match nix::sys::signal::kill(
                Pid::from_raw(child_pid),
                Some(nix::sys::signal::Signal::try_from(sig).unwrap()),
            ) {
                Err(Errno::ESRCH) => break,
                _ => {}
            }
        }
    });

    let status = child
        .wait()
        .with_ctx(|_| (ErrorKind::Filesystem, "pipe-wrap: waiting on child"))?;
    std::process::exit(status.code().unwrap_or(1))
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
#[group(skip)]
pub struct PipeWrapParams {
    #[arg(trailing_var_arg = true, help = "help.arg.command-to-execute")]
    command: Vec<OsString>,
}
