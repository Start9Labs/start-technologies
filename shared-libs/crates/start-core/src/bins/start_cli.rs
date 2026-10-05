use std::ffi::OsString;

use clap::builder::PossibleValuesParser;
use clap::{Parser, ValueEnum};
use clap_complete::Shell;
use rpc_toolkit::CliApp;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::context::CliContext;
use crate::context::config::ClientConfig;
use crate::prelude::{Error, ErrorKind, eyre};
use crate::util::logger::LOGGER;

fn app() -> CliApp<CliContext, ClientConfig> {
    CliApp::new(
        |cfg: ClientConfig| Ok(CliContext::init(cfg.load()?)?),
        crate::main_api(),
    )
    .mutate_command(super::translate_cli)
    .mutate_command(|cmd| cmd.name("start-cli").version(super::cli_version()))
}

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

pub fn completions(
    _: CliContext,
    CompletionsParams { shell }: CompletionsParams,
) -> Result<(), Error> {
    let shell = shell
        .parse::<Shell>()
        .map_err(|e| Error::new(eyre!("{e}"), ErrorKind::InvalidRequest))?;
    clap_complete::generate(
        shell,
        &mut app().into_command(),
        "start-cli",
        &mut std::io::stdout(),
    );
    Ok(())
}

pub fn main(args: impl IntoIterator<Item = OsString>) {
    LOGGER.enable();

    if let Err(e) = app().run(args) {
        match e.data {
            Some(Value::String(s)) => eprintln!("{}: {}", e.message, s),
            Some(Value::Object(o)) => {
                if let Some(Value::String(s)) = o.get("details") {
                    eprintln!("{}: {}", e.message, s);
                    if let Some(Value::String(s)) = o.get("debug") {
                        tracing::debug!("{}", s)
                    }
                }
            }
            Some(a) => eprintln!("{}: {}", e.message, a),
            None => eprintln!("{}", e.message),
        }

        std::process::exit(e.code);
    }
}

#[test]
fn no_shadowed_args_start_cli() {
    super::assert_no_shadowed_args(app().into_command());
}

#[test]
fn export_manpage_start_cli() {
    // Pages live with the start-cli product; anchored to start-core's crate dir.
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
