use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use clap::Parser;
use clap::builder::{StringValueParser, TypedValueParser, ValueParser, ValueParserFactory};
use imbl_value::InternedString;
use reqwest::Url;
use serde::de::{DeserializeOwned, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::MAIN_DATA;
use crate::prelude::*;
use crate::util::serde::IoFormat;
use crate::version::VersionT;

pub const DEVICE_CONFIG_PATH: &str = "/media/startos/config/config.yaml"; // "/media/startos/config/config.yaml";
pub const CONFIG_PATH: &str = "/etc/startos/config.yaml";
pub const CONFIG_PATH_LOCAL: &str = ".startos/config.yaml";

/// The profile a `-H`/`-r` with no explicit name resolves to.
pub const DEFAULT_PROFILE: &str = "default";

/// Named `host`/`registry` targets — the namespace every config file shares
/// (the workspace `.startos/config.yaml`, `~/.startos/config.yaml`,
/// `/etc/startos/config.yaml`, and any `-c` file). A bare value is shorthand for
/// the `default` profile, so a legacy flat `host: https://x` and an explicit
/// `host: { default: https://x }` mean the same thing.
///
/// Values are kept as written and only parsed into a URL at resolve time (like a
/// literal `-H`), so a stale or malformed ambient config never fails a command that
/// doesn't actually target it — it errors only if it's the one selected. A
/// null/empty value is an empty namespace, not an error.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Profiles(pub BTreeMap<String, String>);
impl<'de> Deserialize<'de> for Profiles {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ProfilesVisitor;
        impl<'de> Visitor<'de> for ProfilesVisitor {
            type Value = Profiles;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a URL, or a map of profile name to URL")
            }
            fn visit_unit<E>(self) -> Result<Profiles, E> {
                Ok(Profiles::default())
            }
            fn visit_none<E>(self) -> Result<Profiles, E> {
                Ok(Profiles::default())
            }
            fn visit_str<E>(self, value: &str) -> Result<Profiles, E> {
                Ok(if value.trim().is_empty() {
                    Profiles::default()
                } else {
                    Profiles(BTreeMap::from([(
                        DEFAULT_PROFILE.to_owned(),
                        value.to_owned(),
                    )]))
                })
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Profiles, A::Error> {
                let mut profiles = BTreeMap::new();
                while let Some((name, url)) = map.next_entry::<String, String>()? {
                    profiles.insert(name, url);
                }
                Ok(Profiles(profiles))
            }
        }
        deserializer.deserialize_any(ProfilesVisitor)
    }
}
impl Profiles {
    /// Add every profile from `lower` that `self` doesn't already define, so a
    /// higher-precedence config keeps its own entries and only inherits the rest.
    fn merge_under(&mut self, lower: Profiles) {
        for (name, url) in lower.0 {
            self.0.entry(name).or_insert(url);
        }
    }
}
/// A `-H`/`-r` value is shorthand for the `default` profile — so the flag needs no
/// special handling, it just contributes `{ default: <value> }` at the top of the stack.
impl std::str::FromStr for Profiles {
    type Err = std::convert::Infallible;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(if value.trim().is_empty() {
            Profiles::default()
        } else {
            Profiles(BTreeMap::from([(
                DEFAULT_PROFILE.to_owned(),
                value.to_owned(),
            )]))
        })
    }
}
impl ValueParserFactory for Profiles {
    type Parser = ValueParser;
    fn value_parser() -> Self::Parser {
        ValueParser::new(StringValueParser::new().try_map(|value| value.parse::<Profiles>()))
    }
}

/// Union `from`'s profiles into `into`, keeping `into`'s entry for any shared alias.
fn merge_profiles(into: &mut Option<Profiles>, from: Option<Profiles>) {
    if let Some(from) = from {
        into.get_or_insert_with(Profiles::default).merge_under(from);
    }
}

pub fn local_config_path() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        Some(Path::new(&home).join(CONFIG_PATH_LOCAL))
    } else {
        None
    }
}

