//! Removes secrets from a support-diagnostics report and replaces the
//! identifiers in it with tokens.
//!
//! A token is a keyed hash of the value it replaces, so one value maps to one
//! token throughout a report and to an unrelated token in the next.

use std::borrow::Cow;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::ops::Range;
use std::path::Path;
use std::sync::LazyLock;

use itertools::Itertools;
use regex::{Captures, Regex, RegexBuilder};

pub const NOTICE: &str = "Redacted: passwords and keys are removed. MAC addresses, public IP \
                          addresses, names and domains are replaced with tokens; a token \
                          stands for the same value throughout this file.\n";

const REDACTED: &str = "[redacted]";

/// UCI options whose values are secrets.
const SECRET_OPTIONS: &[&str] = &[
    "api_key",
    "apikey",
    "auth_secret",
    "key",
    "passwd",
    "password",
    "preshared_key",
    "priv_key",
    "private_key",
    "psk",
    "sae_password",
    "secret",
    "token",
];

/// Shorter configured values would match unrelated text.
const MIN_LITERAL_LEN: usize = 6;

const PUBLIC_RESOLVERS: &[Ipv4Addr] = &[
    Ipv4Addr::new(1, 0, 0, 1),
    Ipv4Addr::new(1, 1, 1, 1),
    Ipv4Addr::new(8, 8, 4, 4),
    Ipv4Addr::new(8, 8, 8, 8),
    Ipv4Addr::new(9, 9, 9, 9),
    Ipv4Addr::new(149, 112, 112, 112),
];

/// Activity text that quotes a setting rather than a name.
const QUOTED_SETTINGS: &[&str] = &["remote access to ", "Timezone "];

fn compile(pattern: &str) -> Regex {
    Regex::new(pattern).expect("static pattern")
}

