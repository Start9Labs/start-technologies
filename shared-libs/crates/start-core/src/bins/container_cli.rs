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
    .mutate_command(|cmd| cmd.name("start-container").version(super::product_version()))
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
fn export_manpage_start_container() {
    // start-container is part of the start-os product; anchored to start-core's crate dir.
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../projects/start-os/man");
    std::fs::create_dir_all(dir).unwrap();
    clap_mangen::generate_to(app().into_command(), dir).unwrap();
}
