use crate::profiles::{self, ProfileId, ProfileIdOpt};
use crate::utils::DeserializeStdin;
use crate::utils::HandlerExtSerde;
use crate::CtrlContext;
use crate::prelude::*;
use rpc_toolkit::{from_fn_async_local, ParentHandler};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use crate::invoke::Invoke;
use std::time::Duration;
use uciedit::openwrt::{
    DeviceType, InterfaceProto, NetworkBridgeVlan, NetworkDevice, NetworkInterface,
    NetworkVlanPort, NetworkVlanPortTagging,
};
use uciedit::{dump_all, parse_all, Arena, Configs};

pub const DEFAULT_LAN_BRIDGE: &str = "br-lan";

/// Find the LAN bridge device in the network config.
/// Prefers the device named `br-lan`; falls back to any bridge.
pub fn find_lan_bridge(cfgs: &Configs) -> Result<Option<NetworkDevice>, Error> {
    let mut found = None;
    cfgs["network"].try_each(|_, dev: NetworkDevice| {
        if dev.ty == Some(DeviceType::BRIDGE)
            && (found.is_none() || dev.name == DEFAULT_LAN_BRIDGE)
        {
            found = Some(dev);
        }
        Ok::<_, Error>(())
    })?;
    Ok(found)
}

pub const DEFAULT_WAN_INTERFACE: &str = "wan";
pub const DEFAULT_WAN6_INTERFACE: &str = "wan6";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Port<Id: Ord = ProfileId> {
    pub profile: Option<Id>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ethernet<Id: Ord = ProfileId> {
    pub wan_ipv6: bool,
    pub wan_port: Option<String>,
    pub ports: BTreeMap<String, Port<Id>>,
}

/// `ethernet.set` request: the desired port layout plus a confirmation flag.
/// Reassigning a port to a different profile moves its devices to a new subnet,
/// which breaks any published ports forwarding to them. When the flag is false
/// and such ports exist, `set` applies nothing and returns them in
/// `EthernetSetResult` for a confirmation dialog; with the flag true it deletes
/// those published ports as part of the reassignment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthernetSetRequest {
    #[serde(flatten)]
    pub ethernet: Ethernet<ProfileIdOpt>,
    #[serde(default)]
    pub confirm_published_port_deletion: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EthernetSetResult {
    /// Non-empty (and nothing applied) when published ports would be deleted and
    /// the caller hasn't confirmed yet. Empty once the change is applied.
    pub pending_published_port_deletions: Vec<crate::published_ports::AffectedPublishedPort>,
}

/// Names of ports whose profile (VLAN) assignment is changing: present in
/// `old_ports` but resolving to a different VLAN than `requested`. Newly added
/// ports (absent from `old_ports`) are excluded — they have no existing devices
/// to move off a subnet.
fn changed_vlan_ports(
    requested: &BTreeMap<String, Port<ProfileIdOpt>>,
    old_ports: &HashMap<String, Option<u16>>,
    lookup: &profiles::Lookup,
) -> Result<HashSet<String>, Error> {
    let mut changed = HashSet::new();
    for (name, port) in requested {
        let new_vlan = match &port.profile {
            Some(id) => Some(lookup.resolve(id)?.vlan_tag),
            None => None,
        };
        let old_vlan = old_ports.get(name).copied().flatten();
        if old_ports.contains_key(name) && old_vlan != new_vlan {
            changed.insert(name.clone());
        }
    }
    Ok(changed)
}

pub fn ethernet<C: CtrlContext + Clone>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand("get", from_fn_async_local(get::<C>).with_display_serializable())
        .subcommand("set", from_fn_async_local(set::<C>).with_display_serializable())
        .subcommand("edit", from_fn_async_local(edit::<C>).with_display_serializable())
}

#[instrument(skip_all)]
pub async fn get<C: CtrlContext>(ctx: C) -> Result<Ethernet, Error> {
    let arena = Arena::new();
    let cfgs = parse_all(ctx.uci_root(), &arena, &["network", "startwrt", "firewall"]).await?;
    get_config(ctx, &cfgs)
}

fn get_config(ctx: impl CtrlContext, cfgs: &Configs) -> Result<Ethernet, Error> {
    let lookup = profiles::Lookup::parse(ctx.clone(), cfgs)?;
    let found_bridge = find_lan_bridge(cfgs)?.ok_or_else(|| Error::new(eyre!("missing LAN bridge"), ErrorKind::MissingLanBridge))?;

    let mut wan_ipv6 = false;
    let mut wan_port = None;
    let all_ports: HashSet<String> = found_bridge
        .ports
        .into_iter()
        .filter(|p| !p.starts_with("wlan") && !p.starts_with("phy"))
        .collect();
    let mut vlan_ports = HashMap::new();
    for section in &cfgs["network"].sections {
        if let Some(vlan) = section.get_typed::<NetworkBridgeVlan>()? {
            for port in vlan.ports {
                match port.tagging {
                    None | Some(NetworkVlanPortTagging::TAGGED) => (),
                    Some(NetworkVlanPortTagging::UNTAGGED) => {
                        vlan_ports.entry(port.port).or_insert((vlan.vlan, false));
                    }
                    Some(NetworkVlanPortTagging::PRIMARY) => {
                        let entry = vlan_ports.entry(port.port).or_insert((vlan.vlan, true));
                        if !entry.1 {
                            entry.0 = vlan.vlan; // primary overrides untagged
                        }
                    }
                }
            }
        }
        if let Some(iface) = section.get_typed::<NetworkInterface>()? {
            if section.name().as_deref() == Some(DEFAULT_WAN_INTERFACE) {
                wan_port = Some(iface.device.clone());
            }
        }
    }
    let mut ports: BTreeMap<String, Port> = all_ports
        .iter()
        .map(|name| {
            (
                name.clone(),
                Port {
                    profile: vlan_ports
                        .get(name)
                        .and_then(|(tag, _)| lookup.from_vlan(*tag))
                        .cloned(),
                },
            )
        })
        .collect();
    if let Some(ref wp) = wan_port {
        ports.entry(wp.clone()).or_insert(Port { profile: None });
    }
    for section in &cfgs["network"].sections {
        if section.name().as_deref() == Some(DEFAULT_WAN6_INTERFACE) {
            if let Some(iface) = section.get_typed::<NetworkInterface>()? {
                if iface.proto == InterfaceProto::DHCPV6 {
                    wan_ipv6 = true;
                }
            }
            break;
        }
    }
    Ok(Ethernet {
        wan_ipv6,
        wan_port,
        ports,
    })
}

/// IEEE 802.3 Clause 28 `break_link_timer` requires 1200–1500 ms of link pulse
/// suppression for the link partner to reliably detect carrier loss.  We use 2 s
/// to provide margin above the 1.5 s maximum.
const PORT_BOUNCE_DOWN_SECS: u64 = 2;

