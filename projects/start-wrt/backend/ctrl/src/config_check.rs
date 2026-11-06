//! Finds configuration, typically written by hand or in LuCI, that a StartWRT page cannot
//! represent, and names the pages it affects.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use rpc_toolkit::{from_fn_async, ParentHandler};
use serde::{Deserialize, Serialize};
use uciedit::openwrt::{
    Dhcp, FirewallForwarding, FirewallZone, NetworkBridgeVlan, NetworkDevice, NetworkInterface,
    WifiDevice, WifiInterface, WifiMode,
};
use uciedit::{read_all, Arena, Config, Line, Section, Token, TypedSection};

use crate::lan::{LAN_INTERFACE, LAN_NETMASK};
use crate::prelude::*;
use crate::profiles::UciProfile;
use crate::utils::HandlerExtSerde;
use crate::wan::{WAN6_INTERFACE, WAN_INTERFACE};
use crate::CtrlContext;

/// A page of the StartWRT UI, named by its route. Settings, which holds the recovery tools,
/// is never one.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    visit_rs::VisitVariants,
)]
#[serde(rename_all = "kebab-case")]
pub enum ConfigView {
    Wan,
    PublishedPorts,
    Outbound,
    Lan,
    Devices,
    Profiles,
    Ethernet,
    Wifi,
    Inbound,
}
rpc_toolkit::reflect_ts!(ConfigView);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, visit_rs::VisitVariants)]
#[serde(rename_all = "kebab-case")]
pub enum ConfigProblem {
    /// The file does not parse.
    Syntax,
    /// A value that does not parse as the option's type.
    Value,
    /// A required option is absent.
    Missing,
    /// A value that parses but that the page cannot show or keep.
    Unsupported,
    /// A section without a name where the page needs one.
    Unnamed,
    /// A section the page requires is absent.
    Absent,
    /// A section beyond the one the page shows in its place.
    Extra,
    /// A value beyond the one the page manages, left as written.
    Unmanaged,
}
rpc_toolkit::reflect_ts!(ConfigProblem);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFinding {
    /// Pages that cannot represent it.
    pub views: Vec<ConfigView>,
    /// The file's name under `/etc/config`.
    pub config: String,
    /// One-based.
    pub line: Option<u32>,
    pub section: Option<String>,
    pub section_type: Option<String>,
    pub option: Option<String>,
    pub found: Option<String>,
    pub problem: ConfigProblem,
    /// Shown beside the page's settings rather than in their place.
    pub notice: bool,
    /// The parser's explanation, in English.
    pub detail: Option<String>,
}
rpc_toolkit::reflect_ts!(ConfigFinding);

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct ConfigCheck {
    pub findings: Vec<ConfigFinding>,
}
rpc_toolkit::reflect_ts!(ConfigCheck);

pub fn config<C: CtrlContext>() -> ParentHandler<C> {
    ParentHandler::new().subcommand(
        "check",
        from_fn_async(check::<C>).with_display_serializable(),
    )
}

/// The configs the pages read.
const CONFIGS: &[&str] = &[
    "ddns", "dhcp", "firewall", "network", "startwrt", "wireless",
];

#[instrument(skip_all)]
pub async fn check<C: CtrlContext>(ctx: C) -> Result<ConfigCheck, Error> {
    let bytes = read_all(ctx.uci_root(), CONFIGS).await?;
    let arena = Arena::new();
    Ok(ConfigCheck {
        findings: check_configs(bytes.parse_each(&arena)),
    })
}

use ConfigView::*;

const ALL: &[ConfigView] = &[
    Wan,
    PublishedPorts,
    Outbound,
    Lan,
    Devices,
    Profiles,
    Ethernet,
    Wifi,
    Inbound,
];

/// The pages whose requests read the file.
fn file_views(config: &str) -> &'static [ConfigView] {
    match config {
        "network" => &[
            Wan,
            PublishedPorts,
            Outbound,
            Lan,
            Devices,
            Profiles,
            Ethernet,
            Wifi,
            Inbound,
        ],
        "firewall" => &[
            Wan,
            PublishedPorts,
            Lan,
            Devices,
            Profiles,
            Ethernet,
            Wifi,
            Inbound,
        ],
        "dhcp" => &[Wan, PublishedPorts, Lan, Devices, Profiles],
        "wireless" => &[Wifi, Profiles],
        "ddns" => &[Wan, PublishedPorts, Inbound],
        _ => ALL,
    }
}

