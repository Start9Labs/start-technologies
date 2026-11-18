//! Trunk ports. A trunk carries every profile's VLAN tagged and no profile
//! untagged, for a managed switch behind it to hand each of its own ports to one
//! profile. Peers on a trunk can select any profile, Admin included.
//!
//! Only profile VLANs decide whether a port is a trunk. A bridge VLAN that
//! belongs to no profile does not change the answer, whatever the port's
//! membership in it.

use std::collections::BTreeSet;

use uciedit::openwrt::{NetworkBridgeVlan, NetworkVlanPort, NetworkVlanPortTagging};
use uciedit::Configs;

use crate::prelude::*;

/// Ports of the bridge tagged in a profile VLAN and untagged in none.
pub fn trunk_ports(
    cfgs: &Configs,
    bridge: &str,
    profile_vlans: &BTreeSet<u16>,
) -> Result<BTreeSet<String>, Error> {
    let mut tagged = BTreeSet::new();
    let mut untagged = BTreeSet::new();
    for section in &cfgs["network"].sections {
        let Some(vlan) = section.get_typed::<NetworkBridgeVlan>()? else {
            continue;
        };
        if vlan.device != bridge || !profile_vlans.contains(&vlan.vlan) {
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
\toption device 'br-lan'
\toption vlan '4000'
\tlist ports 'eth0:u*'
\tlist ports 'eth3:t'

config bridge-vlan
\toption device 'br-other'
\toption vlan '7'
\tlist ports 'eth2:t'
",
        )
        .unwrap();
        let arena = Arena::new();
        let cfgs = parse_all(dir.path(), &arena, &["network"]).await.unwrap();
        let profiles = BTreeSet::from([1, 3]);

        // VLAN 4000 belongs to no profile: eth0's untagged membership there
        // leaves it a trunk, and eth3's tagged one does not make it one.
        let trunks = trunk_ports(&cfgs, "br-lan", &profiles).unwrap();
        assert_eq!(trunks, BTreeSet::from(["eth0".to_string()]));
        assert!(trunk_ports(&cfgs, "br-other", &BTreeSet::from([7]))
            .unwrap()
            .contains("eth2"));
        assert!(trunk_ports(&cfgs, "br-other", &profiles)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn member_tagging() {
        assert_eq!(member("eth0", true).to_string(), "eth0:t");
        assert_eq!(member("eth0", false).to_string(), "eth0:u*");
    }
}
