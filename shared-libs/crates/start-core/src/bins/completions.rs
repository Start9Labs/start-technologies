use clap::builder::PossibleValuesParser;
use clap::{Command, Parser, ValueEnum};
use clap_complete::Shell;
use serde::{Deserialize, Serialize};

use crate::prelude::{Error, ErrorKind, eyre};

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
pub struct CompletionsParams {
    #[arg(help = "help.arg.completions-shell", value_parser = shells())]
    shell: String,
}

fn shells() -> PossibleValuesParser {
    PossibleValuesParser::new(
        Shell::value_variants()
            .iter()
            .filter_map(ValueEnum::to_possible_value),
    )
}

impl CompletionsParams {
    pub fn generate(
        self,
        mut command: Command,
        output: &mut dyn std::io::Write,
    ) -> Result<(), Error> {
        let shell = self
            .shell
            .parse::<Shell>()
            .map_err(|e| Error::new(eyre!("{e}"), ErrorKind::InvalidRequest))?;
        let name = command.get_name().to_owned();
        // clap-rs/clap#6421: hyphenated bin names break nested Bash completion.
        let completion_name = if shell == Shell::Bash {
            name.replace('-', "_")
        } else {
            name.clone()
        };
        clap_complete::generate(shell, &mut command, &completion_name, output);
        if completion_name != name {
            writeln!(
                output,
                "eval \"$(complete -p {completion_name}) {name}\"\ncomplete -r {completion_name}",
            )
            .map_err(|e| Error::new(e, ErrorKind::Filesystem))?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(super) fn assert_bash_completion(command: Command, words: &[&str], expected: &str) {
    use std::io::Write;
    use std::process::{Command as ProcessCommand, Stdio};

    let name = command.get_name().to_owned();
    let params = CompletionsParams::try_parse_from(["completions", "bash"]).unwrap();
    let mut script = Vec::new();
    params.generate(command, &mut script).unwrap();
    writeln!(script, "COMP_WORDS=({})", words.join(" ")).unwrap();
    writeln!(script, "COMP_CWORD={}", words.len() - 1).unwrap();
    writeln!(
        script,
        "[[ $(complete -p {name}) =~ -F[[:space:]]+([^[:space:]]+) ]]"
    )
    .unwrap();
    writeln!(
        script,
        "\"${{BASH_REMATCH[1]}}\" {name} {} {}",
        words[words.len() - 1],
        words[words.len() - 2],
    )
    .unwrap();
    writeln!(script, "printf '%s\\n' \"${{COMPREPLY[@]}}\"").unwrap();

    let mut child = ProcessCommand::new("bash")
        .args(["--noprofile", "--norc", "-e"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&script).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), expected);
}
