//! Running commands in a service's subcontainers.

use std::ffi::OsStr;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use nix::sys::signal::Signal;
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

use super::{RootCommand, SubcontainerInfo, get_passwd_command};
use crate::context::RpcContext;
use crate::lxc::ContainerId;
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::util::env::Environment;
use crate::{CAP_1_KiB, ImageId, PackageId};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
const TERMINATION_GRACE: Duration = Duration::from_secs(5);
const OUTPUT_DRAIN_GRACE: Duration = Duration::from_secs(1);
const OUTPUT_CAP: usize = 64 * CAP_1_KiB;

/// The subcontainer a command runs in, and the identity it runs as.
pub(super) struct SubcontainerTarget {
    container_id: ContainerId,
    subcontainer_id: Guid,
    image_id: ImageId,
    image_env: Option<Environment>,
    user: InternedString,
    workdir: Option<String>,
    root_command: RootCommand,
}
impl SubcontainerTarget {
    /// Fails unless the selectors match exactly one subcontainer.
    pub(super) async fn resolve(
        ctx: &RpcContext,
        id: &PackageId,
        subcontainer: Option<Guid>,
        name: Option<InternedString>,
        image_id: Option<ImageId>,
        user: Option<InternedString>,
    ) -> Result<Self, Error> {
        let service = ctx.services.get(id).await;

        let service_ref = service.as_ref().or_not_found(id)?;

        let container = &service_ref.seed.persistent_container;
        let root_dir = container
            .lxc_container
            .get()
            .map(|x| x.rootfs_dir().to_owned())
            .or_not_found(format!("container for {id}"))?;

        let subcontainer = subcontainer.map(|x| AsRef::<str>::as_ref(&x).to_uppercase());
        let name = name.map(|x| AsRef::<str>::as_ref(&x).to_uppercase());
        let image_id = image_id.map(|x| AsRef::<Path>::as_ref(&x).to_string_lossy().to_uppercase());

        let subcontainers = container.subcontainers.lock().await;
        let subcontainer_ids: Vec<_> = subcontainers
            .iter()
            .filter(|(x, wrapper)| {
                if let Some(subcontainer) = subcontainer.as_ref() {
                    AsRef::<str>::as_ref(x).contains(AsRef::<str>::as_ref(subcontainer))
                } else if let Some(name) = name.as_ref() {
                    AsRef::<str>::as_ref(&wrapper.name)
                        .to_uppercase()
                        .contains(AsRef::<str>::as_ref(name))
                } else if let Some(image_id) = image_id.as_ref() {
                    let Some(wrapper_image_id) = AsRef::<Path>::as_ref(&wrapper.image_id).to_str()
                    else {
                        return false;
                    };
                    wrapper_image_id
                        .to_uppercase()
                        .contains(AsRef::<str>::as_ref(&image_id))
                } else {
                    true
                }
            })
            .collect();
        let Some((subcontainer_id, image_id)) = subcontainer_ids
            .first()
            .map::<(Guid, ImageId), _>(|&x| (x.0.clone(), x.1.image_id.clone()))
        else {
            drop(subcontainers);
            let subcontainers = container
                .subcontainers
                .lock()
                .await
                .iter()
                .map(|(g, s)| SubcontainerInfo {
                    id: g.clone(),
                    name: s.name.clone(),
                    image_id: s.image_id.clone(),
                })
                .collect::<Vec<_>>();
            return Err(Error::new(
                eyre!("{}", t!("service.mod.no-matching-subcontainers", id = id)),
                ErrorKind::NotFound,
            )
            .with_info(to_value(&subcontainers)?));
        };

        let passwd = root_dir
            .join("media/startos/subcontainers")
            .join(subcontainer_id.as_ref())
            .join("etc")
            .join("passwd");

        let image_meta = serde_json::from_str::<Value>(
            &tokio::fs::read_to_string(
                root_dir
                    .join("media/startos/images/")
                    .join(&image_id)
                    .with_extension("json"),
            )
            .await?,
        )
        .with_kind(ErrorKind::Deserialization)?;

        let user = user
            .clone()
            .or_else(|| image_meta["user"].as_str().map(InternedString::intern))
            .unwrap_or_else(|| InternedString::intern("root"));

        let root_command = get_passwd_command(passwd, &*user).await;

        let workdir = image_meta["workdir"].as_str().map(|s| s.to_owned());
        let image_env: Option<Environment> = from_value(image_meta["env"].clone())?;

        if subcontainer_ids.len() > 1 {
            let subcontainers = subcontainer_ids
                .into_iter()
                .map(|(g, s)| SubcontainerInfo {
                    id: g.clone(),
                    name: s.name.clone(),
                    image_id: s.image_id.clone(),
                })
                .collect::<Vec<_>>();
            return Err(Error::new(
                eyre!(
                    "{}",
                    t!("service.mod.multiple-subcontainers-found", id = id,)
                ),
                ErrorKind::InvalidRequest,
            )
            .with_info(to_value(&subcontainers)?));
        }

        Ok(Self {
            container_id: service_ref.container_id()?,
            subcontainer_id,
            image_id,
            image_env,
            user,
            workdir,
            root_command,
        })
    }

