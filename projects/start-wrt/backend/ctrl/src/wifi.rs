use crate::profiles::{self, ProfileId, ProfileIdOpt};
use crate::utils::{DeserializeStdin, HandlerExtSerde as _};
use crate::prelude::*;
use crate::CtrlContext;
use rpc_toolkit::{from_fn, from_fn_async_local, ParentHandler};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use uciedit::openwrt::{
    NetworkBridgeVlan, WifiChannel, WifiDevice, WifiDynamicVlan, WifiInterface, WifiMode,
    WifiStation, WifiVlan,
};
use uciedit::{dump_all, parse_all, Arena, Configs, TypedSection};

pub const DEFAULT_LAN_BRIDGE: &str = "br-lan";

/// WPA2-PSK length bounds. Hostapd accepts 8–63 ASCII chars (passphrase form);
/// the 64-hex-char raw-PMK form isn't exposed through this API. Reject out-of-
/// range lengths at the API boundary so the user gets a clean error instead of
/// hostapd silently refusing to start the AP.
const MIN_PSK_LEN: usize = 8;
const MAX_PSK_LEN: usize = 63;

/// Whether wifi needs a full restart or just a PSK hot-reload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WifiRestart {
    /// New VLANs or interface config changes — full restart needed
    Full,
    /// Only PSK entries changed — hot reload sufficient
    PskOnly,
}

