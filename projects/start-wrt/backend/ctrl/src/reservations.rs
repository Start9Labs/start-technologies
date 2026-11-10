//! A device's DHCP host sections. The first holds its settings, each holds at
//! most one IPv4 reservation, and a device has at most one reservation per
//! profile.
//!
//! dnsmasq answers a client from the first `dhcp-host` line for its MAC that
//! fits the subnet serving it, and a line without an address fits every
//! subnet. So while a device has a reservation, every one of its sections
//! holds one, and each carries the device's name.

use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;

use ipnet::Ipv4Net;
use uciedit::openwrt::{DhcpHost, InterfaceProto, NetworkInterface};
use uciedit::{Config, Configs, Section};

use crate::error::ErrorKind;
use crate::prelude::*;
use crate::profiles::Lookup;
use crate::Error;

pub(crate) struct ProfileNetwork {
    pub(crate) vlan: u16,
    pub(crate) device: String,
    pub(crate) name: String,
    /// The router's address on the profile, with its prefix length.
    pub(crate) address: Option<Ipv4Net>,
}

/// `cfgs` must contain "network".
pub(crate) fn profile_networks(cfgs: &Configs, lookup: &Lookup) -> Vec<ProfileNetwork> {
    lookup
        .list()
        .iter()
        .filter_map(|p| {
            let iface = cfgs["network"]
                .sections
                .iter()
                .find(|s| s.name().as_deref() == Some(p.interface.as_str()))?
                .get::<NetworkInterface>()
                .ok()?;
            let address = if iface.proto == InterfaceProto::STATIC {
                iface
                    .ipv4()
                    .zip(iface.ipv4_netmask())
                    .and_then(|(ip, mask)| Ipv4Net::with_netmask(ip, mask).ok())
            } else {
                None
            };
            Some(ProfileNetwork {
                vlan: p.vlan_tag,
                device: iface.device,
                name: p.fullname.clone(),
                address,
            })
        })
        .collect()
}

pub(crate) fn profile_of(networks: &[ProfileNetwork], ip: Ipv4Addr) -> Option<&ProfileNetwork> {
    networks
        .iter()
        .find(|p| p.address.is_some_and(|net| net.contains(&ip)))
}

/// The reservation DHCP gives the device: the one in the subnet of the profile
/// it is on, or for a device on no known subnet, its only one.
pub(crate) fn active_reservation(
    reserved: &[Ipv4Addr],
    profile_subnet: Option<Ipv4Net>,
) -> Option<Ipv4Addr> {
    match (profile_subnet, reserved) {
        (Some(net), _) => reserved.iter().copied().find(|ip| net.contains(ip)),
        (None, [only]) => Some(*only),
        (None, _) => None,
    }
}

/// Addresses outside every profile count as one profile.
pub(crate) fn same_profile(networks: &[ProfileNetwork], a: Ipv4Addr, b: Ipv4Addr) -> bool {
    profile_of(networks, a).map(|p| p.vlan) == profile_of(networks, b).map(|p| p.vlan)
}

fn host(section: &Section<'_>) -> Option<DhcpHost> {
    section.get_typed::<DhcpHost>().ok().flatten()
}

fn reserved_ip(host: &DhcpHost) -> Option<Ipv4Addr> {
    host.ip.as_deref()?.parse().ok()
}

/// Each device's reservations in config order, by uppercase MAC.
pub(crate) fn reservations_by_mac(dhcp: &Config<'_>) -> HashMap<String, Vec<Ipv4Addr>> {
    let mut by_mac: HashMap<String, Vec<Ipv4Addr>> = HashMap::new();
    for host in dhcp.sections.iter().filter_map(host) {
        if let Some(ip) = reserved_ip(&host) {
            by_mac.entry(host.mac.to_uppercase()).or_default().push(ip);
        }
    }
    by_mac
}

pub(crate) fn reservations_of(dhcp: &Config<'_>, mac: &str) -> Vec<Ipv4Addr> {
    dhcp.sections
        .iter()
        .filter_map(host)
        .filter(|h| h.mac.eq_ignore_ascii_case(mac))
        .filter_map(|h| reserved_ip(&h))
        .collect()
}