pub trait ContextConfig: DeserializeOwned + Default {
    fn next(&mut self) -> Option<PathBuf>;
    fn merge_with(&mut self, other: Self);
    fn from_path(path: impl AsRef<Path>) -> Result<Self, Error> {
        let format: IoFormat = path
            .as_ref()
            .extension()
            .and_then(|s| s.to_str())
            .map(|f| f.parse())
            .transpose()?
            .unwrap_or_default();
        format.from_reader(
            File::open(path.as_ref())
                .with_ctx(|_| (ErrorKind::Filesystem, path.as_ref().display()))?,
        )
    }
    fn load_path_rec(&mut self, path: Option<impl AsRef<Path>>) -> Result<(), Error> {
        if let Some(path) = path.filter(|p| p.as_ref().exists()) {
            let mut other = Self::from_path(path)?;
            let path = other.next();
            self.merge_with(other);
            self.load_path_rec(path)?;
        }
        Ok(())
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "kebab-case")]
#[command(rename_all = "kebab-case")]
#[command(version = crate::version::Current::default().semver().to_string())]
pub struct ClientConfig {
    #[arg(short = 'c', long, help = "help.arg.config-file-path")]
    pub config: Option<PathBuf>,
    /// The `host` namespace — named profiles from every config source, merged in
    /// precedence order (`-H`, a `-c` chain, the workspace, `~/.startos`, `/etc/startos`).
    /// `-H <value>` is shorthand for `{ default: <value> }`, so the flag just seeds the
    /// top of the stack; resolution follows the `default` profile, chasing a value that
    /// names another profile until it reaches a URL.
    #[arg(short = 'H', long, help = "help.arg.host-url")]
    pub host: Option<Profiles>,
    /// The `registry` namespace, merged the same way. See [`Self::host`].
    #[arg(short = 'r', long, help = "help.arg.registry-url")]
    pub registry: Option<Profiles>,
    /// A workspace marker: `.startos/config.yaml` sets `schema: 1`. Read only during
    /// walk-up discovery, to tell a real workspace from a legacy flat config; every
    /// other config source ignores it.
    #[arg(skip)]
    #[serde(default)]
    pub schema: Option<u64>,
    #[arg(long, help = "help.arg.registry-hostname")]
    pub registry_hostname: Option<Vec<InternedString>>,
    #[arg(skip)]
    pub registry_listen: Option<SocketAddr>,
    #[arg(long, help = "help.s9pk-s3base")]
    pub s9pk_s3base: Option<Url>,
    #[arg(long, help = "help.s9pk-s3bucket")]
    pub s9pk_s3bucket: Option<InternedString>,
    #[arg(short = 't', long, help = "help.arg.tunnel-address")]
    pub tunnel: Option<SocketAddr>,
    #[arg(skip)]
    pub tunnel_listen: Option<SocketAddr>,
    #[arg(short = 'p', long, help = "help.arg.proxy-url")]
    pub proxy: Option<Url>,
    #[arg(skip)]
    pub socks_listen: Option<SocketAddr>,
    #[serde(alias = "developer-key-path")]
    #[arg(long, alias = "developer-key-path", help = "help.arg.id-key-path")]
    pub id_key_path: Option<PathBuf>,
    /// PEM-encoded root CA certificate(s) to trust when talking to a
    /// StartOS server with a self-signed cert (e.g. immediately after
    /// `setup complete`, before the device's CA has been imported into
    /// the local trust store). Repeatable.
    #[arg(long = "root-ca", value_name = "PEM_PATH")]
    #[serde(default)]
    pub root_ca: Option<Vec<PathBuf>>,
    /// Skip TLS certificate verification entirely. Intended for
    /// unattended bring-up against a fresh StartOS server whose
    /// self-signed CA hasn't been pinned yet. **Do not use over the
    /// public internet.**
    #[arg(long)]
    #[serde(default)]
    pub insecure: bool,
}
impl ContextConfig for ClientConfig {
    fn next(&mut self) -> Option<PathBuf> {
        self.config.take()
    }
    fn merge_with(&mut self, other: Self) {
        // Profiles merge *beneath* what's already loaded, so a nearer source's profiles
        // win a shared alias while distinct aliases accumulate into the union.
        merge_profiles(&mut self.host, other.host);
        merge_profiles(&mut self.registry, other.registry);
        self.registry_hostname = self.registry_hostname.take().or(other.registry_hostname);
        self.registry_listen = self.registry_listen.take().or(other.registry_listen);
        self.s9pk_s3base = self.s9pk_s3base.take().or(other.s9pk_s3base);
        self.s9pk_s3bucket = self.s9pk_s3bucket.take().or(other.s9pk_s3bucket);
        self.tunnel = self.tunnel.take().or(other.tunnel);
        self.tunnel_listen = self.tunnel_listen.take().or(other.tunnel_listen);
        self.proxy = self.proxy.take().or(other.proxy);
        self.socks_listen = self.socks_listen.take().or(other.socks_listen);
        self.id_key_path = self.id_key_path.take().or(other.id_key_path);
        self.root_ca = self.root_ca.take().or(other.root_ca);
        self.insecure = self.insecure || other.insecure;
    }
}
impl ClientConfig {
    pub fn load(mut self) -> Result<Self, Error> {
        // `-H`/`-r` already sit in `host`/`registry` as `{ default: <value> }` (clap parsed
        // them via `Profiles`), at the top of the stack. Merge config files under them,
        // nearest-first — a `-c` chain, the discovered workspace, `~/.startos`, then
        // `/etc/startos` — each only *adding* profiles a nearer layer didn't already define.
        let path = self.next();
        self.load_path_rec(path)?;
        if let Some(workspace) = find_workspace_config()? {
            // Only its `host`/`registry` profiles — a workspace found by walking up from
            // cwd must not reach into TLS, proxy or signing-key settings just
            // because you `cd`'d into its tree (unlike the fixed-path files below, which
            // you own and which `merge_with` folds in whole).
            merge_profiles(&mut self.host, workspace.host);
            merge_profiles(&mut self.registry, workspace.registry);
        }
        self.load_path_rec(local_config_path())?;
        self.load_path_rec(Some(CONFIG_PATH))?;
        Ok(self)
    }
}

