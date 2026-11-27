use std::ffi::OsString;

use rpc_toolkit::CliApp;
use serde_json::Value;

use crate::service::cli::{ContainerCliContext, ContainerClientConfig};
use crate::util::logger::LOGGER;

fn app() -> CliApp<ContainerCliContext, ContainerClientConfig> {
    CliApp::new(
        |cfg: ContainerClientConfig| Ok(ContainerCliContext::init(cfg)),
        crate::service::effects::handler(),
    )
    .mutate_command(super::translate_cli)
    // start-container ships only in start-os, so it reports the OS version directly rather
    // than PRODUCT_VERSION — which is unset in the `export_manpage_` test, leaving the
    // generated page on start-core's crate version. Same reasoning as `cli_version`.
    .mutate_command(|cmd| {
        cmd.name("start-container")
            .version(super::startos_version())
    })
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
fn no_shadowed_args_start_container() {
    super::assert_no_shadowed_args(app().into_command());
}

#[test]
fn every_start_container_command_is_described() {
    fn undescribed(cmd: &clap::Command, path: &str, found: &mut Vec<String>) {
        for sub in cmd.get_subcommands() {
            let path = format!("{path} {}", sub.get_name());
            if sub.get_about().is_none() {
                found.push(path.clone());
            }
            undescribed(sub, &path, found);
        }
    }
    let mut found = Vec::new();
    undescribed(&app().into_command(), "start-container", &mut found);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn effects_without_callbacks_or_streams_have_commands() {
    let cmd = app().into_command();
    for path in [
        &["bind"][..],
        &["bind-range"],
        &["set-health"],
        &["clear-health"],
        &["set-init-progress"],
        &["set-backup-progress"],
        &["mount"],
        &["mount-shared-volume"],
        &["get-shared-volumes"],
        &["get-host-info"],
        &["get-service-interface"],
        &["list-service-interfaces"],
        &["export-service-interface"],
        &["export-range-service-interface"],
        &["get-ssl-certificate"],
        &["get-ssl-key"],
        &["get-system-smtp"],
        &["notification", "create"],
        &["plugin", "url", "register"],
        &["plugin", "url", "export-url"],
        &["plugin", "url", "clear-urls"],
        &["action", "export"],
        &["action", "create-task"],
    ] {
        let mut sub = &cmd;
        for name in path {
            sub = sub
                .find_subcommand(name)
                .unwrap_or_else(|| panic!("missing start-container {}", path.join(" ")));
        }
    }
    for hidden in [
        "clear-callbacks",
        "prepare-upload",
        "prepare-download",
        "resolve-upload",
        "get-upload-limits",
    ] {
        assert!(cmd.find_subcommand(hidden).is_none(), "{hidden}");
    }
}

#[test]
fn clear_health_command_parses_flags_and_translates_help() {
    let cmd = app().into_command();
    let sub = cmd
        .find_subcommand("clear-health")
        .expect("missing start-container clear-health");
    assert_ne!(
        sub.get_about().map(ToString::to_string).as_deref(),
        Some("about.container-clear-health")
    );
    for (arg_id, raw_key) in [
        ("only", "help.arg.only-health-checks"),
        ("except", "help.arg.except-health-checks"),
    ] {
        let help = sub
            .get_arguments()
            .find(|a| a.get_id() == arg_id)
            .and_then(|a| a.get_help())
            .map(ToString::to_string);
        assert!(help.is_some(), "missing help for --{arg_id}");
        assert_ne!(help.as_deref(), Some(raw_key));
    }

    let only_matches = cmd
        .clone()
        .try_get_matches_from([
            "start-container",
            "clear-health",
            "--only",
            "worker-a",
            "worker-b",
        ])
        .unwrap();
    let (_, clear_matches) = only_matches.subcommand().unwrap();
    let only: Vec<_> = clear_matches
        .get_many::<crate::HealthCheckId>("only")
        .unwrap()
        .map(ToString::to_string)
        .collect();
    assert_eq!(only, vec!["worker-a", "worker-b"]);

    let except_all_matches = cmd
        .clone()
        .try_get_matches_from(["start-container", "clear-health", "--except"])
        .unwrap();
    let (_, clear_matches) = except_all_matches.subcommand().unwrap();
    let except: Vec<_> = clear_matches
        .get_many::<crate::HealthCheckId>("except")
        .unwrap()
        .map(ToString::to_string)
        .collect();
    assert!(except.is_empty());

    assert!(
        cmd.clone()
            .try_get_matches_from(["start-container", "clear-health"])
            .is_err()
    );
    assert!(
        cmd.clone()
            .try_get_matches_from([
                "start-container",
                "clear-health",
                "--only",
                "worker-a",
                "--except",
                "worker-b",
            ])
            .is_err()
    );
}

#[test]
fn export_manpage_start_container() {
    // start-container is part of the start-os product; anchored to start-core's crate dir.
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../projects/start-os/man"
    );
    std::fs::create_dir_all(dir).unwrap();
    clap_mangen::generate_to(app().into_command(), dir).unwrap();
}