/// The device's reservations once `ip` replaces the one in its profile.
pub(crate) fn with_reservation(
    current: &[Ipv4Addr],
    networks: &[ProfileNetwork],
    ip: Ipv4Addr,
) -> Vec<Ipv4Addr> {
    let mut want: Vec<Ipv4Addr> = current
        .iter()
        .copied()
        .filter(|r| !same_profile(networks, *r, ip))
        .collect();
    want.push(ip);
    want
}

/// Refuses an address DHCP cannot hand the device: outside the subnet of its
/// profile, or of every profile when its profile is unknown, or not a host
/// address there.
pub(crate) fn validate_reservation(
    ip: Ipv4Addr,
    networks: &[ProfileNetwork],
    current_vlan: Option<u16>,
) -> Result<(), Error> {
    let current = current_vlan
        .and_then(|vlan| networks.iter().find(|p| p.vlan == vlan))
        .filter(|p| p.address.is_some());
    let candidates: Vec<&ProfileNetwork> = match current {
        Some(p) => vec![p],
        None => networks.iter().filter(|p| p.address.is_some()).collect(),
    };
    if candidates.is_empty() {
        return Ok(());
    }
    let invalid = |msg: String| Err(Error::new(eyre!("{msg}"), ErrorKind::InvalidValue));
    let Some((profile, net)) = candidates
        .iter()
        .filter_map(|p| Some((*p, p.address?)))
        .find(|(_, net)| net.contains(&ip))
    else {
        return match current.and_then(|p| Some((p, p.address?))) {
            Some((p, net)) => invalid(format!(
                "{ip} is outside {}, the subnet of the {} profile this device is on",
                net.trunc(),
                p.name
            )),
            None => invalid(format!("{ip} is outside the subnet of every profile")),
        };
    };
    if ip == net.addr() {
        return invalid(format!(
            "{ip} is the router's address on the {} profile",
            profile.name
        ));
    }
    if ip == net.network() || ip == net.broadcast() {
        return invalid(format!("{ip} is not a host address in {}", net.trunc()));
    }
    Ok(())
}

/// Refuses an address another device has reserved: dnsmasq does not start
/// with one address on two hosts.
pub(crate) fn validate_unclaimed(dhcp: &Config<'_>, mac: &str, ip: Ipv4Addr) -> Result<(), Error> {
    let Some(holder) = dhcp
        .sections
        .iter()
        .filter_map(host)
        .find(|h| !h.mac.eq_ignore_ascii_case(mac) && reserved_ip(h) == Some(ip))
    else {
        return Ok(());
    };
    let holder = holder
        .name
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| holder.mac.to_uppercase());
    Err(Error::new(
        eyre!("{ip} is already reserved for {holder}"),
        ErrorKind::InvalidValue,
    ))
}