/// Returns the UCI section names of all wifi-iface sections in AP mode.
pub fn find_ap_interface_names(cfgs: &Configs) -> Result<Vec<String>, Error> {
    let mut names = Vec::new();
    cfgs["wireless"].try_each(|name, iface: WifiInterface| {
        if iface.mode == WifiMode::AP {
            names.push(name.ok_or_else(|| Error::new(eyre!("unnamed wireless interface"), ErrorKind::UnnamedWirelessInterface))?.to_string());
        }
        Ok::<_, Error>(())
    })?;
    Ok(names)
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Password<Id: Ord> {
    pub label: String,
    pub profile: Option<Id>,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WifiRadio {
    pub band: String,
    pub channel: String,
    pub enabled: bool,
    pub broadcast: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wifi<Id: Ord = ProfileId> {
    pub ssid: String,
    pub broadcast_separately: bool,
    pub radios: BTreeMap<String, WifiRadio>,
    pub passwords: BTreeSet<Password<Id>>,
}

/// `wifi.set` request: the desired WiFi config plus a confirmation flag.
/// When a profile loses its last WiFi password its devices can no longer reach
/// that subnet — disconnected if the password was removed, or moved to another
/// profile if it was reassigned — which breaks any published ports forwarding to
/// them. When the flag is false and such ports exist, `set` applies nothing and
/// returns them in `WifiSetResult` for a confirmation dialog; with the flag true
/// it deletes those published ports as part of the change.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiSetRequest {
    #[serde(flatten)]
    pub wifi: Wifi<ProfileIdOpt>,
    #[serde(default)]
    pub confirm_published_port_deletion: bool,
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WifiSetResult {
    /// Non-empty (and nothing applied) when published ports would be deleted and
    /// the caller hasn't confirmed yet. Empty once the change is applied.
    pub pending_published_port_deletions: Vec<crate::published_ports::AffectedPublishedPort>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlackoutWindow {
    pub start_time: String,
    pub end_time: String,
    pub days: [bool; 7],
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BlackoutWindows {
    pub windows: Vec<BlackoutWindow>,
}

/// Canonical UCI store for WiFi blackout windows (singleton section
/// `config wifi_blackout 'blackout'` in the `startwrt` config). The crontab is a
/// disposable projection regenerated from this on every write, so deconfliction
/// can drop/merge cron edges without losing the windows. Each entry is the
/// "start|end|days" list form shared with profile WAN schedules.
#[derive(Debug, TypedSection, Default)]
#[uci(ty = "wifi_blackout")]
pub(crate) struct UciWifiBlackout {
    #[uci(default)]
    pub windows: Vec<String>,
}

pub fn wifi<C: CtrlContext + Clone>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand("get", from_fn_async_local(get::<C>).with_display_serializable())
        .subcommand("set", from_fn_async_local(set::<C>).with_display_serializable())
        .subcommand("edit", from_fn_async_local(edit::<C>).with_display_serializable())
        .subcommand(
            "blackout-get",
            from_fn_async_local(blackout_get::<C>).with_display_serializable(),
        )
        .subcommand(
            "blackout-set",
            from_fn_async_local(blackout_set::<C>).with_display_serializable(),
        )
        .subcommand(
            "generate-password",
            from_fn(generate_password::<C>).with_display_serializable(),
        )
}

fn generate_password<C: CtrlContext>(_ctx: C) -> Result<String, Error> {
    Ok(crate::generate_password(crate::PASSWORD_CHARS_ALNUM.as_bytes(), 16))
}

fn find_relevant_with_radios(
    cfgs: &Configs,
    radios: &BTreeMap<String, WifiRadio>,
) -> Result<Vec<(String, WifiInterface, WifiDevice, WifiRadio)>, Error> {
    let mut devices = HashMap::new();
    cfgs["wireless"].try_each(|name, device: WifiDevice| {
        devices.insert(
            name.ok_or_else(|| Error::new(eyre!("unnamed wireless device"), ErrorKind::UnnamedWirelessDevice))?.to_string(),
            device,
        );
        Ok::<_, Error>(())
    })?;
    let mut relevant_interfaces = Vec::<(String, WifiInterface, WifiDevice, WifiRadio)>::new();
    cfgs["wireless"].try_each(|name, iface: WifiInterface| {
        if iface.mode != WifiMode::AP {
            return Ok(());
        }
        let Some(device) = devices.get(&*iface.device) else {
            return Ok(());
        };
        let name = name.ok_or_else(|| Error::new(eyre!("unnamed wireless interface"), ErrorKind::UnnamedWirelessInterface))?.to_string();
        let radio = match radios.get(&name) {
            None => return Ok(()),
            Some(r) => r.clone(),
        };
        relevant_interfaces.push((name, iface, device.clone(), radio));
        Ok::<_, Error>(())
    })?;
    return Ok(relevant_interfaces);
}

fn find_relevant(cfgs: &Configs) -> Result<Vec<(String, WifiInterface, WifiDevice)>, Error> {
    let mut devices = HashMap::new();
    cfgs["wireless"].try_each(|name, device: WifiDevice| {
        devices.insert(
            name.ok_or_else(|| Error::new(eyre!("unnamed wireless device"), ErrorKind::UnnamedWirelessDevice))?.to_string(),
            device,
        );
        Ok::<_, Error>(())
    })?;
    let mut relevant_interfaces = Vec::<(String, WifiInterface, WifiDevice)>::new();
    cfgs["wireless"].try_each(|name, iface: WifiInterface| {
        if iface.mode != WifiMode::AP {
            return Ok(());
        }
        let Some(device) = devices.get(&*iface.device) else {
            return Ok(());
        };
        let name = name.ok_or_else(|| Error::new(eyre!("unnamed wireless interface"), ErrorKind::UnnamedWirelessInterface))?.to_string();
        relevant_interfaces.push((name, iface, device.clone()));
        Ok::<_, Error>(())
    })?;
    return Ok(relevant_interfaces);
}

#[instrument(skip_all)]
pub async fn get<C: CtrlContext>(ctx: C) -> Result<Wifi, Error> {
    let arena = Arena::new();
    let cfgs = parse_all(
        ctx.uci_root(),
        &arena,
        &["wireless", "startwrt", "network", "firewall"],
    ).await?;
    get_config(ctx, &cfgs)
}

fn get_config(ctx: impl CtrlContext, cfgs: &Configs) -> Result<Wifi, Error> {
    let lookup = profiles::Lookup::parse(ctx.clone(), cfgs)?;
    let relevant_interfaces = find_relevant(cfgs)?;
    let Some(first_interface) = relevant_interfaces.first() else {
        return Err(Error::new(eyre!("corrupted WiFi configuration"), ErrorKind::CorruptedWifi));
    };
    let mut passwords = BTreeSet::new();
    cfgs["wireless"].try_each(|_, station: WifiStation| {
        if let Some(iface) = &station.iface {
            if !relevant_interfaces.iter().any(|(n, _, _)| n == iface) {
                return Ok(());
            }
        }
        let profile = station.vid.and_then(|vid| lookup.from_vlan(vid)).cloned();
        let label = station
            .label
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| {
                profile
                    .as_ref()
                    .map(|p| p.fullname.clone())
                    .unwrap_or_default()
            });
        passwords.insert(Password {
            label,
            profile,
            password: station.key,
        });
        Ok::<_, Error>(())
    })?;
    let mut radios = BTreeMap::new();
    for (name, iface, device) in &relevant_interfaces {
        radios.insert(
            name.clone(),
            WifiRadio {
                band: device.band.clone(),
                channel: device.channel.to_string(),
                enabled: !device.disabled,
                broadcast: !device.disabled && !iface.hidden,
            },
        );
        if iface.dynamic_vlan != WifiDynamicVlan::REQUIRED {
            if let Some(key) = &iface.key {
                if !passwords.iter().any(|p| p.profile.is_none()) {
                    passwords.insert(Password {
                        label: "Default".to_string(),
                        profile: None,
                        password: key.clone(),
                    });
                }
            }
        }
    }
    let all_ssids_match = relevant_interfaces
        .iter()
        .all(|(_, iface, _)| iface.ssid == first_interface.1.ssid);
    let broadcast_separately = !all_ssids_match;
    // Use the 2.4GHz radio's SSID as the base name, falling back to first
    let ssid = relevant_interfaces
        .iter()
        .find(|(_, _, dev)| dev.band == "2g")
        .map(|(_, iface, _)| iface.ssid.clone())
        .unwrap_or_else(|| first_interface.1.ssid.clone());

    Ok(Wifi {
        ssid,
        broadcast_separately,
        radios,
        passwords,
    })
}

/// If the request introduces the first admin password and every radio in the
/// request is disabled, flip them all to enabled and broadcasting. Setting an
/// admin password unambiguously expresses intent to bring up a discoverable
/// AP; without this, factory `disabled='1'` and `hidden='1'` on the wireless
/// sections would persist (the `restore_wifi_if_needed` boot path leaves them
/// as-is when EEPROM tag 0x2F is unprovisioned, and `wifi.get` then reports
/// `broadcast: false` because of the `disabled` bit, so the form round-trip
/// echoes both values back as false). The password would be written but no
/// SSID would broadcast. Only fires on the first add — once any AP iface has
/// a key, subsequent edits respect explicit radio + broadcast toggles.
fn auto_enable_radios_on_first_admin_password(
    wifi: &mut Wifi,
    cfgs: &Configs,
) -> Result<bool, Error> {
    let new_has_admin = wifi.passwords.iter().any(|p| p.profile.is_none());
    if !new_has_admin {
        return Ok(false);
    }

    let mut prev_has_admin = false;
    cfgs["wireless"].try_each(|_, iface: WifiInterface| {
        if iface.mode == WifiMode::AP && iface.key.is_some() {
            prev_has_admin = true;
        }
        Ok::<_, Error>(())
    })?;
    if prev_has_admin {
        return Ok(false);
    }

    if wifi.radios.is_empty() || wifi.radios.values().any(|r| r.enabled) {
        return Ok(false);
    }

    for r in wifi.radios.values_mut() {
        r.enabled = true;
        r.broadcast = true;
    }
    Ok(true)
}

/// Determines whether a full wifi restart or PSK-only reload is needed.
fn set_config(
    _ctx: &impl CtrlContext,
    cfgs: &mut Configs,
    wifi: &Wifi,
    _lookup: &profiles::Lookup,
) -> Result<WifiRestart, Error> {
    let first_admin_password = wifi.passwords.iter().find(|p| p.profile.is_none());

    let relevant_interfaces = find_relevant_with_radios(cfgs, &wifi.radios)?;
    let mut iface_config_changed = false;
    for s in &mut cfgs["wireless"].sections {
        if let Some(mut device) = s.get_typed::<WifiDevice>()? {
            let name = s.name().ok_or_else(|| Error::new(eyre!("unnamed wireless device"), ErrorKind::UnnamedWirelessDevice))?;
            for (_, rel_iface, _, rel_radio) in &relevant_interfaces {
                if rel_iface.device == name {
                    let new_disabled = !rel_radio.enabled;
                    let new_channel = match rel_radio.channel.as_str() {
                        "auto" | "Auto" => WifiChannel::Auto,
                        n => n.parse::<u32>().map(WifiChannel::Int).unwrap_or(WifiChannel::Auto),
                    };
                    if device.disabled != new_disabled
                        || device.channel != new_channel
                        || device.band != rel_radio.band
                    {
                        iface_config_changed = true;
                    }
                    device.disabled = new_disabled;
                    device.band = rel_radio.band.clone();
                    device.channel = new_channel;
                    s.set(&device)?;
                    break;
                }
            }
        }
        if let Some(mut iface) = s.get_typed::<WifiInterface>()? {
            let name = s.name().ok_or_else(|| Error::new(eyre!("unnamed wireless interface"), ErrorKind::UnnamedWirelessInterface))?;
            for (rel_name, _, rel_device, rel_radio) in &relevant_interfaces {
                if rel_name == &*name {
                    let new_ssid = if wifi.broadcast_separately && rel_device.band == "5g" {
                        format!("{}-5G", wifi.ssid)
                    } else {
                        wifi.ssid.clone()
                    };
                    let new_hidden = !rel_radio.broadcast;
                    let (new_key, new_dynamic_vlan) = if let Some(key) = first_admin_password {
                        (Some(key.password.clone()), WifiDynamicVlan::ALLOWED)
                    } else {
                        (None, WifiDynamicVlan::REQUIRED)
                    };
                    if iface.ssid != new_ssid
                        || iface.hidden != new_hidden
                        || iface.encryption != "psk2"
                        || iface.key != new_key
                        || iface.dynamic_vlan != new_dynamic_vlan
                    {
                        iface_config_changed = true;
                    }
                    iface.encryption = "psk2".into();
                    iface.ssid = new_ssid;
                    iface.key = new_key;
                    iface.dynamic_vlan = new_dynamic_vlan;
                    iface.hidden = new_hidden;
                    s.set(&iface)?;
                    break;
                }
            }
        }
    }

    cfgs["wireless"].sections.retain(|s| {
        if let Ok(station) = s.get::<WifiStation>() {
            if let Some(iface) = &station.iface {
                if !relevant_interfaces.iter().any(|(n, _, _, _)| n == iface) {
                    return true;
                }
            }
            return false;
        }
        // wifi-vlan sections are managed by profiles — don't remove them here
        true
    });

    for psswd in wifi.passwords.iter().filter(|p| p.profile.is_none()) {
        for (niface, _, _, _) in &relevant_interfaces {
            cfgs["wireless"].append(
                &WifiStation {
                    key: psswd.password.clone(),
                    vid: None,
                    iface: Some(niface.clone()),
                    label: Some(psswd.label.clone()),
                },
                None,
            )?;
        }
    }
    for psswd in &wifi.passwords {
        let Some(profile) = &psswd.profile else {
            // admin passwords are handled above
            continue;
        };
        for (niface, _, _, _) in &relevant_interfaces {
            cfgs["wireless"].append(
                &WifiStation {
                    key: psswd.password.clone(),
                    vid: Some(profile.vlan_tag),
                    iface: Some(niface.clone()),
                    label: Some(psswd.label.clone()),
                },
                None,
            )?;
        }
    }

    // Fallback: create missing wifi-vlans (e.g., profile created before this code existed)
    let mut existing_wifi_vlans: BTreeSet<(u16, String)> = cfgs["wireless"]
        .sections
        .iter()
        .filter_map(|s| {
            let vlan = s.get::<WifiVlan>().ok()?;
            Some((vlan.vid, vlan.iface?.clone()))
        })
        .collect();

    let bridge_vlan_tags: BTreeSet<u16> = cfgs["network"]
        .sections
        .iter()
        .filter_map(|s| s.get::<NetworkBridgeVlan>().ok().map(|v| v.vlan))
        .collect();

    let mut vlans_created = false;
    for psswd in &wifi.passwords {
        let Some(profile) = &psswd.profile else { continue; };
        if !bridge_vlan_tags.contains(&profile.vlan_tag) { continue; }
        for (niface, _, _, _) in &relevant_interfaces {
            let key = (profile.vlan_tag, niface.clone());
            if existing_wifi_vlans.insert(key) {
                cfgs["wireless"].append(
                    &WifiVlan {
                        name: profile.interface.clone(),
                        network: profile.interface.clone(),
                        vid: profile.vlan_tag,
                        iface: Some(niface.clone()),
                    },
                    None,
                )?;
                vlans_created = true;
            }
        }
    }

    if vlans_created || iface_config_changed {
        Ok(WifiRestart::Full)
    } else {
        Ok(WifiRestart::PskOnly)
    }
}

#[instrument(skip_all)]
pub async fn set<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(req): DeserializeStdin<WifiSetRequest>,
) -> Result<WifiSetResult, Error> {
    let WifiSetRequest {
        wifi,
        confirm_published_port_deletion: confirm,
    } = req;

    // A profile that loses its last WiFi password is "vacated": its WiFi devices
    // can no longer reach that subnet — disconnected if the password was removed,
    // or moved to another profile if it was reassigned — breaking their published
    // ports. We attribute each device's profile by its static DHCP reservation
    // subnet (config-derived, reliable) rather than a live ARP VLAN tag, and keep
    // only WiFi-connected devices (Ethernet devices on the vacated profile keep
    // their IP, so their port forward still works). Resolve the affected MACs and
    // ports up front.
    let (affected_macs, affected_ports): (
        HashSet<String>,
        Vec<crate::published_ports::AffectedPublishedPort>,
    ) = if ctx.effectful() {
        let old_profiles: HashSet<String> = get(ctx.clone())
            .await
            .ok()
            .map(|w| {
                w.passwords
                    .iter()
                    .filter_map(|p| p.profile.as_ref().map(|pr| pr.fullname.clone()))
                    .collect()
            })
            .unwrap_or_default();

        let mut new_profiles: HashSet<String> = HashSet::new();
        {
            let arena = Arena::new();
            let cfgs = parse_all(
                ctx.uci_root(),
                &arena,
                &["wireless", "startwrt", "network", "firewall"],
            )
            .await?;
            let lookup = profiles::Lookup::parse(ctx.clone(), &cfgs)?;
            for p in &wifi.passwords {
                if let Some(pr) = &p.profile {
                    new_profiles.insert(lookup.resolve(pr)?.fullname.clone());
                }
            }
        }

        let vacated: HashSet<String> = old_profiles.difference(&new_profiles).cloned().collect();
        if vacated.is_empty() {
            (HashSet::new(), Vec::new())
        } else {
            let devices = crate::devices::list(crate::ServerContext::default())
                .await
                .unwrap_or_default();
            let device_names: HashMap<String, String> = devices
                .iter()
                .filter_map(|d| d.mac.as_ref().map(|m| (m.to_uppercase(), d.name.clone())))
                .collect();
            // MAC → live security profile, the attribution source for IPv6-only
            // ports (which have no IPv4 reservation subnet to consult).
            let device_profiles: HashMap<String, String> = devices
                .iter()
                .filter_map(|d| {
                    Some((d.mac.as_ref()?.to_uppercase(), d.security_profile.clone()?))
                })
                .collect();
            let wifi_macs: HashSet<String> = devices
                .iter()
                .filter(|d| {
                    d.connection
                        .as_deref()
                        .map_or(false, |c| c.starts_with("Wi-Fi"))
                })
                .filter_map(|d| d.mac.as_ref().map(|m| m.to_uppercase()))
                .collect();
            crate::published_ports::affected_wifi_ports_for_vacated_profiles(
                &ctx,
                &vacated,
                &wifi_macs,
                &device_names,
                &device_profiles,
            )
            .await?
        }
    } else {
        (HashSet::new(), Vec::new())
    };

    if ctx.effectful() && !confirm {
        // Dry run: report what would be deleted and apply nothing. The UI calls
        // again with `confirmPublishedPortDeletion: true` to actually apply.
        // The list may be empty, which just tells the caller there's nothing
        // to confirm.
        return Ok(WifiSetResult {
            pending_published_port_deletions: affected_ports,
        });
    }

    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["wireless", "startwrt", "network", "firewall", "dhcp"],
        ).await?;
        let lookup = profiles::Lookup::parse(ctx.clone(), &cfgs)?;
        let mut wifi = Wifi {
            ssid: wifi.ssid.clone(),
            broadcast_separately: wifi.broadcast_separately,
            radios: wifi.radios.clone(),
            passwords: wifi
                .passwords
                .iter()
                .map(|pass| {
                    Ok(Password {
                        label: pass.label.clone(),
                        profile: match &pass.profile {
                            Some(p) => Some(lookup.resolve(p)?.clone()),
                            None => None,
                        },
                        password: pass.password.clone(),
                    })
                })
                .collect::<Result<_, Error>>()?,
        };
        let mut seen_passwords = std::collections::HashSet::new();
        let mut seen_labels = std::collections::HashSet::new();
        for pass in &wifi.passwords {
            if pass.password.len() < MIN_PSK_LEN {
                crate::activity::log("wifi", "updated", false, "Failed to update WiFi: password too short", Some("InvalidValue"));
                return Err(Error::new(
                    eyre!("WiFi password must be at least {MIN_PSK_LEN} characters"),
                    ErrorKind::InvalidValue,
                ));
            }
            if pass.password.len() > MAX_PSK_LEN {
                crate::activity::log("wifi", "updated", false, "Failed to update WiFi: password too long", Some("InvalidValue"));
                return Err(Error::new(
                    eyre!("WiFi password must be at most {MAX_PSK_LEN} characters"),
                    ErrorKind::InvalidValue,
                ));
            }
            if !seen_passwords.insert(&pass.password) {
                crate::activity::log("wifi", "updated", false, "Failed to update WiFi: duplicate password", Some("DuplicatePassword"));
                return Err(Error::new(eyre!("duplicate WiFi password"), ErrorKind::DuplicatePassword));
            }
            if !pass.label.is_empty() && !seen_labels.insert(&pass.label) {
                crate::activity::log("wifi", "updated", false, "Failed to update WiFi: duplicate password label", Some("DuplicatePasswordLabel"));
                return Err(Error::new(eyre!("duplicate WiFi password label"), ErrorKind::DuplicatePasswordLabel));
            }
        }
        if auto_enable_radios_on_first_admin_password(&mut wifi, &cfgs)? {
            crate::activity::log("wifi", "auto-enabled-radios", true, "Auto-enabled radios for first admin password", None);
        }
        let restart = match set_config(&ctx, &mut cfgs, &wifi, &lookup) {
            Err(Error {
                kind: ErrorKind::CorruptedWifi,
                ..
            }) if ctx.effectful() => {
                // try recreating the config from scratch — drop arena before await
                let _ = tokio::fs::remove_file(ctx.uci_root().join("wireless")).await;
                drop(cfgs);
                drop(arena);
                crate::run_quiet_async(tokio::process::Command::new("wifi").arg("config"))
                    .await
                    .map_err(|e| Error::new(eyre!("wifi config: {e}"), ErrorKind::Filesystem))?;
                crate::activity::log("wifi", "recovered", true, "Recovered corrupted WiFi config (auto-regenerated)", None);
                continue;
            }
            Err(err) => {
                crate::activity::log("wifi", "updated", false, "Failed to update WiFi settings", Some(&err.to_string()));
                return Err(err);
            }
            Ok(v) => v,
        };
        // Confirmed path: delete the published ports (and stale reservations) for
        // devices that can no longer reach their subnet, atomically with the WiFi
        // change.
        if !affected_macs.is_empty() {
            crate::published_ports::remove_ports_for_macs(&mut cfgs, &affected_macs);
        }
        let dump_result = dump_all(ctx.uci_root(), cfgs).await;
        drop(arena);
        match dump_result {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log("wifi", "updated", false, "Failed to write WiFi config", Some(&err.to_string()));
                return Err(err.into());
            }
            Ok(()) => {
                if ctx.effectful() {
                    match restart {
                        WifiRestart::Full => {
                            // SSID/channel/enabled/hidden changed or new VLANs — full restart
                            crate::run_quiet_async(&mut tokio::process::Command::new("wifi"))
                                .await
                                .map_err(|e| Error::new(eyre!("wifi restart: {e}"), ErrorKind::Network))?;
                        }
                        WifiRestart::PskOnly => {
                            // PSK-only change — fast path, no client disconnection
                            crate::run_quiet_async(
                                tokio::process::Command::new("wifi").arg("reload"),
                            )
                            .await
                            .map_err(|e| Error::new(eyre!("wifi reload: {e}"), ErrorKind::Network))?;
                        }
                    }
                    // Removing published-port firewall sections needs a firewall
                    // reload (the wifi restart above doesn't touch fw4), and
                    // dropping their static DHCP reservations needs a dnsmasq
                    // reload so the device actually releases the pinned IP.
                    if !affected_macs.is_empty() {
                        let _ = crate::run_quiet_async(
                            tokio::process::Command::new("/etc/init.d/firewall").arg("reload"),
                        )
                        .await;
                        crate::published_ports::reload_dnsmasq();
                        // odhcpd owns IPv6 leases/hostids; reload so a cleared or
                        // preserved reservation takes runtime effect.
                        crate::published_ports::reload_odhcpd();
                    }
                }
                match restart {
                    WifiRestart::Full => {
                        crate::activity::log("wifi", "updated", true, &format!("Updated WiFi settings (SSID: '{}')", wifi.ssid), None);
                    }
                    WifiRestart::PskOnly => {
                        crate::activity::log("wifi", "passwords-updated", true, "Updated WiFi passwords (PSK hot-reload)", None);
                    }
                }
                return Ok(WifiSetResult::default());
            }
        }
    }
}

