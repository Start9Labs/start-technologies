use axum::extract::ws::{Message, Utf8Bytes};
use nix::sys::signal::Signal;
use nix::unistd::Pid;
use tokio::io::AsyncWriteExt;
use tokio::process::ChildStdin;

use crate::prelude::*;

pub(super) async fn handle(
    message: Message,
    current_in: &mut Utf8Bytes,
    stdin: &mut Option<ChildStdin>,
    pid: Pid,
) -> Result<(), Error> {
    match message {
        Message::Text(in_ty) => {
            if in_ty == "close-stdin" {
                stdin.take();
            }
            *current_in = in_ty;
        }
        Message::Binary(data) => match &**current_in {
            "stdin" => {
                if let Some(stdin) = stdin {
                    stdin.write_all(&data).await?;
                }
            }
            "signal" => {
                if data.len() != 4 {
                    return Err(Error::new(
                        eyre!(
                            "{}",
                            t!(
                                "service.mod.invalid-byte-length-for-signal",
                                length = data.len()
                            )
                        ),
                        ErrorKind::InvalidRequest,
                    ));
                }
                let mut sig_buf = [0u8; 4];
                sig_buf.clone_from_slice(&data);
                nix::sys::signal::kill(
                    pid,
                    Signal::try_from(i32::from_be_bytes(sig_buf))
                        .with_kind(ErrorKind::InvalidRequest)?,
                )
                .with_kind(ErrorKind::Filesystem)?;
            }
            _ => (),
        },
        _ => (),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::process::Stdio;
    use std::time::Duration;

    use tokio::process::Command;
    use tokio::time::timeout;

    use super::*;

    #[tokio::test]
    async fn text_eof_finishes_a_command_reading_stdin() {
        for chunks in [
            vec![],
            vec![b"x\n".as_slice()],
            vec![b"x\0".as_slice(), b"y\n"],
        ] {
            let mut child = Command::new("sh")
                .args(["-c", "cat; printf done; exit 3"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .unwrap();
            let pid = Pid::from_raw(child.id().unwrap() as i32);
            let mut stdin = child.stdin.take();
            let mut current_in = "stdin".into();
            let mut expected = Vec::new();
            handle(
                Message::Text("stdin".into()),
                &mut current_in,
                &mut stdin,
                pid,
            )
            .await
            .unwrap();
            for chunk in chunks {
                expected.extend_from_slice(chunk);
                handle(
                    Message::Binary(chunk.to_vec().into()),
                    &mut current_in,
                    &mut stdin,
                    pid,
                )
                .await
                .unwrap();
            }
            handle(
                Message::Text("close-stdin".into()),
                &mut current_in,
                &mut stdin,
                pid,
            )
            .await
            .unwrap();
            let output = timeout(Duration::from_secs(5), child.wait_with_output())
                .await
                .expect("command did not receive EOF")
                .unwrap();
            expected.extend_from_slice(b"done");
            assert_eq!(output.stdout, expected);
            assert_eq!(output.status.code(), Some(3));
        }
    }

    #[tokio::test]
    async fn signal_mode_does_not_write_to_stdin() {
        let mut child = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let pid = Pid::from_raw(child.id().unwrap() as i32);
        let mut stdin = child.stdin.take();
        let mut current_in = "stdin".into();
        for message in [
            Message::Text("signal".into()),
            Message::Binary((Signal::SIGCONT as i32).to_be_bytes().to_vec().into()),
            Message::Text("stdin".into()),
            Message::Binary(b"after signal\n".to_vec().into()),
            Message::Text("close-stdin".into()),
        ] {
            handle(message, &mut current_in, &mut stdin, pid)
                .await
                .unwrap();
        }
        let output = timeout(Duration::from_secs(5), child.wait_with_output())
            .await
            .expect("command did not receive EOF")
            .unwrap();
        assert_eq!(output.stdout, b"after signal\n");
        assert!(output.status.success());
    }
}