static JSON_SECRET: LazyLock<Regex> = LazyLock::new(|| {
    compile(&format!(
        r#""({})"(\s*[:,]\s*)"(?:[^"\\]|\\.)*""#,
        SECRET_OPTIONS.join("|")
    ))
});
static JSON_USER: LazyLock<Regex> =
    LazyLock::new(|| compile(r#""user"(\s*,\s*)"((?:[^"\\]|\\.)*)""#));
static UCI_SECRET: LazyLock<Regex> = LazyLock::new(|| {
    compile(&format!(
        r#"\b((?:option|list)\s+['"]?(?:{})['"]?\s+)\S.*"#,
        SECRET_OPTIONS.join("|")
    ))
});
static INVALID_PSK: LazyLock<Regex> = LazyLock::new(|| compile(r"(Invalid PSK ').*(')"));
static WG_BAD_KEY: LazyLock<Regex> =
    LazyLock::new(|| compile(r"(Key is not the correct length or format: `).*(')"));
static LUCI_LOGIN: LazyLock<Regex> =
    LazyLock::new(|| compile(r"(luci: (?:accepted|failed) login on \S* for )(.*)( from)"));
static CONSOLE_LOGIN: LazyLock<Regex> =
    LazyLock::new(|| compile(r"(invalid password for ')(.*)(' on)"));
static URL_USERINFO: LazyLock<Regex> =
    LazyLock::new(|| compile(r#"([A-Za-z][A-Za-z0-9+.-]*://)[^/\s@'"]+@"#));
const URL_SECRET_OPTIONS: &str =
    "password|passwd|pass|pwd|token|key|secret|apikey|api_key|stok|auth";

static URL_SECRET_PARAM: LazyLock<Regex> = LazyLock::new(|| {
    compile(&format!(
        r##"(?i)([?&](?:{URL_SECRET_OPTIONS})=)[^&\s'"#]*"##,
    ))
});
static URL_PATH_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    compile(&format!(
        r##"(?i)(;(?:{URL_SECRET_OPTIONS})=)[^/&\s'"?#;]*"##,
    ))
});

static SSH_FINGERPRINT: LazyLock<Regex> = LazyLock::new(|| compile(r"SHA256:[A-Za-z0-9+/]{43}=?"));
static BASE64_KEY: LazyLock<Regex> =
    LazyLock::new(|| compile(r"[A-Za-z0-9+/]{42}[AEIMQUYcgkosw048]="));
static HEX_ID: LazyLock<Regex> =
    LazyLock::new(|| compile(r"(?:[0-9A-Fa-f]{2}:){6,}[0-9A-Fa-f]{2}"));
static MAC: LazyLock<Regex> = LazyLock::new(|| compile(r"[0-9A-Fa-f]{2}(?:[:-][0-9A-Fa-f]{2}){5}"));
static IPV4: LazyLock<Regex> = LazyLock::new(|| compile(r"(?:\d{1,3}\.){3}\d{1,3}"));
static IPV6: LazyLock<Regex> = LazyLock::new(|| {
    compile(r"[0-9A-Fa-f]{0,4}(?::[0-9A-Fa-f]{0,4}){2,7}(?:%[0-9A-Za-z_.@-]+)?(?:/\d{1,3})?")
});
static EMAIL: LazyLock<Regex> =
    LazyLock::new(|| compile(r"[A-Za-z0-9._%+-]+@((?:[A-Za-z0-9-]+\.)+[A-Za-z]{2,})"));

static DHCP_NAME: LazyLock<Regex> = LazyLock::new(|| compile(r"(DHCPACK\([^)]*\) \S+ \S+ )(\S+)"));
static HOST_NAMES: LazyLock<[Regex; 4]> = LazyLock::new(|| {
    [
        compile(r"(not giving name )(\S+)( to the DHCP lease)"),
        compile(r"(for DHCP host name )(\S+)()"),
        compile(r"(Host name is )(\S+?)(\.local\.)"),
        compile(r"(Host name conflict, retrying with )(\S+)()"),
    ]
});
static DOMAIN_NAMES: LazyLock<[Regex; 5]> = LazyLock::new(|| {
    [
        compile(r"(Get registered/public IP for ')([^']+)(')"),
        compile(r"(dropping SNI route )(\S+)( on)"),
        compile(r"(DNS-inject sweep dropped )(\S+)( )"),
        compile(r"(refused: )(\S+)( is (?:inside the reserved|owned by another))"),
        compile(r"(: `)([^`'\[\]]+?)(:\d+'\. Trying again)"),
    ]
});
static PROFILE_NETWORK: LazyLock<Regex> = LazyLock::new(|| compile(r"\((.+?) profile(, VPN)?\)"));

/// Values read from the router's configuration.
#[derive(Default)]
pub struct Known {
    secrets: Vec<String>,
    users: Vec<String>,
    domains: Vec<String>,
    /// Profile display name to interface.
    profiles: HashMap<String, String>,
    macs: Vec<[u8; 6]>,
}

impl Known {
    /// Reads every config under the directory; unreadable files contribute nothing.
    pub async fn read(config_dir: &Path) -> Self {
        let mut known = Self::default();
        let Ok(mut dir) = tokio::fs::read_dir(config_dir).await else {
            return known;
        };
        while let Ok(Some(entry)) = dir.next_entry().await {
            if let Ok(text) = tokio::fs::read_to_string(entry.path()).await {
                known.add_config(&entry.file_name().to_string_lossy(), &text);
            }
        }
        known
    }

    /// Reads values from a config file's text, including lines uciedit rejects.
    pub fn add_config(&mut self, config: &str, text: &str) {
        let mut ty = String::new();
        let mut profile = (None, None);
        for (n, line) in text.lines().enumerate() {
            let (option, value) = match uciedit::Line::parse(line, n) {
                Ok(uciedit::Line::Section { ty: section, .. }) => {
                    self.add_profile(&ty, std::mem::take(&mut profile));
                    ty = section.unquoted_string();
                    continue;
                }
                Ok(uciedit::Line::Option { option, value, .. }) => {
                    (option.unquoted_string(), value.unquoted_string())
                }
                Ok(uciedit::Line::List { list, item, .. }) => {
                    (list.unquoted_string(), item.unquoted_string())
                }
                Ok(_) => continue,
                Err(_) => match raw_option(line) {
                    Some(pair) => pair,
                    None => continue,
                },
            };
            match (config, ty.as_str(), option.as_str()) {
                ("startwrt", "profile", "fullname") => profile.0 = Some(value),
                ("startwrt", "profile", "interface") => profile.1 = Some(value),
                _ => self.add_value(config, &option, value),
            }
        }
        self.add_profile(&ty, profile);
    }

    pub fn add_macs<'a>(&mut self, macs: impl IntoIterator<Item = &'a str>) {
        self.macs.extend(macs.into_iter().filter_map(parse_mac));
    }

    fn add_profile(&mut self, ty: &str, (name, interface): (Option<String>, Option<String>)) {
        if let ("profile", Some(name), Some(interface)) = (ty, name, interface) {
            self.profiles.insert(name, interface);
        }
    }

    fn add_value(&mut self, config: &str, option: &str, value: String) {
        if value.is_empty() {
            return;
        }
        if SECRET_OPTIONS.contains(&option) {
            if value.len() >= MIN_LITERAL_LEN {
                self.secrets.push(value);
            }
            return;
        }
        match (config, option) {
            ("network" | "ddns", "username") if value.len() >= MIN_LITERAL_LEN => {
                self.users.push(value)
            }
            (_, "email") if value.contains('@') && value.len() >= MIN_LITERAL_LEN => {
                self.users.push(value)
            }
            ("ddns", "domain" | "lookup_host") => match value.split_once('@') {
                Some((host, zone)) => {
                    if !host.is_empty() {
                        self.add_domain(&format!("{host}.{zone}"));
                    }
                    self.add_domain(zone);
                }
                None => self.add_domain(&value),
            },
            ("network", "endpoint_host") => self.add_domain(&value),
            ("startwrt", "endpoint") => {
                let host = value
                    .rsplit_once(':')
                    .map_or(value.as_str(), |(host, _)| host);
                self.add_domain(host);
            }
            (_, "mac" | "macaddr") => self.add_macs(value.split_whitespace()),
            _ => {}
        }
    }

    fn add_domain(&mut self, name: &str) {
        let name = name.trim_end_matches('.').to_ascii_lowercase();
        if name.contains('.') && name.parse::<IpAddr>().is_err() {
            self.domains.push(name);
        }
    }
}

/// The option name and value of an `option` or `list` line uciedit rejects.
fn raw_option(line: &str) -> Option<(String, String)> {
    let line = line.trim_start();
    let rest = line
        .strip_prefix("option")
        .or_else(|| line.strip_prefix("list"))?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let (name, value) = rest.trim_start().split_once(char::is_whitespace)?;
    let value = value.trim();
    let value = value.strip_prefix(['\'', '"']).unwrap_or(value);
    let value = value.strip_suffix(['\'', '"']).unwrap_or(value);
    Some((name.trim_matches(['\'', '"']).to_owned(), value.to_owned()))
}

pub struct Redactor {
    key: [u8; 32],
    secrets: Option<Regex>,
    /// Encoded form to the value it encodes.
    users: Option<(Regex, HashMap<String, String>)>,
    domains: Option<Regex>,
    profiles: HashMap<String, String>,
    /// Known MACs written as 12 bare hex digits.
    mac_hex: Option<(Regex, HashMap<String, [u8; 6]>)>,
}

impl Redactor {
    pub fn new(known: Known, key: [u8; 32]) -> Self {
        let secrets = alternation(known.secrets.iter().flat_map(|s| encodings(s)), false);
        let user_forms: HashMap<String, String> = known
            .users
            .iter()
            .flat_map(|user| {
                encodings(user)
                    .into_iter()
                    .map(move |form| (form, user.clone()))
            })
            .collect();
        let users = alternation(user_forms.keys().cloned(), false).map(|re| (re, user_forms));
        let domains = alternation(known.domains.iter().cloned(), true).and_then(|names| {
            RegexBuilder::new(&format!(r"(?:[a-z0-9-]+\.)*(?:{})", names.as_str()))
                .case_insensitive(true)
                .build()
                .ok()
        });
        let hex: HashMap<String, [u8; 6]> = known
            .macs
            .iter()
            .map(|mac| (mac.iter().map(|b| format!("{b:02x}")).collect(), *mac))
            .collect();
        let mac_hex = alternation(hex.keys().cloned(), true).map(|re| (re, hex));
        Self {
            key,
            secrets,
            users,
            domains,
            profiles: known.profiles,
            mac_hex,
        }
    }

    /// Redacts a log, line by line, across every core.
    pub fn log(&self, log: &[u8]) -> Vec<u8> {
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
        std::thread::scope(|scope| {
            let workers: Vec<_> = line_chunks(log, threads)
                .into_iter()
                .map(|chunk| scope.spawn(move || self.lines(chunk)))
                .collect();
            let mut out = Vec::with_capacity(log.len());
            for worker in workers {
                out.extend(worker.join().expect("redaction thread"));
            }
            out
        })
    }

    fn lines(&self, log: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(log.len());
        for line in log.split_inclusive(|b| *b == b'\n') {
            let (body, newline) = match line.split_last() {
                Some((&b'\n', body)) => (body, true),
                _ => (line, false),
            };
            let body = String::from_utf8_lossy(body);
            let activity = body.contains(crate::activity::SYSLOG_TAG);
            out.extend_from_slice(self.line(&body, activity).as_bytes());
            if newline {
                out.push(b'\n');
            }
        }
        out
    }

    /// Redacts one line; an activity line also has its quoted names replaced.
    pub fn line(&self, line: &str, activity: bool) -> String {
        let mut s = line.to_owned();
        update(&mut s, |s| self.secrets(s));
        if activity {
            update(&mut s, |s| self.activity_names(s));
        }
        if let Some((re, forms)) = &self.users {
            update(&mut s, |s| {
                replace_with(re, s, |s, r| {
                    let word = |c: char| c.is_alphanumeric();
                    (!prev(s, r.start).is_some_and(word) && !next(s, r.end).is_some_and(word))
                        .then(|| self.user(&forms[&s[r]]))
                })
            });
        }
        if let Some(re) = &self.domains {
            update(&mut s, |s| {
                replace_with(re, s, |s, r| {
                    let label = |c: char| c.is_ascii_alphanumeric() || c == '-';
                    (!prev(s, r.start).is_some_and(|c| label(c) || c == '.')
                        && !next(s, r.end).is_some_and(label))
                    .then(|| self.domain(&s[r]))
                })
            });
        }
        update(&mut s, |s| {
            DHCP_NAME.replace_all(s, |c: &Captures| format!("{}{}", &c[1], self.host(&c[2])))
        });
        for re in HOST_NAMES.iter() {
            update(&mut s, |s| {
                re.replace_all(s, |c: &Captures| {
                    format!("{}{}{}", &c[1], self.host(&c[2]), &c[3])
                })
            });
        }
        for re in DOMAIN_NAMES.iter() {
            update(&mut s, |s| {
                re.replace_all(s, |c: &Captures| {
                    let name = if c[2].parse::<IpAddr>().is_ok() || c[2].starts_with('<') {
                        c[2].to_owned()
                    } else {
                        self.domain(&c[2])
                    };
                    format!("{}{name}{}", &c[1], &c[3])
                })
            });
        }
        update(&mut s, |s| {
            SSH_FINGERPRINT.replace_all(s, |c: &Captures| {
                format!("SHA256:{}", self.token("key", &c[0]))
            })
        });
        if s.contains('=') {
            update(&mut s, |s| {
                replace_with(&BASE64_KEY, s, |s, r| {
                    let b64 = |c: char| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=');
                    (!prev(s, r.start).is_some_and(b64) && !next(s, r.end).is_some_and(b64))
                        .then(|| self.token("wgkey", &s[r]))
                })
            });
        }
        if s.contains(':') || s.contains('-') {
            update(&mut s, |s| self.hex_ids(s));
            update(&mut s, |s| self.macs(s));
        }
        if let Some((re, macs)) = &self.mac_hex {
            update(&mut s, |s| {
                replace_with(re, s, |s, r| {
                    let hex = |c: char| c.is_ascii_hexdigit();
                    (!prev(s, r.start).is_some_and(hex) && !next(s, r.end).is_some_and(hex))
                        .then(|| self.mac_tag(macs[&s[r].to_ascii_lowercase()]))
                })
            });
        }
        if s.contains(':') {
            update(&mut s, |s| self.ipv6s(s));
        }
        if s.contains('.') {
            update(&mut s, |s| self.ipv4s(s));
        }
        if s.contains('@') && !s.contains("kernel: ") {
            update(&mut s, |s| {
                EMAIL.replace_all(s, |c: &Captures| {
                    format!(
                        "{}@{}",
                        self.token("user", &c[0].to_ascii_lowercase()),
                        &c[1]
                    )
                })
            });
        }
        s
    }

    /// A MAC's replacement, or `None` for text that is not a MAC.
    pub fn mac(&self, mac: &str) -> Option<String> {
        let mac = parse_mac(mac)?;
        let oui = format!("{:02x}:{:02x}:{:02x}:", mac[0], mac[1], mac[2]);
        Some(self.render_mac(mac, &oui))
    }

    /// A user-chosen name's token.
    pub fn name(&self, name: &str) -> String {
        self.token("name", name)
    }

    fn token(&self, kind: &str, value: &str) -> String {
        let mut hasher = blake3::Hasher::new_keyed(&self.key);
        hasher.update(kind.as_bytes());
        hasher.update(&[0]);
        hasher.update(value.as_bytes());
        format!("<{kind}-{}>", &hasher.finalize().to_hex()[..6])
    }

    fn user(&self, user: &str) -> String {
        let token = self.token("user", user);
        match user.rsplit_once('@') {
            Some((_, realm)) => format!("{token}@{realm}"),
            None => token,
        }
    }

    /// Keeps the top-level domain.
    fn domain(&self, name: &str) -> String {
        let name = name.trim_end_matches('.').to_ascii_lowercase();
        let token = self.token("dom", &name);
        match name.rsplit_once('.') {
            Some((_, tld)) if !tld.is_empty() => format!("{token}.{tld}"),
            _ => token,
        }
    }

    /// Keeps the router's default names.
    fn host(&self, name: &str) -> String {
        let base = name
            .strip_suffix(".lan")
            .or_else(|| name.strip_suffix(".local"))
            .unwrap_or(name);
        let default = base == "localhost"
            || base.strip_prefix("router").is_some_and(|n| {
                n.is_empty()
                    || n.strip_prefix('-')
                        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            });
        if default || name.starts_with('<') {
            name.to_owned()
        } else {
            self.name(name)
        }
    }

    fn secrets<'h>(&self, s: &'h str) -> Cow<'h, str> {
        let mut s = Cow::Borrowed(s);
        if let Some(re) = &self.secrets {
            replace_cow(&mut s, |s| re.replace_all(s, REDACTED));
        }
        replace_cow(&mut s, |s| {
            JSON_SECRET.replace_all(s, |c: &Captures| {
                format!("\"{}\"{}\"{REDACTED}\"", &c[1], &c[2])
            })
        });
        replace_cow(&mut s, |s| {
            JSON_USER.replace_all(s, |c: &Captures| {
                let user = serde_json::from_str::<String>(&format!("\"{}\"", &c[2]))
                    .unwrap_or_else(|_| c[2].to_owned());
                format!("\"user\"{}\"{}\"", &c[1], self.user(&user))
            })
        });
        replace_cow(&mut s, |s| {
            UCI_SECRET.replace_all(s, format!("${{1}}{REDACTED}"))
        });
        replace_cow(&mut s, |s| {
            INVALID_PSK.replace_all(s, format!("${{1}}{REDACTED}${{2}}"))
        });
        replace_cow(&mut s, |s| {
            WG_BAD_KEY.replace_all(s, format!("${{1}}{REDACTED}${{2}}"))
        });
        for re in [&*LUCI_LOGIN, &*CONSOLE_LOGIN] {
            replace_cow(&mut s, |s| {
                re.replace_all(s, |c: &Captures| {
                    let user = if &c[2] == "root" { "root" } else { REDACTED };
                    format!("{}{user}{}", &c[1], &c[3])
                })
            });
        }
        replace_cow(&mut s, |s| {
            URL_USERINFO.replace_all(s, format!("${{1}}{REDACTED}@"))
        });
        for re in [&*URL_SECRET_PARAM, &*URL_PATH_TOKEN] {
            replace_cow(&mut s, |s| re.replace_all(s, format!("${{1}}{REDACTED}")));
        }
        s
    }

    /// Quoted values remain private even when they resemble rendered tokens.
    fn activity_names<'h>(&self, s: &'h str) -> Cow<'h, str> {
        let mut s = Cow::Borrowed(s);
        replace_cow(&mut s, |s| {
            let mut out = String::new();
            let mut last = 0;
            for value in quoted(s) {
                if value.is_empty()
                    || QUOTED_SETTINGS
                        .iter()
                        .any(|context| s[..value.start - 1].ends_with(context))
                {
                    continue;
                }
                out.push_str(&s[last..value.start]);
                out.push_str(&self.activity_name(&s[value.clone()]));
                last = value.end;
            }
            if last == 0 {
                return Cow::Borrowed(s);
            }
            out.push_str(&s[last..]);
            Cow::Owned(out)
        });
        replace_cow(&mut s, |s| {
            PROFILE_NETWORK.replace_all(s, |c: &Captures| {
                format!(
                    "({} profile{})",
                    self.activity_name(&c[1]),
                    c.get(2).map_or("", |m| m.as_str())
                )
            })
        });
        s
    }

    fn activity_name(&self, value: &str) -> String {
        if let Some(interface) = self.profiles.get(value) {
            return format!("<profile-{interface}>");
        }
        if let Some((_, forms)) = &self.users {
            if let Some(user) = forms.get(value) {
                return self.user(user);
            }
        }
        if self.domains.as_ref().is_some_and(|re| {
            re.find(value)
                .is_some_and(|m| m.start() == 0 && m.end() == value.len())
        }) {
            return self.domain(value);
        }
        self.name(value)
    }

    /// Colon-separated hex longer than a MAC: DUIDs, client IDs, MD5 fingerprints.
    fn hex_ids<'h>(&self, s: &'h str) -> Cow<'h, str> {
        replace_with(&HEX_ID, s, |s, r| {
            let hexish = |c: char| c.is_ascii_hexdigit() || c == ':';
            (!prev(s, r.start).is_some_and(hexish) && !next(s, r.end).is_some_and(hexish))
                .then(|| self.token("id", &s[r].to_ascii_lowercase()))
        })
    }

    fn macs<'h>(&self, s: &'h str) -> Cow<'h, str> {
        replace_with(&MAC, s, |s, r| {
            let text = &s[r.clone()];
            let sep = text.as_bytes()[2];
            if text.bytes().skip(2).step_by(3).any(|b| b != sep) {
                return None;
            }
            let joined = |c: char| c.is_ascii_hexdigit() || c == ':' || c == sep as char;
            if prev(s, r.start).is_some_and(joined) || next(s, r.end).is_some_and(joined) {
                return None;
            }
            let mac = parse_mac(text)?;
            (mac[0] & 1 == 0 && mac != [0; 6]).then(|| self.render_mac(mac, &text[..9]))
        })
    }

    /// Keeps a vendor prefix; a locally administered MAC has none.
    fn render_mac(&self, mac: [u8; 6], oui: &str) -> String {
        if mac[0] & 2 == 0 {
            format!("{oui}{}", self.mac_tag(mac))
        } else {
            self.mac_tag(mac)
        }
    }

    fn mac_tag(&self, mac: [u8; 6]) -> String {
        let kind = if mac[0] & 2 == 0 { "mac" } else { "rmac" };
        self.token(kind, &mac.iter().map(|b| format!("{b:02x}")).join(":"))
    }

    fn ipv4s<'h>(&self, s: &'h str) -> Cow<'h, str> {
        replace_with(&IPV4, s, |s, r| {
            if prev(s, r.start).is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_'))
            {
                return None;
            }
            let mut after = s[r.end..].chars();
            match (after.next(), after.next()) {
                (Some(c), _) if c.is_ascii_alphanumeric() || c == '_' => return None,
                (Some('.'), Some(c)) if c.is_ascii_digit() => return None,
                _ => {}
            }
            let ip: Ipv4Addr = s[r].parse().ok()?;
            (!kept_ipv4(ip)).then(|| self.token("ip4", &ip.to_string()))
        })
    }

    fn ipv6s<'h>(&self, s: &'h str) -> Cow<'h, str> {
        replace_with(&IPV6, s, |s, r| {
            let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
            if prev(s, r.start).is_some_and(|c| word(c) || matches!(c, ':' | '.'))
                || next(s, r.end).is_some_and(|c| word(c) || c == ':')
            {
                return None;
            }
            let text = &s[r];
            let core_end = text.find(['%', '/']).unwrap_or(text.len());
            let (core, suffix) = text.split_at(core_end);
            if let Ok(addr) = core.parse::<Ipv6Addr>() {
                return self.render_ipv6(addr).map(|a| format!("{a}{suffix}"));
            }
            let trimmed = core
                .strip_suffix(':')
                .filter(|c| !c.ends_with(':') && suffix.is_empty())?;
            let addr = trimmed.parse::<Ipv6Addr>().ok()?;
            self.render_ipv6(addr).map(|a| format!("{a}:"))
        })
    }

    /// Replaces the routing prefix and any MAC-derived interface ID.
    fn render_ipv6(&self, addr: Ipv6Addr) -> Option<String> {
        let seg = addr.segments();
        if addr.is_unspecified()
            || addr.is_loopback()
            || addr.is_multicast()
            || addr.to_ipv4_mapped().is_some()
            || (seg[0] == 0x2001 && seg[1] == 0x0db8)
        {
            return None;
        }
        let eui = eui64_mac(&seg);
        if seg[0] & 0xffc0 == 0xfe80 {
            return eui.map(|mac| format!("fe80::{}", self.mac_tag(mac)));
        }
        let kind = if seg[0] & 0xfe00 == 0xfc00 {
            "ula"
        } else {
            "gua"
        };
        let prefix = self.token(kind, &format!("{:x}:{:x}:{:x}", seg[0], seg[1], seg[2]));
        Some(match eui {
            Some(mac) => format!("{prefix}:{:x}:{}", seg[3], self.mac_tag(mac)),
            None => format!("{prefix}{}", compressed_tail(&seg[3..])),
        })
    }
}