pub fn check_configs<'a>(
    parsed: Vec<(String, Result<Config<'a>, uciedit::Error>)>,
) -> Vec<ConfigFinding> {
    let mut out = Findings::default();
    let mut cfgs = BTreeMap::new();
    for (name, result) in parsed {
        match result {
            Ok(cfg) => {
                cfgs.insert(name, cfg);
            }
            Err(err) => out.0.push(ConfigFinding {
                views: file_views(&name).to_vec(),
                line: one_based(err.src().line()),
                config: name,
                section: None,
                section_type: None,
                option: None,
                found: None,
                problem: ConfigProblem::Syntax,
                notice: false,
                detail: Some(err.to_string()),
            }),
        }
    }
    let profiles = cfgs
        .get("startwrt")
        .map(profile_interfaces)
        .unwrap_or_default();
    if let Some(cfg) = cfgs.get("network") {
        check_network(cfg, &profiles, &mut out);
    }
    if let Some(cfg) = cfgs.get("wireless") {
        check_wireless(cfg, &mut out);
    }
    if let Some(cfg) = cfgs.get("dhcp") {
        check_dhcp(cfg, &profiles, &mut out);
    }
    if let Some(cfg) = cfgs.get("firewall") {
        check_firewall(cfg, &mut out);
    }
    if let Some(cfg) = cfgs.get("startwrt") {
        check_startwrt(cfg, &mut out);
    }
    out.0
}

#[derive(Default)]
struct Findings(Vec<ConfigFinding>);

impl Findings {
    fn section(
        &mut self,
        config: &str,
        section: &Section,
        views: &[ConfigView],
        problem: ConfigProblem,
    ) -> &mut ConfigFinding {
        self.0.push(ConfigFinding {
            views: views.to_vec(),
            config: config.into(),
            line: one_based(section.header_src().line()),
            section: section.name().map(Cow::into_owned),
            section_type: Some(section.ty().into_owned()),
            option: None,
            found: None,
            problem,
            notice: false,
            detail: None,
        });
        self.0.last_mut().unwrap()
    }

    fn value(
        &mut self,
        config: &str,
        section: &Section,
        views: &[ConfigView],
        problem: ConfigProblem,
        option: &str,
        token: Token,
    ) {
        let finding = self.section(config, section, views, problem);
        finding.option = Some(option.into());
        finding.found = Some(token.unquoted_string());
        if let Some(line) = token.line() {
            finding.line = one_based(Some(line));
        }
    }

    /// List items beyond the value the page reads: its `option`, else the first item.
    fn unmanaged(&mut self, config: &str, section: &Section, views: &[ConfigView], name: &str) {
        let items = list(section, name);
        let extra = match option(section, name) {
            Some(_) => items.first(),
            None => items.get(1),
        };
        if let Some(extra) = extra {
            self.value(
                config,
                section,
                views,
                ConfigProblem::Unmanaged,
                name,
                *extra,
            );
            self.0.last_mut().unwrap().notice = true;
        }
    }

    fn absent(&mut self, config: &str, section: &str, views: &[ConfigView]) {
        self.0.push(ConfigFinding {
            views: views.to_vec(),
            config: config.into(),
            line: None,
            section: Some(section.into()),
            section_type: None,
            option: None,
            found: None,
            problem: ConfigProblem::Absent,
            notice: false,
            detail: None,
        });
    }

    /// Records a failed read.
    fn read<'a, T: TypedSection<'a>>(
        &mut self,
        config: &str,
        section: &Section<'a>,
        views: &[ConfigView],
    ) -> Option<T> {
        let err = match section.get::<T>() {
            Ok(value) => return Some(value),
            Err(err) => err,
        };
        let (problem, option, found, detail) = match &err {
            uciedit::Error::InvalidValue {
                option,
                found,
                desc,
                ..
            } => (
                ConfigProblem::Value,
                Some(option.clone()),
                Some(found.clone()),
                desc.clone(),
            ),
            uciedit::Error::MissingOption { missing, .. } => (
                ConfigProblem::Missing,
                Some(missing.clone()),
                None,
                err.to_string(),
            ),
            other => (ConfigProblem::Value, None, None, other.to_string()),
        };
        let line = err.src().line();
        let finding = self.section(config, section, views, problem);
        finding.option = option;
        finding.found = found;
        finding.detail = Some(detail);
        if line.is_some() {
            finding.line = one_based(line);
        }
        None
    }
}

