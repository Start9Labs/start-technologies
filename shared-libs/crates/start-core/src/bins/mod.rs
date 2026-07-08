use std::collections::{BTreeMap, VecDeque};
use std::ffi::OsString;
use std::path::Path;
use std::sync::OnceLock;

use rust_i18n::t;

pub mod container_cli;
pub mod deprecated;
pub mod registry;
pub mod start_cli;
pub mod start_init;
pub mod startd;
pub mod tunnel;
pub mod unshare_userns;

/// Each product wrapper's `main()` sets this to its own crate `CARGO_PKG_VERSION`; the shared
/// bins report it as `--version`, so each product shows its own version rather than start-core's.
pub static PRODUCT_VERSION: OnceLock<&'static str> = OnceLock::new();

/// The product version set by the wrapper, or start-core's own version as a fallback.
pub fn product_version() -> &'static str {
    PRODUCT_VERSION
        .get()
        .copied()
        .unwrap_or(env!("CARGO_PKG_VERSION"))
}

pub fn set_locale_from_env() {
    let lang = std::env::var("LANG").ok();
    let lang = lang
        .as_deref()
        .map_or("C", |l| l.strip_suffix(".UTF-8").unwrap_or(l));
    set_locale(lang)
}

pub fn set_locale(lang: &str) {
    let mut best = None;
    let prefix = lang.split_inclusive("_").next().unwrap();
    for l in rust_i18n::available_locales!() {
        if l == lang {
            best = Some(l);
            break;
        }
        if best.is_none() && l.starts_with(prefix) {
            best = Some(l);
        }
    }
    rust_i18n::set_locale(best.unwrap_or(lang));
}

pub fn translate_cli(mut cmd: clap::Command) -> clap::Command {
    fn translate(s: impl std::fmt::Display) -> String {
        t!(s.to_string()).into_owned()
    }
    if let Some(s) = cmd.get_about() {
        let s = translate(s);
        cmd = cmd.about(s);
    }
    if let Some(s) = cmd.get_long_about() {
        let s = translate(s);
        cmd = cmd.long_about(s);
    }
    if let Some(s) = cmd.get_before_help() {
        let s = translate(s);
        cmd = cmd.before_help(s);
    }
    if let Some(s) = cmd.get_before_long_help() {
        let s = translate(s);
        cmd = cmd.before_long_help(s);
    }
    if let Some(s) = cmd.get_after_help() {
        let s = translate(s);
        cmd = cmd.after_help(s);
    }
    if let Some(s) = cmd.get_after_long_help() {
        let s = translate(s);
        cmd = cmd.after_long_help(s);
    }

    let arg_ids = cmd
        .get_arguments()
        .map(|a| a.get_id().clone())
        .collect::<Vec<_>>();
    for id in arg_ids {
        cmd = cmd.mut_arg(id, |arg| {
            let arg = if let Some(s) = arg.get_help() {
                let s = translate(s);
                arg.help(s)
            } else {
                arg
            };
            if let Some(s) = arg.get_long_help() {
                let s = translate(s);
                arg.long_help(s)
            } else {
                arg
            }
        });
    }
    for cmd in cmd.get_subcommands_mut() {
        *cmd = translate_cli(cmd.clone());
    }

    cmd
}

#[derive(Default)]
pub struct MultiExecutable {
    default: Option<&'static str>,
    bins: BTreeMap<&'static str, fn(VecDeque<OsString>)>,
}
impl MultiExecutable {
    pub fn enable_startd(&mut self) -> &mut Self {
        self.bins.insert("startd", startd::main);
        self.bins
            .insert("embassyd", |_| deprecated::renamed("embassyd", "startd"));
        self.bins
            .insert("embassy-init", |_| deprecated::removed("embassy-init"));
        self
    }
    pub fn enable_start_cli(&mut self) -> &mut Self {
        self.bins.insert("start-cli", start_cli::main);
        self.bins.insert("embassy-cli", |_| {
            deprecated::renamed("embassy-cli", "start-cli")
        });
        self.bins
            .insert("embassy-sdk", |_| deprecated::removed("embassy-sdk"));
        self
    }
    pub fn enable_start_container(&mut self) -> &mut Self {
        self.bins.insert("start-container", container_cli::main);
        self
    }
    pub fn enable_start_registryd(&mut self) -> &mut Self {
        self.bins.insert("start-registryd", registry::main);
        self
    }
    pub fn enable_start_registry(&mut self) -> &mut Self {
        self.bins.insert("start-registry", registry::cli);
        self
    }
    pub fn enable_start_tunneld(&mut self) -> &mut Self {
        self.bins.insert("start-tunneld", tunnel::main);
        self
    }
    pub fn enable_start_tunnel(&mut self) -> &mut Self {
        self.bins.insert("start-tunnel", tunnel::cli);
        self
    }
    pub fn enable_unshare_userns(&mut self) -> &mut Self {
        self.bins.insert("unshare-userns", unshare_userns::main);
        self
    }

    pub fn set_default(&mut self, name: &str) -> &mut Self {
        if let Some((name, _)) = self.bins.get_key_value(name) {
            self.default = Some(*name);
        } else {
            panic!("{}", t!("bins.mod.does-not-exist", name = name));
        }
        self
    }

    fn select_executable(&self, name: &str) -> Option<fn(VecDeque<OsString>)> {
        self.bins.get(&name).copied()
    }

    pub fn execute(&self) {
        #[cfg(feature = "backtrace-on-stack-overflow")]
        unsafe {
            backtrace_on_stack_overflow::enable()
        };

        // ring = rustls process-default provider; the dual-provider build can't auto-pick one.
        tokio_rustls::rustls::crypto::ring::default_provider()
            .install_default()
            .ok();

        set_locale_from_env();

        let mut popped = Vec::with_capacity(2);
        let mut args = std::env::args_os().collect::<VecDeque<_>>();

        for _ in 0..2 {
            if let Some(s) = args.pop_front() {
                if let Some(name) = Path::new(&*s).file_name().and_then(|s| s.to_str()) {
                    if name == "--contents" {
                        for name in self.bins.keys() {
                            println!("{name}");
                        }
                        return;
                    }
                    if let Some(x) = self.select_executable(&name) {
                        args.push_front(s);
                        return x(args);
                    }
                }
                popped.push(s);
            }
        }
        if let Some(default) = self.default {
            while let Some(arg) = popped.pop() {
                args.push_front(arg);
            }
            return self.bins[default](args);
        }
        let args = std::env::args().collect::<VecDeque<_>>();
        eprintln!(
            "{}",
            t!(
                "bins.mod.unknown-executable",
                name = args
                    .get(1)
                    .or_else(|| args.get(0))
                    .map(|s| s.as_str())
                    .unwrap_or("N/A")
            )
        );
        std::process::exit(1);
    }
}
