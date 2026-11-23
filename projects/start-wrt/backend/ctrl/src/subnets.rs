//! The subnets the router occupies: every profile's `/24` and the WAN's networks.
//!
//! A write that adds or moves a profile or LAN gateway snapshots [`Subnets`] right
//! after parsing and passes the result to [`check`] before dumping. Unchanged
//! subnets always pass.
//!
//! A profile may overlap the WAN's network; [`overlaps`] reports it.
//! [`crate::wan::WAN_ROUTE_METRIC`] keeps the profile's route ahead of an equal
//! or wider WAN route, so its devices keep the router and the router loses that
//! part of the upstream network.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use ipnet::Ipv4Net;
use serde::{Deserialize, Serialize};
use uciedit::openwrt::{DhcpHost, InterfaceProto, NetworkInterface};
use uciedit::{parse_all, Arena, Configs};

use crate::lan::LAN_INTERFACE;
use crate::prelude::*;
use crate::profiles::UciProfile;
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
}

fn invalid(message: String) -> Error {
    Error::new(eyre!("{message}"), ErrorKind::InvalidRequest)
}

/// Rejects a change from `before` to `after` that leaves any added or moved subnet
/// outside the block, on another profile's `/24`, or that strands a DHCP
/// reservation in the subnet it left.
fn validate(before: &Subnets, after: &Subnets, reservations: &[Ipv4Addr]) -> Result<(), Error> {
    let block = after.block();
    let block_changed = before.block() != block;
    for (interface, member) in &after.profiles {
        let subnet = slash24(member.gateway);
        let old = before.profiles.get(interface).map(|m| m.gateway);
        let moved = old.map(slash24) != Some(subnet);
        if moved || block_changed {
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
        }
        if moved {
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
pub(crate) fn check(before: &Subnets, cfgs: &Configs) -> Result<(), Error> {
    let reservations: Vec<Ipv4Addr> = cfgs["dhcp"]
        .sections
        .iter()
        .filter_map(|s| s.get::<DhcpHost>().ok()?.ip?.parse().ok())
        .collect();
    validate(before, &Subnets::of(cfgs), &reservations)
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use rpc_toolkit::Context;
    use tokio::runtime::Runtime;

    use super::*;
    use crate::profiles::{self, LanAccess, Profile, ProfileIdOpt, ProfileSetRequest, WanAccess};
    use crate::utils::DeserializeStdin;

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

    fn guest(gateway_ip: Ipv4Addr) -> Profile<ProfileIdOpt> {
        Profile {
            id: ProfileIdOpt {
                fullname: Some("Guest".into()),
                interface: None,
                vlan_tag: None,
            },
            gateway_ip,
            outbound: "wan".into(),
            lan_access: LanAccess::SameProfile,
            wan_access: WanAccess::All,
            dns_override: Vec::new(),
            dns_source: String::new(),
            access_to_new_profiles: false,
            owns_lan: false,
        }
    }

    async fn reported(ctx: TestContext) -> Vec<(String, String)> {
        overlaps(ctx)
            .await
            .unwrap()
            .into_iter()
            .map(|o| (o.profile, o.wan_network))
            .collect()
    }

    #[tokio::test]
    async fn a_new_subnet_on_the_upstream_network_is_saved_and_reported() {
        let dir = router_with(&[]).await;
        let ctx = TestContext(dir.path().into(), vec![net("192.168.5.37/24")]);

        profiles::create(
            ctx.clone(),
            DeserializeStdin(guest(Ipv4Addr::new(192, 168, 5, 1))),
        )
        .await
        .unwrap();

        assert_eq!(
            reported(ctx).await,
            [("Guest".into(), "192.168.5.0/24".into())]
        );
    }

    #[tokio::test]
    async fn a_lan_subnet_on_the_upstream_network_is_saved_and_reported() {
        let dir = router_with(&[]).await;
        let ctx = TestContext(dir.path().into(), vec![net("192.168.5.37/24")]);

        crate::lan::ipv4_set(
            ctx.clone(),
            DeserializeStdin(crate::lan::LanIpv4SetRequest {
                address: "192.168.5.1".into(),
                force: false,
            }),
        )
        .await
        .unwrap();

        assert_eq!(
            gateways(&ctx).await,
            [("Admin".into(), Ipv4Addr::new(192, 168, 5, 1))]
        );
        assert_eq!(
            reported(ctx).await,
            [("Admin".into(), "192.168.5.0/24".into())]
        );
    }

    #[tokio::test]
    async fn a_static_wan_on_a_profile_subnet_is_saved_and_reported() {
        let dir = router_with(&[("Guest", 2)]).await;
        let ctx = TestContext(dir.path().into(), Vec::new());

        crate::wan::ipv4_set(
            ctx.clone(),
            DeserializeStdin(crate::wan::WanIpv4SetRequest {
                mode: crate::wan::WanIpv4Mode::Static,
                address: Some("192.168.2.50".into()),
                netmask: Some("255.255.255.0".into()),
                gateway: Some("192.168.2.254".into()),
                username: None,
                password: None,
                device: None,
            }),
        )
        .await
        .unwrap();

        assert_eq!(
            reported(ctx).await,
            [("Guest".into(), "192.168.2.0/24".into())]
        );
    }

    #[tokio::test]
    async fn lan_ownership_is_resolved_before_block_changes() {
        let dir = router_with(&[("Guest", 2)]).await;
        let ctx = TestContext(dir.path().into(), Vec::new());
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
        profile.owns_lan = false;
        profile.gateway_ip = "10.42.0.1".parse().unwrap();

        profiles::set(
            ctx.clone(),
            DeserializeStdin(ProfileSetRequest {
                profile,
                force: false,
            }),
        )
        .await
        .unwrap();

        assert_eq!(
            gateways(&ctx).await,
            [
                ("Admin".into(), Ipv4Addr::new(10, 42, 0, 1)),
                ("Guest".into(), Ipv4Addr::new(10, 42, 2, 1)),
            ]
        );
    }

    fn net(s: &str) -> Ipv4Net {
        s.parse().unwrap()
    }

    fn subnets(profiles: &[(&str, [u8; 4])], static_wan: Option<&str>) -> Subnets {
        Subnets {
            profiles: profiles
                .iter()
                .map(|(interface, gateway)| {
                    (
                        (*interface).into(),
                        Member {
                            fullname: interface.to_uppercase(),
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
    fn a_moved_subnet_must_stay_in_the_block() {
        let before = subnets(&[("lan", [10, 42, 1, 1])], None);
        let ok = subnets(&[("lan", [10, 42, 1, 1]), ("guest", [10, 42, 5, 1])], None);
        assert_eq!(kind(validate(&before, &ok, &[])), None);
        let out = subnets(
            &[("lan", [10, 42, 1, 1]), ("guest", [192, 168, 5, 1])],
            None,
        );
        assert_eq!(
            kind(validate(&before, &out, &[])),
            Some(ErrorKind::InvalidRequest)
        );
        let public = subnets(&[("lan", [8, 8, 1, 1])], None);
        assert_eq!(
            kind(validate(&before, &public, &[])),
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
                kind(validate(&before, &after, &[])),
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
        assert_eq!(kind(validate(&before, &free, &[])), None);
        assert_eq!(kind(validate(&before, &before, &[])), None);
    }

    #[test]
    fn an_existing_conflict_does_not_block_an_unrelated_save() {
        let broken = subnets(&[("lan", [10, 0, 15, 1]), ("guest", [10, 0, 15, 1])], None);
        assert_eq!(kind(validate(&broken, &broken, &[])), None);
        let recovered = subnets(&[("lan", [10, 0, 20, 1]), ("guest", [10, 0, 15, 1])], None);
        assert_eq!(kind(validate(&broken, &recovered, &[])), None);
    }

    #[test]
    fn a_block_change_must_take_every_profile_with_it() {
        let before = subnets(
            &[("lan", [192, 168, 0, 1]), ("guest", [192, 168, 2, 1])],
            None,
        );
        let stranded = subnets(
            &[("lan", [10, 42, 0, 1]), ("guest", [192, 168, 2, 1])],
            None,
        );
        assert_eq!(
            kind(validate(&before, &stranded, &[])),
            Some(ErrorKind::InvalidRequest)
        );
        let moved = subnets(&[("lan", [10, 42, 0, 1]), ("guest", [10, 42, 2, 1])], None);
        assert_eq!(kind(validate(&before, &moved, &[])), None);
    }

    #[test]
    fn an_unchanged_subnet_allows_a_gateway_host_change() {
        let before = subnets(&[("lan", [10, 0, 15, 1]), ("guest", [10, 0, 15, 2])], None);
        let after = subnets(
            &[("lan", [10, 0, 15, 254]), ("guest", [10, 0, 15, 2])],
            None,
        );
        assert_eq!(kind(validate(&before, &after, &[])), None);
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
            kind(validate(&before, &after, &[Ipv4Addr::new(192, 168, 2, 40)])),
            Some(ErrorKind::DhcpStaticHostsInSubnet)
        );
        assert_eq!(
            kind(validate(&before, &after, &[Ipv4Addr::new(192, 168, 0, 40)])),
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