fn one_based(line: Option<usize>) -> Option<u32> {
    line.and_then(|line| u32::try_from(line + 1).ok())
}

fn option<'a>(section: &Section<'a>, name: &str) -> Option<Token<'a>> {
    section.lines.iter().find_map(|line| match line {
        Line::Option { option, value, .. } if option.as_str() == name => Some(*value),
        _ => None,
    })
}

fn list<'a>(section: &Section<'a>, name: &str) -> Vec<Token<'a>> {
    section
        .lines
        .iter()
        .filter_map(|line| match line {
            Line::List { list, item, .. } if list.as_str() == name => Some(*item),
            _ => None,
        })
        .collect()
}

fn named<'c, 'a>(cfg: &'c Config<'a>, ty: &str, name: &str) -> Option<&'c Section<'a>> {
    cfg.sections
        .iter()
        .find(|s| s.ty() == ty && s.name().as_deref() == Some(name))
}

/// Read without the typed reader, which a broken profile would fail.
fn profile_interfaces(cfg: &Config) -> BTreeSet<String> {
    cfg.sections
        .iter()
        .filter(|s| s.ty() == UciProfile::TY)
        .filter_map(|s| option(s, "interface"))
        .map(Token::unquoted_string)
        .collect()
}

fn check_network(cfg: &Config, profiles: &BTreeSet<String>, out: &mut Findings) {
    const C: &str = "network";
    for section in &cfg.sections {
        let ty = section.ty();
        let name = section.name();
        if ty == NetworkInterface::TY {
            let mut views = vec![Ethernet, Profiles];
            match name.as_deref() {
                Some(WAN_INTERFACE | WAN6_INTERFACE) => views.push(Wan),
                Some(LAN_INTERFACE) => views.push(Lan),
                Some(iface) if profiles.contains(iface) => views.push(Lan),
                _ => {}
            }
            out.read::<NetworkInterface>(C, section, &views);
        } else if ty == NetworkDevice::TY {
            out.read::<NetworkDevice>(C, section, &[Ethernet, Profiles]);
        } else if ty == NetworkBridgeVlan::TY {
            out.read::<NetworkBridgeVlan>(C, section, &[Ethernet, Profiles]);
        }
    }

    let bridged = cfg.sections.iter().any(|s| {
        s.ty() == NetworkDevice::TY && option(s, "type").is_some_and(|t| t.as_str() == "bridge")
    });
    if !bridged {
        out.absent(C, "br-lan", &[Ethernet, Profiles]);
    }

    match named(cfg, NetworkInterface::TY, WAN_INTERFACE) {
        None => out.absent(C, WAN_INTERFACE, &[Wan]),
        Some(wan) => {
            supported_proto(C, wan, &["dhcp", "static", "pppoe"], &[Wan], out);
            out.unmanaged(C, wan, &[Wan], "ipaddr");
        }
    }
    if let Some(wan6) = named(cfg, NetworkInterface::TY, WAN6_INTERFACE) {
        supported_proto(C, wan6, &["none", "dhcpv6", "static", "6rd"], &[Wan], out);
        out.unmanaged(C, wan6, &[Wan], "ip6addr");
    }

    match named(cfg, NetworkInterface::TY, LAN_INTERFACE) {
        None => out.absent(C, LAN_INTERFACE, &[Lan, Profiles]),
        Some(lan) => static_address(C, lan, &[Lan, Profiles], out),
    }
    for iface in profiles.iter().filter(|i| *i != LAN_INTERFACE) {
        match named(cfg, NetworkInterface::TY, iface) {
            None => out.absent(C, iface, &[Profiles]),
            Some(section) => static_address(C, section, &[Profiles, Lan], out),
        }
    }
}

fn supported_proto(
    config: &str,
    section: &Section,
    supported: &[&str],
    views: &[ConfigView],
    out: &mut Findings,
) {
    if let Some(proto) = option(section, "proto") {
        if !supported.contains(&&*proto.as_str()) {
            out.value(
                config,
                section,
                views,
                ConfigProblem::Unsupported,
                "proto",
                proto,
            );
        }
    }
}

