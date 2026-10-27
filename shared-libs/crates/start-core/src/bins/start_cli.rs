use std::ffi::OsString;
use std::io::Write;

use rpc_toolkit::{CliApp, output};
use serde_json::Value;

use super::completions::CompletionsParams;
use crate::context::CliContext;
use crate::context::config::ClientConfig;
use crate::prelude::Error;
use crate::util::logger::LOGGER;

#[cfg(test)]
mod tests;

fn app() -> CliApp<CliContext, ClientConfig> {
    CliApp::new(
        |cfg: ClientConfig| Ok(CliContext::init(cfg.load()?)?),
        crate::main_api(),
    )
    .mutate_command(super::translate_cli)
    .mutate_command(|cmd| cmd.name("start-cli").version(super::cli_version()))
}

pub fn completions(_: CliContext, params: CompletionsParams) -> Result<(), Error> {
    params.generate(app().into_command(), &mut output::stdout())
}

pub fn main(args: impl IntoIterator<Item = OsString>) {
    LOGGER.enable();

    if let Err(e) = app().run(args) {
        let mut stderr = output::stderr();
        let _ = match e.data {
            Some(Value::String(s)) => writeln!(stderr, "{}: {}", e.message, s),
            Some(Value::Object(o)) => match o.get("details") {
                Some(Value::String(s)) => {
                    if let Some(Value::String(s)) = o.get("debug") {
                        tracing::debug!("{}", s)
                    }
                    writeln!(stderr, "{}: {}", e.message, s)
                }
                _ => Ok(()),
            },
            Some(a) => writeln!(stderr, "{}: {}", e.message, a),
            None => writeln!(stderr, "{}", e.message),
        };

        std::process::exit(e.code);
    }
}

#[test]
fn bash_completion_start_cli() {
    super::completions::assert_bash_completion(
        app().into_command(),
        &["start-cli", "server", "shut"],
        "shutdown",
    );
    super::completions::assert_bash_completion(
        app().into_command(),
        &["start-cli", "server", "logs", "--lim"],
        "--limit",
    );
    super::completions::assert_bash_completion(
        app().into_command(),
        &["start-cli", "completions", "ba"],
        "bash",
    );
}

#[test]
fn no_shadowed_args_start_cli() {
    super::assert_no_shadowed_args(app().into_command());
}

#[test]
fn diagnostic_update_accepts_trust_without_an_ignored_registry_argument() {
    let matches = app()
        .into_command()
        .try_get_matches_from(["start-cli", "diagnostic", "update", "--trust"])
        .unwrap();
    assert!(
        matches
            .subcommand_matches("diagnostic")
            .unwrap()
            .subcommand_matches("update")
            .unwrap()
            .get_flag("trust")
    );
}

#[test]
fn export_manpage_start_cli() {
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../projects/start-cli/man"
    );
    std::fs::create_dir_all(dir).unwrap();
    clap_mangen::generate_to(app().into_command(), dir).unwrap();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let page = std::fs::read_to_string(&path).unwrap();
        let page = page
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(path, format!("{page}\n")).unwrap();
    }
}
