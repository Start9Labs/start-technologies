use tokio::process::Command;

use crate::lxc::slot::{
    CONTAINER_NET, CONTAINER_NET_V6, HOST_IP, HOST_IPV6, OS_LISTEN_IP, OS_LISTEN_IPV6,
};
use crate::net::forward::{START9_BRIDGE_IFACE, enable_forwarding};
use crate::prelude::*;
use crate::util::Invoke;

/// Brings up the container network and returns once a container can start on it.
///
/// `lxcbr0` has no ports. It holds the server's container-network addresses, which each container
/// reaches over its own routed veth.
pub async fn ensure() -> Result<(), Error> {
    enable_forwarding().await?;
    if Command::new("ip")
        .args(["link", "show", "dev", START9_BRIDGE_IFACE])
        .invoke(ErrorKind::Network)
        .await
        .is_err()
    {
        Command::new("ip")
            .args(["link", "add", START9_BRIDGE_IFACE, "type", "bridge"])
            .invoke(ErrorKind::Network)
            .await?;
    }
    for address in [HOST_IP, OS_LISTEN_IP] {
        Command::new("ip")
            .args(["-4", "addr", "replace"])
            .arg(format!("{address}/{}", CONTAINER_NET.prefix_len()))
            .args(["dev", START9_BRIDGE_IFACE])
            .invoke(ErrorKind::Network)
            .await?;
    }
    for address in [HOST_IPV6, OS_LISTEN_IPV6] {
        // Duplicate detection never finishes on a bridge without carrier.
        Command::new("ip")
            .args(["-6", "addr", "replace"])
            .arg(format!("{address}/{}", CONTAINER_NET_V6.prefix_len()))
            .args(["dev", START9_BRIDGE_IFACE, "nodad"])
            .invoke(ErrorKind::Network)
            .await?;
    }
    Command::new("ip")
        .args(["link", "set", START9_BRIDGE_IFACE, "up"])
        .invoke(ErrorKind::Network)
        .await?;
    Command::new("nft")
        .arg(masquerade_rules())
        .invoke(ErrorKind::Network)
        .await?;
    Ok(())
}

/// Masquerades container traffic that leaves the container network.
fn masquerade_rules() -> String {
    let mut rules = String::new();
    for (family, net) in [
        ("ip", CONTAINER_NET.to_string()),
        ("ip6", CONTAINER_NET_V6.to_string()),
    ] {
        rules += &format!(
            "add table {family} startos_containers\n\
             add chain {family} startos_containers postrouting \
             {{ type nat hook postrouting priority srcnat + 1; policy accept; }}\n\
             flush chain {family} startos_containers postrouting\n\
             add rule {family} startos_containers postrouting \
             {family} saddr {net} {family} daddr != {net} masquerade\n"
        );
    }
    rules
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn masquerade_covers_both_container_networks_and_spares_internal_traffic() {
        let rules = masquerade_rules();
        assert!(rules.contains(
            "add rule ip startos_containers postrouting ip saddr 172.16.0.0/16 ip daddr != 172.16.0.0/16 masquerade"
        ));
        assert!(rules.contains(
            "add rule ip6 startos_containers postrouting ip6 saddr fd00:3::/64 ip6 daddr != fd00:3::/64 masquerade"
        ));
    }
}