    /// Runs the user's login shell for an empty command.
    pub(super) fn command(
        &self,
        exec_options: &[String],
        command: &[impl AsRef<OsStr>],
    ) -> Result<Command, Error> {
        let mut cmd = Command::new("lxc-attach");
        cmd.kill_on_drop(true)
            .arg(&*self.container_id)
            .arg("--")
            .arg("start-container")
            .arg("subcontainer")
            .arg("exec");
        super::attach_environment::configure(&mut cmd, &self.image_id, self.image_env.as_ref())?;
        cmd.arg("--user").arg(&*self.user);
        if let Some(workdir) = &self.workdir {
            cmd.arg("--workdir").arg(workdir);
        }
        cmd.args(exec_options)
            .arg(Path::new("/media/startos/subcontainers").join(self.subcontainer_id.as_ref()))
            .arg("--");
        if command.is_empty() {
            cmd.arg(&self.root_command.0);
        } else {
            cmd.args(command);
        }
        Ok(cmd)
    }
}

/// Shell convention: 128 plus the signal for a killed process.
pub(crate) fn exit_code(status: ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or_default())
}

#[derive(Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct ExecParams {
    pub id: PackageId,
    /// Program and arguments, run without a shell. Empty runs the user's login shell.
    pub command: Vec<String>,
    /// Written to the command's standard input, which is otherwise empty.
    stdin: Option<String>,
    /// Seconds before the command is terminated. Defaults to 120.
    timeout_secs: Option<u64>,
    /// Part of the subcontainer's ID. Overrides `name` and `imageId`.
    subcontainer: Option<Guid>,
    /// Part of the subcontainer's name, case-insensitive. Overrides `imageId`.
    name: Option<InternedString>,
    /// Part of the subcontainer's image ID, case-insensitive.
    image_id: Option<ImageId>,
    /// Defaults to the image's user, else root.
    user: Option<InternedString>,
}
rpc_toolkit::reflect_ts!(ExecParams);

#[derive(Debug, Default, Deserialize, Serialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub timed_out: bool,
    /// Whether either output kept only its final 64 KiB.
    pub truncated: bool,
}
rpc_toolkit::reflect_ts!(ExecResult);

pub async fn exec(
    ctx: RpcContext,
    ExecParams {
        id,
        command,
        stdin,
        timeout_secs,
        subcontainer,
        name,
        image_id,
        user,
    }: ExecParams,
) -> Result<ExecResult, Error> {
    let target = SubcontainerTarget::resolve(&ctx, &id, subcontainer, name, image_id, user).await?;
    run_to_completion(
        target.command(&[], &command)?,
        stdin,
        timeout_secs.map_or(DEFAULT_TIMEOUT, Duration::from_secs),
    )
    .await
}