#[instrument(skip_all)]
pub async fn set<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(req): DeserializeStdin<EthernetSetRequest>,
) -> Result<EthernetSetResult, Error> {
    let EthernetSetRequest {
        ethernet,
        confirm_published_port_deletion: confirm,
    } = req;

    // Snapshot current port→profile mapping so we can detect changes after write.
    let old_ports = if ctx.effectful() {
        get(ctx.clone())
            .await
            .ok()
            .map(|e| {
                e.ports
                    .into_iter()
                    .map(|(name, port)| (name, port.profile.map(|p| p.vlan_tag)))
                    .collect::<HashMap<String, Option<u16>>>()
            })
            .unwrap_or_default()
    } else {
        HashMap::new()
    };

    // Devices behind any port whose profile (VLAN) is changing will move to a new
    // subnet, breaking their published ports. Find those ports' MACs (bridge FDB
    // is port-precise) and the published ports they'd break.
    let (affected_macs, changed_ports): (HashSet<String>, HashSet<String>) =
        if ctx.effectful() {
            let arena = Arena::new();
            let cfgs =
                parse_all(ctx.uci_root(), &arena, &["network", "startwrt", "firewall"]).await?;
            let lookup = profiles::Lookup::parse(ctx.clone(), &cfgs)?;
            let changed_ports = changed_vlan_ports(&ethernet.ports, &old_ports, &lookup)?;
            let macs = crate::devices::get_bridge_fdb()
                .await
                .into_iter()
                .filter(|(_, port)| changed_ports.contains(port))
                .map(|(mac, _)| mac)
                .collect();
            (macs, changed_ports)
        } else {
            (HashSet::new(), HashSet::new())
        };

    if ctx.effectful() && !confirm {
        // Dry run: report what would be deleted and apply nothing. The UI calls
        // again with `confirm_published_port_deletion: true` (through its
        // network-restart flow) to actually apply. The list may be empty, which
        // just tells the caller there's nothing to confirm.
        let affected = crate::published_ports::affected_ports_for_macs(
            ctx.uci_root().as_path(),
            &affected_macs,
            &HashMap::new(),
        )
        .await?;
        return Ok(EthernetSetResult {
            pending_published_port_deletions: affected,
        });
    }

    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs =
            parse_all(ctx.uci_root(), &arena, &["network", "startwrt", "firewall", "dhcp"]).await?;
        let lookup = profiles::Lookup::parse(ctx.clone(), &cfgs)?;
        let ethernet = Ethernet::<ProfileId> {
            wan_ipv6: ethernet.wan_ipv6,
            wan_port: ethernet.wan_port.clone(),
            ports: ethernet
                .ports
                .iter()
                .map(|(k, v)| {
                    Ok((
                        k.clone(),
                        Port {
                            profile: match &v.profile {
                                Some(id) => Some(lookup.resolve(id)?.clone()),
                                None => None,
                            },
                        },
                    ))
                })
                .collect::<Result<_, Error>>()?,
        };
        if let Err(err) = set_config(&ctx, &mut cfgs, &ethernet, &lookup) {
            crate::activity::log("ethernet", "updated", false, "Failed to update Ethernet port assignments", Some(&err.to_string()));
            return Err(err);
        }
        // Confirmed path: delete the published ports (and stale reservations) for
        // devices being moved off their subnet, atomically with the reassignment.
        if !affected_macs.is_empty() {
            crate::published_ports::remove_ports_for_macs(&mut cfgs, &affected_macs);
        }
        match dump_all(ctx.uci_root(), cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log("ethernet", "updated", false, "Failed to update Ethernet port assignments", Some(&err.to_string()));
                return Err(err.into());
            }
            Ok(()) => {
                let changed: Vec<&str> = ethernet
                    .ports
                    .keys()
                    .filter(|name| changed_ports.contains(name.as_str()))
                    .map(|s| s.as_str())
                    .collect();
                let summary = if changed.is_empty() {
                    "Updated Ethernet port assignments".to_string()
                } else {
                    format!("Updated Ethernet port assignments (changed: {})", changed.join(", "))
                };
                crate::activity::log("ethernet", "updated", true, &summary, None);

                if ctx.effectful() {
                    // Spawn the reload sequence so the response returns
                    // before the network disruption begins.  This endpoint
                    // differs from the typical inline pattern (lan, wan,
                    // profiles) because set_config rewrites every
                    // bridge-vlan section on br-lan: netifd then tears
                    // down and rebuilds the entire bridge VLAN filter
                    // table.  During that window wlan0's VLAN 1
                    // membership briefly disappears and packets between
                    // WiFi clients and br-lan are dropped — including
                    // this RPC's response if we blocked here.
                    //
                    // `network reload` is sufficient and correct: netifd
                    // reprograms an existing bridge member's VLAN/PVID live
                    // via RTM_SETLINK/RTM_DELLINK netlink (no teardown, no
                    // reboot).  odhcpd re-binds automatically (it has a
                    // `network` procd reload trigger); dnsmasq normally needs
                    // nothing because moving eth0 between VLANs doesn't recreate
                    // the per-profile sub-interfaces it's bound to — but when we
                    // also removed published-port DHCP reservations
                    // (`dhcp_changed`) it must reload to drop them.
                    //
                    // `firewall reload` (NOT `restart`): a port-VLAN move
                    // doesn't change any zone↔interface binding, so the fw4
                    // ruleset is still valid and only needs an incremental
                    // refresh.  `firewall restart` flushes the whole table
                    // and rebuilds from zero — under the default `input
                    // REJECT` policy that opens a window where management
                    // traffic is dropped, which (racing netifd's bridge
                    // work) intermittently locked out all access until a
                    // manual reboot.
                    //
                    // `wifi reload` AFTER the network reload: the AP netdevs
                    // are attached to br-lan at runtime by hostapd, not by
                    // netifd, and they never appear in the `network` bridge
                    // config — so `network reload` rebuilds the bridge VLAN
                    // table without them and they lose their VLAN membership.
                    // Re-running `wifi` re-attaches them so they pick up
                    // their PVID once the VLANs exist again (this mirrors
                    // `lan::restart_network_services` and `profiles`).  It
                    // doesn't flush the firewall or cycle br-lan/ethernet, so
                    // it can't re-trigger the lockout above; WiFi clients
                    // just briefly reassociate.  Run sequentially so it
                    // doesn't race the network reload.
                    let dhcp_changed = !affected_macs.is_empty();
                    tokio::spawn(async move {
                        let _ = crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/network").arg("reload")).await;
                        let _ = crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/firewall").arg("reload")).await;
                        let _ = crate::run_quiet_async(&mut tokio::process::Command::new("wifi")).await;
                        if dhcp_changed {
                            crate::published_ports::reload_dnsmasq();
                            // odhcpd owns IPv6 leases/hostids; reload so a cleared
                            // or preserved reservation takes runtime effect.
                            crate::published_ports::reload_odhcpd();
                        }
                        bounce_changed_ports(&old_ports, &ethernet).await;
                    });
                }
                return Ok(EthernetSetResult::default());
            }
        }
    }
}

