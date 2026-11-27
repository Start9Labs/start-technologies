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
fn export_manpage_start_container() {
    super::export_manpages(app().into_command(), "start-os", super::startos_version());
}