/// Rewrites the device's sections to hold exactly `want`. A kept reservation
/// stays in its section, and the first section keeps the settings.
pub(crate) fn write_reservations(
    dhcp: &mut Config<'_>,
    mac: &str,
    want: &[Ipv4Addr],
) -> Result<(), Error> {
    let mut owned: Vec<(usize, DhcpHost)> = dhcp
        .sections
        .iter()
        .enumerate()
        .filter_map(|(i, s)| Some((i, host(s)?)))
        .filter(|(_, h)| h.mac.eq_ignore_ascii_case(mac))
        .collect();

    let mut pending: Vec<Ipv4Addr> = Vec::new();
    for ip in want {
        if !pending.contains(ip) {
            pending.push(*ip);
        }
    }
    let mut slots: Vec<Option<Ipv4Addr>> = owned
        .iter()
        .map(|(_, h)| {
            let ip = reserved_ip(h)?;
            let at = pending.iter().position(|p| *p == ip)?;
            Some(pending.remove(at))
        })
        .collect();
    for slot in &mut slots {
        if slot.is_none() && !pending.is_empty() {
            *slot = Some(pending.remove(0));
        }
    }
    if slots.first().is_some_and(Option::is_none) {
        if let Some(j) = slots.iter().position(Option::is_some) {
            slots[0] = slots[j].take();
        }
    }

    let name = owned
        .iter()
        .find_map(|(_, h)| h.name.clone().filter(|n| !n.is_empty()));
    let mut removed = Vec::new();
    for k in (1..owned.len()).rev() {
        if slots[k].is_some() {
            continue;
        }
        let (index, extra) = owned.remove(k);
        slots.remove(k);
        if let Some((_, settings)) = owned.first_mut() {
            settings._allow_pcp = settings._allow_pcp.take().or(extra._allow_pcp);
            settings._allow_dns_inject = settings
                ._allow_dns_inject
                .take()
                .or(extra._allow_dns_inject);
            settings._allow_ra = settings._allow_ra.take().or(extra._allow_ra);
            settings.hostid = settings.hostid.take().or(extra.hostid);
        }
        removed.push(index);
    }
    for ((index, mut host), slot) in owned.into_iter().zip(slots) {
        host.ip = slot.map(|ip| ip.to_string());
        host.name = name.clone();
        dhcp.sections[index].set(&host)?;
    }
    removed.sort_unstable();
    for index in removed.into_iter().rev() {
        dhcp.sections.remove(index);
    }

    let mut taken: HashSet<String> = dhcp
        .sections
        .iter()
        .filter_map(|s| s.name().map(|n| n.into_owned()))
        .collect();
    let base = format!("host_{}", mac.replace(':', "").to_lowercase());
    for ip in pending {
        let first = !dhcp
            .sections
            .iter()
            .filter_map(host)
            .any(|h| h.mac.eq_ignore_ascii_case(mac));
        let section_name = if first && !taken.contains(&base) {
            base.clone()
        } else {
            (2..)
                .map(|n| format!("{base}_{n}"))
                .find(|n| !taken.contains(n))
                .unwrap_or_default()
        };
        taken.insert(section_name.clone());
        dhcp.append(
            &DhcpHost {
                mac: mac.to_uppercase(),
                ip: Some(ip.to_string()),
                name: name.clone(),
                ..Default::default()
            },
            Some(&section_name),
        )?;
    }
    Ok(())
}

/// Deletes every reservation in `subnet`. Returns them by uppercase MAC, in
/// MAC order.
pub(crate) fn remove_in_subnet(
    dhcp: &mut Config<'_>,
    subnet: Ipv4Net,
) -> Result<Vec<(String, Ipv4Addr)>, Error> {
    let mut by_mac: Vec<(String, Vec<Ipv4Addr>)> = reservations_by_mac(dhcp).into_iter().collect();
    by_mac.sort();
    let mut removed = Vec::new();
    for (mac, ips) in by_mac {
        let (gone, kept): (Vec<_>, Vec<_>) = ips.into_iter().partition(|ip| subnet.contains(ip));
        if gone.is_empty() {
            continue;
        }
        write_reservations(dhcp, &mac, &kept)?;
        removed.extend(gone.into_iter().map(|ip| (mac.clone(), ip)));
    }
    Ok(removed)
}