#[derive(Default)]
struct OutputTail {
    bytes: Vec<u8>,
    truncated: bool,
}
impl OutputTail {
    fn push(&mut self, chunk: &[u8]) {
        self.bytes.extend_from_slice(chunk);
        if let Some(excess) = self.bytes.len().checked_sub(OUTPUT_CAP).filter(|n| *n > 0) {
            self.bytes.drain(..excess);
            self.truncated = true;
        }
    }

    async fn read_from(&mut self, mut reader: impl AsyncRead + Unpin) -> Result<(), Error> {
        let mut chunk = [0; 8192];
        loop {
            let len = reader.read(&mut chunk).await?;
            if len == 0 {
                return Ok(());
            }
            self.push(&chunk[..len]);
        }
    }
}

/// Processes that relay signals to the command.
fn children(supervisor: Pid) -> Vec<Pid> {
    let Ok(tasks) = std::fs::read_dir(format!("/proc/{supervisor}/task")) else {
        return Vec::new();
    };
    tasks
        .flatten()
        .filter_map(|task| std::fs::read_to_string(task.path().join("children")).ok())
        .flat_map(|children| {
            children
                .split_whitespace()
                .filter_map(|pid| pid.parse().ok())
                .map(Pid::from_raw)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn signal_children(supervisor: Pid, signal: Signal) {
    for child in children(supervisor) {
        nix::sys::signal::kill(child, signal).ok();
    }
}

/// Runs a supervisor that does not forward signals, such as `lxc-attach`.
/// At the timeout its children are terminated, then killed with it after a grace period.
async fn run_to_completion(
    mut cmd: Command,
    stdin: Option<String>,
    timeout: Duration,
) -> Result<ExecResult, Error> {
    let mut child = cmd
        .kill_on_drop(true)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let pid = Pid::from_raw(child.id().or_not_found("child pid")? as i32);
    let input = child.stdin.take();
    let stdout_pipe = child.stdout.take().or_not_found("child stdout")?;
    let stderr_pipe = child.stderr.take().or_not_found("child stderr")?;

    let mut stdout = OutputTail::default();
    let mut stderr = OutputTail::default();
    let (status, timed_out) = {
        let feed = async move {
            if let (Some(mut input), Some(stdin)) = (input, stdin) {
                // A command that exits without reading its input closes the pipe.
                input.write_all(stdin.as_bytes()).await.ok();
            }
        };
        let collect = async {
            let (_, out, err) = tokio::join!(
                feed,
                stdout.read_from(stdout_pipe),
                stderr.read_from(stderr_pipe)
            );
            out.and(err)
        };
        let wait = async {
            if let Ok(status) = tokio::time::timeout(timeout, child.wait()).await {
                return Ok::<_, Error>((status?, false));
            }
            signal_children(pid, Signal::SIGTERM);
            if let Ok(status) = tokio::time::timeout(TERMINATION_GRACE, child.wait()).await {
                return Ok((status?, true));
            }
            signal_children(pid, Signal::SIGKILL);
            child.kill().await?;
            Ok((child.wait().await?, true))
        };
        tokio::pin!(collect, wait);
        tokio::select! {
            res = &mut collect => {
                res?;
                wait.await?
            }
            exit = &mut wait => {
                // A background process can hold the pipes open past exit.
                if let Ok(res) = tokio::time::timeout(OUTPUT_DRAIN_GRACE, collect).await {
                    res?;
                }
                exit?
            }
        }
    };

    Ok(ExecResult {
        stdout: String::from_utf8_lossy(&stdout.bytes).into_owned(),
        stderr: String::from_utf8_lossy(&stderr.bytes).into_owned(),
        exit_code: exit_code(status),
        timed_out,
        truncated: stdout.truncated || stderr.truncated,
    })
}

#[cfg(test)]
mod test {
    use super::*;

    fn sh(script: &str) -> Command {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(script);
        cmd
    }

    #[tokio::test]
    async fn returns_output_and_exit_code() {
        let res = run_to_completion(
            sh("cat; echo err >&2; exit 3"),
            Some("in".into()),
            DEFAULT_TIMEOUT,
        )
        .await
        .unwrap();
        assert_eq!(res.stdout, "in");
        assert_eq!(res.stderr, "err\n");
        assert_eq!(res.exit_code, 3);
        assert!(!res.timed_out && !res.truncated);
    }

    #[tokio::test]
    async fn stdin_is_empty_unless_given() {
        let res = run_to_completion(sh("cat; echo done"), None, DEFAULT_TIMEOUT)
            .await
            .unwrap();
        assert_eq!(res.stdout, "done\n");
        assert_eq!(res.exit_code, 0);
    }

    #[tokio::test]
    async fn timeout_terminates_and_keeps_partial_output() {
        let res = run_to_completion(
            sh("echo started; sleep 30"),
            None,
            Duration::from_millis(200),
        )
        .await
        .unwrap();
        assert_eq!(res.stdout, "started\n");
        assert!(res.timed_out);
        assert_eq!(res.exit_code, 128 + Signal::SIGTERM as i32);
    }

    fn is_running(pid: i32) -> bool {
        std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
            stat.rsplit_once(") ")
                .is_some_and(|(_, rest)| !rest.starts_with('Z'))
        })
    }

    #[tokio::test]
    async fn timeout_reaches_a_command_behind_a_non_forwarding_supervisor() {
        let res = run_to_completion(
            sh("sleep 30 & echo $!; wait"),
            None,
            Duration::from_millis(500),
        )
        .await
        .unwrap();
        let command = res.stdout.trim().parse().unwrap();
        let running = is_running(command);
        nix::sys::signal::kill(Pid::from_raw(command), Signal::SIGKILL).ok();
        assert!(res.timed_out);
        assert!(!running, "the command survives the timeout");
    }

    #[tokio::test]
    async fn timeout_kills_a_command_that_ignores_termination() {
        let started = std::time::Instant::now();
        let res = run_to_completion(
            sh("trap '' TERM; sleep 30 & echo $!; wait"),
            None,
            Duration::from_millis(200),
        )
        .await
        .unwrap();
        let command = res.stdout.trim().parse().unwrap();
        let running = is_running(command);
        nix::sys::signal::kill(Pid::from_raw(command), Signal::SIGKILL).ok();
        assert!(res.timed_out);
        assert!(!running, "the command survives the timeout");
        assert!(started.elapsed() < TERMINATION_GRACE + Duration::from_secs(3));
    }

    #[test]
    fn params_accept_the_command_as_strings() {
        let params: ExecParams = from_value(imbl_value::json!({
            "id": "hello-world",
            "command": ["sh", "-c", "echo hi"],
        }))
        .unwrap();
        assert_eq!(params.command, ["sh", "-c", "echo hi"]);
    }

    #[test]
    fn signal_status_uses_the_shell_exit_code() {
        assert_eq!(exit_code(ExitStatus::from_raw(7 << 8)), 7);
        assert_eq!(exit_code(ExitStatus::from_raw(Signal::SIGTERM as i32)), 143);
        assert_eq!(exit_code(ExitStatus::from_raw(Signal::SIGKILL as i32)), 137);
    }

    #[tokio::test]
    async fn exit_returns_while_a_background_process_holds_output() {
        let started = std::time::Instant::now();
        let res = run_to_completion(sh("sleep 30 & echo parent"), None, DEFAULT_TIMEOUT)
            .await
            .unwrap();
        assert_eq!(res.stdout, "parent\n");
        assert!(!res.timed_out);
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[tokio::test]
    async fn output_keeps_its_end() {
        let res = run_to_completion(
            sh("head -c 100000 /dev/zero | tr '\\0' a; echo end"),
            None,
            DEFAULT_TIMEOUT,
        )
        .await
        .unwrap();
        assert!(res.truncated);
        assert_eq!(res.stdout.len(), OUTPUT_CAP);
        assert!(res.stdout.ends_with("aend\n"));
    }
}