fn update(s: &mut String, f: impl FnOnce(&str) -> Cow<'_, str>) {
    let replaced = f(s);
    if let Cow::Owned(replaced) = replaced {
        *s = replaced;
    }
}

fn replace_cow<'h>(s: &mut Cow<'h, str>, f: impl FnOnce(&str) -> Cow<'_, str>) {
    let replaced = f(s);
    if let Cow::Owned(replaced) = replaced {
        *s = Cow::Owned(replaced);
    }
}

/// Replaces each match `f` returns a replacement for.
fn replace_with<'h>(
    re: &Regex,
    s: &'h str,
    mut f: impl FnMut(&str, Range<usize>) -> Option<String>,
) -> Cow<'h, str> {
    let mut out = String::new();
    let mut last = 0;
    let mut changed = false;
    for m in re.find_iter(s) {
        if let Some(replacement) = f(s, m.range()) {
            out.push_str(&s[last..m.start()]);
            out.push_str(&replacement);
            last = m.end();
            changed = true;
        }
    }
    if !changed {
        return Cow::Borrowed(s);
    }
    out.push_str(&s[last..]);
    Cow::Owned(out)
}

/// About `count` chunks of similar size, each ending at a line break.
fn line_chunks(log: &[u8], count: usize) -> Vec<&[u8]> {
    let target = log.len() / count.max(1) + 1;
    let mut chunks = Vec::new();
    let mut rest = log;
    while !rest.is_empty() {
        let cut = rest
            .get(target..)
            .and_then(|tail| tail.iter().position(|b| *b == b'\n'))
            .map_or(rest.len(), |newline| target + newline + 1);
        let (chunk, tail) = rest.split_at(cut);
        chunks.push(chunk);
        rest = tail;
    }
    chunks
}

