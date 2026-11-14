//! Trunk ports. A trunk carries every profile's VLAN tagged and no untagged
//! traffic, for a managed switch behind it to hand each of its own ports to one
//! profile. Which profile a device lands in is then the switch's decision.

use std::collections::BTreeSet;

use uciedit::openwrt::{NetworkBridgeVlan, NetworkVlanPort, NetworkVlanPortTagging};
use uciedit::Configs;

use crate::prelude::*;

/// Ports of the bridge whose every VLAN membership is tagged.
pub fn trunk_ports(cfgs: &Configs, bridge: &str) -> Result<BTreeSet<String>, Error> {
    let mut tagged = BTreeSet::new();
    let mut untagged = BTreeSet::new();
    for section in &cfgs["network"].sections {
        let Some(vlan) = section.get_typed::<NetworkBridgeVlan>()? else {
            continue;
        };
        if vlan.device != bridge {
            continue;
        }
        for port in vlan.ports {
            match port.tagging {
                Some(NetworkVlanPortTagging::TAGGED) => tagged.insert(port.port),
                _ => untagged.insert(port.port),
            };
        }
    }
    Ok(tagged.difference(&untagged).cloned().collect())
}

/// A port's membership in one profile's VLAN.
pub fn member(port: &str, trunk: bool) -> NetworkVlanPort {
    NetworkVlanPort {
        port: port.to_string(),
        tagging: Some(if trunk {
            NetworkVlanPortTagging::TAGGED
        } else {
            NetworkVlanPortTagging::PRIMARY
        }),
    }
}

#[cfg(test)]
mod tests {
    use uciedit::{parse_all, Arena};

    use super::*;

    #[tokio::test]
    async fn only_all_tagged_ports_are_trunks() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("network"),
            "\
config device
\toption name 'br-lan'
\toption type 'bridge'
\tlist ports 'eth0'
\tlist ports 'eth1'
\tlist ports 'eth2'
\tlist ports 'eth3'

config bridge-vlan
\toption device 'br-lan'
\toption vlan '1'
\tlist ports 'eth0:t'
\tlist ports 'eth1:u*'
\tlist ports 'eth3'

config bridge-vlan
\toption device 'br-lan'
\toption vlan '3'
\tlist ports 'eth0:t'
\tlist ports 'eth1:t'
\tlist ports 'eth2:u*'

config bridge-vlan
\toption device 'br-other'
\toption vlan '7'
\tlist ports 'eth2:t'
",
        )
        .unwrap();
        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["network"]).await.unwrap();

        let trunks = trunk_ports(&cfgs, "br-lan").unwrap();
        assert_eq!(trunks, BTreeSet::from(["eth0".to_string()]));
        assert!(trunk_ports(&cfgs, "br-other").unwrap().contains("eth2"));
    }

    #[test]
    fn member_tagging() {
        assert_eq!(member("eth0", true).to_string(), "eth0:t");
        assert_eq!(member("eth0", false).to_string(), "eth0:u*");
    }
}
