use std::io::{IsTerminal, Write};
use std::str::FromStr;

use r3bl_tui::{DefaultIoDevices, ReadlineAsyncContext, ReadlineEvent};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};

use crate::prelude::*;

fn map_miette(m: miette::Error) -> Error {
    Error::new(eyre!("{m}"), ErrorKind::Filesystem)
}
fn noninteractive_err() -> Error {
    Error::new(
        eyre!("{}", t!("util.tui.terminal-must-be-interactive")),
        ErrorKind::Filesystem,
    )
}

/// Reads a secret from an environment variable or a hidden terminal prompt.
pub fn read_password(env: &str, prompt: &str) -> Result<String, Error> {
    if let Ok(password) = std::env::var(env) {
        Ok(password)
    } else {
        Ok(rpassword::prompt_password(prompt)?)
    }
}

pub fn parse_as<'a, T>(what: &'a str) -> impl Fn(&str) -> Result<T, String> + 'a
where
    T: FromStr,
{
    move |s| {
        s.parse::<T>()
            .map_err(|_| t!("util.tui.enter-valid-value", what = what).to_string())
    }
}

pub async fn prompt<T, E: std::fmt::Display, Parse: FnMut(&str) -> Result<T, E>>(
    prompt: &str,
    mut parse: Parse,
    default: Option<T>,
) -> Result<T, Error> {
    let mut rl_ctx = ReadlineAsyncContext::try_new(Some(prompt), None)
        .await
        .map_err(map_miette)?
        .ok_or_else(noninteractive_err)?;
    let res = loop {
        match rl_ctx.read_line().await.map_err(map_miette)? {
            ReadlineEvent::Line(l) => {
                let l = l.trim();
                if !l.is_empty() {
                    match parse(l) {
                        Ok(a) => break a,
                        Err(e) => {
                            writeln!(&mut rl_ctx.shared_writer, "{e}")?;
                        }
                    }
                } else if let Some(default) = default {
                    break default;
                }
            }
            ReadlineEvent::Eof | ReadlineEvent::Interrupted => {
                return Err(Error::new(
                    eyre!("{}", t!("util.tui.aborted")),
                    ErrorKind::Cancelled,
                ));
            }
            _ => (),
        }
    };

    rl_ctx.request_shutdown(None).await.map_err(map_miette)?;
    rl_ctx.await_shutdown().await;

    Ok(res)
}

pub async fn prompt_multiline<
    T,
    E: std::fmt::Display,
    HandleLine: FnMut(String) -> Result<Option<T>, E>,
>(
    prompt: &str,
    mut handle_line: HandleLine,
) -> Result<T, Error> {
    println!("{prompt}");
    let mut rl_ctx = ReadlineAsyncContext::try_new(None::<&str>, None)
        .await
        .map_err(map_miette)?
        .ok_or_else(noninteractive_err)?;
    let res = loop {
        match rl_ctx.read_line().await.map_err(map_miette)? {
            ReadlineEvent::Line(l) => match handle_line(l) {
                Ok(Some(a)) => break a,
                Ok(None) => (),
                Err(e) => writeln!(&mut rl_ctx.shared_writer, "{e}")?,
            },
            ReadlineEvent::Eof | ReadlineEvent::Interrupted => {
                return Err(Error::new(
                    eyre!("{}", t!("util.tui.aborted")),
                    ErrorKind::Cancelled,
                ));
            }
            _ => (),
        }
    };

    rl_ctx.request_shutdown(None).await.map_err(map_miette)?;
    rl_ctx.await_shutdown().await;

    Ok(res)
}

pub async fn choose_custom_display<'t, T>(
    prompt: &str,
    choices: &'t [T],
    mut display: impl FnMut(&T) -> String,
) -> Result<&'t T, Error> {
    let string_choices = choices.into_iter().map(|c| display(c)).collect::<Vec<_>>();

    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        let mut io = DefaultIoDevices::default();
        let style = r3bl_tui::readline_async::StyleSheet::default();
        let choice = r3bl_tui::readline_async::choose(
            prompt,
            string_choices.clone(),
            None,
            None,
            r3bl_tui::HowToChoose::Single,
            style,
            (
                &mut io.output_device,
                &mut io.input_device,
                io.maybe_shared_writer,
            ),
        )
        .await
        .map_err(map_miette)?;
        if choice.len() < 1 {
            return Err(Error::new(
                eyre!("{}", t!("util.tui.aborted")),
                ErrorKind::Cancelled,
            ));
        }
        let (idx, choice_str) = string_choices
            .iter()
            .enumerate()
            .find(|(_, s)| s.as_str() == choice[0].as_str())
            .ok_or_else(|| {
                Error::new(
                    eyre!("{}", t!("util.tui.selected-choice-not-in-input")),
                    ErrorKind::Incoherent,
                )
            })?;
        let choice = &choices[idx];
        println!("{prompt} {choice_str}");
        return Ok(&choice);
    }

    let mut stderr = std::io::stderr().lock();
    let idx = choose_non_tty(
        prompt,
        &string_choices,
        BufReader::new(tokio::io::stdin()),
        &mut stderr,
    )
    .await?;
    Ok(&choices[idx])
}