/// Names every section of the device, adding its settings section for a name
/// it has none for. `None` or empty clears the name.
pub(crate) fn write_name(
    dhcp: &mut Config<'_>,
    mac: &str,
    name: Option<&str>,
) -> Result<(), Error> {
    let name = name.filter(|n| !n.is_empty()).map(str::to_string);
    let mut found = false;
    for section in &mut dhcp.sections {
        let Some(mut host) = host(section).filter(|h| h.mac.eq_ignore_ascii_case(mac)) else {
            continue;
        };
        found = true;
        host.name = name.clone();
        section.set(&host)?;
    }
    if !found && name.is_some() {
        let section_name = format!("host_{}", mac.replace(':', "").to_lowercase());
        dhcp.append(
            &DhcpHost {
                mac: mac.to_string(),
                name,
                ..Default::default()
            },
            Some(&section_name),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use uciedit::Arena;

    use super::*;

    fn networks() -> Vec<ProfileNetwork> {
        vec![
            ProfileNetwork {
                vlan: 1,
                device: "br-lan.1".into(),
                name: "Admin".into(),
                address: Some("192.168.1.1/24".parse().unwrap()),
            },
            ProfileNetwork {
                vlan: 20,
                device: "br-lan.20".into(),
                name: "Guest".into(),
                address: Some("192.168.20.1/24".parse().unwrap()),
            },
        ]
    }

    fn ip(s: &str) -> Ipv4Addr {
        s.parse().unwrap()
    }

    fn reservation_error(addr: &str, current_vlan: Option<u16>) -> Option<String> {
        validate_reservation(ip(addr), &networks(), current_vlan)
            .err()
            .map(|err| err.to_string())
    }

    #[test]
    fn reservation_must_sit_in_the_device_profile() {
        assert_eq!(reservation_error("192.168.20.50", Some(20)), None);
        let err = reservation_error("192.168.1.50", Some(20)).expect("other profile refused");
        assert!(err.contains("192.168.20.0/24"), "{err}");
        assert!(err.contains("Guest"), "{err}");
    }

    #[test]
    fn reservation_for_unplaced_device_needs_some_profile() {
        assert_eq!(reservation_error("192.168.1.50", None), None);
        assert_eq!(reservation_error("192.168.20.50", None), None);
        let err = reservation_error("10.0.0.5", None).expect("no profile refused");
        assert!(err.contains("every profile"), "{err}");
    }

    #[test]
    fn reservation_refuses_non_host_addresses() {
        for addr in ["192.168.20.0", "192.168.20.255", "192.168.20.1"] {
            assert!(
                reservation_error(addr, Some(20)).is_some(),
                "{addr} accepted"
            );
            assert!(
                reservation_error(addr, None).is_some(),
                "{addr} accepted unplaced"
            );
        }
    }

    #[test]
    fn reservation_checks_nothing_without_a_static_subnet() {
        let mut networks = networks();
        for p in &mut networks {
            p.address = None;
        }
        assert!(validate_reservation(ip("10.0.0.5"), &networks, Some(20)).is_ok());
    }

    #[test]
    fn a_reservation_replaces_the_one_in_its_profile_only() {
        let current = [ip("192.168.1.50"), ip("192.168.20.60")];
        assert_eq!(
            with_reservation(&current, &networks(), ip("192.168.20.70")),
            vec![ip("192.168.1.50"), ip("192.168.20.70")]
        );
        assert_eq!(
            with_reservation(&current[..1], &networks(), ip("192.168.20.70")),
            vec![ip("192.168.1.50"), ip("192.168.20.70")]
        );
    }

    const MAC: &str = "AA:BB:CC:DD:EE:FF";

    fn rewrite(dhcp: &str, want: &[&str]) -> (String, Vec<Ipv4Addr>) {
        let arena = Arena::new();
        let mut config = Config::parse_str(&arena, arena.alloc(dhcp.to_string())).unwrap();
        let want: Vec<Ipv4Addr> = want.iter().map(|s| ip(s)).collect();
        write_reservations(&mut config, MAC, &want).unwrap();
        let held = reservations_of(&config, MAC);
        (config.dump_str(), held)
    }

    fn sections(dump: &str) -> Vec<Vec<&str>> {
        dump.split("\nconfig ")
            .map(|s| s.lines().map(str::trim).filter(|l| !l.is_empty()).collect())
            .collect()
    }

    const NAMED: &str = "\
config dnsmasq
\toption domain 'lan'

config host 'host_aabbccddeeff'
\toption mac 'AA:BB:CC:DD:EE:FF'
\toption name 'nas'
\toption ip '192.168.1.50'
\toption _allow_pcp '1'
";

    #[test]
    fn a_second_profile_gets_its_own_named_section() {
        let (dump, held) = rewrite(NAMED, &["192.168.1.50", "192.168.20.60"]);
        assert_eq!(held, vec![ip("192.168.1.50"), ip("192.168.20.60")]);
        let extra = sections(&dump)
            .into_iter()
            .find(|s| s.contains(&"option ip '192.168.20.60'"))
            .expect("extra section");
        assert_eq!(extra[0], "host host_aabbccddeeff_2");
        assert!(extra.contains(&"option name 'nas'"), "{dump}");
        assert!(
            !extra.iter().any(|l| l.contains("_allow_pcp")),
            "settings stay on the first section: {dump}"
        );
    }

    #[test]
    fn every_section_keeps_an_address_while_any_reservation_remains() {
        let two = "\
config host 'host_aabbccddeeff'
\toption mac 'AA:BB:CC:DD:EE:FF'
\toption name 'nas'
\toption ip '192.168.1.50'
\toption _allow_pcp '1'

config host 'host_aabbccddeeff_2'
\toption mac 'AA:BB:CC:DD:EE:FF'
\toption name 'nas'
\toption ip '192.168.20.60'
";
        let (dump, held) = rewrite(two, &["192.168.20.60"]);
        assert_eq!(held, vec![ip("192.168.20.60")]);
        let hosts: Vec<Vec<&str>> = sections(&dump)
            .into_iter()
            .filter(|s| s.iter().any(|l| l.contains(MAC)))
            .collect();
        assert_eq!(hosts.len(), 1, "{dump}");
        assert!(hosts[0].contains(&"option ip '192.168.20.60'"), "{dump}");
        assert!(hosts[0].contains(&"option _allow_pcp '1'"), "{dump}");
        assert!(hosts[0].contains(&"option name 'nas'"), "{dump}");
    }

    #[test]
    fn clearing_every_reservation_keeps_the_settings() {
        let (dump, held) = rewrite(NAMED, &[]);
        assert!(held.is_empty());
        assert!(dump.contains("option name 'nas'"), "{dump}");
        assert!(dump.contains("option _allow_pcp '1'"), "{dump}");
        assert!(!dump.contains("option ip"), "{dump}");
    }

    #[test]
    fn a_device_without_sections_gets_settings_then_extras() {
        let (dump, held) = rewrite("", &["192.168.1.50", "192.168.20.60"]);
        assert_eq!(held, vec![ip("192.168.1.50"), ip("192.168.20.60")]);
        assert!(dump.contains("config host host_aabbccddeeff\n"), "{dump}");
        assert!(dump.contains("config host host_aabbccddeeff_2\n"), "{dump}");
    }

    #[test]
    fn unchanged_reservations_stay_in_their_sections() {
        let (once, _) = rewrite(NAMED, &["192.168.1.50", "192.168.20.60"]);
        let (twice, _) = rewrite(&once, &["192.168.20.60", "192.168.1.50"]);
        assert_eq!(once, twice);
    }

    #[test]
    fn an_address_held_by_another_device_is_refused() {
        let arena = Arena::new();
        let config = Config::parse_str(&arena, NAMED).unwrap();
        let err = validate_unclaimed(&config, "11:22:33:44:55:66", ip("192.168.1.50"))
            .expect_err("duplicate refused");
        assert!(err.to_string().contains("nas"), "{err}");
        assert!(validate_unclaimed(&config, MAC, ip("192.168.1.50")).is_ok());
        assert!(validate_unclaimed(&config, "11:22:33:44:55:66", ip("192.168.1.51")).is_ok());
    }

    #[test]
    fn a_name_reaches_every_section() {
        let arena = Arena::new();
        let mut config = Config::parse_str(&arena, NAMED).unwrap();
        write_reservations(&mut config, MAC, &[ip("192.168.1.50"), ip("192.168.20.60")]).unwrap();
        write_name(&mut config, MAC, Some("vault")).unwrap();
        let dump = config.dump_str();
        assert_eq!(dump.matches("option name 'vault'").count(), 2, "{dump}");
        write_name(&mut config, MAC, None).unwrap();
        assert!(!config.dump_str().contains("option name"));
    }
}