/// Walk up from cwd for the nearest workspace `.startos/config.yaml` — the one that
/// sets `schema`. A legacy flat config (no `schema`) is skipped so the walk continues,
/// and it's still picked up as an ambient config by [`ClientConfig::load`]. EACCES (or
/// any other IO error) on a candidate stops the walk and reports no workspace; an
/// inaccessible ancestor isn't surfaced as an error.
fn find_workspace_config() -> Result<Option<ClientConfig>, Error> {
    let mut dir = std::env::current_dir()?;
    loop {
        let candidate = dir.join(".startos").join("config.yaml");
        match File::open(&candidate) {
            Ok(file) => {
                if let Ok(config) = IoFormat::Yaml.from_reader::<_, ClientConfig>(file) {
                    if config.schema.is_some() {
                        return Ok(Some(config));
                    }
                    // Present but not a workspace (no `schema`) — keep walking up.
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Ok(None),
        }
        if !dir.pop() {
            return Ok(None);
        }
    }
}

/// Resolve a profile namespace to a URL by following the `default` profile: its value
/// is either a URL or the name of another profile, which is chased (guarding against
/// cycles) until a URL is reached. `None` when there's no namespace or no `default` — so
/// an unqualified command with no config falls back to localhost / no registry. The final
/// value is parsed here, so a bad or unknown target errors only when it's the one selected.
pub fn resolve_target(profiles: Option<&Profiles>) -> Result<Option<Url>, Error> {
    let Some(profiles) = profiles else {
        return Ok(None);
    };
    let Some(mut target) = profiles.0.get(DEFAULT_PROFILE) else {
        return Ok(None);
    };
    let mut visited = BTreeSet::from([DEFAULT_PROFILE]);
    while let Some((name, value)) = profiles.0.get_key_value(target.as_str()) {
        if !visited.insert(name.as_str()) {
            return Err(Error::new(
                eyre!("{}", t!("context.cli.profile-cycle", value = name)),
                ErrorKind::InvalidRequest,
            ));
        }
        target = value;
    }
    Url::parse(target).map(Some).map_err(|_| {
        Error::new(
            eyre!(
                "{}",
                t!("context.cli.unknown-profile-or-url", value = target)
            ),
            ErrorKind::InvalidRequest,
        )
    })
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, Parser)]
#[group(skip)]
#[serde(rename_all = "kebab-case")]
#[command(rename_all = "kebab-case")]
pub struct ServerConfig {
    #[arg(short, long, help = "help.arg.config-file-path")]
    pub config: Option<PathBuf>,
    #[arg(long, help = "help.arg.socks-listen-address")]
    pub socks_listen: Option<SocketAddr>,
    #[arg(long, help = "help.arg.revision-cache-size")]
    pub revision_cache_size: Option<usize>,
    #[arg(long, help = "help.arg.disable-encryption")]
    pub disable_encryption: Option<bool>,
    #[arg(long, help = "help.arg.multi-arch-s9pks")]
    pub multi_arch_s9pks: Option<bool>,
    #[serde(alias = "developer-key-path")]
    #[arg(long, alias = "developer-key-path", help = "help.arg.id-key-path")]
    pub id_key_path: Option<PathBuf>,
    #[arg(long, help = "help.arg.max-proxy-conns-per-target")]
    pub max_proxy_conns_per_target: Option<usize>,
}
impl ContextConfig for ServerConfig {
    fn next(&mut self) -> Option<PathBuf> {
        self.config.take()
    }
    fn merge_with(&mut self, other: Self) {
        self.socks_listen = self.socks_listen.take().or(other.socks_listen);
        self.revision_cache_size = self
            .revision_cache_size
            .take()
            .or(other.revision_cache_size);
        self.disable_encryption = self.disable_encryption.take().or(other.disable_encryption);
        self.multi_arch_s9pks = self.multi_arch_s9pks.take().or(other.multi_arch_s9pks);
        self.id_key_path = self.id_key_path.take().or(other.id_key_path);
        self.max_proxy_conns_per_target = self
            .max_proxy_conns_per_target
            .take()
            .or(other.max_proxy_conns_per_target);
    }
}

impl ServerConfig {
    pub fn load(mut self) -> Result<Self, Error> {
        let path = self.next();
        self.load_path_rec(path)?;
        self.load_path_rec(Some(DEVICE_CONFIG_PATH))?;
        self.load_path_rec(Some(CONFIG_PATH))?;
        Ok(self)
    }
    pub async fn db(&self) -> Result<PatchDb, Error> {
        let db_path = Path::new(MAIN_DATA).join("embassy.db");
        let db = PatchDb::open(&db_path)
            .await
            .with_ctx(|_| (crate::ErrorKind::Filesystem, db_path.display().to_string()))?;

        Ok(db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profiles(pairs: &[(&str, &str)]) -> Profiles {
        Profiles(
            pairs
                .iter()
                .map(|(name, url)| (name.to_string(), url.to_string()))
                .collect(),
        )
    }

    /// Parse a config file the way `load_path_rec` does.
    fn parse(yaml: &str) -> ClientConfig {
        IoFormat::Yaml.from_reader(yaml.as_bytes()).unwrap()
    }

    fn resolved(profiles: &Profiles) -> Option<String> {
        resolve_target(Some(profiles))
            .unwrap()
            .map(|url| url.to_string())
    }

    /// A `-H`/`-r` flag value is parsed straight into the `default` profile, so the flag
    /// is a profile map like every other source and needs no special handling.
    #[test]
    fn a_flag_value_is_the_default_profile() {
        assert_eq!(
            "prod".parse::<Profiles>().unwrap(),
            profiles(&[("default", "prod")]),
        );
        assert_eq!(
            "https://x/".parse::<Profiles>().unwrap(),
            profiles(&[("default", "https://x/")]),
        );
        assert_eq!("".parse::<Profiles>().unwrap(), Profiles::default());
    }

    // A config file's `host`/`registry` is one key: a bare URL is the `default` profile,
    // a map is taken verbatim, and null/empty is no namespace.

    #[test]
    fn a_bare_url_is_the_default_profile() {
        assert_eq!(
            parse("host: https://flat/").host,
            Some(profiles(&[("default", "https://flat/")])),
        );
    }

    #[test]
    fn a_map_is_taken_as_named_profiles() {
        assert_eq!(
            parse("host:\n  default: https://ws/\n  remote: https://remote/").host,
            Some(profiles(&[
                ("default", "https://ws/"),
                ("remote", "https://remote/"),
            ])),
        );
    }

    /// A leftover `host:` (null) or `host: ''` is no namespace, not a parse error — it
    /// can't brick unrelated commands, and a sibling key still loads.
    #[test]
    fn a_null_or_empty_host_is_no_namespace() {
        assert!(parse("host:").host.unwrap_or_default().0.is_empty());
        assert!(parse("host: ''").host.unwrap_or_default().0.is_empty());
        assert!(parse("host:\nregistry: https://r/").registry.is_some());
    }

    /// Merging unions namespaces; a shared alias keeps the nearer source's value.
    #[test]
    fn merging_unions_profiles() {
        let mut config = ClientConfig {
            host: Some(profiles(&[("default", "https://near/")])),
            ..Default::default()
        };
        config.merge_with(parse(
            "host:\n  default: https://far/\n  remote: https://far-remote/",
        ));
        assert_eq!(
            config.host,
            Some(profiles(&[
                ("default", "https://near/"),      // shared alias: nearer source wins
                ("remote", "https://far-remote/"), // new alias: added to the union
            ])),
        );
    }

    /// Sources merge nearest-first: `~/.startos` before `/etc/startos`.
    #[test]
    fn a_nearer_source_wins_a_shared_alias() {
        let mut config = ClientConfig::default();
        config.merge_with(parse("host:\n  default: https://home/"));
        config.merge_with(parse("host:\n  default: https://etc/\n  ci: https://ci/"));
        assert_eq!(
            config.host,
            Some(profiles(&[
                ("default", "https://home/"),
                ("ci", "https://ci/")
            ])),
        );
    }

    // Resolution follows the `default` profile, chasing profile references to a URL.

    #[test]
    fn no_namespace_or_default_resolves_to_nothing() {
        assert_eq!(resolve_target(None).unwrap(), None);
        assert_eq!(resolved(&Profiles::default()), None);
        assert_eq!(resolved(&profiles(&[("prod", "https://p/")])), None);
    }

    #[test]
    fn a_default_url_resolves() {
        assert_eq!(
            resolved(&profiles(&[("default", "https://d/")])),
            Some("https://d/".into()),
        );
    }

    /// `default` may name another profile (this is how `-H prod`, and a flat
    /// `host: prod`, target a profile) — resolution follows the reference.
    #[test]
    fn a_default_may_reference_another_profile() {
        let ns = profiles(&[("default", "prod"), ("prod", "https://prod/")]);
        assert_eq!(resolved(&ns), Some("https://prod/".into()));
    }

    #[test]
    fn a_reference_chain_is_followed_to_a_url() {
        let ns = profiles(&[("default", "a"), ("a", "b"), ("b", "https://leaf/")]);
        assert_eq!(resolved(&ns), Some("https://leaf/".into()));
    }

    /// A `default` that is neither a known profile nor a valid URL is an error.
    #[test]
    fn an_unresolvable_default_errors() {
        assert!(resolve_target(Some(&profiles(&[("default", "nope")]))).is_err());
    }

    /// A profile reference cycle errors instead of looping forever.
    #[test]
    fn a_profile_cycle_errors() {
        let ns = profiles(&[("default", "a"), ("a", "b"), ("b", "a")]);
        assert!(resolve_target(Some(&ns)).is_err());
    }

    /// A bare non-URL value loads fine (so a stale ambient config never fails a
    /// command that doesn't target it) and errors only when it's the one resolved.
    #[test]
    fn a_non_url_value_loads_but_errors_only_when_selected() {
        let config = parse("host: box.local");
        assert_eq!(config.host, Some(profiles(&[("default", "box.local")])));
        assert!(resolve_target(config.host.as_ref()).is_err());
    }

    /// `-H`/`-r` (parsed as `{ default: <value> }`) override whatever `default` the files
    /// set, because they sit at the top of the merge stack.
    #[test]
    fn a_flag_overrides_the_files_default() {
        // as `load` sees it: the flag is already in `host`; files merge under it.
        let mut config = ClientConfig {
            host: Some("prod".parse::<Profiles>().unwrap()),
            ..Default::default()
        };
        config.merge_with(parse(
            "host:\n  default: https://file-default/\n  prod: https://prod/",
        ));
        assert_eq!(
            resolved(config.host.as_ref().unwrap()),
            Some("https://prod/".into()),
        );
    }

    /// The stack `load` builds: the `-H` `default` outranks every file's, and aliases from
    /// every tier combine into one namespace resolved by following references. The
    /// walk-up workspace contributes only its `host`/`registry` (via `merge_profiles`).
    #[test]
    fn the_layered_namespace_combines_every_tier() {
        let mut config = ClientConfig {
            host: Some("https://flag/".parse::<Profiles>().unwrap()),
            ..Default::default()
        };
        config.merge_with(parse(
            "host:\n  default: https://cli/\n  shared: https://cli-shared/",
        ));
        merge_profiles(
            &mut config.host,
            parse("host:\n  shared: https://ws-shared/\n  ws-only: https://ws-only/").host,
        );
        config.merge_with(parse("host:\n  home-only: https://home-only/"));

        let ns = config.host.unwrap();
        // -H set `default`; it outranks every file's `default`.
        assert_eq!(resolved(&ns), Some("https://flag/".into()));
        // a shared alias resolves at its highest tier, and every tier's unique aliases survive.
        let follow = |name: &str| {
            let mut ns = ns.clone();
            ns.0.insert("default".into(), name.into());
            resolved(&ns)
        };
        assert_eq!(follow("shared"), Some("https://cli-shared/".into()));
        assert_eq!(follow("ws-only"), Some("https://ws-only/".into()));
        assert_eq!(follow("home-only"), Some("https://home-only/".into()));
    }
}