/// A profile's interface must have a static IPv4 address.
fn static_address(config: &str, section: &Section, views: &[ConfigView], out: &mut Findings) {
    match option(section, "proto") {
        Some(proto) if proto.as_str() != "static" => out.value(
            config,
            section,
            views,
            ConfigProblem::Unsupported,
            "proto",
            proto,
        ),
        _ => {}
    }
    out.unmanaged(config, section, views, "ipaddr");
    if option(section, "ipaddr").is_none() && list(section, "ipaddr").is_empty() {
        out.section(config, section, views, ConfigProblem::Missing)
            .option = Some("ipaddr".into());
    }
    if let Ok(iface) = section.get::<NetworkInterface>() {
        if iface.ipv4_netmask().is_some_and(|mask| mask != LAN_NETMASK) {
            let address =
                option(section, "ipaddr").or_else(|| list(section, "ipaddr").first().copied());
            let (name, token) = if iface.ipaddr.is_some_and(|address| address.prefix.is_some()) {
                ("ipaddr", address)
            } else {
                ("netmask", option(section, "netmask"))
            };
            if let Some(token) = token {
                out.value(
                    config,
                    section,
                    views,
                    ConfigProblem::Unsupported,
                    name,
                    token,
                );
            }
        }
    }
}

fn check_wireless(cfg: &Config, out: &mut Findings) {
    const C: &str = "wireless";
    const VIEWS: &[ConfigView] = &[Wifi, Profiles];
    let mut devices = BTreeSet::new();
    for section in cfg.sections.iter().filter(|s| s.ty() == WifiDevice::TY) {
        if section.name().is_none() {
            out.section(C, section, VIEWS, ConfigProblem::Unnamed);
        } else if out.read::<WifiDevice>(C, section, VIEWS).is_some() {
            devices.extend(section.name().map(Cow::into_owned));
        }
    }
    let mut access_points = BTreeSet::new();
    for section in cfg.sections.iter().filter(|s| s.ty() == WifiInterface::TY) {
        if section.name().is_none() {
            out.section(C, section, VIEWS, ConfigProblem::Unnamed);
            continue;
        }
        let Some(iface) = out.read::<WifiInterface>(C, section, VIEWS) else {
            continue;
        };
        if !devices.contains(&iface.device) {
            continue;
        }
        if let Some(disabled) = option(section, "disabled") {
            if disabled.parse_bool("disabled").unwrap_or(false) {
                out.value(
                    C,
                    section,
                    &[Wifi],
                    ConfigProblem::Unsupported,
                    "disabled",
                    disabled,
                );
                continue;
            }
        }
        if let Some(network) = option(section, "network") {
            if network.as_str() != LAN_INTERFACE {
                out.value(
                    C,
                    section,
                    &[Wifi],
                    ConfigProblem::Unsupported,
                    "network",
                    network,
                );
                continue;
            }
        }
        if iface.mode != WifiMode::AP {
            if let Some(mode) = option(section, "mode") {
                out.value(
                    C,
                    section,
                    &[Wifi],
                    ConfigProblem::Unsupported,
                    "mode",
                    mode,
                );
            }
            continue;
        }
        if !access_points.insert(iface.device.clone()) {
            out.section(C, section, &[Wifi], ConfigProblem::Extra);
            continue;
        }
        if let Some(encryption) = option(section, "encryption") {
            if !matches!(&*encryption.as_str(), "psk2" | "none") {
                out.value(
                    C,
                    section,
                    &[Wifi],
                    ConfigProblem::Unsupported,
                    "encryption",
                    encryption,
                );
            }
        }
    }
}

fn check_dhcp(cfg: &Config, profiles: &BTreeSet<String>, out: &mut Findings) {
    const C: &str = "dhcp";
    for section in &cfg.sections {
        if section.ty() == Dhcp::TY {
            let Some(name) = section.name() else {
                continue;
            };
            let views: &[ConfigView] = if name == LAN_INTERFACE {
                &[Lan]
            } else if profiles.contains(&*name) {
                &[Profiles, Lan]
            } else {
                continue;
            };
            if out.read::<Dhcp>(C, section, views).is_none() {
                continue;
            }
            if let Some(ignore) = option(section, "ignore") {
                if ignore.parse_bool("ignore").unwrap_or(false) {
                    out.value(
                        C,
                        section,
                        views,
                        ConfigProblem::Unsupported,
                        "ignore",
                        ignore,
                    );
                }
            }
            for name in ["ra", "dhcpv6"] {
                if let Some(mode) = option(section, name) {
                    if !matches!(&*mode.as_str(), "server" | "disabled") {
                        out.value(C, section, views, ConfigProblem::Unsupported, name, mode);
                    }
                }
            }
        } else if section.ty() == "host" {
            let macs = list(section, "mac");
            if let [_, extra, ..] = macs[..] {
                out.value(
                    C,
                    section,
                    &[Devices],
                    ConfigProblem::Unsupported,
                    "mac",
                    extra,
                );
            }
        }
    }
}

