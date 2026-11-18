use std::io::Write;

use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};

use crate::prelude::*;
use crate::util::net::tungstenite::{close_result, unexpected_eof};

/// Attach completes after the four-byte exit result, never a Close.
pub(super) fn handle<O: Write, E: Write>(
    message: Message,
    current: &mut Utf8Bytes,
    stdout: &mut Option<O>,
    stderr: &mut Option<E>,
) -> Result<Option<i32>, Error> {
    match message {
        Message::Text(mode) => *current = mode,
        Message::Binary(data) => match &**current {
            "stdout" => {
                if let Some(stdout) = stdout {
                    stdout.write_all(&data)?;
                    stdout.flush()?;
                }
            }
            "stderr" => {
                if let Some(stderr) = stderr {
                    stderr.write_all(&data)?;
                    stderr.flush()?;
                }
            }
            "close-stdout" => {
                stdout.take();
            }
            "close-stderr" => {
                stderr.take();
            }
            "exit" => {
                if data.len() != 4 {
                    return Err(Error::new(
                        eyre!(
                            "{}",
                            t!(
                                "service.mod.invalid-byte-length-for-exit-code",
                                length = data.len()
                            )
                        ),
                        ErrorKind::InvalidRequest,
                    ));
                }
                let mut code = [0u8; 4];
                code.copy_from_slice(&data);
                return Ok(Some(i32::from_be_bytes(code)));
            }
            _ => (),
        },
        Message::Close(frame) => {
            close_result(frame)?;
            return Err(unexpected_eof());
        }
        _ => (),
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use tokio_tungstenite::tungstenite::protocol::CloseFrame;
    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

    use super::*;

    fn run(
        messages: impl IntoIterator<Item = Message>,
    ) -> Result<(Option<i32>, Vec<u8>, Vec<u8>), Error> {
        let mut current = "stdout".into();
        let mut stdout = Some(Vec::new());
        let mut stderr = Some(Vec::new());
        for message in messages {
            if let Some(code) = handle(message, &mut current, &mut stdout, &mut stderr)? {
                return Ok((
                    Some(code),
                    stdout.unwrap_or_default(),
                    stderr.unwrap_or_default(),
                ));
            }
        }
        Ok((None, stdout.unwrap_or_default(), stderr.unwrap_or_default()))
    }

    #[test]
    fn normal_and_empty_close_cannot_substitute_for_exit() {
        for frame in [
            None,
            Some(CloseFrame {
                code: CloseCode::Normal,
                reason: "exit".into(),
            }),
        ] {
            let error = run([Message::Close(frame)]).unwrap_err();
            assert_eq!(error.kind, ErrorKind::Network);
        }
    }

    #[test]
    fn abnormal_close_keeps_its_diagnostic() {
        let error = run([Message::Close(Some(CloseFrame {
            code: CloseCode::Error,
            reason: "worker failed".into(),
        }))])
        .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Network);
        assert!(error.to_string().contains("worker failed"));
    }

    #[test]
    fn explicit_exit_preserves_codes_and_prior_binary_output() {
        for code in [0i32, 7, 137, -1, i32::MAX] {
            let (actual, stdout, stderr) = run([
                Message::Binary(b"out\0\xff".to_vec().into()),
                Message::Text("stderr".into()),
                Message::Binary(b"err\0\xfe".to_vec().into()),
                Message::Text("exit".into()),
                Message::Binary(code.to_be_bytes().to_vec().into()),
            ])
            .unwrap();
            assert_eq!(actual, Some(code));
            assert_eq!(stdout, b"out\0\xff");
            assert_eq!(stderr, b"err\0\xfe");
        }
    }

    #[test]
    fn exit_mode_alone_is_not_a_result_and_bad_lengths_fail() {
        assert_eq!(run([Message::Text("exit".into())]).unwrap().0, None);
        for length in [0, 1, 3, 5, 8] {
            let error = run([
                Message::Text("exit".into()),
                Message::Binary(vec![0; length].into()),
            ])
            .unwrap_err();
            assert_eq!(error.kind, ErrorKind::InvalidRequest);
        }
    }
}
