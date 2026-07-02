pub mod cli;
pub mod daemon;

use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::path::Path;

type Handler = fn(VecDeque<OsString>);

pub struct MultiExecutable {
    handlers: HashMap<&'static str, Handler>,
    default: Option<&'static str>,
}

impl Default for MultiExecutable {
    fn default() -> Self {
        Self {
            handlers: HashMap::new(),
            default: None,
        }
    }
}

impl MultiExecutable {
    pub fn enable_daemon(&mut self) -> &mut Self {
        self.handlers.insert("startwrt-ctrld", daemon::main);
        self
    }

    pub fn enable_cli(&mut self) -> &mut Self {
        self.handlers.insert("startwrt-cli", cli::main);
        self
    }

    pub fn set_default(&mut self, name: &'static str) -> &mut Self {
        self.default = Some(name);
        self
    }

    pub fn execute(&self) {
        let mut args: VecDeque<OsString> = std::env::args_os().collect();

        // Try argv[0] — binary name or symlink name
        if let Some(argv0) = args.front() {
            let name = Path::new(argv0)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if let Some(handler) = self.handlers.get(name) {
                args.pop_front();
                return handler(args);
            }
        }

        // Remove argv[0] (the binary name, which didn't match)
        args.pop_front();

        // Try argv[1] — subcommand name
        if let Some(argv1) = args.front() {
            if let Some(name) = argv1.to_str() {
                if let Some(handler) = self.handlers.get(name) {
                    args.pop_front();
                    return handler(args);
                }
            }
        }

        // Fall back to default handler
        if let Some(default_name) = self.default {
            if let Some(handler) = self.handlers.get(default_name) {
                return handler(args);
            }
        }

        eprintln!("unknown command");
        std::process::exit(1);
    }
}