/// Ranges of quoted values, each ending at the last closing quote before the
/// next opening one.
fn quoted(s: &str) -> Vec<Range<usize>> {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    let quotes: Vec<(usize, bool, bool)> = s
        .match_indices('\'')
        .map(|(at, _)| (at, !word(prev(s, at)), !word(next(s, at + 1))))
        .collect();
    let mut values = Vec::new();
    let mut i = 0;
    while let Some(&(open, opens, _)) = quotes.get(i) {
        i += 1;
        if !opens {
            continue;
        }
        let mut close = None;
        for (j, &(at, opens, closes)) in quotes.iter().enumerate().skip(i) {
            if opens && close.is_some() {
                break;
            }
            if closes {
                close = Some((j, at));
            }
        }
        let Some((j, at)) = close else {
            break;
        };
        values.push(open + 1..at);
        i = j + 1;
    }
    values
}

fn prev(s: &str, at: usize) -> Option<char> {
    s[..at].chars().next_back()
}

fn next(s: &str, at: usize) -> Option<char> {
    s[at..].chars().next()
}

/// One pattern matching any of the values, longest first.
fn alternation(values: impl IntoIterator<Item = String>, case_insensitive: bool) -> Option<Regex> {
    let mut values: Vec<String> = values.into_iter().filter(|v| !v.is_empty()).collect();
    values.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    values.dedup();
    if values.is_empty() {
        return None;
    }
    RegexBuilder::new(&values.iter().map(|v| regex::escape(v)).join("|"))
        .case_insensitive(case_insensitive)
        .size_limit(64 << 20)
        .build()
        .ok()
}

