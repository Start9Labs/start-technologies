use std::ffi::OsString;

use rpc_toolkit::CliApp;
use serde_json::Value;

use crate::context::CliContext;
use crate::context::config::ClientConfig;
use crate::util::logger::LOGGER;

fn app() -> CliApp<CliContext, ClientConfig> {
    CliApp::new(
        |cfg: ClientConfig| Ok(CliContext::init(cfg.load()?)?),
        crate::main_api(),
    )
    .mutate_command(super::translate_cli)
    .mutate_command(|cmd| cmd.name("start-cli").version(super::product_version()))
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
fn export_manpage_start_cli() {
    // Pages live with the start-cli product; anchored to start-core's crate dir.
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../projects/start-cli/man");
    std::fs::create_dir_all(dir).unwrap();
    clap_mangen::generate_to(app().into_command(), dir).unwrap();
}