/// Bounce (link down/up) any ports whose VLAN assignment changed, so that
/// connected clients detect carrier loss and re-run DHCP on the new subnet.
async fn bounce_changed_ports(
    old_ports: &HashMap<String, Option<u16>>,
    new_ethernet: &Ethernet,
) {
    let mut to_bounce = Vec::new();
    for (name, new_port) in &new_ethernet.ports {
        let new_vlan = new_port.profile.as_ref().map(|p| p.vlan_tag);
        let old_vlan = old_ports.get(name).copied().flatten();
        // Only bounce if the port existed before and its VLAN actually changed.
        // Skip the WAN port — it's not a bridge member.
        if old_ports.contains_key(name)
            && old_vlan != new_vlan
            && new_ethernet.wan_port.as_ref() != Some(name)
        {
            to_bounce.push(name.clone());
        }
    }
    if to_bounce.is_empty() {
        return;
    }
    // Bring changed ports down
    for port in &to_bounce {
        let _ = tokio::process::Command::new("ip")
            .args(["link", "set", port, "down"])
            .capture(false)
            .invoke(ErrorKind::Network.into())
            .await;
    }
    // IEEE 802.3 break_link_timer: hold down for ≥1.5 s
    tokio::time::sleep(Duration::from_secs(PORT_BOUNCE_DOWN_SECS)).await;
    // Bring them back up
    for port in &to_bounce {
        let _ = tokio::process::Command::new("ip")
            .args(["link", "set", port, "up"])
            .capture(false)
            .invoke(ErrorKind::Network.into())
            .await;
    }
}

fn set_config(
    _ctx: &impl CtrlContext,
    cfgs: &mut Configs,
    ethernet: &Ethernet,
    lookup: &profiles::Lookup,
) -> Result<(), Error> {
    let mut bridge = match find_lan_bridge(cfgs)? {
        Some(br) => br,
        None => NetworkDevice {
            name: DEFAULT_LAN_BRIDGE.into(),
            ty: Some(DeviceType::BRIDGE),
            ports: Vec::new(),
            macaddr: None,
        },
    };
    // Save non-ethernet ports (WiFi, etc.) that the bridge already has
    let non_ethernet_ports: Vec<String> = bridge
        .ports
        .iter()
        .filter(|p| p.starts_with("wlan") || p.starts_with("phy"))
        .cloned()
        .collect();

    bridge.ports.clear();
    for (port_name, port) in &ethernet.ports {
        if Some(port_name) == ethernet.wan_port.as_ref() {
            if port.profile.is_some() {
                return Err(Error::new(eyre!("WAN port cannot have profile: {port_name}"), ErrorKind::WanPortWithProfile));
            }
        } else {
            bridge.ports.push(port_name.clone());
        }
    }

    // Re-add non-ethernet ports (WiFi, etc.)
    bridge.ports.extend(non_ethernet_ports.iter().cloned());

    let mut pending_bridge = true;
    let mut pending_ipv4 = ethernet.wan_port.is_some();
    let mut pending_ipv6 = ethernet.wan_ipv6 && ethernet.wan_port.is_some();

    // Remove vlans for this bridge, and update interfaces/devices
    cfgs["network"].sections.retain(|section| {
        if let Some(vlan) = section.get::<NetworkBridgeVlan>().ok() {
            if vlan.device == bridge.name {
                return false; // we'll re-add these later
            }
        }
        true
    });

    // Update WAN interfaces, if they exist
    for section in &mut cfgs["network"].sections {
        if let Some(dev) = section.get_typed::<NetworkDevice>()? {
            if dev.ty == Some(DeviceType::BRIDGE) && dev.name == bridge.name {
                if dev.ports != bridge.ports {
                    section.set(&bridge)?;
                }
                pending_bridge = false;
            }
        }
        if let Some(mut iface) = section.get_typed::<NetworkInterface>()? {
            if section.name().as_deref() == Some(DEFAULT_WAN_INTERFACE) {
                pending_ipv4 = false;
                if let Some(wan_port) = &ethernet.wan_port {
                    iface.device = wan_port.clone();
                    section.set(&iface)?;
                }
            }
            if iface.proto == InterfaceProto::DHCPV6
                && section.name().as_deref() == Some(DEFAULT_WAN6_INTERFACE)
            {
                pending_ipv6 = false;
                if let Some(wan_port) = &ethernet.wan_port {
                    if ethernet.wan_ipv6 {
                        iface.device = wan_port.clone();
                        section.set(&iface)?;
                    }
                }
            }
        }
    }

    // Remove WAN interfaces that are no longer needed
    cfgs["network"].sections.retain(|section| {
        if let Ok(iface) = section.get::<NetworkInterface>() {
            if section.name().as_deref() == Some(DEFAULT_WAN_INTERFACE)
                && ethernet.wan_port.is_none()
            {
                return false;
            }
            if iface.proto == InterfaceProto::DHCPV6
                && section.name().as_deref() == Some(DEFAULT_WAN6_INTERFACE)
                && (ethernet.wan_port.is_none() || !ethernet.wan_ipv6)
            {
                return false;
            }
        }
        true
    });

    // Add WAN interfaces if still needed
    if let Some(wan_port) = &ethernet.wan_port {
        if pending_ipv4 {
            cfgs["network"].append(
                &NetworkInterface {
                    device: wan_port.clone(),
                    proto: InterfaceProto::DHCP,
                    ..Default::default()
                },
                Some(DEFAULT_WAN_INTERFACE),
            )?;
        }
        if pending_ipv6 {
            cfgs["network"].append(
                &NetworkInterface {
                    device: wan_port.clone(),
                    proto: InterfaceProto::DHCPV6,
                    ..Default::default()
                },
                Some(DEFAULT_WAN6_INTERFACE),
            )?;
        }
    }

    if pending_bridge {
        cfgs["network"].append(&bridge, None)?;
    }

    let needs_vlan_filtering = lookup.list().iter().any(|p| p.vlan_tag != 1);

    for profile in lookup.list() {
        let mut ports = Vec::new();
        for (port_name, port) in &ethernet.ports {
            if Some(port_name) == ethernet.wan_port.as_ref() {
                continue;
            }
            let assigned = port.profile.as_ref() == Some(profile);
            let default_to_admin = needs_vlan_filtering
                && port.profile.is_none()
                && profile.vlan_tag == 1;
            if assigned || default_to_admin {
                ports.push(NetworkVlanPort {
                    port: port_name.clone(),
                    tagging: Some(NetworkVlanPortTagging::PRIMARY),
                });
            }
        }

        // Non-ethernet bridge ports (WiFi, etc.) need a VLAN assignment when
        // filtering is active, otherwise `network reload` ejects them from the
        // bridge.  Place them in VLAN 1 (the admin VLAN), matching the behavior
        // of ensure_vlan_filtering() and profiles::create_config().
        if profile.vlan_tag == 1 && needs_vlan_filtering {
            for port_name in &non_ethernet_ports {
                ports.push(NetworkVlanPort {
                    port: port_name.clone(),
                    tagging: Some(NetworkVlanPortTagging::PRIMARY),
                });
            }
        }

        let vlan = profile.vlan_tag;
        // VLAN filtering OFF (no profile has a tag != 1): br-lan is a plain
        // flat bridge — the `lan` L3 interface sits on `br-lan` directly, not
        // `br-lan.1`, and no bridge-vlan sections exist (the br-lan -> br-lan.1
        // flip + VID-1 creation happens only in profiles::ensure_vlan_filtering,
        // when the first non-admin profile is created). There is no Admin VLAN
        // to preserve here; emitting an empty VID-1 section would needlessly
        // turn the flat bridge into a filtering bridge with no member ports.
        // So skip it.
        //
        // VLAN filtering ON: do NOT skip. The (possibly empty) VID-1 section is
        // what keeps the Admin VLAN on the bridge, carrying the router's own
        // `br-lan.1` management interface and the default `network='lan'`
        // wireless SSID — the AP ports are attached at runtime by hostapd and
        // never appear in `ethernet.ports`. Dropping it there was the bug that
        // left WiFi/gateway with "No route to host"; hence the
        // `&& !needs_vlan_filtering` guard.
        if ports.is_empty() && vlan == 1 && !needs_vlan_filtering {
            continue;
        }
        cfgs["network"].append(
            &NetworkBridgeVlan {
                device: bridge.name.clone(),
                vlan,
                ports,
            },
            None,
        )?;
    }
    Ok(())
}