/// The forms a value takes in a log: raw, JSON-escaped and percent-encoded.
fn encodings(value: &str) -> Vec<String> {
    let json = serde_json::to_string(value).unwrap_or_default();
    let json = json.trim_matches('"').to_owned();
    let percent = value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect();
    vec![value.to_owned(), json.replace('/', "\\/"), json, percent]
}

fn parse_mac(text: &str) -> Option<[u8; 6]> {
    let hex: Vec<u8> = text.bytes().filter(|b| !matches!(b, b':' | b'-')).collect();
    if hex.len() != 12 || !hex.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    let mut mac = [0; 6];
    for (byte, pair) in mac.iter_mut().zip(hex.chunks(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(mac)
}

fn kept_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || a == 0
        || a >= 240
        || (a == 100 && b & 0xc0 == 64)
        || (a == 198 && b & 0xfe == 18)
        || PUBLIC_RESOLVERS.contains(&ip)
}

/// The MAC a modified EUI-64 interface ID was built from.
fn eui64_mac(seg: &[u16; 8]) -> Option<[u8; 6]> {
    (seg[5] & 0xff == 0xff && seg[6] >> 8 == 0xfe).then(|| {
        [
            (seg[4] >> 8) as u8 ^ 2,
            seg[4] as u8,
            (seg[5] >> 8) as u8,
            seg[6] as u8,
            (seg[7] >> 8) as u8,
            seg[7] as u8,
        ]
    })
}

/// Groups written after a replaced prefix, with the longest zero run compressed.
fn compressed_tail(groups: &[u16]) -> String {
    let (mut best, mut run) = ((0, 0), (0, 0));
    for (i, group) in groups.iter().enumerate() {
        if *group == 0 {
            if run.1 == 0 {
                run.0 = i;
            }
            run.1 += 1;
            if run.1 > best.1 {
                best = run;
            }
        } else {
            run.1 = 0;
        }
    }
    let hex = |groups: &[u16]| groups.iter().map(|g| format!("{g:x}")).join(":");
    if best.1 < 2 {
        return format!(":{}", hex(groups));
    }
    let (left, right) = (&groups[..best.0], &groups[best.0 + best.1..]);
    if left.is_empty() {
        format!("::{}", hex(right))
    } else {
        format!(":{}::{}", hex(left), hex(right))
    }
}

#[cfg(test)]
mod tests;
