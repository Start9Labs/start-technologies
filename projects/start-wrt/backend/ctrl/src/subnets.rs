//! The subnets the router occupies: every profile's `/24` and the WAN's networks.
//!
//! A write that adds or moves a profile or LAN gateway, or changes the static WAN
//! address, snapshots [`Subnets`] right after parsing and passes the result to
//! [`check`] before dumping. Unchanged subnets always pass.

use std::collections::{BTreeMap, BTreeSet};
use std::net::Ipv4Addr;

use ipnet::Ipv4Net;
use serde::{Deserialize, Serialize};
use uciedit::openwrt::{
    DhcpHost, InterfaceProto, NetworkBridgeVlan, NetworkInterface, WifiStation,
};
use uciedit::{parse_all, Arena, Configs};

use crate::lan::LAN_INTERFACE;
use crate::prelude::*;
use crate::profiles::{self, Profile, ProfileIdOpt, ProfileSetRequest, UciProfile};
use crate::utils::DeserializeStdin;
use crate::wan::WAN_INTERFACE;
use crate::CtrlContext;

fn overlap(a: Ipv4Net, b: Ipv4Net) -> bool {
    a.contains(&b.addr()) || b.contains(&a.addr())
}

fn slash24(gateway: Ipv4Addr) -> Ipv4Net {
    Ipv4Net::new(gateway, 24).unwrap().trunc()
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct Member {
    fullname: String,
    vlan_tag: u16,
    gateway: Ipv4Addr,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Subnets {
    profiles: BTreeMap<String, Member>,
    /// `None` unless the WAN is static.
    static_wan: Option<Ipv4Net>,
}

impl Subnets {
    pub(crate) fn of(cfgs: &Configs) -> Self {
        let interface = |name: &str| {
            cfgs["network"]
                .sections
                .iter()
                .find(|s| s.name().as_deref() == Some(name))
                .and_then(|s| s.get::<NetworkInterface>().ok())
        };
        let profiles: BTreeMap<String, Member> = cfgs["startwrt"]
            .sections
            .iter()
            .filter_map(|s| s.get::<UciProfile>().ok())
            .filter_map(|p| {
                let gateway = interface(&p.interface)?.ipv4()?;
                Some((
                    p.interface,
                    Member {
                        fullname: p.fullname,
                        vlan_tag: p.vlan_tag,
                        gateway,
                    },
                ))
            })
            .collect();
        let mut profiles: BTreeMap<String, Member> = profiles;
        if !profiles.contains_key(LAN_INTERFACE) {
            if let Some(gateway) = interface(LAN_INTERFACE).and_then(|lan| lan.ipv4()) {
                profiles.insert(
                    LAN_INTERFACE.into(),
                    Member {
                        fullname: "LAN".into(),
                        vlan_tag: 1,
                        gateway,
                    },
                );
            }
        }
        let static_wan = interface(WAN_INTERFACE)
            .filter(|wan| wan.proto == InterfaceProto::STATIC)
            .and_then(|wan| Ipv4Net::with_netmask(wan.ipv4()?, wan.ipv4_netmask()?).ok());
        Self {
            profiles,
            static_wan,
        }
    }

    fn wan(&self, live: Vec<Ipv4Net>) -> Vec<Ipv4Net> {
        live.into_iter().chain(self.static_wan).collect()
    }

    fn block(&self) -> Option<[u8; 2]> {
        let [a, b, ..] = self.profiles.get(LAN_INTERFACE)?.gateway.octets();
        Some([a, b])
    }

    fn overlapping<'a>(
        &'a self,
        wan: &'a [Ipv4Net],
    ) -> impl Iterator<Item = (&'a String, &'a Member, Ipv4Net)> {
        self.profiles.iter().filter_map(|(interface, member)| {
            let subnet = slash24(member.gateway);
            let wan = *wan.iter().find(|w| overlap(**w, subnet))?;
            Some((interface, member, wan.trunc()))
        })
    }

    /// The lowest `/24` in the block, from `preferred` then `.10` up, that no
    /// profile holds and no WAN network overlaps.
    pub(crate) fn free_gateway(
        &self,
        wan: &[Ipv4Net],
        preferred: impl IntoIterator<Item = u8>,
    ) -> Option<Ipv4Addr> {
        let [a, b] = self.block()?;
        let taken: BTreeSet<Ipv4Net> = self.profiles.values().map(|m| slash24(m.gateway)).collect();
        preferred
            .into_iter()
            .chain(10..=254)
            .chain(1..10)
            .map(|third| Ipv4Addr::new(a, b, third, 1))
            .find(|&gateway| {
                let subnet = slash24(gateway);
                !taken.contains(&subnet) && !wan.iter().any(|w| overlap(*w, subnet))
            })
    }

    fn claim(&mut self, interface: &str, gateway: Ipv4Addr) {
        self.profiles.insert(
            interface.into(),
            Member {
                fullname: String::new(),
                vlan_tag: 0,
                gateway,
            },
        );
    }
}

fn invalid(message: String) -> Error {
    Error::new(eyre!("{message}"), ErrorKind::InvalidRequest)
}

/// Rejects a change from `before` to `after` that leaves any added or moved subnet
/// outside the block, on another profile's `/24`, on a WAN network, or that strands
/// a DHCP reservation in the subnet it left. A changed static WAN is checked
/// against every profile.
fn validate(
    before: &Subnets,
    after: &Subnets,
    live_wan: Vec<Ipv4Net>,
    reservations: &[Ipv4Addr],
) -> Result<(), Error> {
    let wan_changed = before.static_wan != after.static_wan;
    let wan = if wan_changed {
        after.static_wan.into_iter().collect()
    } else {
        after.wan(live_wan)
    };
    let block = after.block();
    for (interface, member) in &after.profiles {
        let subnet = slash24(member.gateway);
        let old = before.profiles.get(interface).map(|m| m.gateway);
        let moved = old != Some(member.gateway);
        if moved {
            if interface == LAN_INTERFACE {
                crate::lan::validate_lan_block(member.gateway)?;
            } else if let Some([a, b]) = block {
                if member.gateway.octets()[..2] != [a, b] {
                    return Err(invalid(format!(
                        "profile subnet {} is outside the LAN network block {a}.{b}.0.0/16",
                        member.gateway
                    )));
                }
            }
            if let Some((_, other)) = after
                .profiles
                .iter()
                .find(|(i, m)| *i != interface && slash24(m.gateway) == subnet)
            {
                return Err(Error::new(
                    eyre!(
                        "subnet {subnet} is already in use by profile '{}'",
                        other.fullname
                    ),
                    ErrorKind::SubnetCollision,
                ));
            }
        }
        if moved || wan_changed {
            if let Some(w) = wan.iter().find(|w| overlap(**w, subnet)) {
                return Err(invalid(if wan_changed {
                    format!(
                        "the WAN network {} overlaps profile '{}' at {subnet}",
                        w.trunc(),
                        member.fullname
                    )
                } else {
                    format!(
                        "subnet {subnet} overlaps the upstream network {}",
                        w.trunc()
                    )
                }));
            }
        }
        if let Some(old) = old.map(slash24).filter(|old| *old != subnet) {
            let stranded: Vec<String> = reservations
                .iter()
                .filter(|ip| old.contains(*ip))
                .map(ToString::to_string)
                .collect();
            if !stranded.is_empty() {
                return Err(Error::new(
                    eyre!(
                        "cannot change subnet while devices have static IP reservations: {stranded:?}"
                    ),
                    ErrorKind::DhcpStaticHostsInSubnet,
                ));
            }
        }
    }
    Ok(())
}

/// [`validate`] against the configs about to be dumped.
pub(crate) fn check(ctx: &impl CtrlContext, before: &Subnets, cfgs: &Configs) -> Result<(), Error> {
    let reservations: Vec<Ipv4Addr> = cfgs["dhcp"]
        .sections
        .iter()
        .filter_map(|s| s.get::<DhcpHost>().ok()?.ip?.parse().ok())
        .collect();
    validate(
        before,
        &Subnets::of(cfgs),
        ctx.wan_ipv4_networks(),
        &reservations,
    )
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

pub async fn overlaps<C: CtrlContext>(ctx: C) -> Result<Vec<WanOverlap>, Error> {
    let live = ctx.wan_ipv4_networks();
    let arena = Arena::new();
    let cfgs = parse_all(ctx.uci_root(), &arena, &["startwrt", "network"]).await?;
    let subnets = Subnets::of(&cfgs);
    let wan = subnets.wan(live);
    Ok(subnets
        .overlapping(&wan)
        .map(|(interface, member, wan)| WanOverlap {
            profile: member.fullname.clone(),
            interface: interface.clone(),
            subnet: slash24(member.gateway).to_string(),
            wan_network: wan.to_string(),
        })
        .collect())
}

/// A Wi-Fi password, a LAN port, or an inbound VPN server leads to the profile.
fn has_point_of_entry(cfgs: &Configs, interface: &str, vlan_tag: u16) -> bool {
    let wifi = cfgs["wireless"].sections.iter().any(|s| {
        s.get::<WifiStation>()
            .is_ok_and(|station| station.vid == Some(vlan_tag))
    });
    let port = cfgs["network"].sections.iter().any(|s| {
        s.get::<NetworkBridgeVlan>()
            .is_ok_and(|vlan| vlan.vlan == vlan_tag && !vlan.ports.is_empty())
    });
    let vpn = format!("wg_{interface}");
    let vpn = cfgs["network"]
        .sections
        .iter()
        .any(|s| s.ty() == "interface" && s.name().as_deref() == Some(vpn.as_str()));
    wifi || port || vpn
}

/// Moves each overlapping profile whose clients could not reach the dashboard to
/// see a warning, the LAN owner and profiles nothing leads to, to a free subnet.
#[instrument(skip_all)]
pub async fn resolve<C: CtrlContext>(ctx: C) -> Result<(), Error> {
    let live = ctx.wan_ipv4_networks();
    let moves = {
        let arena = Arena::new();
        let cfgs = parse_all(ctx.uci_root(), &arena, &["startwrt", "network", "wireless"]).await?;
        let mut subnets = Subnets::of(&cfgs);
        let wan = subnets.wan(live);
        let found: Vec<(String, Member, Ipv4Net)> = subnets
            .overlapping(&wan)
            .filter(|(interface, member, _)| {
                interface.as_str() == LAN_INTERFACE
                    || !has_point_of_entry(&cfgs, interface, member.vlan_tag)
            })
            .map(|(i, m, w)| (i.clone(), m.clone(), w))
            .collect();
        let mut moves = Vec::new();
        for (interface, member, wan_network) in found {
            let Some(gateway) = subnets.free_gateway(&wan, []) else {
                tracing::warn!("no free subnet for profile {}", member.fullname);
                continue;
            };
            subnets.claim(&interface, gateway);
            moves.push((interface, member, wan_network, gateway));
        }
        moves
    };
    for (interface, member, wan_network, gateway) in moves {
        let mut profile: Profile<ProfileIdOpt> = profiles::get(
            ctx.clone(),
            ProfileIdOpt {
                fullname: None,
                interface: Some(interface),
                vlan_tag: None,
            },
        )
        .await?
        .into();
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
                "Moved profile '{}' from {} to {}, off the upstream network {wan_network}",
                member.fullname,
                slash24(member.gateway),
                slash24(gateway),
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
    async fn a_static_wan_on_a_profile_subnet_is_refused() {
        let dir = router_with(&[("Guest", 2)]).await;
        let ctx = TestContext(dir.path().into(), Vec::new());
        let set = |address: &str| {
            crate::wan::ipv4_set(
                ctx.clone(),
                DeserializeStdin(crate::wan::WanIpv4SetRequest {
                    mode: crate::wan::WanIpv4Mode::Static,
                    address: Some(address.into()),
                    netmask: Some("255.255.255.0".into()),
                    gateway: Some("192.168.2.254".into()),
                    username: None,
                    password: None,
                    device: None,
                }),
            )
        };
        let err = set("192.168.2.50").await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidRequest);
        set("203.0.113.9").await.unwrap();
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

    fn subnets(profiles: &[(&str, [u8; 4])], static_wan: Option<&str>) -> Subnets {
        Subnets {
            profiles: profiles
                .iter()
                .enumerate()
                .map(|(i, (interface, gateway))| {
                    (
                        (*interface).into(),
                        Member {
                            fullname: interface.to_uppercase(),
                            vlan_tag: 100 + i as u16,
                            gateway: Ipv4Addr::from(*gateway),
                        },
                    )
                })
                .collect(),
            static_wan: static_wan.map(net),
        }
    }

    fn kind(result: Result<(), Error>) -> Option<ErrorKind> {
        result.err().map(|e| e.kind)
    }

    #[test]
    fn overlap_follows_the_shorter_prefix() {
        assert!(overlap(net("192.168.0.50/24"), net("192.168.0.1/24")));
        assert!(overlap(net("192.168.0.1/16"), net("192.168.7.1/24")));
        assert!(overlap(net("192.168.7.1/24"), net("192.168.0.1/16")));
        assert!(!overlap(net("192.168.1.50/24"), net("192.168.0.1/24")));
        assert!(!overlap(net("10.0.0.2/30"), net("10.0.0.5/30")));
        assert!(overlap(net("100.64.3.9/10"), net("100.127.0.1/24")));
    }

    #[test]
    fn a_free_gateway_skips_taken_and_upstream_subnets() {
        let s = subnets(
            &[
                ("lan", [192, 168, 0, 1]),
                ("a", [192, 168, 10, 1]),
                ("b", [192, 168, 11, 1]),
            ],
            None,
        );
        assert_eq!(
            s.free_gateway(&[net("192.168.12.1/24")], []),
            Some(Ipv4Addr::new(192, 168, 13, 1))
        );
        assert_eq!(
            s.free_gateway(&[net("192.168.2.1/24")], [2, 3]),
            Some(Ipv4Addr::new(192, 168, 3, 1))
        );
        assert_eq!(s.free_gateway(&[net("192.168.0.0/16")], [2]), None);
    }

    #[test]
    fn a_moved_subnet_must_stay_in_the_block() {
        let before = subnets(&[("lan", [10, 42, 1, 1])], None);
        let ok = subnets(&[("lan", [10, 42, 1, 1]), ("guest", [10, 42, 5, 1])], None);
        assert_eq!(kind(validate(&before, &ok, Vec::new(), &[])), None);
        let out = subnets(
            &[("lan", [10, 42, 1, 1]), ("guest", [192, 168, 5, 1])],
            None,
        );
        assert_eq!(
            kind(validate(&before, &out, Vec::new(), &[])),
            Some(ErrorKind::InvalidRequest)
        );
        let public = subnets(&[("lan", [8, 8, 1, 1])], None);
        assert_eq!(
            kind(validate(&before, &public, Vec::new(), &[])),
            Some(ErrorKind::InvalidRequest)
        );
    }

    #[test]
    fn a_moved_subnet_must_not_share_a_slash24() {
        let before = subnets(&[("lan", [10, 0, 0, 1]), ("guest", [10, 0, 5, 1])], None);
        for after in [
            subnets(
                &[
                    ("lan", [10, 0, 0, 1]),
                    ("guest", [10, 0, 5, 1]),
                    ("new", [10, 0, 5, 1]),
                ],
                None,
            ),
            subnets(
                &[
                    ("lan", [10, 0, 0, 1]),
                    ("guest", [10, 0, 5, 1]),
                    ("new", [10, 0, 0, 1]),
                ],
                None,
            ),
            subnets(&[("lan", [10, 0, 5, 1]), ("guest", [10, 0, 5, 1])], None),
        ] {
            assert_eq!(
                kind(validate(&before, &after, Vec::new(), &[])),
                Some(ErrorKind::SubnetCollision)
            );
        }
        let free = subnets(
            &[
                ("lan", [10, 0, 0, 1]),
                ("guest", [10, 0, 5, 1]),
                ("new", [10, 0, 9, 1]),
            ],
            None,
        );
        assert_eq!(kind(validate(&before, &free, Vec::new(), &[])), None);
        assert_eq!(kind(validate(&before, &before, Vec::new(), &[])), None);
    }

    #[test]
    fn an_existing_conflict_does_not_block_an_unrelated_save() {
        let broken = subnets(&[("lan", [10, 0, 15, 1]), ("guest", [10, 0, 15, 1])], None);
        let wan = vec![net("10.0.15.37/24")];
        assert_eq!(kind(validate(&broken, &broken, wan.clone(), &[])), None);
        let recovered = subnets(&[("lan", [10, 0, 20, 1]), ("guest", [10, 0, 15, 1])], None);
        assert_eq!(kind(validate(&broken, &recovered, wan, &[])), None);
    }

    #[test]
    fn a_moved_subnet_must_clear_the_wan() {
        let before = subnets(&[("lan", [192, 168, 0, 1])], None);
        let after = subnets(
            &[("lan", [192, 168, 0, 1]), ("guest", [192, 168, 2, 1])],
            None,
        );
        assert_eq!(
            kind(validate(&before, &after, vec![net("192.168.2.37/24")], &[])),
            Some(ErrorKind::InvalidRequest)
        );
        assert_eq!(
            kind(validate(&before, &after, vec![net("192.168.3.37/24")], &[])),
            None
        );
    }

    #[test]
    fn a_static_wan_must_clear_every_profile() {
        let before = subnets(
            &[("lan", [192, 168, 0, 1]), ("guest", [192, 168, 2, 1])],
            None,
        );
        let onto_guest = subnets(
            &[("lan", [192, 168, 0, 1]), ("guest", [192, 168, 2, 1])],
            Some("192.168.2.50/24"),
        );
        assert_eq!(
            kind(validate(&before, &onto_guest, Vec::new(), &[])),
            Some(ErrorKind::InvalidRequest)
        );
        let clear = subnets(
            &[("lan", [192, 168, 0, 1]), ("guest", [192, 168, 2, 1])],
            Some("203.0.113.9/24"),
        );
        assert_eq!(
            kind(validate(&before, &clear, vec![net("192.168.2.37/24")], &[])),
            None
        );
    }

    #[test]
    fn a_moved_subnet_must_not_strand_a_reservation() {
        let before = subnets(
            &[("lan", [192, 168, 0, 1]), ("guest", [192, 168, 2, 1])],
            None,
        );
        let after = subnets(
            &[("lan", [192, 168, 0, 1]), ("guest", [192, 168, 9, 1])],
            None,
        );
        assert_eq!(
            kind(validate(
                &before,
                &after,
                Vec::new(),
                &[Ipv4Addr::new(192, 168, 2, 40)]
            )),
            Some(ErrorKind::DhcpStaticHostsInSubnet)
        );
        assert_eq!(
            kind(validate(
                &before,
                &after,
                Vec::new(),
                &[Ipv4Addr::new(192, 168, 0, 40)]
            )),
            None
        );
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
}
