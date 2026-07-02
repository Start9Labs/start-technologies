use std::time::Duration;

use clap::Parser;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::prelude::*;
use crate::ServerContext;

#[derive(Parser, Serialize, Deserialize)]
pub struct ExecReq {
    pub command: String,
    pub args: Vec<String>,
    /// Timeout in milliseconds
    #[clap(long, default_value = "5000")]
    pub timeout: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecRes {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[instrument(skip_all)]
pub async fn exec_command(_ctx: ServerContext, req: ExecReq) -> Result<ExecRes, Error> {
    let output = tokio::time::timeout(
        Duration::from_millis(req.timeout),
        Command::new(&req.command).args(&req.args).output(),
    )
    .await
    .map_err(|_| Error::new(eyre!("command {} timed out after {}ms", req.command, req.timeout), ErrorKind::Timeout))?
    .map_err(|e| Error::new(eyre!("failed to execute {}: {}", req.command, e), ErrorKind::Filesystem))?;

    Ok(ExecRes {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_exec_echo() {
        let result = exec_command(
            ServerContext::default(),
            ExecReq {
                command: "echo".to_string(),
                args: vec!["hello".to_string(), "world".to_string()],
                timeout: 5000,
            },
        )
        .await;

        assert!(result.is_ok());
        let res = result.unwrap();
        assert_eq!(res.stdout.trim(), "hello world");
        assert_eq!(res.stderr, "");
        assert_eq!(res.exit_code, 0);
    }

    #[tokio::test]
    async fn test_exec_nonexistent_command() {
        let result = exec_command(
            ServerContext::default(),
            ExecReq {
                command: "/nonexistent/command".to_string(),
                args: vec![],
                timeout: 5000,
            },
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_exec_with_exit_code() {
        let result = exec_command(
            ServerContext::default(),
            ExecReq {
                command: "sh".to_string(),
                args: vec!["-c".to_string(), "exit 42".to_string()],
                timeout: 5000,
            },
        )
        .await;

        assert!(result.is_ok());
        let res = result.unwrap();
        assert_eq!(res.exit_code, 42);
    }

    #[tokio::test]
    async fn test_exec_stderr() {
        let result = exec_command(
            ServerContext::default(),
            ExecReq {
                command: "sh".to_string(),
                args: vec!["-c".to_string(), "echo error >&2".to_string()],
                timeout: 5000,
            },
        )
        .await;

        assert!(result.is_ok());
        let res = result.unwrap();
        assert_eq!(res.stderr.trim(), "error");
        assert_eq!(res.exit_code, 0);
    }

    #[tokio::test]
    async fn test_exec_timeout() {
        let result = exec_command(
            ServerContext::default(),
            ExecReq {
                command: "sleep".to_string(),
                args: vec!["10".to_string()],
                timeout: 100,
            },
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("timed out"));
    }
}