async fn choose_non_tty<R, W>(
    prompt: &str,
    string_choices: &[String],
    mut input: R,
    mut err: W,
) -> Result<usize, Error>
where
    R: AsyncBufRead + Unpin,
    W: Write,
{
    let listing = string_choices
        .iter()
        .enumerate()
        .map(|(i, s)| format!("  {}) {s}", i + 1))
        .collect::<Vec<_>>()
        .join("\n");

    writeln!(&mut err, "{prompt}")?;
    writeln!(&mut err, "{listing}")?;
    write!(
        &mut err,
        "{} ",
        t!("util.tui.select-by-number", max = string_choices.len())
    )?;
    err.flush()?;

    let mut line = String::new();
    let n = input.read_line(&mut line).await?;
    if n == 0 {
        return Err(Error::new(
            eyre!(
                "{}\n{prompt}\n{listing}",
                t!("util.tui.could-not-read-response"),
            ),
            ErrorKind::InvalidRequest,
        ));
    }
    let trimmed = line.trim();
    let invalid = || {
        Error::new(
            eyre!(
                "{}",
                t!("util.tui.invalid-selection", max = string_choices.len())
            ),
            ErrorKind::InvalidRequest,
        )
    };
    let idx: usize = trimmed.parse().map_err(|_| invalid())?;
    if idx < 1 || idx > string_choices.len() {
        return Err(invalid());
    }
    Ok(idx - 1)
}

pub async fn choose<'t, T: std::fmt::Display>(
    prompt: &str,
    choices: &'t [T],
) -> Result<&'t T, Error> {
    choose_custom_display(prompt, choices, |t| t.to_string()).await
}

#[cfg(test)]
mod test {
    use super::*;

    fn run(input: &[u8]) -> (Result<usize, Error>, String) {
        let choices = vec![
            "alpha".to_string(),
            "bravo".to_string(),
            "charlie".to_string(),
        ];
        let mut err: Vec<u8> = Vec::new();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let res = rt.block_on(choose_non_tty(
            "pick one",
            &choices,
            tokio::io::BufReader::new(input),
            &mut err,
        ));
        (res, String::from_utf8(err).unwrap())
    }

    #[test]
    fn valid_selection_returns_index() {
        let (res, stderr) = run(b"2\n");
        assert_eq!(res.unwrap(), 1);
        assert!(stderr.contains("pick one"));
        assert!(stderr.contains("  1) alpha"));
        assert!(stderr.contains("  2) bravo"));
        assert!(stderr.contains("  3) charlie"));
        assert!(stderr.contains("between 1 and 3"));
    }

    #[test]
    fn whitespace_around_selection_ok() {
        let (res, _) = run(b"  3  \n");
        assert_eq!(res.unwrap(), 2);
    }

    #[test]
    fn out_of_range_errors() {
        let (res, _) = run(b"4\n");
        let e = res.unwrap_err();
        assert_eq!(e.kind, ErrorKind::InvalidRequest);
        assert!(format!("{e}").contains("Invalid selection"));
    }

    #[test]
    fn zero_is_out_of_range() {
        let (res, _) = run(b"0\n");
        assert!(res.is_err());
    }

    #[test]
    fn non_numeric_errors() {
        let (res, _) = run(b"foo\n");
        let e = res.unwrap_err();
        assert_eq!(e.kind, ErrorKind::InvalidRequest);
        assert!(format!("{e}").contains("Invalid selection"));
    }

    #[test]
    fn eof_errors_with_listing() {
        let (res, _) = run(b"");
        let e = res.unwrap_err();
        assert_eq!(e.kind, ErrorKind::InvalidRequest);
        let msg = format!("{e}");
        assert!(msg.contains("Could not read response from stdin"));
        assert!(msg.contains("pick one"));
        assert!(msg.contains("  1) alpha"));
        assert!(msg.contains("  2) bravo"));
        assert!(msg.contains("  3) charlie"));
    }
}
