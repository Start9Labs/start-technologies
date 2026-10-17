use std::ffi::OsString;

use rpc_toolkit::CliApp;
use serde_json::Value;

use super::completions::CompletionsParams;
use crate::context::CliContext;
use crate::context::config::ClientConfig;
use crate::prelude::Error;
use crate::util::logger::LOGGER;

fn app() -> CliApp<CliContext, ClientConfig> {
    CliApp::new(
        |cfg: ClientConfig| Ok(CliContext::init(cfg.load()?)?),
        crate::main_api(),
    )
    .mutate_command(super::translate_cli)
    .mutate_command(|cmd| cmd.name("start-cli").version(super::cli_version()))
}

pub fn completions(_: CliContext, params: CompletionsParams) -> Result<(), Error> {
    params.generate(app().into_command(), &mut std::io::stdout())
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