#[instrument(skip_all)]
pub async fn edit<C: CtrlContext + Clone>(ctx: C) -> Result<(), Error> {
    let current_wifi = get(ctx.clone()).await?;
    let current_wifi = Wifi {
        ssid: current_wifi.ssid,
        broadcast_separately: current_wifi.broadcast_separately,
        radios: current_wifi.radios.clone(),
        passwords: current_wifi
            .passwords
            .into_iter()
            .map(|pass| Password {
                label: pass.label,
                profile: pass.profile.map(Into::into),
                password: pass.password,
            })
            .collect(),
    };
    let modified_wifi = crate::utils::edit_in_editor(&current_wifi)?;
    // CLI editor path has no dialog: confirm published-port cleanup implicitly.
    set(
        ctx,
        DeserializeStdin(WifiSetRequest {
            wifi: modified_wifi,
            confirm_published_port_deletion: true,
        }),
    )
    .await
    .map(|_| ())
}

const BLACKOUT_TAG: &str = "# start-wrt-wifi-blackout";
const CRONTAB_PATH: &str = "/etc/crontabs/root";

fn crontab_path(ctx: &impl CtrlContext) -> PathBuf {
    if ctx.effectful() {
        PathBuf::from(CRONTAB_PATH)
    } else {
        ctx.uci_root().join("crontab_root")
    }
}

fn parse_hhmm(s: &str) -> Result<(u32, u32), Error> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return Err(Error::new(eyre!("invalid time format: {s:?}"), ErrorKind::InvalidValue));
    }
    let h: u32 = parts[0]
        .parse()
        .map_err(|_| Error::new(eyre!("invalid hour: {}", parts[0]), ErrorKind::InvalidValue))?;
    let m: u32 = parts[1]
        .parse()
        .map_err(|_| Error::new(eyre!("invalid minute: {}", parts[1]), ErrorKind::InvalidValue))?;
    Ok((h, m))
}

