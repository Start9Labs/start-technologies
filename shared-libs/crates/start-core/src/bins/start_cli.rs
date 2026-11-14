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
fn log_level_requires_one_of_the_named_levels() {
    use crate::util::logger::LogLevel;

    for (value, expected) in [
        ("error", LogLevel::Error),
        ("warn", LogLevel::Warn),
        ("info", LogLevel::Info),
        ("debug", LogLevel::Debug),
        ("trace", LogLevel::Trace),
    ] {
        let matches = app()
            .into_command()
            .try_get_matches_from(["start-cli", "server", "set-log-level", value])
            .unwrap();
        let params = matches
            .subcommand_matches("server")
            .unwrap()
            .subcommand_matches("set-log-level")
            .unwrap();
        assert_eq!(params.get_one::<LogLevel>("level"), Some(&expected));
    }
    assert!(
        app()
            .into_command()
            .try_get_matches_from(["start-cli", "server", "set-log-level"])
            .is_err()
    );
    for value in ["invalid", "off", "5", "true", "TRACE"] {
        assert!(
            app()
                .into_command()
                .try_get_matches_from(["start-cli", "server", "set-log-level", value])
                .is_err()
        );
    }
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
fn backup_history_cli_defaults_and_pagination() {
    for (args, offset, limit) in [
        (vec![], 0, 10),
        (vec!["--offset", "10", "--limit", "5"], 10, 5),
        (vec!["--limit", "0"], 0, 0),
    ] {
        let matches = app()
            .into_command()
            .try_get_matches_from(
                ["start-cli", "backup", "history", "list"]
                    .into_iter()
                    .chain(args),
            )
            .unwrap();
        let params = matches
            .subcommand_matches("backup")
            .unwrap()
            .subcommand_matches("history")
            .unwrap()
            .subcommand_matches("list")
            .unwrap();
        assert_eq!(params.get_one::<usize>("offset"), Some(&offset));
        assert_eq!(params.get_one::<usize>("limit"), Some(&limit));
    }
    assert!(
        app()
            .into_command()
            .try_get_matches_from(["start-cli", "backup", "history", "list", "--limit", "-1"])
            .is_err()
    );
}

#[test]
fn export_manpage_start_cli() {
    super::export_manpages(app().into_command(), "start-cli", super::cli_version());
}

#[test]
fn cli_version_matches_manifest() {
    assert_eq!(super::cli_version(), super::manifest_version("start-cli"));
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
