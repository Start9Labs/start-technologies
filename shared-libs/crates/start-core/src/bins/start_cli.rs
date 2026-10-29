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
            "--endpoint",
            "8.8.8.8",
        ],
        vec![
            "device",
            "show-config",
            "10.59.0.1/24",
            "10.59.0.2",
            "--endpoint",
            "2606:4700:4700::1111",
        ],
        vec![
            "device",
            "show-config",
            "10.59.0.1/24",
            "10.59.0.2",
            "--endpoint",
            "10.0.0.3",
        ],
        vec![
            "device",
            "show-config",
            "10.59.0.1/24",
            "10.59.0.2",
            "--endpoint",
            "vpn.example.com",
        ],
        vec![
            "device",
            "show-config",
            "10.59.0.1/24",
            "10.59.0.2",
            "--endpoint",
            "[2001:db8::1]",
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
            "--endpoint",
            "https://vpn.example.com",
        ])
        .collect();
    assert!(app().into_command().try_get_matches_from(args).is_err());
}