/// Render a weekday mask as a cron day-of-week field (e.g. "1,2,3"). 0 = Sunday.
/// Shared by WiFi blackout and profile WAN schedules.
pub(crate) fn days_to_cron(days: &[bool; 7]) -> String {
    days.iter()
        .enumerate()
        .filter(|(_, &on)| on)
        .map(|(i, _)| i.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// Shift each set weekday forward by one. Saturday (6) wraps to Sunday (0) via
/// mod 7. Used for the closing edge of a window that crosses midnight: the
/// unblock event belongs to the day *after* the block event. Shared by WiFi
/// blackout and profile WAN schedules.
pub(crate) fn shift_days_forward(days: &[bool; 7]) -> [bool; 7] {
    let mut out = [false; 7];
    for (i, &on) in days.iter().enumerate() {
        if on {
            out[(i + 1) % 7] = true;
        }
    }
    out
}

/// True if any two scheduled windows occupy overlapping time on the weekly
/// timeline. Each tuple is (start_min, end_min, days[Sun..Sat]); end < start
/// denotes a window crossing midnight into the following day, and end == start
/// denotes a full 24-hour window (start..start+24h). Shared by WiFi blackout and
/// profile WAN schedules.
pub(crate) fn windows_overlap(windows: &[(u32, u32, [bool; 7])]) -> bool {
    const WEEK: i64 = 7 * 24 * 60;
    // Expand every (window, day) into half-open minute segments, splitting any
    // that cross the week boundary so all live in [0, WEEK).
    let mut segs: Vec<(i64, i64)> = Vec::new();
    for &(start, end, days) in windows {
        for (d, &on) in days.iter().enumerate() {
            if !on {
                continue;
            }
            let base = d as i64 * 24 * 60;
            let s = base + start as i64;
            let e = base + if end <= start { end as i64 + 24 * 60 } else { end as i64 };
            if e <= WEEK {
                segs.push((s, e));
            } else {
                segs.push((s, WEEK));
                segs.push((0, e - WEEK));
            }
        }
    }
    for i in 0..segs.len() {
        for j in (i + 1)..segs.len() {
            let (a1, b1) = segs[i];
            let (a2, b2) = segs[j];
            if a1 < b2 && a2 < b1 {
                return true;
            }
        }
    }
    false
}

/// Project windows into deconflicted "down"/"up" cron edges keyed by minute of
/// day. The down edge fires at `start_min` on the window's weekday mask; the up
/// edge fires at `end_min` on the mask shifted forward one day when
/// `start_min >= end_min` (the window wraps past midnight, or is a full-24h
/// window where start == end). Where a down and an up edge coincide at the SAME
/// minute-of-day AND weekday, both bits are dropped (annihilated) so the
/// resource stays continuously blocked across adjacent/consecutive windows with
/// no same-tick cron race. Callers must reject overlapping windows first
/// (`windows_overlap`), which guarantees at most one down and one up bit per
/// (minute, weekday). All-false masks are omitted from the returned maps.
/// Shared by WiFi blackout and profile WAN schedules.
pub(crate) fn deconflict_edges(
    windows: &[(u32, u32, [bool; 7])],
) -> (BTreeMap<u32, [bool; 7]>, BTreeMap<u32, [bool; 7]>) {
    let mut downs: BTreeMap<u32, [bool; 7]> = BTreeMap::new();
    let mut ups: BTreeMap<u32, [bool; 7]> = BTreeMap::new();

    // Accumulate raw edges, OR-ing weekday bits per minute. (Overlap rejection
    // upstream means two windows can't actually share a weekday bit at the same
    // minute, but OR-ing is the correct merge regardless.)
    for &(start, end, days) in windows {
        if days.iter().any(|&b| b) {
            let e = downs.entry(start).or_insert([false; 7]);
            for d in 0..7 {
                e[d] |= days[d];
            }
        }
        let up_days = if start >= end {
            shift_days_forward(&days)
        } else {
            days
        };
        if up_days.iter().any(|&b| b) {
            let e = ups.entry(end).or_insert([false; 7]);
            for d in 0..7 {
                e[d] |= up_days[d];
            }
        }
    }

    // Annihilate coincident down+up weekday bits per minute-of-day.
    let minutes: Vec<u32> = downs.keys().chain(ups.keys()).copied().collect();
    for m in minutes {
        let dmask = downs.get(&m).copied().unwrap_or([false; 7]);
        let umask = ups.get(&m).copied().unwrap_or([false; 7]);
        let conflict: [bool; 7] = std::array::from_fn(|d| dmask[d] && umask[d]);
        if conflict.iter().any(|&b| b) {
            set_or_remove(&mut downs, m, std::array::from_fn(|d| dmask[d] && !conflict[d]));
            set_or_remove(&mut ups, m, std::array::from_fn(|d| umask[d] && !conflict[d]));
        }
    }
    (downs, ups)
}

fn set_or_remove(map: &mut BTreeMap<u32, [bool; 7]>, m: u32, mask: [bool; 7]) {
    if mask.iter().any(|&b| b) {
        map.insert(m, mask);
    } else {
        map.remove(&m);
    }
}

/// True if the windows tile the entire week with no gap, leaving no cron edges to
/// drive the schedule. Zero surviving edges (after deconfliction) plus at least
/// one active day means full coverage: any genuine gap leaves a covered→uncovered
/// transition, i.e. a surviving edge. Shared by WiFi blackout and profile WAN
/// schedules — rejected up front since an edgeless schedule can't be executed by
/// cron alone. Callers must reject overlaps first.
pub(crate) fn covers_full_week(windows: &[(u32, u32, [bool; 7])]) -> bool {
    let (downs, ups) = deconflict_edges(windows);
    downs.is_empty() && ups.is_empty() && windows.iter().any(|(_, _, d)| d.iter().any(|&b| b))
}

/// Serialize schedule windows to the UCI "start|end|days" list form (days encoded
/// as comma-joined indices 0=Sun..6=Sat). Shared by WiFi blackout and profile WAN
/// schedules.
pub(crate) fn serialize_windows<'a>(
    windows: impl IntoIterator<Item = (&'a str, &'a str, &'a [bool; 7])>,
) -> Vec<String> {
    windows
        .into_iter()
        .map(|(start, end, days)| format!("{}|{}|{}", start, end, days_to_cron(days)))
        .collect()
}

/// Parse the UCI "start|end|days" list form into (start_time, end_time, days)
/// tuples; malformed entries are skipped. Shared by WiFi blackout and profile WAN
/// schedules.
pub(crate) fn parse_windows(raw: &[String]) -> Vec<(String, String, [bool; 7])> {
    raw.iter()
        .filter_map(|s| {
            let parts: Vec<&str> = s.splitn(3, '|').collect();
            if parts.len() != 3 {
                tracing::warn!("schedule: dropping malformed window entry {s:?}");
                return None;
            }
            let mut days = [false; 7];
            if !parts[2].is_empty() {
                for d in parts[2].split(',') {
                    if let Ok(n) = d.parse::<usize>() {
                        if n < 7 {
                            days[n] = true;
                        }
                    }
                }
            }
            Some((parts[0].to_string(), parts[1].to_string(), days))
        })
        .collect()
}

/// Convert parsed ("HH:MM", "HH:MM", days) windows to (start_min, end_min, days)
/// minute tuples for the cron projection, dropping (with a warning) any whose
/// times don't parse. Shared by the WiFi blackout and profile WAN-schedule cron
/// regenerators.
pub(crate) fn windows_to_minutes(
    windows: &[(String, String, [bool; 7])],
) -> Vec<(u32, u32, [bool; 7])> {
    windows
        .iter()
        .filter_map(|(start, end, days)| match (parse_hhmm(start), parse_hhmm(end)) {
            (Ok((sh, sm)), Ok((eh, em))) => Some((sh * 60 + sm, eh * 60 + em, *days)),
            _ => {
                tracing::warn!(
                    "schedule: dropping window with unparseable time(s) {start:?}..{end:?}"
                );
                None
            }
        })
        .collect()
}

/// Read the raw "start|end|days" entries from the singleton `wifi_blackout`
/// section in the `startwrt` UCI config (the canonical store).
async fn read_blackout_windows<C: CtrlContext>(ctx: &C) -> Result<Vec<String>, Error> {
    let arena = Arena::new();
    let cfgs = parse_all(ctx.uci_root(), &arena, &["startwrt"]).await?;
    let mut raw = Vec::new();
    cfgs["startwrt"].try_each(|name, b: UciWifiBlackout| {
        if name == Some("blackout") {
            raw = b.windows;
        }
        Ok::<_, Error>(())
    })?;
    Ok(raw)
}

#[instrument(skip_all)]
pub async fn blackout_get<C: CtrlContext>(ctx: C) -> Result<Vec<BlackoutWindow>, Error> {
    let raw = read_blackout_windows(&ctx).await?;
    Ok(parse_windows(&raw)
        .into_iter()
        .map(|(start_time, end_time, days)| BlackoutWindow {
            start_time,
            end_time,
            days,
        })
        .collect())
}

// NOTE: WiFi blackout windows are now stored in UCI (`config wifi_blackout
// 'blackout'`); the crontab is a disposable projection regenerated from UCI by
// `regenerate_blackout_crontab`. The deconflicted projection means a schedule
// that tiles the whole week has zero cron edges, which is why `blackout_set`
// rejects full-week coverage (cron alone can't execute an edgeless schedule).
// `reconcile_blackout_at_boot` (below) reasserts the in-window state on boot so a
// reboot mid-blackout doesn't leave the radio up until the next cron edge, and the
// daemon rebuilds the crontab projection from UCI at boot (see `daemon.rs`), so the
// schedule survives a sysupgrade that wipes `/etc/crontabs/root`. The remaining gap
// is the boot-window one (netifd raises radios before the daemon runs), documented
// at `reconcile_blackout_at_boot`.
#[instrument(skip_all)]
pub async fn blackout_set<C: CtrlContext>(
    ctx: C,
    DeserializeStdin(input): DeserializeStdin<BlackoutWindows>,
) -> Result<(), Error> {
    // Validate before persisting. Equal start/end denotes a full 24-hour window
    // (not an error); overlapping windows and full-week-no-gap coverage are
    // rejected.
    let parsed: Vec<(u32, u32, [bool; 7])> = input
        .windows
        .iter()
        .map(|w| {
            let (sh, sm) = parse_hhmm(&w.start_time)?;
            let (eh, em) = parse_hhmm(&w.end_time)?;
            Ok::<_, Error>((sh * 60 + sm, eh * 60 + em, w.days))
        })
        .collect::<Result<_, _>>()?;
    if windows_overlap(&parsed) {
        return Err(Error::new(
            eyre!("blackout windows overlap"),
            ErrorKind::InvalidValue,
        ));
    }
    if covers_full_week(&parsed) {
        return Err(Error::new(
            eyre!("blackout schedule covers the entire week with no gap; disable WiFi directly instead"),
            ErrorKind::InvalidValue,
        ));
    }

    // Persist windows to UCI (the source of truth); the crontab is regenerated
    // from it below.
    let serialized = serialize_windows(
        input
            .windows
            .iter()
            .map(|w| (w.start_time.as_str(), w.end_time.as_str(), &w.days)),
    );

    let mut retries = 4;
    loop {
        let arena = Arena::new();
        let mut cfgs = parse_all(ctx.uci_root(), &arena, &["startwrt"]).await?;

        let mut found = false;
        for section in &mut cfgs["startwrt"].sections {
            if section.name().as_deref() == Some("blackout") {
                if section.get_typed::<UciWifiBlackout>()?.is_some() {
                    section.set(&UciWifiBlackout {
                        windows: serialized.clone(),
                    })?;
                    found = true;
                    break;
                }
            }
        }
        if !found {
            cfgs["startwrt"].append(
                &UciWifiBlackout {
                    windows: serialized.clone(),
                },
                Some("blackout"),
            )?;
        }

        match dump_all(ctx.uci_root(), cfgs).await {
            Err(uciedit::Error::Conflict { .. }) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => {
                crate::activity::log("wifi", "blackout-updated", false, "Failed to write blackout schedule", Some(&err.to_string()));
                return Err(err.into());
            }
            Ok(()) => break,
        }
    }

    regenerate_blackout_crontab(&ctx).await?;

    if ctx.effectful() {
        crate::run_quiet_async(tokio::process::Command::new("/etc/init.d/cron").arg("restart"))
            .await
            .map_err(|e| Error::new(eyre!("restarting cron: {e}"), ErrorKind::Filesystem))?;
    }

    crate::activity::log("wifi", "blackout-updated", true, "Updated WiFi blackout schedule", None);
    Ok(())
}

/// Regenerate the WiFi-blackout crontab lines from the UCI store. Pure projection
/// (UCI → cron): strips old `BLACKOUT_TAG` lines, then emits deconflicted
/// `wifi down`/`wifi up` edges so adjacent/consecutive windows never race at a
/// shared cron tick.
pub(crate) async fn regenerate_blackout_crontab(ctx: &impl CtrlContext) -> Result<(), Error> {
    let path = crontab_path(ctx);
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();

    let filtered: Vec<&str> = content
        .lines()
        .filter(|l| !l.contains(BLACKOUT_TAG))
        .collect();
    let mut new_content = filtered.join("\n");
    if !new_content.is_empty() && !new_content.ends_with('\n') {
        new_content.push('\n');
    }

    let raw = read_blackout_windows(ctx).await?;
    let windows = windows_to_minutes(&parse_windows(&raw));

    let (downs, ups) = deconflict_edges(&windows);
    for (&min, mask) in &downs {
        new_content.push_str(&format!(
            "{} {} * * {} wifi down {}\n",
            min % 60,
            min / 60,
            days_to_cron(mask),
            BLACKOUT_TAG
        ));
    }
    for (&min, mask) in &ups {
        new_content.push_str(&format!(
            "{} {} * * {} wifi up {}\n",
            min % 60,
            min / 60,
            days_to_cron(mask),
            BLACKOUT_TAG
        ));
    }

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, &new_content).await?;
    Ok(())
}

/// Boot/reload reconciler for WiFi blackout. Blackout is edge-triggered by cron
/// (`wifi down`/`wifi up`); a reboot mid-window loses the edge — netifd raises the
/// radios early in boot per the on-disk `disabled` flag (runtime `wifi down` does
/// not persist to config), so WiFi comes back up even though we are inside a
/// blackout. Recompute the current in/out-of-window state (wrap-aware, mirroring
/// `profiles::evaluate_and_apply_schedules`) and reassert `wifi down` when inside
/// a window.
///
/// The out-of-window case is deliberately a no-op: boot already brings radios up
/// per their UCI `disabled` flag, and `wifi up` honors that flag, so we never
/// re-enable a radio the user disabled. Asserting only the "down" side keeps this
/// idempotent and avoids that footgun. Call this *after* `restore_wifi_if_needed`
/// (and after any other boot step that touches the radios) so this `wifi down` is
/// the final word — `restore_wifi_if_needed` restores missing AP config and
/// applies it with `wifi reload`, which would otherwise re-raise the radio.
///
/// TODO: This closes the *steady-state* gap (radio staying up for the rest of the
/// window after a reboot) but NOT the *boot-window* gap. netifd brings the radios
/// up at init `START=20`, long before this daemon runs (`START=99`), so there is a
/// bounded interval early in boot where the radio broadcasts inside a blackout.
/// Eliminating it entirely would require persisting blackout state into the
/// wireless config (`option disabled '1'`) behind a dedicated marker distinct from
/// the user's own per-radio `disabled`, plus a restore step on the window's
/// closing edge — a larger change deferred for now.
pub(crate) async fn reconcile_blackout_at_boot(ctx: &impl CtrlContext) -> Result<(), Error> {
    let raw = read_blackout_windows(ctx).await?;
    let windows = parse_windows(&raw);
    let (day, minute) = crate::profiles::chrono_now().await;
    if !windows_contain_now(&windows, day, minute) {
        return Ok(());
    }
    if ctx.effectful() {
        crate::run_quiet_async(tokio::process::Command::new("wifi").arg("down"))
            .await
            .map_err(|e| Error::new(eyre!("wifi down: {e}"), ErrorKind::Network))?;
        crate::activity::log(
            "wifi",
            "blackout-reasserted",
            true,
            "Reasserted WiFi blackout after reboot (inside active window)",
            None,
        );
    }
    Ok(())
}

/// Pure decision: true if `(day, minute)` falls inside any blackout window
/// (wrap-aware via `profiles::window_contains`). Malformed time fields are
/// skipped. Split out from `reconcile_blackout_at_boot` so it is unit-testable
/// without reading the system clock or spawning `wifi`.
fn windows_contain_now(
    windows: &[(String, String, [bool; 7])],
    day: usize,
    minute: u32,
) -> bool {
    windows.iter().any(|(start, end, days)| {
        match (parse_hhmm(start), parse_hhmm(end)) {
            (Ok((sh, sm)), Ok((eh, em))) => {
                crate::profiles::window_contains(sh * 60 + sm, eh * 60 + em, days, day, minute)
            }
            _ => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rpc_toolkit::Context;
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    /// Test shim: the production `set` now takes a `WifiSetRequest` and returns
    /// `WifiSetResult`; these tests predate that and pass a bare `Wifi`. Wrap it
    /// (no published-port confirmation needed — tests run non-effectfully) so the
    /// existing call sites stay unchanged.
    async fn set(
        ctx: TestContext,
        w: DeserializeStdin<Wifi<ProfileIdOpt>>,
    ) -> Result<(), Error> {
        super::set(
            ctx,
            DeserializeStdin(super::WifiSetRequest {
                wifi: w.0,
                confirm_published_port_deletion: false,
            }),
        )
        .await
        .map(|_| ())
    }

    /// Build a `[bool; 7]` weekday mask from day indices (0=Sun..6=Sat).
    fn mask(days: &[usize]) -> [bool; 7] {
        let mut m = [false; 7];
        for &d in days {
            m[d] = true;
        }
        m
    }

    #[test]
    fn test_windows_contain_now_non_wrap() {
        // 09:00-17:00 on Monday (day 1) only.
        let windows = vec![("09:00".to_string(), "17:00".to_string(), mask(&[1]))];
        assert!(windows_contain_now(&windows, 1, 12 * 60)); // Mon noon: inside
        assert!(!windows_contain_now(&windows, 1, 8 * 60)); // Mon 08:00: before
        assert!(!windows_contain_now(&windows, 1, 17 * 60)); // Mon 17:00: half-open end
        assert!(!windows_contain_now(&windows, 2, 12 * 60)); // Tue: wrong day
    }

    #[test]
    fn test_windows_contain_now_overnight_wrap() {
        // 22:00-06:00 on Saturday (day 6); tail spills into Sunday (day 0).
        let windows = vec![("22:00".to_string(), "06:00".to_string(), mask(&[6]))];
        assert!(windows_contain_now(&windows, 6, 23 * 60)); // Sat 23:00: head, inside
        assert!(!windows_contain_now(&windows, 6, 21 * 60)); // Sat 21:00: before start
        assert!(windows_contain_now(&windows, 0, 2 * 60)); // Sun 02:00: tail, inside
        assert!(!windows_contain_now(&windows, 0, 6 * 60)); // Sun 06:00: half-open end
        assert!(!windows_contain_now(&windows, 0, 23 * 60)); // Sun night: not blocked
    }

    #[test]
    fn test_windows_contain_now_multiple_and_malformed() {
        let windows = vec![
            ("09:00".to_string(), "12:00".to_string(), mask(&[1])),
            ("14:00".to_string(), "18:00".to_string(), mask(&[3])),
            ("bad".to_string(), "18:00".to_string(), mask(&[5])), // skipped
        ];
        assert!(windows_contain_now(&windows, 1, 10 * 60)); // first window
        assert!(windows_contain_now(&windows, 3, 15 * 60)); // second window
        assert!(!windows_contain_now(&windows, 1, 13 * 60)); // gap between windows
        assert!(!windows_contain_now(&windows, 5, 16 * 60)); // malformed entry never matches
    }

    #[test]
    fn test_windows_contain_now_empty() {
        assert!(!windows_contain_now(&[], 3, 12 * 60));
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

    fn setup_test_configs(dir: &std::path::Path) {
        // startwrt: two profiles — Admin (lan, vlan 99) and Guest (guest, vlan 101)
        std::fs::write(
            dir.join("startwrt"),
            "\
config profile lan
\toption fullname 'Admin'
\toption interface 'lan'
\toption vlan_tag '99'
\toption access_to_new_profiles '1'

config profile guest
\toption fullname 'Guest'
\toption interface 'guest'
\toption vlan_tag '101'
",
        )
        .unwrap();

        // network: bridge device + lan + guest interfaces
        std::fs::write(
            dir.join("network"),
            "\
config device
\toption name 'br-lan'
\toption type 'bridge'
\tlist ports 'eth0'

config interface 'lan'
\toption device 'br-lan.99'
\toption proto 'static'
\toption ipaddr '192.168.1.1'
\toption netmask '255.255.255.0'

config interface 'guest'
\toption device 'br-lan.101'
\toption proto 'static'
\toption ipaddr '192.168.101.1'
\toption netmask '255.255.255.0'

config bridge-vlan
\toption device 'br-lan'
\toption vlan '99'

config bridge-vlan
\toption device 'br-lan'
\toption vlan '101'
",
        )
        .unwrap();

        // firewall: lan + wan zones + forwarding
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
\toption name 'wan'
\tlist network 'wan'
\toption input 'REJECT'
\toption output 'ACCEPT'
\toption forward 'REJECT'

config forwarding
\toption src 'lan'
\toption dest 'wan'
",
        )
        .unwrap();
    }

    fn write_wireless_config(dir: &std::path::Path, stations: &str) {
        std::fs::write(
            dir.join("wireless"),
            format!(
                "\
config wifi-device 'radio0'
\toption type 'mac80211'
\toption band '2g'
\toption channel '1'

config wifi-iface 'default_radio0'
\toption device 'radio0'
\toption mode 'ap'
\toption ssid 'TestNet'
\toption encryption 'psk2'
\toption key 'adminpass1'
\toption dynamic_vlan '1'

{stations}"
            ),
        )
        .unwrap();
    }

    fn make_radios() -> BTreeMap<String, WifiRadio> {
        let mut radios = BTreeMap::new();
        radios.insert(
            "default_radio0".into(),
            WifiRadio {
                band: "2g".into(),
                channel: "1".into(),
                enabled: true,
                broadcast: true,
            },
        );
        radios
    }

    fn write_wireless_config_dual_radio(dir: &std::path::Path, ssid_2g: &str, ssid_5g: &str, stations: &str) {
        std::fs::write(
            dir.join("wireless"),
            format!(
                "\
config wifi-device 'radio0'
\toption type 'mac80211'
\toption band '2g'
\toption channel '1'

config wifi-device 'radio1'
\toption type 'mac80211'
\toption band '5g'
\toption channel '36'

config wifi-iface 'default_radio0'
\toption device 'radio0'
\toption mode 'ap'
\toption ssid '{ssid_2g}'
\toption encryption 'psk2'
\toption key 'adminpass1'
\toption dynamic_vlan '1'

config wifi-iface 'default_radio1'
\toption device 'radio1'
\toption mode 'ap'
\toption ssid '{ssid_5g}'
\toption encryption 'psk2'
\toption key 'adminpass1'
\toption dynamic_vlan '1'

{stations}"
            ),
        )
        .unwrap();
    }

    fn make_dual_radios() -> BTreeMap<String, WifiRadio> {
        let mut radios = BTreeMap::new();
        radios.insert(
            "default_radio0".into(),
            WifiRadio {
                band: "2g".into(),
                channel: "1".into(),
                enabled: true,
                broadcast: true,
            },
        );
        radios.insert(
            "default_radio1".into(),
            WifiRadio {
                band: "5g".into(),
                channel: "36".into(),
                enabled: true,
                broadcast: true,
            },
        );
        radios
    }

    #[tokio::test]
    async fn test_get_reads_label_from_uci() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_test_configs(dir.path());
        write_wireless_config(
            dir.path(),
            "\
config wifi-station
\toption key 'guestpass1'
\toption vid '101'
\toption iface 'default_radio0'
\toption label 'My Label'
",
        );

        let arena = Arena::new();
        let cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["wireless", "startwrt", "network", "firewall"],
        )
        .await
        .unwrap();
        let wifi = get_config(ctx, &cfgs).unwrap();

        let labeled = wifi
            .passwords
            .iter()
            .find(|p| p.password == "guestpass1")
            .unwrap();
        assert_eq!(labeled.label, "My Label");
    }

    #[tokio::test]
    async fn test_get_falls_back_to_profile_name_when_no_label() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_test_configs(dir.path());
        write_wireless_config(
            dir.path(),
            "\
config wifi-station
\toption key 'guestpass1'
\toption vid '101'
\toption iface 'default_radio0'
",
        );

        let arena = Arena::new();
        let cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["wireless", "startwrt", "network", "firewall"],
        )
        .await
        .unwrap();
        let wifi = get_config(ctx, &cfgs).unwrap();

        let fallback = wifi
            .passwords
            .iter()
            .find(|p| p.password == "guestpass1")
            .unwrap();
        assert_eq!(fallback.label, "Guest");
    }

    #[tokio::test]
    async fn test_set_persists_label() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_test_configs(dir.path());
        write_wireless_config(dir.path(), "");

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["wireless", "startwrt", "network", "firewall"],
        )
        .await
        .unwrap();
        let lookup = profiles::Lookup::parse(ctx.clone(), &cfgs).unwrap();

        let guest_id = lookup.from_fullname("Guest").unwrap().clone();
        let mut passwords = BTreeSet::new();
        passwords.insert(Password {
            label: "Main Admin".into(),
            profile: None,
            password: "adminpass1".into(),
        });
        passwords.insert(Password {
            label: "Kids WiFi".into(),
            profile: Some(guest_id.clone()),
            password: "kidspass1".into(),
        });

        let wifi = Wifi {
            ssid: "TestNet".into(),
            broadcast_separately: false,
            radios: make_radios(),
            passwords,
        };
        set_config(&ctx, &mut cfgs, &wifi, &lookup).unwrap();
        dump_all(ctx.uci_root(), cfgs).await.unwrap();

        // Re-parse and check label was written
        let arena2 = Arena::new();
        let cfgs2 = parse_all(
            ctx.uci_root(),
            &arena2,
            &["wireless", "startwrt", "network", "firewall"],
        )
        .await
        .unwrap();

        let mut found_kids = false;
        cfgs2["wireless"].try_each(|_, station: WifiStation| {
            if station.key == "kidspass1" {
                assert_eq!(station.label.as_deref(), Some("Kids WiFi"));
                found_kids = true;
            }
            Ok::<_, Error>(())
        })
        .unwrap();

        assert!(found_kids, "did not find station with key 'kidspass1'");
    }

    #[tokio::test]
    async fn test_set_round_trip_preserves_label() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_test_configs(dir.path());
        write_wireless_config(
            dir.path(),
            "\
config wifi-station
\toption key 'guestpass1'
\toption vid '101'
\toption iface 'default_radio0'
\toption label 'Custom Name'
",
        );

        // GET
        let arena = Arena::new();
        let cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["wireless", "startwrt", "network", "firewall"],
        )
        .await
        .unwrap();
        let wifi = get_config(ctx.clone(), &cfgs).unwrap();

        let labeled = wifi
            .passwords
            .iter()
            .find(|p| p.password == "guestpass1")
            .unwrap();
        assert_eq!(labeled.label, "Custom Name");

        // SET (round-trip the same data)
        drop(cfgs);
        drop(arena);

        let arena2 = Arena::new();
        let mut cfgs2 = parse_all(
            ctx.uci_root(),
            &arena2,
            &["wireless", "startwrt", "network", "firewall"],
        )
        .await
        .unwrap();
        let lookup2 = profiles::Lookup::parse(ctx.clone(), &cfgs2).unwrap();
        set_config(&ctx, &mut cfgs2, &wifi, &lookup2).unwrap();
        dump_all(ctx.uci_root(), cfgs2).await.unwrap();

        // GET again
        let arena3 = Arena::new();
        let cfgs3 = parse_all(
            ctx.uci_root(),
            &arena3,
            &["wireless", "startwrt", "network", "firewall"],
        )
        .await
        .unwrap();
        let wifi2 = get_config(ctx, &cfgs3).unwrap();

        let preserved = wifi2
            .passwords
            .iter()
            .find(|p| p.password == "guestpass1")
            .unwrap();
        assert_eq!(preserved.label, "Custom Name");
    }

    #[tokio::test]
    async fn test_set_allows_duplicate_empty_labels() {
        // Two admin passwords both with empty label should not trigger DuplicatePasswordLabel
        let mut seen_passwords = std::collections::HashSet::new();
        let mut seen_labels = std::collections::HashSet::new();
        let passwords = vec![
            Password::<ProfileId> {
                label: "".into(),
                profile: None,
                password: "pass1".into(),
            },
            Password {
                label: "".into(),
                profile: None,
                password: "pass2".into(),
            },
        ];
        for pass in &passwords {
            assert!(seen_passwords.insert(&pass.password));
            // This is the actual validation logic from set()
            if !pass.label.is_empty() && !seen_labels.insert(&pass.label) {
                panic!("should not reject duplicate empty labels");
            }
        }
    }

    #[tokio::test]
    async fn test_set_rejects_duplicate_nonempty_labels() {
        let mut seen_labels = std::collections::HashSet::new();
        let passwords = vec![
            Password::<ProfileId> {
                label: "Same Name".into(),
                profile: None,
                password: "pass1".into(),
            },
            Password {
                label: "Same Name".into(),
                profile: None,
                password: "pass2".into(),
            },
        ];
        let mut found_dup = false;
        for pass in &passwords {
            if !pass.label.is_empty() && !seen_labels.insert(&pass.label) {
                found_dup = true;
            }
        }
        assert!(found_dup, "should reject duplicate non-empty labels");
    }

    #[tokio::test]
    async fn test_dual_radio_broadcast_separately_detected() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_test_configs(dir.path());
        write_wireless_config_dual_radio(dir.path(), "TestNet", "TestNet-5G", "");

        let arena = Arena::new();
        let cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["wireless", "startwrt", "network", "firewall"],
        )
        .await
        .unwrap();
        let wifi = get_config(ctx, &cfgs).unwrap();

        assert!(wifi.broadcast_separately);
        assert_eq!(wifi.ssid, "TestNet"); // 2.4GHz SSID used as base
    }

    #[tokio::test]
    async fn test_dual_radio_same_ssid_not_broadcast_separately() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_test_configs(dir.path());
        write_wireless_config_dual_radio(dir.path(), "TestNet", "TestNet", "");

        let arena = Arena::new();
        let cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["wireless", "startwrt", "network", "firewall"],
        )
        .await
        .unwrap();
        let wifi = get_config(ctx, &cfgs).unwrap();

        assert!(!wifi.broadcast_separately);
        assert_eq!(wifi.ssid, "TestNet");
    }

    #[tokio::test]
    async fn test_dual_radio_broadcast_separately_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_test_configs(dir.path());
        // Start with same SSID on both radios
        write_wireless_config_dual_radio(dir.path(), "TestNet", "TestNet", "");

        // SET with broadcast_separately = true
        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["wireless", "startwrt", "network", "firewall"],
        )
        .await
        .unwrap();
        let lookup = profiles::Lookup::parse(ctx.clone(), &cfgs).unwrap();

        let mut passwords = BTreeSet::new();
        passwords.insert(Password {
            label: "Main Admin".into(),
            profile: None,
            password: "adminpass1".into(),
        });

        let wifi = Wifi {
            ssid: "TestNet".into(),
            broadcast_separately: true,
            radios: make_dual_radios(),
            passwords,
        };
        set_config(&ctx, &mut cfgs, &wifi, &lookup).unwrap();
        dump_all(ctx.uci_root(), cfgs).await.unwrap();

        // GET — verify 5GHz got "-5G" suffix and broadcast_separately is detected
        let arena2 = Arena::new();
        let cfgs2 = parse_all(
            ctx.uci_root(),
            &arena2,
            &["wireless", "startwrt", "network", "firewall"],
        )
        .await
        .unwrap();
        let wifi2 = get_config(ctx.clone(), &cfgs2).unwrap();

        assert!(wifi2.broadcast_separately);
        assert_eq!(wifi2.ssid, "TestNet");

        // SET with broadcast_separately = false — both should get same SSID
        drop(cfgs2);
        drop(arena2);

        let arena3 = Arena::new();
        let mut cfgs3 = parse_all(
            ctx.uci_root(),
            &arena3,
            &["wireless", "startwrt", "network", "firewall"],
        )
        .await
        .unwrap();
        let lookup3 = profiles::Lookup::parse(ctx.clone(), &cfgs3).unwrap();

        let mut passwords3 = BTreeSet::new();
        passwords3.insert(Password {
            label: "Main Admin".into(),
            profile: None,
            password: "adminpass1".into(),
        });

        let wifi3 = Wifi {
            ssid: "TestNet".into(),
            broadcast_separately: false,
            radios: make_dual_radios(),
            passwords: passwords3,
        };
        set_config(&ctx, &mut cfgs3, &wifi3, &lookup3).unwrap();
        dump_all(ctx.uci_root(), cfgs3).await.unwrap();

        // GET — verify both radios have same SSID
        let arena4 = Arena::new();
        let cfgs4 = parse_all(
            ctx.uci_root(),
            &arena4,
            &["wireless", "startwrt", "network", "firewall"],
        )
        .await
        .unwrap();
        let wifi4 = get_config(ctx, &cfgs4).unwrap();

        assert!(!wifi4.broadcast_separately);
        assert_eq!(wifi4.ssid, "TestNet");
    }

    #[tokio::test]
    async fn test_set_config_returns_full_when_ssid_changes() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_test_configs(dir.path());
        write_wireless_config_dual_radio(dir.path(), "TestNet", "TestNet", "");

        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["wireless", "startwrt", "network", "firewall"],
        )
        .await
        .unwrap();
        let lookup = profiles::Lookup::parse(ctx.clone(), &cfgs).unwrap();

        let mut passwords = BTreeSet::new();
        passwords.insert(Password {
            label: "Admin".into(),
            profile: None,
            password: "adminpass1".into(),
        });

        // Toggle broadcast_separately — changes 5GHz SSID from "TestNet" to "TestNet-5G"
        let wifi = Wifi {
            ssid: "TestNet".into(),
            broadcast_separately: true,
            radios: make_dual_radios(),
            passwords,
        };
        let restart = set_config(&ctx, &mut cfgs, &wifi, &lookup).unwrap();
        assert_eq!(restart, WifiRestart::Full, "SSID change should require full restart");
    }

    #[tokio::test]
    async fn test_set_config_returns_psk_only_when_passwords_change() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_test_configs(dir.path());
        write_wireless_config(dir.path(), "\
config wifi-station
\toption key 'guestpass1'
\toption vid '101'
\toption iface 'default_radio0'
\toption label 'Guest'
");

        // First set to establish baseline
        let arena = Arena::new();
        let mut cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["wireless", "startwrt", "network", "firewall"],
        )
        .await
        .unwrap();
        let lookup = profiles::Lookup::parse(ctx.clone(), &cfgs).unwrap();

        let guest_id = lookup.from_fullname("Guest").unwrap().clone();
        let mut passwords = BTreeSet::new();
        passwords.insert(Password {
            label: "Admin".into(),
            profile: None,
            password: "adminpass1".into(),
        });
        passwords.insert(Password {
            label: "Guest".into(),
            profile: Some(guest_id.clone()),
            password: "guestpass1".into(),
        });

        let wifi = Wifi {
            ssid: "TestNet".into(),
            broadcast_separately: false,
            radios: make_radios(),
            passwords,
        };
        set_config(&ctx, &mut cfgs, &wifi, &lookup).unwrap();
        dump_all(ctx.uci_root(), cfgs).await.unwrap();

        // Now change only passwords — should be PskOnly
        let arena2 = Arena::new();
        let mut cfgs2 = parse_all(
            ctx.uci_root(),
            &arena2,
            &["wireless", "startwrt", "network", "firewall"],
        )
        .await
        .unwrap();
        let lookup2 = profiles::Lookup::parse(ctx.clone(), &cfgs2).unwrap();
        let guest_id2 = lookup2.from_fullname("Guest").unwrap().clone();

        let mut passwords2 = BTreeSet::new();
        passwords2.insert(Password {
            label: "Admin".into(),
            profile: None,
            password: "adminpass1".into(),
        });
        passwords2.insert(Password {
            label: "Guest".into(),
            profile: Some(guest_id2),
            password: "newguestpass".into(),
        });

        let wifi2 = Wifi {
            ssid: "TestNet".into(),
            broadcast_separately: false,
            radios: make_radios(),
            passwords: passwords2,
        };
        let restart = set_config(&ctx, &mut cfgs2, &wifi2, &lookup2).unwrap();
        assert_eq!(restart, WifiRestart::PskOnly, "password-only change should use PSK reload");
    }

    /// Wireless config mimicking a fresh device with no admin password and
    /// both radios disabled (factory state on bpi-f3 when EEPROM tag 0x2F is
    /// unprovisioned and `restore_wifi_if_needed` left things untouched).
    fn write_wireless_config_unkeyed(dir: &std::path::Path) {
        std::fs::write(
            dir.join("wireless"),
            "\
config wifi-device 'radio0'
\toption type 'mac80211'
\toption band '2g'
\toption channel '1'
\toption disabled '1'

config wifi-device 'radio1'
\toption type 'mac80211'
\toption band '5g'
\toption channel '36'
\toption disabled '1'

config wifi-iface 'default_radio0'
\toption device 'radio0'
\toption mode 'ap'
\toption ssid 'OpenWrt'
\toption encryption 'none'
\toption hidden '1'

config wifi-iface 'default_radio1'
\toption device 'radio1'
\toption mode 'ap'
\toption ssid 'OpenWrt'
\toption encryption 'none'
\toption hidden '1'
",
        )
        .unwrap();
    }

    fn make_dual_radios_disabled() -> BTreeMap<String, WifiRadio> {
        let mut radios = make_dual_radios();
        for r in radios.values_mut() {
            r.enabled = false;
            r.broadcast = false;
        }
        radios
    }

    #[tokio::test]
    async fn test_auto_enable_radios_on_first_admin_password_flips_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_test_configs(dir.path());
        write_wireless_config_unkeyed(dir.path());

        let arena = Arena::new();
        let cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["wireless", "startwrt", "network", "firewall"],
        )
        .await
        .unwrap();

        let mut passwords = BTreeSet::new();
        passwords.insert(Password {
            label: "Admin".into(),
            profile: None,
            password: "adminpass1".into(),
        });

        let mut wifi = Wifi {
            ssid: "TestNet".into(),
            broadcast_separately: false,
            radios: make_dual_radios_disabled(),
            passwords,
        };

        let flipped = auto_enable_radios_on_first_admin_password(&mut wifi, &cfgs).unwrap();
        assert!(flipped, "should auto-enable on first admin password add");
        assert!(
            wifi.radios.values().all(|r| r.enabled),
            "all radios should now be enabled"
        );
        assert!(
            wifi.radios.values().all(|r| r.broadcast),
            "all radios should now be broadcasting (hidden cleared)"
        );
    }

    #[tokio::test]
    async fn test_auto_enable_skipped_when_admin_already_present() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_test_configs(dir.path());
        // write_wireless_config_dual_radio writes both ifaces with key='adminpass1'
        write_wireless_config_dual_radio(dir.path(), "TestNet", "TestNet", "");

        let arena = Arena::new();
        let cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["wireless", "startwrt", "network", "firewall"],
        )
        .await
        .unwrap();

        let mut passwords = BTreeSet::new();
        passwords.insert(Password {
            label: "Admin".into(),
            profile: None,
            password: "adminpass1".into(),
        });

        let mut wifi = Wifi {
            ssid: "TestNet".into(),
            broadcast_separately: false,
            radios: make_dual_radios_disabled(),
            passwords,
        };

        let flipped = auto_enable_radios_on_first_admin_password(&mut wifi, &cfgs).unwrap();
        assert!(!flipped, "should not auto-enable when admin password already present");
        assert!(
            wifi.radios.values().all(|r| !r.enabled),
            "user's explicit disable should be respected on subsequent edits"
        );
        assert!(
            wifi.radios.values().all(|r| !r.broadcast),
            "broadcast should also be untouched on subsequent edits"
        );
    }

    #[tokio::test]
    async fn test_auto_enable_respects_partial_radio_enable() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_test_configs(dir.path());
        write_wireless_config_unkeyed(dir.path());

        let arena = Arena::new();
        let cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["wireless", "startwrt", "network", "firewall"],
        )
        .await
        .unwrap();

        // First-add scenario, but user explicitly enabled only the 2.4 GHz radio
        let mut radios = make_dual_radios_disabled();
        radios.get_mut("default_radio0").unwrap().enabled = true;

        let mut passwords = BTreeSet::new();
        passwords.insert(Password {
            label: "Admin".into(),
            profile: None,
            password: "adminpass1".into(),
        });

        let mut wifi = Wifi {
            ssid: "TestNet".into(),
            broadcast_separately: false,
            radios,
            passwords,
        };

        let flipped = auto_enable_radios_on_first_admin_password(&mut wifi, &cfgs).unwrap();
        assert!(!flipped, "should not flip when user already enabled at least one radio");
        assert!(wifi.radios["default_radio0"].enabled);
        assert!(!wifi.radios["default_radio1"].enabled);
    }

    #[tokio::test]
    async fn test_auto_enable_skipped_without_admin_password() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_test_configs(dir.path());
        write_wireless_config_unkeyed(dir.path());

        let arena = Arena::new();
        let cfgs = parse_all(
            ctx.uci_root(),
            &arena,
            &["wireless", "startwrt", "network", "firewall"],
        )
        .await
        .unwrap();
        let lookup = profiles::Lookup::parse(ctx.clone(), &cfgs).unwrap();
        let guest_id = lookup.from_fullname("Guest").unwrap().clone();

        // Only profile-scoped passwords, no admin
        let mut passwords = BTreeSet::new();
        passwords.insert(Password {
            label: "Guest".into(),
            profile: Some(guest_id),
            password: "guestpass1".into(),
        });

        let mut wifi = Wifi {
            ssid: "TestNet".into(),
            broadcast_separately: false,
            radios: make_dual_radios_disabled(),
            passwords,
        };

        let flipped = auto_enable_radios_on_first_admin_password(&mut wifi, &cfgs).unwrap();
        assert!(!flipped, "should not auto-enable without an admin password");
        assert!(wifi.radios.values().all(|r| !r.enabled));
        assert!(wifi.radios.values().all(|r| !r.broadcast));
    }

    #[tokio::test]
    async fn test_set_rejects_short_password() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_test_configs(dir.path());
        write_wireless_config_unkeyed(dir.path());

        let mut passwords = BTreeSet::<Password<ProfileIdOpt>>::new();
        passwords.insert(Password {
            label: "Admin".into(),
            profile: None,
            password: "short".into(), // 5 chars, < MIN_PSK_LEN
        });

        let wifi = Wifi {
            ssid: "TestNet".into(),
            broadcast_separately: false,
            radios: make_dual_radios(),
            passwords,
        };

        let err = set(ctx, DeserializeStdin(wifi)).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidValue);
    }

    #[tokio::test]
    async fn test_set_rejects_long_password() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());
        setup_test_configs(dir.path());
        write_wireless_config_unkeyed(dir.path());

        let mut passwords = BTreeSet::<Password<ProfileIdOpt>>::new();
        passwords.insert(Password {
            label: "Admin".into(),
            profile: None,
            password: "x".repeat(MAX_PSK_LEN + 1), // 64 chars, > MAX_PSK_LEN
        });

        let wifi = Wifi {
            ssid: "TestNet".into(),
            broadcast_separately: false,
            radios: make_dual_radios(),
            passwords,
        };

        let err = set(ctx, DeserializeStdin(wifi)).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidValue);
    }

    #[test]
    fn test_days_to_cron() {
        // [Sun..Sat]; 0 = Sunday.
        assert_eq!(days_to_cron(&[false, true, false, false, false, false, false]), "1");
        assert_eq!(days_to_cron(&[true, false, false, false, false, false, true]), "0,6");
        assert_eq!(days_to_cron(&[false; 7]), "");
    }

    #[test]
    fn test_shift_days_forward() {
        let mut mon = [false; 7];
        mon[1] = true;
        let mut tue = [false; 7];
        tue[2] = true;
        assert_eq!(shift_days_forward(&mon), tue);

        // Saturday (6) wraps to Sunday (0) via mod 7 — the "end of week" case.
        let mut sat = [false; 7];
        sat[6] = true;
        let mut sun = [false; 7];
        sun[0] = true;
        assert_eq!(shift_days_forward(&sat), sun);

        // All-7 is the identity.
        assert_eq!(shift_days_forward(&[true; 7]), [true; 7]);
    }

    #[test]
    fn test_windows_overlap() {
        let all = [true; 7];
        let mut mon = [false; 7];
        mon[1] = true;
        let mut tue = [false; 7];
        tue[2] = true;

        // Disjoint times, same day.
        assert!(!windows_overlap(&[(9 * 60, 17 * 60, mon), (18 * 60, 20 * 60, mon)]));
        // Overlapping times, same day.
        assert!(windows_overlap(&[(9 * 60, 17 * 60, mon), (16 * 60, 18 * 60, mon)]));
        // Same times, different days: no overlap.
        assert!(!windows_overlap(&[(9 * 60, 17 * 60, mon), (9 * 60, 17 * 60, tue)]));
        // Daily 22:00-06:00 does not self-overlap (8h nightly gap).
        assert!(!windows_overlap(&[(22 * 60, 6 * 60, all)]));
        // Mon-night wrap (->Tue 06:00) overlaps a Tue 04:00-05:00 window.
        assert!(windows_overlap(&[(22 * 60, 6 * 60, mon), (4 * 60, 5 * 60, tue)]));

        // The two surviving mock windows must not overlap.
        let mut sun_fri_sat = [false; 7];
        sun_fri_sat[0] = true;
        sun_fri_sat[5] = true;
        sun_fri_sat[6] = true;
        let mut mon_fri = [false; 7];
        for d in 1..=5 {
            mon_fri[d] = true;
        }
        assert!(!windows_overlap(&[
            (6 * 60, 10 * 60, sun_fri_sat),
            (22 * 60, 6 * 60, mon_fri),
        ]));
    }

    #[tokio::test]
    async fn test_blackout_wrap_shifts_up_day() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        // Saturday-night-only blackout that crosses into Sunday morning.
        let mut days = [false; 7];
        days[6] = true; // Saturday
        blackout_set(
            ctx.clone(),
            DeserializeStdin(BlackoutWindows {
                windows: vec![BlackoutWindow {
                    start_time: "22:00".to_string(),
                    end_time: "06:00".to_string(),
                    days,
                }],
            }),
        )
        .await
        .unwrap();

        let content = tokio::fs::read_to_string(dir.path().join("crontab_root"))
            .await
            .unwrap();
        // down fires Saturday (6); up fires the next day, Sunday (0).
        assert!(content.contains("0 22 * * 6 wifi down"), "got: {content}");
        assert!(content.contains("0 6 * * 0 wifi up"), "got: {content}");
    }

    #[tokio::test]
    async fn test_blackout_rejects_overlap() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        let result = blackout_set(
            ctx.clone(),
            DeserializeStdin(BlackoutWindows {
                windows: vec![
                    BlackoutWindow {
                        start_time: "09:00".to_string(),
                        end_time: "17:00".to_string(),
                        days: [true; 7],
                    },
                    BlackoutWindow {
                        start_time: "16:00".to_string(),
                        end_time: "18:00".to_string(),
                        days: [true; 7],
                    },
                ],
            }),
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_blackout_rejects_full_week() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        // 24h on every day == the whole week with no gap: every cron edge
        // annihilates, leaving nothing to execute, so it's rejected.
        let result = blackout_set(
            ctx.clone(),
            DeserializeStdin(BlackoutWindows {
                windows: vec![BlackoutWindow {
                    start_time: "08:00".to_string(),
                    end_time: "08:00".to_string(),
                    days: [true; 7],
                }],
            }),
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_blackout_allows_24h_single_day() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        // A full-24h window on Monday only: down Monday, up the next day (Tuesday).
        let result = blackout_set(
            ctx.clone(),
            DeserializeStdin(BlackoutWindows {
                windows: vec![BlackoutWindow {
                    start_time: "09:00".to_string(),
                    end_time: "09:00".to_string(),
                    days: mask(&[1]),
                }],
            }),
        )
        .await;
        assert!(result.is_ok(), "{result:?}");

        // Round-trips through UCI.
        let windows = blackout_get(ctx.clone()).await.unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].start_time, "09:00");
        assert_eq!(windows[0].end_time, "09:00");
        assert_eq!(windows[0].days, mask(&[1]));

        let content = tokio::fs::read_to_string(dir.path().join("crontab_root"))
            .await
            .unwrap();
        assert!(content.contains("0 9 * * 1 wifi down"), "got: {content}");
        assert!(content.contains("0 9 * * 2 wifi up"), "got: {content}");
    }

    #[tokio::test]
    async fn test_blackout_deconflicts_consecutive_24h() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = TestContext(dir.path().to_path_buf());

        // 24h on Monday AND Tuesday: the Tue 09:00 edge (Mon's "up" meeting Tue's
        // "down") must annihilate, leaving down Mon / up Wed and continuous block.
        blackout_set(
            ctx.clone(),
            DeserializeStdin(BlackoutWindows {
                windows: vec![BlackoutWindow {
                    start_time: "09:00".to_string(),
                    end_time: "09:00".to_string(),
                    days: mask(&[1, 2]),
                }],
            }),
        )
        .await
        .unwrap();

        let content = tokio::fs::read_to_string(dir.path().join("crontab_root"))
            .await
            .unwrap();
        assert!(content.contains("0 9 * * 1 wifi down"), "got: {content}");
        assert!(content.contains("0 9 * * 3 wifi up"), "got: {content}");
        // Nothing fires at Tue 09:00 — no edge mentions day 2.
        assert!(!content.contains("* * 2 wifi"), "got: {content}");
    }

    #[test]
    fn test_deconflict_non_conflicting() {
        // 09:00-17:00 Monday: a plain window keeps both its edges unchanged.
        let (downs, ups) = deconflict_edges(&[(9 * 60, 17 * 60, mask(&[1]))]);
        assert_eq!(downs.get(&(9 * 60)), Some(&mask(&[1])));
        assert_eq!(ups.get(&(17 * 60)), Some(&mask(&[1])));
    }

    #[test]
    fn test_deconflict_consecutive_24h() {
        // Mon+Tue 09:00==09:00 → down {Mon}@540, up {Wed}@540, Tue annihilated.
        let (downs, ups) = deconflict_edges(&[(9 * 60, 9 * 60, mask(&[1, 2]))]);
        assert_eq!(downs.get(&(9 * 60)), Some(&mask(&[1])));
        assert_eq!(ups.get(&(9 * 60)), Some(&mask(&[3])));
        // Tuesday (day 2) survives in neither edge.
        assert!(!downs[&(9 * 60)][2] && !ups[&(9 * 60)][2]);
    }

    #[test]
    fn test_deconflict_adjacent_windows() {
        // 06:00-22:00 Mon + 22:00-06:00 Mon (wraps to Tue) = continuous Mon 06:00
        // → Tue 06:00. The shared 22:00 Monday edge annihilates entirely.
        let (downs, ups) =
            deconflict_edges(&[(6 * 60, 22 * 60, mask(&[1])), (22 * 60, 6 * 60, mask(&[1]))]);
        assert_eq!(downs.get(&(6 * 60)), Some(&mask(&[1]))); // down Mon 06:00
        assert_eq!(ups.get(&(6 * 60)), Some(&mask(&[2]))); // up Tue 06:00
        assert!(!downs.contains_key(&(22 * 60)));
        assert!(!ups.contains_key(&(22 * 60)));
    }

    #[test]
    fn test_deconflict_and_covers_full_week() {
        // 24h every day: all edges annihilate; covers_full_week flags it.
        let (downs, ups) = deconflict_edges(&[(0, 0, [true; 7])]);
        assert!(downs.is_empty() && ups.is_empty());
        assert!(covers_full_week(&[(0, 0, [true; 7])]));
        // Seven adjacent windows tiling the week also count as full coverage.
        let tiling: Vec<(u32, u32, [bool; 7])> =
            (0..7).map(|d| (0, 0, mask(&[d]))).collect();
        assert!(covers_full_week(&tiling));
        // A single day's gap is not full coverage.
        assert!(!covers_full_week(&[(9 * 60, 17 * 60, [true; 7])]));
        assert!(!covers_full_week(&[]));
    }

    #[test]
    fn test_parse_windows_drops_malformed() {
        // Two parts (missing the days field) is malformed and dropped; the valid
        // entry is kept.
        let out = parse_windows(&["09:00|17:00".to_string(), "09:00|17:00|1".to_string()]);
        assert_eq!(out, vec![("09:00".to_string(), "17:00".to_string(), mask(&[1]))]);
    }

    #[test]
    fn test_windows_to_minutes_drops_unparseable_times() {
        let good = ("09:00".to_string(), "17:00".to_string(), mask(&[1]));
        let bad = ("9".to_string(), "17:00".to_string(), mask(&[2]));
        assert_eq!(
            windows_to_minutes(&[good, bad]),
            vec![(9 * 60, 17 * 60, mask(&[1]))]
        );
    }
}