fn check_firewall(cfg: &Config, out: &mut Findings) {
    const C: &str = "firewall";
    let mut wan = false;
    for section in &cfg.sections {
        if section.ty() == FirewallZone::TY {
            let zone = out.read::<FirewallZone>(C, section, &[Profiles, PublishedPorts]);
            wan |= zone.is_some_and(|z| z.name == WAN_INTERFACE);
        } else if section.ty() == "forwarding" {
            out.read::<FirewallForwarding>(C, section, &[Profiles]);
        }
    }
    if !wan {
        out.absent(C, WAN_INTERFACE, &[Profiles]);
    }
}

fn check_startwrt(cfg: &Config, out: &mut Findings) {
    for section in cfg.sections.iter().filter(|s| s.ty() == UciProfile::TY) {
        out.read::<UciProfile>("startwrt", section, &[Profiles]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRSTBOOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../firstboot_config");

    fn findings(files: &[(&str, &str)]) -> Vec<ConfigFinding> {
        let arena = Arena::new();
        let parsed = files
            .iter()
            .map(|(name, text)| {
                let text: &str = arena.alloc(text.to_string());
                (
                    name.to_string(),
                    Config::parse_str(&arena, text).map_err(|e| {
                        e.map_src(|src| match src {
                            uciedit::Source::UnknownLine(n) => {
                                uciedit::Source::Line(std::path::PathBuf::from(*name), n)
                            }
                            src => src,
                        })
                    }),
                )
            })
            .collect();
        check_configs(parsed)
    }

    fn stock() -> Vec<(&'static str, String)> {
        ["network", "dhcp", "firewall", "wireless"]
            .into_iter()
            .map(|name| {
                let text = std::fs::read_to_string(format!("{FIRSTBOOT}/{name}")).unwrap();
                (name, text)
            })
            .chain([(
                "startwrt",
                "config profile 'lan'\n\toption fullname 'Admin'\n\toption interface 'lan'\n\toption vlan_tag '1'\n"
                    .to_string(),
            )])
            .collect()
    }

    fn with(changes: &[(&str, &str)]) -> Vec<ConfigFinding> {
        let mut files = stock();
        for (name, extra) in changes {
            let entry = files.iter_mut().find(|(n, _)| n == name).unwrap();
            entry.1.push_str(extra);
        }
        let files: Vec<(&str, &str)> = files.iter().map(|(n, t)| (*n, t.as_str())).collect();
        findings(&files)
    }

    #[test]
    fn stock_config_is_representable() {
        assert_eq!(with(&[]), Vec::new());
    }

    #[test]
    fn syntax_error_names_the_line_and_every_reader() {
        let found = with(&[(
            "wireless",
            "\nconfig wifi-iface 'x'\n\toption ssid 'Rich'\\''s' 'extra'\n",
        )]);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].problem, ConfigProblem::Syntax);
        assert_eq!(found[0].views, vec![Wifi, Profiles]);
        let lines = stock()
            .into_iter()
            .find(|(n, _)| *n == "wireless")
            .unwrap()
            .1
            .lines()
            .count();
        assert_eq!(found[0].line, Some(lines as u32 + 3));
    }

    #[test]
    fn unparseable_section_names_option_and_value() {
        let found = with(&[(
            "network",
            "\nconfig device\n\toption name 'eth1.20'\n\toption type '8021q'\n",
        )]);
        assert_eq!(found.len(), 1, "{found:#?}");
        let f = &found[0];
        assert_eq!(f.problem, ConfigProblem::Value);
        assert_eq!(f.views, vec![Ethernet, Profiles]);
        assert_eq!(f.option.as_deref(), Some("type"));
        assert_eq!(f.found.as_deref(), Some("8021q"));
        assert_eq!(f.section_type.as_deref(), Some("device"));
    }

    #[test]
    fn wan_protocols_the_page_cannot_show() {
        let found = findings(&[(
            "network",
            "config device\n\toption name 'br-lan'\n\toption type 'bridge'\n\
             config interface 'lan'\n\toption proto 'static'\n\toption ipaddr '192.168.0.1'\n\
             config interface 'wan'\n\toption proto 'qmi'\n\
             config interface 'wan6'\n\toption proto 'dhcpv6'\n",
        )]);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].views, vec![Wan]);
        assert_eq!(found[0].problem, ConfigProblem::Unsupported);
        assert_eq!(found[0].found.as_deref(), Some("qmi"));
        assert_eq!(found[0].line, Some(8));
    }

    #[test]
    fn lan_address_written_as_a_list() {
        const STOCK: &str = "config device\n\toption name 'br-lan'\n\toption type 'bridge'\n\
             config interface 'lan'\n\toption proto 'static'\n\tlist ipaddr '192.168.0.1/24'\n";
        let found = findings(&[(
            "network",
            &format!("{STOCK}config interface 'wan'\n\toption proto 'dhcp'\n"),
        )]);
        assert!(found.is_empty(), "{found:#?}");
        let found = findings(&[(
            "network",
            &format!(
                "{STOCK}\tlist ipaddr '10.0.0.1/24'\n\
                 config interface 'wan'\n\toption proto 'dhcp'\n"
            ),
        )]);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].views, vec![Lan, Profiles]);
        assert_eq!(found[0].problem, ConfigProblem::Unmanaged);
        assert!(found[0].notice);
        assert_eq!(found[0].found.as_deref(), Some("10.0.0.1/24"));
        assert_eq!(found[0].line, Some(7));
        let found = findings(&[(
            "network",
            "config device\n\toption name 'br-lan'\n\toption type 'bridge'\n\
             config interface 'lan'\n\toption proto 'static'\n\toption ipaddr '192.168.0.1'\n\
             \tlist ipaddr '10.0.0.1/24'\n\toption netmask '255.255.255.0'\n\
             config interface 'wan'\n\toption proto 'dhcp'\n",
        )]);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(found[0].notice);
        assert_eq!(found[0].found.as_deref(), Some("10.0.0.1/24"));
        assert_eq!(found[0].line, Some(7));
    }

    #[test]
    fn lan_masks_the_forms_cannot_keep() {
        for address in [
            "option ipaddr '192.168.1.1/16'",
            "list ipaddr '192.168.1.1/16'",
            "option ipaddr '192.168.1.1'\n\toption netmask '255.255.0.0'",
        ] {
            let found = findings(&[(
                "network",
                &format!(
                    "config device\n\toption name 'br-lan'\n\toption type 'bridge'\n\
                     config interface 'lan'\n\toption proto 'static'\n\t{address}\n\
                     config interface 'wan'\n\toption proto 'dhcp'\n"
                ),
            )]);
            assert_eq!(found.len(), 1, "{found:#?}");
            assert_eq!(found[0].problem, ConfigProblem::Unsupported);
            assert_eq!(found[0].views, vec![Lan, Profiles]);
            assert!(!found[0].notice);
        }
    }

    #[test]
    fn dhcp_turned_off_outside_startwrt() {
        let mut files = stock();
        let dhcp = &mut files.iter_mut().find(|(n, _)| *n == "dhcp").unwrap().1;
        *dhcp = dhcp.replacen(
            "config dhcp lan\n",
            "config dhcp lan\n\toption ignore '1'\n",
            1,
        );
        assert!(dhcp.contains("option ignore"), "{dhcp}");
        let files: Vec<(&str, &str)> = files.iter().map(|(n, t)| (*n, t.as_str())).collect();
        let found = findings(&files);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].views, vec![Lan]);
        assert_eq!(found[0].option.as_deref(), Some("ignore"));
        assert!(!found[0].notice);
    }

    #[test]
    fn wifi_interface_disabled_or_moved_outside_startwrt() {
        let mut files = stock();
        let wireless = &mut files.iter_mut().find(|(n, _)| *n == "wireless").unwrap().1;
        *wireless = wireless.replacen("option network 'lan'", "option network 'guest'", 1);
        let files: Vec<(&str, &str)> = files.iter().map(|(n, t)| (*n, t.as_str())).collect();
        let found = findings(&files);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].option.as_deref(), Some("network"));
        assert_eq!(found[0].views, vec![Wifi]);
        let found = with(&[(
            "wireless",
            "\nconfig wifi-iface 'off'\n\toption device 'radio0'\n\toption mode 'ap'\n\toption ssid 'Off'\n\toption disabled '1'\n",
        )]);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].option.as_deref(), Some("disabled"));
    }

    #[test]
    fn lan_ipv6_relay() {
        let found = with(&[("dhcp", "")]);
        assert!(found.is_empty());
        let mut files = stock();
        let dhcp = &mut files.iter_mut().find(|(n, _)| *n == "dhcp").unwrap().1;
        *dhcp = dhcp.replacen("option ra\t\tserver", "option ra\t\trelay", 1);
        let files: Vec<(&str, &str)> = files.iter().map(|(n, t)| (*n, t.as_str())).collect();
        let found = findings(&files);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].views, vec![Lan]);
        assert_eq!(found[0].found.as_deref(), Some("relay"));
    }

    #[test]
    fn wifi_sections_outside_the_page() {
        let found = with(&[(
            "wireless",
            "\nconfig wifi-iface 'guest'\n\toption device 'radio0'\n\toption mode 'ap'\n\toption ssid 'Guest'\n\
             \nconfig wifi-iface 'wwan'\n\toption device 'radio1'\n\toption mode 'sta'\n\toption ssid 'Upstream'\n\
             \nconfig wifi-iface\n\toption device 'radio1'\n\toption ssid 'x'\n",
        )]);
        let problems: Vec<_> = found
            .iter()
            .map(|f| (f.section.as_deref(), f.problem))
            .collect();
        assert_eq!(
            problems,
            vec![
                (Some("guest"), ConfigProblem::Extra),
                (Some("wwan"), ConfigProblem::Unsupported),
                (None, ConfigProblem::Unnamed),
            ],
            "{found:#?}"
        );
    }

    #[test]
    fn wifi_encryption_the_page_cannot_keep() {
        let mut files = stock();
        let wireless = &mut files.iter_mut().find(|(n, _)| *n == "wireless").unwrap().1;
        *wireless = wireless.replacen("option encryption 'none'", "option encryption 'sae'", 1);
        let files: Vec<(&str, &str)> = files.iter().map(|(n, t)| (*n, t.as_str())).collect();
        let found = findings(&files);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].option.as_deref(), Some("encryption"));
        assert_eq!(found[0].views, vec![Wifi]);
    }

    #[test]
    fn missing_wan_zone_and_profile_interface() {
        let mut files = stock();
        let firewall = &mut files.iter_mut().find(|(n, _)| *n == "firewall").unwrap().1;
        *firewall = firewall.replacen("option name\t\twan", "option name\t\tupstream", 1);
        let startwrt = &mut files.iter_mut().find(|(n, _)| *n == "startwrt").unwrap().1;
        startwrt.push_str(
            "\nconfig profile 'guest'\n\toption fullname 'Guest'\n\toption interface 'guest'\n\toption vlan_tag '20'\n",
        );
        let files: Vec<(&str, &str)> = files.iter().map(|(n, t)| (*n, t.as_str())).collect();
        let found = findings(&files);
        let absent: Vec<_> = found
            .iter()
            .map(|f| (f.config.as_str(), f.section.as_deref(), f.problem))
            .collect();
        assert_eq!(
            absent,
            vec![
                ("network", Some("guest"), ConfigProblem::Absent),
                ("firewall", Some("wan"), ConfigProblem::Absent),
            ],
            "{found:#?}"
        );
    }

    #[test]
    fn luci_static_lease_reads() {
        let found = with(&[(
            "dhcp",
            "\nconfig host\n\toption name 'nas'\n\tlist mac 'AA:BB:CC:DD:EE:FF'\n\toption ip '192.168.0.50'\n",
        )]);
        assert!(found.is_empty(), "{found:#?}");
        let found = with(&[(
            "dhcp",
            "\nconfig host\n\toption name 'nas'\n\tlist mac 'AA:BB:CC:DD:EE:FF'\n\tlist mac '11:22:33:44:55:66'\n",
        )]);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].views, vec![Devices]);
        assert_eq!(found[0].found.as_deref(), Some("11:22:33:44:55:66"));
    }
}
