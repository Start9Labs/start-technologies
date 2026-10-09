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
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../projects/start-cli/man"
    );
    super::export_manpages(app().into_command(), dir);
}

#[test]
fn tunnel_wan_endpoint_cli_remote() {
    for command in [
        vec!["subnet", "10.59.0.1/24", "set-wan", "--wan-ip", "10.0.0.2"],
        vec![
            "device",
            "set-wan",
            "10.59.0.1/24",
            "10.59.0.2",
            "--wan-ip",
            "10.0.0.2",
        ],
        vec!["device", "show-config", "10.59.0.1/24", "10.59.0.2"],
        vec![
            "device",
            "show-config",
            "10.59.0.1/24",
            "10.59.0.2",
            "--endpoint-ip",
            "8.8.8.8",
        ],
        vec![
            "device",
            "show-config",
            "10.59.0.1/24",
            "10.59.0.2",
            "--endpoint-ip",
            "2606:4700:4700::1111",
        ],
        vec![
            "device",
            "show-config",
            "10.59.0.1/24",
            "10.59.0.2",
            "--endpoint-ip",
            "10.0.0.3",
        ],
    ] {
        let args: Vec<_> = ["start-cli", "--tunnel", "1.1.1.1:443", "tunnel"]
            .into_iter()
            .chain(command)
            .collect();
        app().into_command().try_get_matches_from(args).unwrap();
    }
    let args: Vec<_> = ["start-cli", "--tunnel", "1.1.1.1:443", "tunnel"]
        .into_iter()
        .chain([
            "device",
            "show-config",
            "10.59.0.1/24",
            "10.59.0.2",
            "--endpoint-ip",
            "not-an-ip",
        ])
        .collect();
    assert!(app().into_command().try_get_matches_from(args).is_err());
}
