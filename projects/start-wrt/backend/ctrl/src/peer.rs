//! Names the network a client reaches the router from, for the activity log.

use std::net::IpAddr;

use serde::Deserialize;
use uciedit::openwrt::NetworkInterface;
use uciedit::{Arena, Configs};

use crate::vpn_server::UciVpnServer;
use crate::wan::WAN_INTERFACE;
use crate::ServerContext;

/// `192.168.0.23 (Admin profile)`, or the bare address when its network is
/// unknown.
pub async fn describe(ip: IpAddr) -> String {
    let ip = ip.to_canonical();
    match network_of(ip).await {
        Some(network) => format!("{ip} ({network})"),
        None => ip.to_string(),
    }
}

async fn network_of(ip: IpAddr) -> Option<String> {
    let device = route_device(ip).await?;
    let bytes = uciedit::read_all("/etc/config", &["network", "startwrt"])
        .await
        .ok()?;
    let arena = Arena::new();
    network_label(&bytes.parse(&arena).ok()?, &device)
}

/// The interface the router answers the address through.
async fn route_device(ip: IpAddr) -> Option<String> {
    let output = tokio::process::Command::new("ip")
        .args(["-j", "route", "get", &ip.to_string()])
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_route_device(&output.stdout)
}

fn parse_route_device(json: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    struct Route {
        dev: String,
    }
    serde_json::from_slice::<Vec<Route>>(json)
        .ok()?
        .into_iter()
        .next()
        .map(|route| route.dev)
}

fn network_label(cfgs: &Configs, device: &str) -> Option<String> {
    if device == "lo" {
        return Some("this router".into());
    }
    let interface_device = |section: &str| {
        cfgs["network"]
            .sections
            .iter()
            .find(|s| s.name().as_deref() == Some(section))?
            .get::<NetworkInterface>()
            .ok()
            .map(|iface| iface.device)
    };
    let profiles = crate::profiles::list_config(ServerContext::default(), cfgs).ok()?;
    let profile_name = |interface: &str| {
        profiles
            .iter()
            .find(|p| p.interface == interface)
            .map(|p| p.fullname.clone())
    };
    if let Some(profile) = profiles
        .iter()
        .find(|p| interface_device(&p.interface).as_deref() == Some(device))
    {
        return Some(format!("{} profile", profile.fullname));
    }
    // netifd names a WireGuard device after its interface section.
    if let Some(name) = cfgs["startwrt"]
        .sections
        .iter()
        .filter_map(|s| s.get::<UciVpnServer>().ok())
        .find(|server| server.interface == device)
        .and_then(|server| profile_name(&server.profile_interface))
    {
        return Some(format!("{name} profile, VPN"));
    }
    if device == format!("pppoe-{WAN_INTERFACE}")
        || interface_device(WAN_INTERFACE).as_deref() == Some(device)
    {
        return Some("WAN".into());
    }
    None
}

#[cfg(test)]
mod tests {
    use uciedit::parse_all;

    use super::*;

    const NETWORK: &str = "\
config interface 'lan'
\toption device 'br-lan'
\toption proto 'static'
\toption ipaddr '192.168.0.1'
\toption netmask '255.255.255.0'

config interface 'iot'
\toption device 'br-lan.20'
\toption proto 'static'
\toption ipaddr '192.168.20.1'
\toption netmask '255.255.255.0'

config interface 'wg_lan'
\toption proto 'wireguard'

config interface 'wan'
\toption device 'eth0'
\toption proto 'dhcp'
";

    const STARTWRT: &str = "\
config profile
\toption fullname 'Admin'
\toption interface 'lan'
\toption vlan_tag '1'

config profile
\toption fullname 'IoT'
\toption interface 'iot'
\toption vlan_tag '20'

config vpn_server
\toption interface 'wg_lan'
\toption profile_interface 'lan'
\toption label 'Home'
\toption listen_port '51820'
\toption endpoint 'vpn.example.com'
";

    #[tokio::test]
    async fn names_the_network_behind_each_device() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("network"), NETWORK).unwrap();
        std::fs::write(dir.path().join("startwrt"), STARTWRT).unwrap();
        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["network", "startwrt"])
            .await
            .unwrap();
        for (device, label) in [
            ("br-lan", Some("Admin profile")),
            ("br-lan.20", Some("IoT profile")),
            ("wg_lan", Some("Admin profile, VPN")),
            ("eth0", Some("WAN")),
            ("pppoe-wan", Some("WAN")),
            ("lo", Some("this router")),
            ("wg_out", None),
        ] {
            assert_eq!(network_label(&cfgs, device).as_deref(), label, "{device}");
        }
    }

    #[test]
    fn reads_the_device_from_a_route_lookup() {
        let json = br#"[{"dst":"192.168.20.7","dev":"br-lan.20","prefsrc":"192.168.20.1","flags":[],"uid":0,"cache":[]}]"#;
        assert_eq!(parse_route_device(json).as_deref(), Some("br-lan.20"));
        assert_eq!(parse_route_device(b"[]"), None);
        assert_eq!(parse_route_device(b"not json"), None);
    }
}
