//! Subnet changes reject WAN overlaps; block changes reject overlaps in every moved profile.
//! Existing overlaps do not block edits that leave the subnets unchanged.

use std::collections::BTreeSet;
use std::net::Ipv4Addr;

use ipnet::Ipv4Net;
use serde::{Deserialize, Serialize};
use uciedit::openwrt::{NetworkBridgeVlan, NetworkInterface, WifiStation};
use uciedit::{parse_all, Arena, Configs};

use crate::prelude::*;
use crate::profiles::{self, Lookup, Profile, ProfileId, ProfileIdOpt, ProfileSetRequest};
use crate::utils::DeserializeStdin;
use crate::CtrlContext;

fn subnets_overlap(a: Ipv4Net, b: Ipv4Net) -> bool {
    a.contains(&b.addr()) || b.contains(&a.addr())
}

/// Reads `ipv4-address` from a `network.interface.<name> status` reply.
pub(crate) fn parse_ubus_ipv4(status: &serde_json::Value) -> Vec<Ipv4Net> {
    status
        .get("ipv4-address")
        .and_then(|v| v.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|e| {
                    let addr = e.get("address")?.as_str()?.parse().ok()?;
                    let prefix = e.get("mask").and_then(|m| m.as_u64()).unwrap_or(32);
                    Ipv4Net::new(addr, prefix.min(32) as u8).ok()
                })
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, Deserialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct WanOverlap {
    pub profile: String,
    pub interface: String,
    pub subnet: String,
    pub wan_network: String,
}
rpc_toolkit::reflect_ts!(WanOverlap);

struct Found {
    id: ProfileId,
    subnet: Ipv4Net,
    wan: Ipv4Net,
}

fn profile_subnet(cfgs: &Configs, interface: &str) -> Option<Ipv4Net> {
    cfgs["network"]
        .sections
        .iter()
        .find(|s| s.name().as_deref() == Some(interface))
        .and_then(|s| s.get::<NetworkInterface>().ok()?.ipv4())
        .map(|gateway| Ipv4Net::new(gateway, 24).unwrap().trunc())
}

fn find(cfgs: &Configs, lookup: &Lookup, wan: &[Ipv4Net]) -> Vec<Found> {
    lookup
        .list()
        .iter()
        .filter_map(|id| {
            let subnet = profile_subnet(cfgs, &id.interface)?;
            let wan = *wan.iter().find(|w| subnets_overlap(**w, subnet))?;
            Some(Found {
                id: id.clone(),
                subnet,
                wan: wan.trunc(),
            })
        })
        .collect()
}

/// A Wi-Fi password, a LAN port, or an inbound VPN server leads to the profile.
fn has_point_of_entry(cfgs: &Configs, id: &ProfileId) -> bool {
    let wifi = cfgs["wireless"].sections.iter().any(|s| {
        s.get::<WifiStation>()
            .is_ok_and(|station| station.vid == Some(id.vlan_tag))
    });
    let port = cfgs["network"].sections.iter().any(|s| {
        s.get::<NetworkBridgeVlan>()
            .is_ok_and(|vlan| vlan.vlan == id.vlan_tag && !vlan.ports.is_empty())
    });
    let vpn = format!("wg_{}", id.interface);
    let vpn = cfgs["network"]
        .sections
        .iter()
        .any(|s| s.ty() == "interface" && s.name().as_deref() == Some(vpn.as_str()));
    wifi || port || vpn
}

/// Moves only profiles whose clients could not reach the dashboard to see a
/// warning: the LAN owner, and profiles nothing leads to.
fn moves_automatically(cfgs: &Configs, id: &ProfileId) -> bool {
    id.interface == crate::lan::LAN_INTERFACE || !has_point_of_entry(cfgs, id)
}

/// The lowest free `/24` in the LAN block from `.10` up, then below it.
fn free_gateway(block: [u8; 2], taken: &BTreeSet<u8>, wan: &[Ipv4Net]) -> Option<Ipv4Addr> {
    (10..=254u8).chain(1..10).find_map(|third| {
        let gateway = Ipv4Addr::new(block[0], block[1], third, 1);
        let subnet = Ipv4Net::new(gateway, 24).unwrap();
        let free = !taken.contains(&third) && !wan.iter().any(|w| subnets_overlap(*w, subnet));
        free.then_some(gateway)
    })
}

pub async fn overlaps<C: CtrlContext>(ctx: C) -> Result<Vec<WanOverlap>, Error> {
    let wan = ctx.wan_ipv4_networks();
    if wan.is_empty() {
        return Ok(Vec::new());
    }
    let arena = Arena::new();
    let cfgs = parse_all(ctx.uci_root(), &arena, &["startwrt", "network"]).await?;
    let lookup = Lookup::parse(ctx, &cfgs)?;
    Ok(find(&cfgs, &lookup, &wan)
        .into_iter()
        .map(|found| WanOverlap {
            profile: found.id.fullname,
            interface: found.id.interface,
            subnet: found.subnet.to_string(),
            wan_network: found.wan.to_string(),
        })
        .collect())
}

/// Rejects a profile gateway whose `/24` overlaps a WAN network.
pub(crate) fn guard(ctx: &impl CtrlContext, gateway: Ipv4Addr) -> Result<(), Error> {
    let subnet = Ipv4Net::new(gateway, 24).unwrap().trunc();
    match ctx
        .wan_ipv4_networks()
        .into_iter()
        .find(|w| subnets_overlap(*w, subnet))
    {
        Some(wan) => Err(Error::new(
            eyre!(
                "subnet {subnet} overlaps the upstream network {}",
                wan.trunc()
            ),
            ErrorKind::InvalidRequest,
        )),
        None => Ok(()),
    }
}

/// Rejects a configuration in which any profile's subnet overlaps a WAN network.
pub(crate) fn guard_all(ctx: &impl CtrlContext, cfgs: &Configs) -> Result<(), Error> {
    let wan = ctx.wan_ipv4_networks();
    if wan.is_empty() {
        return Ok(());
    }
    let lookup = Lookup::parse(ctx.clone(), cfgs)?;
    match find(cfgs, &lookup, &wan).into_iter().next() {
        Some(found) => Err(Error::new(
            eyre!(
                "profile {} subnet {} overlaps the upstream network {}",
                found.id.fullname,
                found.subnet,
                found.wan
            ),
            ErrorKind::InvalidRequest,
        )),
        None => Ok(()),
    }
}

/// Moves each overlapping profile that warrants it to a free subnet.
#[instrument(skip_all)]
pub async fn resolve<C: CtrlContext>(ctx: C) -> Result<(), Error> {
    let wan = ctx.wan_ipv4_networks();
    if wan.is_empty() {
        return Ok(());
    }
    let moves = {
        let arena = Arena::new();
        let cfgs = parse_all(ctx.uci_root(), &arena, &["startwrt", "network", "wireless"]).await?;
        let lookup = Lookup::parse(ctx.clone(), &cfgs)?;
        let Some(lan) = profiles::get_lan_ip(&cfgs) else {
            return Ok(());
        };
        let [a, b, ..] = lan.octets();
        let mut taken: BTreeSet<u8> = lookup
            .list()
            .iter()
            .filter_map(|id| profile_subnet(&cfgs, &id.interface))
            .filter(|subnet| subnet.addr().octets()[..2] == [a, b])
            .map(|subnet| subnet.addr().octets()[2])
            .collect();
        let mut moves = Vec::new();
        for found in find(&cfgs, &lookup, &wan) {
            if !moves_automatically(&cfgs, &found.id) {
                continue;
            }
            let Some(gateway) = free_gateway([a, b], &taken, &wan) else {
                tracing::warn!("no free subnet for profile {}", found.id.fullname);
                continue;
            };
            taken.insert(gateway.octets()[2]);
            moves.push((found, gateway));
        }
        moves
    };
    for (found, gateway) in moves {
        let name = found.id.fullname.clone();
        let mut profile: Profile<ProfileIdOpt> =
            profiles::get(ctx.clone(), found.id.into()).await?.into();
        profile.gateway_ip = gateway;
        let moved = profiles::set(
            ctx.clone(),
            DeserializeStdin(ProfileSetRequest {
                profile,
                force: false,
            }),
        )
        .await;
        crate::activity::log(
            "profile",
            "subnet-moved",
            moved.is_ok(),
            &format!(
                "Moved profile '{name}' from {} to {}, off the upstream network {}",
                found.subnet,
                Ipv4Net::new(gateway, 24).unwrap().trunc(),
                found.wan
            ),
            moved.as_ref().err().map(|e| e.to_string()).as_deref(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use rpc_toolkit::Context;
    use tokio::runtime::Runtime;

    use super::*;
    use crate::profiles::{LanAccess, WanAccess};

    #[derive(Clone)]
    struct TestContext(PathBuf, Vec<Ipv4Net>);

    impl Context for TestContext {
        fn runtime(&self) -> Option<Arc<Runtime>> {
            None
        }
    }

    impl CtrlContext for TestContext {
        fn uci_root(&self) -> PathBuf {
            self.0.clone()
        }
        fn effectful(&self) -> bool {
            false
        }
        fn wan_ipv4_networks(&self) -> Vec<Ipv4Net> {
            self.1.clone()
        }
    }

    async fn router_with(profiles: &[(&str, u8)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let firstboot = concat!(env!("CARGO_MANIFEST_DIR"), "/../firstboot_config");
        for config in ["network", "firewall", "dhcp", "wireless"] {
            std::fs::copy(format!("{firstboot}/{config}"), dir.path().join(config)).unwrap();
        }
        std::fs::write(dir.path().join("startwrt"), "").unwrap();
        profiles::bootstrap_admin_profile(dir.path().to_str().unwrap())
            .await
            .unwrap();
        let ctx = TestContext(dir.path().into(), Vec::new());
        for (name, third) in profiles {
            profiles::create(
                ctx.clone(),
                DeserializeStdin(Profile {
                    id: ProfileIdOpt {
                        fullname: Some((*name).into()),
                        interface: None,
                        vlan_tag: None,
                    },
                    gateway_ip: Ipv4Addr::new(192, 168, *third, 1),
                    outbound: "wan".into(),
                    lan_access: LanAccess::SameProfile,
                    wan_access: WanAccess::All,
                    dns_override: Vec::new(),
                    dns_source: String::new(),
                    access_to_new_profiles: false,
                    owns_lan: false,
                }),
            )
            .await
            .unwrap();
        }
        dir
    }

    async fn gateways(ctx: &TestContext) -> Vec<(String, Ipv4Addr)> {
        let mut out = Vec::new();
        for id in profiles::list(ctx.clone()).await.unwrap() {
            let profile = profiles::get(ctx.clone(), id.into()).await.unwrap();
            out.push((profile.id.fullname, profile.gateway_ip));
        }
        out.sort();
        out
    }

    #[tokio::test]
    async fn the_lan_owner_moves_off_the_upstream_subnet() {
        let dir = router_with(&[]).await;
        let ctx = TestContext(dir.path().into(), vec![net("192.168.0.37/24")]);
        assert_eq!(overlaps(ctx.clone()).await.unwrap().len(), 1);

        resolve(ctx.clone()).await.unwrap();

        assert_eq!(
            gateways(&ctx).await,
            [("Admin".into(), Ipv4Addr::new(192, 168, 10, 1))]
        );
        assert!(overlaps(ctx).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_profile_with_a_point_of_entry_is_reported_not_moved() {
        let dir = router_with(&[("Guest", 2), ("Kids", 3)]).await;
        let ctx = TestContext(dir.path().into(), Vec::new());
        let kids = profiles::get(
            ctx.clone(),
            ProfileIdOpt {
                fullname: Some("Kids".into()),
                interface: None,
                vlan_tag: None,
            },
        )
        .await
        .unwrap();
        let wireless = dir.path().join("wireless");
        let mut text = std::fs::read_to_string(&wireless).unwrap();
        text.push_str(&format!(
            "\nconfig wifi-station 'kids'\n\toption key 'kidspassword'\n\toption vid '{}'\n",
            kids.id.vlan_tag
        ));
        std::fs::write(&wireless, text).unwrap();
        let ctx = TestContext(
            dir.path().into(),
            vec![net("192.168.2.37/24"), net("192.168.3.37/24")],
        );

        resolve(ctx.clone()).await.unwrap();

        assert_eq!(
            gateways(&ctx).await,
            [
                ("Admin".into(), Ipv4Addr::new(192, 168, 0, 1)),
                ("Guest".into(), Ipv4Addr::new(192, 168, 10, 1)),
                ("Kids".into(), Ipv4Addr::new(192, 168, 3, 1)),
            ]
        );
        let reported = overlaps(ctx).await.unwrap();
        assert_eq!(
            reported
                .iter()
                .map(|o| (o.profile.as_str(), o.wan_network.as_str()))
                .collect::<Vec<_>>(),
            [("Kids", "192.168.3.0/24")]
        );
    }

    #[tokio::test]
    async fn lan_subnet_change_on_the_upstream_network_is_refused() {
        let dir = router_with(&[]).await;
        let ctx = TestContext(dir.path().into(), vec![net("192.168.5.37/24")]);
        let before = gateways(&ctx).await;

        let err = crate::lan::ipv4_set(
            ctx.clone(),
            DeserializeStdin(crate::lan::LanIpv4SetRequest {
                address: "192.168.5.1".into(),
                force: false,
            }),
        )
        .await
        .unwrap_err();

        assert_eq!(err.kind, ErrorKind::InvalidRequest);
        assert_eq!(gateways(&ctx).await, before);
    }

    #[tokio::test]
    async fn lan_block_change_checks_sibling_subnets() {
        let dir = router_with(&[("Guest", 5)]).await;
        let ctx = TestContext(dir.path().into(), vec![net("10.47.5.37/24")]);
        let before = gateways(&ctx).await;

        let err = crate::lan::ipv4_set(
            ctx.clone(),
            DeserializeStdin(crate::lan::LanIpv4SetRequest {
                address: "10.47.0.1".into(),
                force: false,
            }),
        )
        .await
        .unwrap_err();

        assert_eq!(err.kind, ErrorKind::InvalidRequest);
        assert_eq!(gateways(&ctx).await, before);

        crate::lan::ipv4_set(
            ctx.clone(),
            DeserializeStdin(crate::lan::LanIpv4SetRequest {
                address: "10.48.0.1".into(),
                force: false,
            }),
        )
        .await
        .unwrap();
        assert!(overlaps(ctx).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn profile_block_change_checks_sibling_subnets() {
        let dir = router_with(&[("Guest", 5)]).await;
        let ctx = TestContext(dir.path().into(), vec![net("10.47.5.37/24")]);
        let before = gateways(&ctx).await;
        let mut profile: Profile<ProfileIdOpt> = profiles::get(
            ctx.clone(),
            ProfileIdOpt {
                fullname: Some("Admin".into()),
                interface: None,
                vlan_tag: None,
            },
        )
        .await
        .unwrap()
        .into();
        profile.gateway_ip = "10.47.0.1".parse().unwrap();

        let err = profiles::set(
            ctx.clone(),
            DeserializeStdin(ProfileSetRequest {
                profile,
                force: false,
            }),
        )
        .await
        .unwrap_err();

        assert_eq!(err.kind, ErrorKind::InvalidRequest);
        assert_eq!(gateways(&ctx).await, before);
    }

    #[tokio::test]
    async fn an_existing_overlap_does_not_block_a_lan_noop() {
        let dir = router_with(&[]).await;
        let ctx = TestContext(dir.path().into(), vec![net("192.168.0.37/24")]);

        crate::lan::ipv4_set(
            ctx.clone(),
            DeserializeStdin(crate::lan::LanIpv4SetRequest {
                address: "192.168.0.1".into(),
                force: false,
            }),
        )
        .await
        .unwrap();

        assert_eq!(overlaps(ctx).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_new_subnet_on_the_upstream_network_is_refused() {
        let dir = router_with(&[]).await;
        let ctx = TestContext(dir.path().into(), vec![net("192.168.5.37/24")]);
        let refused = profiles::create(
            ctx,
            DeserializeStdin(Profile {
                id: ProfileIdOpt {
                    fullname: Some("Guest".into()),
                    interface: None,
                    vlan_tag: None,
                },
                gateway_ip: Ipv4Addr::new(192, 168, 5, 1),
                outbound: "wan".into(),
                lan_access: LanAccess::SameProfile,
                wan_access: WanAccess::All,
                dns_override: Vec::new(),
                dns_source: String::new(),
                access_to_new_profiles: false,
                owns_lan: false,
            }),
        )
        .await;
        assert!(refused.is_err());
    }

    fn net(s: &str) -> Ipv4Net {
        s.parse().unwrap()
    }

    #[test]
    fn overlap_follows_the_shorter_prefix() {
        assert!(subnets_overlap(
            net("192.168.0.50/24"),
            net("192.168.0.1/24")
        ));
        assert!(subnets_overlap(
            net("192.168.0.1/16"),
            net("192.168.7.1/24")
        ));
        assert!(subnets_overlap(
            net("192.168.7.1/24"),
            net("192.168.0.1/16")
        ));
        assert!(!subnets_overlap(
            net("192.168.1.50/24"),
            net("192.168.0.1/24")
        ));
        assert!(!subnets_overlap(net("10.0.0.2/30"), net("10.0.0.5/30")));
        assert!(subnets_overlap(net("100.64.3.9/10"), net("100.127.0.1/24")));
    }

    #[test]
    fn ubus_status_yields_address_and_mask() {
        let status = serde_json::json!({
            "ipv4-address": [{ "address": "192.168.2.37", "mask": 24 }, { "address": "10.0.0.1" }]
        });
        assert_eq!(
            parse_ubus_ipv4(&status),
            [net("192.168.2.37/24"), net("10.0.0.1/32")]
        );
    }

    #[test]
    fn a_free_gateway_skips_taken_and_upstream_subnets() {
        let taken = BTreeSet::from([0, 10, 11]);
        assert_eq!(
            free_gateway([192, 168], &taken, &[net("192.168.12.1/24")]),
            Some(Ipv4Addr::new(192, 168, 13, 1))
        );
        assert_eq!(
            free_gateway([192, 168], &taken, &[net("192.168.0.0/16")]),
            None
        );
    }
}
