//! OS backend target identities stay absent from discovery and derived exposure.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub const START9_BRIDGE_IFACE: &str = "lxcbr0";
pub(crate) const START9_BRIDGE_V6_ADDR: Ipv6Addr = Ipv6Addr::new(0xfd00, 3, 0, 0, 0, 0, 0, 1);

pub(crate) const OS_TARGET_IPV4: Ipv4Addr = Ipv4Addr::new(10, 0, 3, 254);
pub(crate) const OS_TARGET_IPV6: Ipv6Addr = Ipv6Addr::new(0xfd00, 3, 0, 0, 0, 0, 0, 0x254);

pub(crate) fn is_os_target(ip: IpAddr) -> bool {
    let ip = ip.to_canonical();
    ip == IpAddr::V4(OS_TARGET_IPV4) || ip == IpAddr::V6(OS_TARGET_IPV6)
}

pub(crate) fn is_os_target_name(name: &str) -> bool {
    name.parse().is_ok_and(is_os_target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_targets_include_mapped_ipv4_but_not_bridge_gateways_or_containers() {
        for ip in [
            IpAddr::V4(OS_TARGET_IPV4),
            IpAddr::V6(OS_TARGET_IPV6),
            IpAddr::V6(OS_TARGET_IPV4.to_ipv6_mapped()),
        ] {
            assert!(is_os_target(ip));
            assert!(is_os_target_name(&ip.to_string()));
        }
        for ip in [
            IpAddr::V4(crate::HOST_IP.into()),
            IpAddr::V6(START9_BRIDGE_V6_ADDR),
            IpAddr::V4(Ipv4Addr::new(10, 0, 3, 2)),
            IpAddr::V6(Ipv6Addr::new(0xfd00, 3, 0, 0, 0, 0, 0, 2)),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(Ipv6Addr::LOCALHOST),
        ] {
            assert!(!is_os_target(ip));
            assert!(!is_os_target_name(&ip.to_string()));
        }
    }
}