/// Convenience for tests: run set_config through configs already in memory.
#[cfg(test)]
fn set_from_config(
    ctx: &impl CtrlContext,
    cfgs: &mut Configs,
    ethernet: &Ethernet,
) -> Result<(), Error> {
    let lookup = profiles::Lookup::parse(ctx.clone(), cfgs)?;
    set_config(ctx, cfgs, ethernet, &lookup)
}

#[instrument(skip_all)]
pub async fn edit<C: CtrlContext + Clone>(ctx: C) -> Result<(), Error> {
    let current_ethernet = get(ctx.clone()).await?;
    let current_ethernet = Ethernet {
        wan_ipv6: current_ethernet.wan_ipv6,
        wan_port: current_ethernet.wan_port,
        ports: current_ethernet
            .ports
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    Port {
                        profile: v.profile.map(Into::into),
                    },
                )
            })
            .collect(),
    };
    let modified_ethernet = crate::utils::edit_in_editor(&current_ethernet)?;
    // CLI editor path has no dialog: confirm published-port cleanup implicitly.
    set(
        ctx,
        DeserializeStdin(EthernetSetRequest {
            ethernet: modified_ethernet,
            confirm_published_port_deletion: true,
        }),
    )
    .await
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rpc_toolkit::Context;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::runtime::Runtime;
    use uciedit::{parse_all, Arena};

    /// Test shim: the production `set` now takes an `EthernetSetRequest` and
    /// returns `EthernetSetResult`; these tests predate that and pass a bare
    /// `Ethernet`. Wrap it (no published-port confirmation needed — tests run
    /// non-effectfully) so the existing call sites stay unchanged.
    async fn set(
        ctx: TestContext,
        e: DeserializeStdin<Ethernet<ProfileIdOpt>>,
    ) -> Result<(), Error> {
        super::set(
            ctx,
            DeserializeStdin(super::EthernetSetRequest {
                ethernet: e.0,
                confirm_published_port_deletion: false,
            }),
        )
        .await
        .map(|_| ())
    }

    #[derive(Clone)]
    struct TestContext(PathBuf);

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
    }

    /// Two profiles: Admin (lan, vlan 1) and Guest (guest, vlan 3).
    /// Bridge with two ports: eth0, eth1. No WAN.
    fn setup_basic(dir: &std::path::Path) {
        std::fs::write(
            dir.join("startwrt"),
            "\
config profile lan
\toption fullname 'Admin'
\toption interface 'lan'
\toption vlan_tag '1'

config profile guest
\toption fullname 'Guest'
\toption interface 'guest'
\toption vlan_tag '3'
",
        )
        .unwrap();

        std::fs::write(
            dir.join("network"),
            "\
config device
\toption name 'br-lan'
\toption type 'bridge'
\tlist ports 'eth0'
\tlist ports 'eth1'

config bridge-vlan
\toption device 'br-lan'
\toption vlan '1'
\tlist ports 'eth0:u*'
\tlist ports 'eth1:u*'

config bridge-vlan
\toption device 'br-lan'
\toption vlan '3'

config interface 'lan'
\toption device 'br-lan.1'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
\toption netmask '255.255.255.0'

config interface 'guest'
\toption device 'br-lan.3'
\toption proto 'static'
\toption ipaddr '192.168.3.1'
\toption netmask '255.255.255.0'
",
        )
        .unwrap();

        std::fs::write(
            dir.join("firewall"),
            "\
config zone
\toption name 'lan'
\tlist network 'lan'
\toption input 'ACCEPT'
\toption output 'ACCEPT'
\toption forward 'ACCEPT'

config zone
\toption name 'vlan_guest'
\tlist network 'guest'
\toption input 'ACCEPT'
\toption output 'ACCEPT'
\toption forward 'ACCEPT'
",
        )
        .unwrap();
    }

    /// Adds a WAN interface on eth2 (third port) with optional IPv6.
    fn setup_with_wan(dir: &std::path::Path, ipv6: bool) {
        setup_basic(dir);
        // Rewrite network to include eth2 + WAN
        std::fs::write(
            dir.join("network"),
            format!(
                "\
config device
\toption name 'br-lan'
\toption type 'bridge'
\tlist ports 'eth0'
\tlist ports 'eth1'
\tlist ports 'eth2'

config bridge-vlan
\toption device 'br-lan'
\toption vlan '1'
\tlist ports 'eth0:u*'
\tlist ports 'eth1:u*'

config interface 'lan'
\toption device 'br-lan.1'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
\toption netmask '255.255.255.0'

config interface 'guest'
\toption device 'br-lan.3'
\toption proto 'static'
\toption ipaddr '192.168.3.1'
\toption netmask '255.255.255.0'

config interface 'wan'
\toption device 'eth2'
\toption proto 'dhcp'
{wan6}",
                wan6 = if ipv6 {
                    "\nconfig interface 'wan6'\n\toption device 'eth2'\n\toption proto 'dhcpv6'\n"
                } else {
                    ""
                }
            ),
        )
        .unwrap();
    }

    // ── get tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn get_returns_all_bridge_ports() {
        let dir = tempfile::tempdir().unwrap();
        setup_basic(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        let result = get(ctx).await.unwrap();
        assert_eq!(result.ports.len(), 2);
        assert!(result.ports.contains_key("eth0"));
        assert!(result.ports.contains_key("eth1"));
    }

    #[tokio::test]
    async fn get_resolves_profile_from_vlan() {
        let dir = tempfile::tempdir().unwrap();
        setup_basic(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        let result = get(ctx).await.unwrap();
        // Both ports are untagged+primary on VLAN 1 → Admin profile
        let eth0_profile = result.ports["eth0"].profile.as_ref().unwrap();
        assert_eq!(eth0_profile.interface, "lan");
        assert_eq!(eth0_profile.vlan_tag, 1);
    }

    #[tokio::test]
    async fn get_no_wan_when_no_wan_interface() {
        let dir = tempfile::tempdir().unwrap();
        setup_basic(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        let result = get(ctx).await.unwrap();
        assert!(result.wan_port.is_none());
        assert!(!result.wan_ipv6);
    }

    #[tokio::test]
    async fn get_detects_wan_port() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_wan(dir.path(), false);
        let ctx = TestContext(dir.path().to_path_buf());

        let result = get(ctx).await.unwrap();
        assert_eq!(result.wan_port.as_deref(), Some("eth2"));
        assert!(!result.wan_ipv6);
    }

    #[tokio::test]
    async fn get_detects_wan_ipv6() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_wan(dir.path(), true);
        let ctx = TestContext(dir.path().to_path_buf());

        let result = get(ctx).await.unwrap();
        assert_eq!(result.wan_port.as_deref(), Some("eth2"));
        assert!(result.wan_ipv6);
    }

    #[tokio::test]
    async fn get_detects_wan_ipv6_with_at_wan_alias() {
        // Real firstboot config uses `@wan` device alias, not the raw port name.
        // This must still be detected as wan_ipv6 = true.
        let dir = tempfile::tempdir().unwrap();
        setup_with_wan(dir.path(), false);
        // Rewrite with @wan alias instead of the direct device name
        let network = std::fs::read_to_string(dir.path().join("network")).unwrap();
        std::fs::write(
            dir.path().join("network"),
            format!(
                "{network}\nconfig interface 'wan6'\n\toption device '@wan'\n\toption proto 'dhcpv6'\n"
            ),
        )
        .unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        let result = get(ctx).await.unwrap();
        assert!(result.wan_ipv6, "wan_ipv6 should be true even with @wan device alias");
    }

    #[tokio::test]
    async fn get_errors_when_no_bridge() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("network"), "").unwrap();
        std::fs::write(dir.path().join("startwrt"), "").unwrap();
        std::fs::write(dir.path().join("firewall"), "").unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        let err = get(ctx).await.unwrap_err();
        assert!(
            err.to_string().contains("bridge"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn get_port_with_no_vlan_assignment_has_none_profile() {
        let dir = tempfile::tempdir().unwrap();
        setup_basic(dir.path());
        // Rewrite network: eth1 has no VLAN assignment (not in any bridge-vlan)
        std::fs::write(
            dir.path().join("network"),
            "\
config device
\toption name 'br-lan'
\toption type 'bridge'
\tlist ports 'eth0'
\tlist ports 'eth1'

config bridge-vlan
\toption device 'br-lan'
\toption vlan '1'
\tlist ports 'eth0:u*'

config interface 'lan'
\toption device 'br-lan.1'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
\toption netmask '255.255.255.0'
",
        )
        .unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        let result = get(ctx).await.unwrap();
        assert!(result.ports["eth0"].profile.is_some());
        assert!(result.ports["eth1"].profile.is_none());
    }

    #[tokio::test]
    async fn get_filters_wifi_interfaces() {
        let dir = tempfile::tempdir().unwrap();
        setup_basic(dir.path());
        // Add wlan0 to bridge ports
        std::fs::write(
            dir.path().join("network"),
            "\
config device
\toption name 'br-lan'
\toption type 'bridge'
\tlist ports 'eth0'
\tlist ports 'eth1'
\tlist ports 'wlan0'

config bridge-vlan
\toption device 'br-lan'
\toption vlan '1'
\tlist ports 'eth0:u*'
\tlist ports 'eth1:u*'

config interface 'lan'
\toption device 'br-lan.1'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
\toption netmask '255.255.255.0'
",
        )
        .unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        let result = get(ctx).await.unwrap();
        assert!(result.ports.contains_key("eth0"));
        assert!(result.ports.contains_key("eth1"));
        assert!(
            !result.ports.contains_key("wlan0"),
            "WiFi interfaces should be filtered from ethernet ports"
        );
    }

    #[tokio::test]
    async fn get_detects_wan_port_not_in_bridge() {
        let dir = tempfile::tempdir().unwrap();
        setup_basic(dir.path());
        // WAN on eth1, which is NOT in the bridge
        std::fs::write(
            dir.path().join("network"),
            "\
config device
\toption name 'br-lan'
\toption type 'bridge'
\tlist ports 'eth0'

config bridge-vlan
\toption device 'br-lan'
\toption vlan '1'
\tlist ports 'eth0:u*'

config interface 'lan'
\toption device 'br-lan.1'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
\toption netmask '255.255.255.0'

config interface 'wan'
\toption device 'eth1'
\toption proto 'dhcp'
",
        )
        .unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        let result = get(ctx).await.unwrap();
        assert_eq!(
            result.wan_port.as_deref(),
            Some("eth1"),
            "WAN port should be detected even when not in bridge"
        );
        assert!(
            result.ports.contains_key("eth1"),
            "WAN port should appear in ports map"
        );
    }

    // ── set tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn set_round_trip_preserves_state() {
        let dir = tempfile::tempdir().unwrap();
        setup_basic(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        let original = get(ctx.clone()).await.unwrap();
        set(
            ctx.clone(),
            DeserializeStdin(Ethernet {
                wan_ipv6: original.wan_ipv6,
                wan_port: original.wan_port.clone(),
                ports: original
                    .ports
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            Port {
                                profile: v.profile.as_ref().map(|p| ProfileIdOpt {
                                    fullname: Some(p.fullname.clone()),
                                    interface: Some(p.interface.clone()),
                                    vlan_tag: Some(p.vlan_tag),
                                }),
                            },
                        )
                    })
                    .collect(),
            }),
        ).await
        .unwrap();

        let after = get(ctx).await.unwrap();
        assert_eq!(original.wan_ipv6, after.wan_ipv6);
        assert_eq!(original.wan_port, after.wan_port);
        assert_eq!(original.ports.len(), after.ports.len());
        for (name, port) in &original.ports {
            assert_eq!(
                port.profile.as_ref().map(|p| &p.interface),
                after.ports[name].profile.as_ref().map(|p| &p.interface),
                "port {name} profile mismatch"
            );
        }
    }

    #[tokio::test]
    async fn set_assigns_port_to_guest_profile() {
        let dir = tempfile::tempdir().unwrap();
        setup_basic(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        // Assign eth1 to Guest
        set(
            ctx.clone(),
            DeserializeStdin(Ethernet {
                wan_ipv6: false,
                wan_port: None,
                ports: BTreeMap::from([
                    (
                        "eth0".into(),
                        Port {
                            profile: Some(ProfileIdOpt {
                                fullname: None,
                                interface: Some("lan".into()),
                                vlan_tag: None,
                            }),
                        },
                    ),
                    (
                        "eth1".into(),
                        Port {
                            profile: Some(ProfileIdOpt {
                                fullname: None,
                                interface: Some("guest".into()),
                                vlan_tag: None,
                            }),
                        },
                    ),
                ]),
            }),
        ).await
        .unwrap();

        let after = get(ctx).await.unwrap();
        assert_eq!(
            after.ports["eth1"].profile.as_ref().unwrap().interface,
            "guest"
        );
        assert_eq!(
            after.ports["eth0"].profile.as_ref().unwrap().interface,
            "lan"
        );
    }

    #[tokio::test]
    async fn set_designates_wan_port() {
        let dir = tempfile::tempdir().unwrap();
        setup_basic(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        // Make eth1 the WAN port (no profile allowed)
        set(
            ctx.clone(),
            DeserializeStdin(Ethernet {
                wan_ipv6: false,
                wan_port: Some("eth1".into()),
                ports: BTreeMap::from([
                    (
                        "eth0".into(),
                        Port {
                            profile: Some(ProfileIdOpt {
                                fullname: None,
                                interface: Some("lan".into()),
                                vlan_tag: None,
                            }),
                        },
                    ),
                    ("eth1".into(), Port { profile: None }),
                ]),
            }),
        ).await
        .unwrap();

        let after = get(ctx).await.unwrap();
        assert_eq!(after.wan_port.as_deref(), Some("eth1"));
        assert!(!after.wan_ipv6);
        assert!(
            after.ports.contains_key("eth1"),
            "WAN port should appear in ports map"
        );
    }

    #[tokio::test]
    async fn set_enables_wan_ipv6() {
        let dir = tempfile::tempdir().unwrap();
        setup_basic(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        set(
            ctx.clone(),
            DeserializeStdin(Ethernet {
                wan_ipv6: true,
                wan_port: Some("eth1".into()),
                ports: BTreeMap::from([
                    (
                        "eth0".into(),
                        Port {
                            profile: Some(ProfileIdOpt {
                                fullname: None,
                                interface: Some("lan".into()),
                                vlan_tag: None,
                            }),
                        },
                    ),
                    ("eth1".into(), Port { profile: None }),
                ]),
            }),
        ).await
        .unwrap();

        let after = get(ctx).await.unwrap();
        assert_eq!(after.wan_port.as_deref(), Some("eth1"));
        assert!(after.wan_ipv6);
    }

    #[tokio::test]
    async fn set_removes_wan_when_unset() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_wan(dir.path(), true);
        let ctx = TestContext(dir.path().to_path_buf());

        // Verify WAN exists first
        let before = get(ctx.clone()).await.unwrap();
        assert!(before.wan_port.is_some());

        // Remove WAN
        set(
            ctx.clone(),
            DeserializeStdin(Ethernet {
                wan_ipv6: false,
                wan_port: None,
                ports: BTreeMap::from([
                    (
                        "eth0".into(),
                        Port {
                            profile: Some(ProfileIdOpt {
                                fullname: None,
                                interface: Some("lan".into()),
                                vlan_tag: None,
                            }),
                        },
                    ),
                    ("eth1".into(), Port { profile: None }),
                    ("eth2".into(), Port { profile: None }),
                ]),
            }),
        ).await
        .unwrap();

        let after = get(ctx).await.unwrap();
        assert!(after.wan_port.is_none());
        assert!(!after.wan_ipv6);
    }

    #[tokio::test]
    async fn set_errors_wan_port_with_profile() {
        let dir = tempfile::tempdir().unwrap();
        setup_basic(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        let err = set(
            ctx,
            DeserializeStdin(Ethernet {
                wan_ipv6: false,
                wan_port: Some("eth1".into()),
                ports: BTreeMap::from([
                    ("eth0".into(), Port { profile: None }),
                    (
                        "eth1".into(),
                        Port {
                            profile: Some(ProfileIdOpt {
                                fullname: None,
                                interface: Some("guest".into()),
                                vlan_tag: None,
                            }),
                        },
                    ),
                ]),
            }),
        ).await
        .unwrap_err();

        assert!(
            err.to_string().contains("eth1") || err.to_string().contains("WAN"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn set_moves_wan_to_different_port() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_wan(dir.path(), false);
        let ctx = TestContext(dir.path().to_path_buf());

        let before = get(ctx.clone()).await.unwrap();
        assert_eq!(before.wan_port.as_deref(), Some("eth2"));

        // Move WAN from eth2 to eth0
        set(
            ctx.clone(),
            DeserializeStdin(Ethernet {
                wan_ipv6: false,
                wan_port: Some("eth0".into()),
                ports: BTreeMap::from([
                    ("eth0".into(), Port { profile: None }),
                    (
                        "eth1".into(),
                        Port {
                            profile: Some(ProfileIdOpt {
                                fullname: None,
                                interface: Some("lan".into()),
                                vlan_tag: None,
                            }),
                        },
                    ),
                    (
                        "eth2".into(),
                        Port {
                            profile: Some(ProfileIdOpt {
                                fullname: None,
                                interface: Some("guest".into()),
                                vlan_tag: None,
                            }),
                        },
                    ),
                ]),
            }),
        ).await
        .unwrap();

        // Verify WAN moved to eth0 in UCI config
        let arena = Arena::new();
        let cfgs = parse_all(ctx.uci_root(), &arena, &["network", "startwrt", "firewall"]).await.unwrap();
        for section in &cfgs["network"].sections {
            if let Some(iface) = section.get_typed::<NetworkInterface>().unwrap() {
                if section.name().as_deref() == Some("wan") && iface.proto == InterfaceProto::DHCP {
                    assert_eq!(iface.device, "eth0", "WAN should now point to eth0");
                }
            }
            // eth2 should be back in the bridge (no longer WAN)
            if let Some(dev) = section.get_typed::<NetworkDevice>().unwrap() {
                if dev.name == "br-lan" {
                    assert!(
                        dev.ports.contains(&"eth2".to_string()),
                        "eth2 should be back in bridge after WAN moved away"
                    );
                    assert!(
                        !dev.ports.contains(&"eth0".to_string()),
                        "eth0 (new WAN) should not be in bridge"
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn set_wan_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        setup_with_wan(dir.path(), true);
        let ctx = TestContext(dir.path().to_path_buf());

        let before = get(ctx.clone()).await.unwrap();
        assert_eq!(before.wan_port.as_deref(), Some("eth2"));
        assert!(before.wan_ipv6);

        // Round-trip: set with the same values
        set(
            ctx.clone(),
            DeserializeStdin(Ethernet {
                wan_ipv6: true,
                wan_port: Some("eth2".into()),
                ports: before
                    .ports
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            Port {
                                profile: v.profile.as_ref().map(|p| ProfileIdOpt {
                                    fullname: Some(p.fullname.clone()),
                                    interface: Some(p.interface.clone()),
                                    vlan_tag: Some(p.vlan_tag),
                                }),
                            },
                        )
                    })
                    .collect(),
            }),
        ).await
        .unwrap();

        let after = get(ctx).await.unwrap();
        assert_eq!(
            after.wan_port.as_deref(),
            Some("eth2"),
            "WAN port should be preserved after round-trip"
        );
        assert!(after.wan_ipv6, "WAN IPv6 should be preserved after round-trip");
        assert!(
            after.ports.contains_key("eth2"),
            "WAN port should appear in ports map"
        );
    }

    #[tokio::test]
    async fn set_unassigned_ports_default_to_admin_when_vlan_filtering() {
        let dir = tempfile::tempdir().unwrap();
        setup_basic(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        // Assign eth0 to Guest, leave eth1 unassigned (None)
        set(
            ctx.clone(),
            DeserializeStdin(Ethernet {
                wan_ipv6: false,
                wan_port: None,
                ports: BTreeMap::from([
                    (
                        "eth0".into(),
                        Port {
                            profile: Some(ProfileIdOpt {
                                fullname: None,
                                interface: Some("guest".into()),
                                vlan_tag: None,
                            }),
                        },
                    ),
                    ("eth1".into(), Port { profile: None }),
                ]),
            }),
        ).await
        .unwrap();

        let after = get(ctx).await.unwrap();
        assert_eq!(
            after.ports["eth0"].profile.as_ref().unwrap().interface,
            "guest"
        );
        // Unassigned port should default to Admin (vlan 1) since other profiles have non-1 vlans
        assert_eq!(
            after.ports["eth1"].profile.as_ref().unwrap().interface,
            "lan",
            "unassigned port should default to admin profile"
        );
    }

    #[tokio::test]
    async fn set_keeps_admin_vlan_when_last_admin_port_reassigned() {
        // Regression for the Admin-VLAN-drop bug: on real hardware the AP
        // netdevs are attached to br-lan at runtime by hostapd and are NOT
        // listed in the `network` bridge `ports`.  Here the bridge has a
        // single ethernet member (eth0) and no wireless ports in config.
        // Moving eth0 (the only Admin port) to Guest leaves Admin with zero
        // ports — the buggy `if ports.is_empty() && vlan == 1` skip then
        // deleted the VLAN-1 bridge-vlan section entirely, taking down the
        // gateway (br-lan.1) and the default-network WiFi SSID.
        let dir = tempfile::tempdir().unwrap();
        setup_basic(dir.path()); // startwrt: lan(vlan 1) + guest(vlan 3); firewall
        // Single-port bridge, no wireless in config (mirrors real hardware).
        std::fs::write(
            dir.path().join("network"),
            "\
config device
\toption name 'br-lan'
\toption type 'bridge'
\tlist ports 'eth0'

config bridge-vlan
\toption device 'br-lan'
\toption vlan '1'
\tlist ports 'eth0:u*'

config interface 'lan'
\toption device 'br-lan.1'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
\toption netmask '255.255.255.0'

config interface 'guest'
\toption device 'br-lan.3'
\toption proto 'static'
\toption ipaddr '192.168.3.1'
\toption netmask '255.255.255.0'
",
        )
        .unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        // Move eth0 (the only Admin port) to Guest.
        set(
            ctx.clone(),
            DeserializeStdin(Ethernet {
                wan_ipv6: false,
                wan_port: None,
                ports: BTreeMap::from([(
                    "eth0".into(),
                    Port {
                        profile: Some(ProfileIdOpt {
                            fullname: None,
                            interface: Some("guest".into()),
                            vlan_tag: None,
                        }),
                    },
                )]),
            }),
        )
        .await
        .unwrap();

        // The Admin VLAN (1) section must still exist so br-lan.1 and the
        // default-network SSID keep their bridge VLAN, even though it now has
        // no member ports.
        let arena = Arena::new();
        let cfgs =
            parse_all(ctx.uci_root(), &arena, &["network", "startwrt", "firewall"]).await.unwrap();
        let mut vlans: Vec<u16> = Vec::new();
        for section in &cfgs["network"].sections {
            if let Some(v) = section.get_typed::<NetworkBridgeVlan>().unwrap() {
                if v.device == "br-lan" {
                    vlans.push(v.vlan);
                }
            }
        }
        assert!(
            vlans.contains(&1),
            "Admin VLAN 1 must survive even with no assigned ports (got vlans: {vlans:?})"
        );
        assert!(
            vlans.contains(&101) || vlans.contains(&3),
            "Guest VLAN should be present too (got vlans: {vlans:?})"
        );
    }

    // ── set_config unit tests (in-memory, no disk I/O) ─────────

    #[tokio::test]
    async fn set_config_removes_old_bridge_vlans() {
        let dir = tempfile::tempdir().unwrap();
        setup_basic(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        let arena = Arena::new();
        let mut cfgs = parse_all(ctx.uci_root(), &arena, &["network", "startwrt", "firewall"]).await.unwrap();

        // Count bridge-vlan sections before
        let before_count = cfgs["network"]
            .sections
            .iter()
            .filter(|s| s.get::<NetworkBridgeVlan>().is_ok())
            .count();
        assert!(before_count > 0, "fixture should have bridge-vlans");

        let ethernet = Ethernet {
            wan_ipv6: false,
            wan_port: None,
            ports: BTreeMap::from([
                ("eth0".into(), Port { profile: None }),
                ("eth1".into(), Port { profile: None }),
            ]),
        };
        set_from_config(&ctx, &mut cfgs, &ethernet).unwrap();

        // All old bridge-vlans should be replaced
        let after_vlans: Vec<_> = cfgs["network"]
            .sections
            .iter()
            .filter_map(|s| s.get::<NetworkBridgeVlan>().ok())
            .collect();

        // With no port assignments and vlan_tag 1 for admin (only profile in play
        // when no ports assigned to non-admin), the admin vlan=1 with empty ports
        // is skipped, but guest vlan=3 should still be created (even with empty ports)
        for vlan in &after_vlans {
            assert_eq!(vlan.device, "br-lan");
        }
    }

    #[tokio::test]
    async fn set_config_creates_wan_interfaces() {
        let dir = tempfile::tempdir().unwrap();
        setup_basic(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        let arena = Arena::new();
        let mut cfgs = parse_all(ctx.uci_root(), &arena, &["network", "startwrt", "firewall"]).await.unwrap();

        let ethernet = Ethernet {
            wan_ipv6: true,
            wan_port: Some("eth1".into()),
            ports: BTreeMap::from([
                ("eth0".into(), Port { profile: None }),
                ("eth1".into(), Port { profile: None }),
            ]),
        };
        set_from_config(&ctx, &mut cfgs, &ethernet).unwrap();

        // Should have wan (DHCP) and wan6 (DHCPv6) interfaces
        let mut found_wan = false;
        let mut found_wan6 = false;
        for section in &cfgs["network"].sections {
            if let Some(iface) = section.get_typed::<NetworkInterface>().unwrap() {
                if section.name().as_deref() == Some("wan") && iface.proto == InterfaceProto::DHCP {
                    assert_eq!(iface.device, "eth1");
                    found_wan = true;
                }
                if section.name().as_deref() == Some("wan6")
                    && iface.proto == InterfaceProto::DHCPV6
                {
                    assert_eq!(iface.device, "eth1");
                    found_wan6 = true;
                }
            }
        }
        assert!(found_wan, "WAN interface should be created");
        assert!(found_wan6, "WAN6 interface should be created");
    }

    #[tokio::test]
    async fn set_config_excludes_wan_port_from_bridge() {
        let dir = tempfile::tempdir().unwrap();
        setup_basic(dir.path());
        let ctx = TestContext(dir.path().to_path_buf());

        let arena = Arena::new();
        let mut cfgs = parse_all(ctx.uci_root(), &arena, &["network", "startwrt", "firewall"]).await.unwrap();

        let ethernet = Ethernet {
            wan_ipv6: false,
            wan_port: Some("eth1".into()),
            ports: BTreeMap::from([
                ("eth0".into(), Port { profile: None }),
                ("eth1".into(), Port { profile: None }),
            ]),
        };
        set_from_config(&ctx, &mut cfgs, &ethernet).unwrap();

        // Bridge should only contain eth0 (eth1 is WAN)
        for section in &cfgs["network"].sections {
            if let Some(dev) = section.get_typed::<NetworkDevice>().unwrap() {
                if dev.name == "br-lan" {
                    assert!(
                        dev.ports.contains(&"eth0".to_string()),
                        "eth0 should be in bridge"
                    );
                    assert!(
                        !dev.ports.contains(&"eth1".to_string()),
                        "WAN port eth1 should NOT be in bridge"
                    );
                }
            }
        }
    }

    // ── find_lan_bridge tests ──────────────────────────────────

    #[tokio::test]
    async fn set_preserves_wifi_bridge_ports() {
        let dir = tempfile::tempdir().unwrap();
        setup_basic(dir.path());
        // Add wlan0 to bridge ports
        std::fs::write(
            dir.path().join("network"),
            "\
config device
\toption name 'br-lan'
\toption type 'bridge'
\tlist ports 'eth0'
\tlist ports 'eth1'
\tlist ports 'wlan0'

config bridge-vlan
\toption device 'br-lan'
\toption vlan '1'
\tlist ports 'eth0:u*'
\tlist ports 'eth1:u*'

config bridge-vlan
\toption device 'br-lan'
\toption vlan '3'

config interface 'lan'
\toption device 'br-lan.1'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
\toption netmask '255.255.255.0'

config interface 'guest'
\toption device 'br-lan.3'
\toption proto 'static'
\toption ipaddr '192.168.3.1'
\toption netmask '255.255.255.0'
",
        )
        .unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        // Change eth0 to Guest profile — wlan0 should be preserved
        set(
            ctx.clone(),
            DeserializeStdin(Ethernet {
                wan_ipv6: false,
                wan_port: None,
                ports: BTreeMap::from([
                    (
                        "eth0".into(),
                        Port {
                            profile: Some(ProfileIdOpt {
                                fullname: None,
                                interface: Some("guest".into()),
                                vlan_tag: None,
                            }),
                        },
                    ),
                    (
                        "eth1".into(),
                        Port {
                            profile: Some(ProfileIdOpt {
                                fullname: None,
                                interface: Some("lan".into()),
                                vlan_tag: None,
                            }),
                        },
                    ),
                ]),
            }),
        ).await
        .unwrap();

        // Verify wlan0 is still in the bridge
        let arena = Arena::new();
        let cfgs =
            parse_all(ctx.uci_root(), &arena, &["network", "startwrt", "firewall"]).await.unwrap();
        for section in &cfgs["network"].sections {
            if let Some(dev) = section.get_typed::<NetworkDevice>().unwrap() {
                if dev.name == "br-lan" {
                    assert!(
                        dev.ports.contains(&"wlan0".to_string()),
                        "wlan0 should be preserved in bridge after ethernet.set, got: {:?}",
                        dev.ports
                    );
                    assert!(dev.ports.contains(&"eth0".to_string()));
                    assert!(dev.ports.contains(&"eth1".to_string()));
                    return;
                }
            }
        }
        panic!("br-lan bridge device not found");
    }

    #[tokio::test]
    async fn find_lan_bridge_prefers_br_lan() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("network"),
            "\
config device
\toption name 'br-other'
\toption type 'bridge'
\tlist ports 'eth0'

config device
\toption name 'br-lan'
\toption type 'bridge'
\tlist ports 'eth1'
",
        )
        .unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        let arena = Arena::new();
        let cfgs = parse_all(ctx.uci_root(), &arena, &["network"]).await.unwrap();
        let bridge = find_lan_bridge(&cfgs).unwrap().unwrap();
        assert_eq!(bridge.name, "br-lan");
    }

    #[tokio::test]
    async fn find_lan_bridge_falls_back_to_any_bridge() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("network"),
            "\
config device
\toption name 'br-custom'
\toption type 'bridge'
\tlist ports 'eth0'
",
        )
        .unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        let arena = Arena::new();
        let cfgs = parse_all(ctx.uci_root(), &arena, &["network"]).await.unwrap();
        let bridge = find_lan_bridge(&cfgs).unwrap().unwrap();
        assert_eq!(bridge.name, "br-custom");
    }

    #[tokio::test]
    async fn find_lan_bridge_returns_none_when_no_bridge() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("network"), "").unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        let arena = Arena::new();
        let cfgs = parse_all(ctx.uci_root(), &arena, &["network"]).await.unwrap();
        assert!(find_lan_bridge(&cfgs).unwrap().is_none());
    }
}
